//! Lobby screen – seznam, vytvoření a čekárna lobby.

use engine::{
    Rect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    ui::{UiCtx, colors},
};
use engine::winit::event::MouseButton;
use engine::winit::keyboard::KeyCode;

use net::{ClientMsg, LobbyInfo, LobbyState, ServerMsg};

use crate::net::{ConnState, NetClient};

use super::{Screen, Transition};

// ── Textová pole ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FocusField {
    None,
    Host,
    Port,
    PlayerName,
    CreateName,
    Chat,
}

// ── Stavy lobby screenu ───────────────────────────────────────────────────────

enum LobbyView {
    /// Zobrazení seznamu dostupných lobby.
    List {
        lobbies:     Vec<LobbyInfo>,
        selected:    Option<usize>,
        create_name: String,
        status_msg:  String,
    },
    /// V lobby (čekárna).
    Waiting {
        state:      LobbyState,
        my_id:      u64,
        ready:      bool,
        chat_input: String,
        chat_log:   Vec<String>,
    },
}

pub struct LobbyScreen {
    net:      Option<NetClient>,
    view:     LobbyView,
    my_id:    u64,
    white_bg: Option<engine::wgpu::BindGroup>,

    connect_host:  String,
    connect_port:  String,
    player_name:   String,
    connect_error: String,

    focus:      FocusField,
    /// Vyplní se když server pošle GameStart; update() to použije k přechodu.
    game_start: Option<(String, u8, u8, Vec<u8>, u32, u32, f32, f32)>,
    // (map_id, your_team, tick_rate, map_tiles, map_w, map_h, base_x, base_y)
}

impl LobbyScreen {
    pub fn new() -> Self {
        Self {
            net:           None,
            view:          LobbyView::List {
                lobbies:     Vec::new(),
                selected:    None,
                create_name: "Moje lobby".into(),
                status_msg:  String::new(),
            },
            my_id:         0,
            white_bg:      None,
            connect_host:  "127.0.0.1".into(),
            connect_port:  net::DEFAULT_PORT.to_string(),
            player_name:   "Hráč".into(),
            connect_error: String::new(),
            focus:      FocusField::None,
            game_start: None,
        }
    }

    fn handle_server_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Hello { player_id, .. } => {
                self.my_id = player_id;
            }
            ServerMsg::LobbyList { lobbies } => {
                if let LobbyView::List { lobbies: ref mut l, status_msg: ref mut s, .. } = self.view {
                    *l = lobbies;
                    *s = format!("Nalezeno {} lobby", l.len());
                }
            }
            ServerMsg::LobbyJoined { lobby } => {
                self.view = LobbyView::Waiting {
                    my_id:      self.my_id,
                    state:      lobby,
                    ready:      false,
                    chat_input: String::new(),
                    chat_log:   vec!["Připojeno do lobby.".into()],
                };
            }
            ServerMsg::LobbyUpdated { lobby } => {
                if let LobbyView::Waiting { state: ref mut s, .. } = self.view {
                    *s = lobby;
                }
            }
            ServerMsg::LobbyLeft => {
                self.view = LobbyView::List {
                    lobbies:     Vec::new(),
                    selected:    None,
                    create_name: "Moje lobby".into(),
                    status_msg:  "Opustili jste lobby.".into(),
                };
                if let Some(net) = &self.net {
                    net.send(ClientMsg::RequestLobbyList);
                }
            }
            ServerMsg::ChatMsg { from, text } => {
                if let LobbyView::Waiting { chat_log: ref mut log, .. } = self.view {
                    log.push(format!("{from}: {text}"));
                    if log.len() > 30 { log.remove(0); }
                }
            }
            ServerMsg::GameStart { map_id, your_team, tick_rate, map_tiles, map_w, map_h, base_x, base_y } => {
                log::info!("Hra začíná! Mapa={map_id}, tým={your_team}");
                self.game_start = Some((map_id, your_team, tick_rate, map_tiles, map_w, map_h, base_x, base_y));
            }
            ServerMsg::Error { msg } => {
                match &mut self.view {
                    LobbyView::List { status_msg, .. } => *status_msg = format!("Chyba: {msg}"),
                    LobbyView::Waiting { chat_log, .. } => chat_log.push(format!("[CHYBA] {msg}")),
                }
            }
            _ => {}
        }
    }
}

impl Screen for LobbyScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        self.white_bg = Some(tex.create_bind_group(ctx, &batch.texture_bind_group_layout));
    }

    fn update(&mut self, _dt: f32, input: &Input, _camera: &mut Camera) -> Transition {
        // Zpracuj přijaté síťové zprávy
        if let Some(net) = &self.net {
            for msg in net.drain() {
                self.handle_server_msg(msg);
            }
        }

        // Přechod do hry – server poslal GameStart
        if let Some((map_id, your_team, tick_rate, map_tiles, map_w, map_h, base_x, base_y))
            = self.game_start.take()
        {
            if let Some(net) = self.net.take() {
                use super::multiplayer::MultiplayerScreen;
                return Transition::To(Box::new(
                    MultiplayerScreen::new(net, map_id, your_team, tick_rate,
                        map_tiles, map_w, map_h, base_x, base_y, _camera)
                ));
            }
        }

        // ESC zpět
        if input.key_just_pressed(KeyCode::Escape) {
            if let LobbyView::Waiting { .. } = &self.view {
                if let Some(net) = &self.net {
                    net.send(ClientMsg::LeaveLobby);
                }
            } else {
                use super::main_menu::MainMenuScreen;
                return Transition::To(Box::new(MainMenuScreen::new()));
            }
        }

        // Text input – přidej znaky do fokusovaného pole
        if !input.text_input.is_empty() {
            match self.focus {
                FocusField::Host => self.connect_host.push_str(&input.text_input),
                FocusField::Port => {
                    for c in input.text_input.chars() {
                        if c.is_ascii_digit() { self.connect_port.push(c); }
                    }
                }
                FocusField::PlayerName => self.player_name.push_str(&input.text_input),
                FocusField::CreateName => {
                    if let LobbyView::List { ref mut create_name, .. } = self.view {
                        create_name.push_str(&input.text_input);
                    }
                }
                FocusField::Chat => {
                    if let LobbyView::Waiting { ref mut chat_input, .. } = self.view {
                        chat_input.push_str(&input.text_input);
                    }
                }
                FocusField::None => {}
            }
        }

        // Backspace
        if input.key_just_pressed(KeyCode::Backspace) {
            match self.focus {
                FocusField::Host       => { self.connect_host.pop(); }
                FocusField::Port       => { self.connect_port.pop(); }
                FocusField::PlayerName => { self.player_name.pop(); }
                FocusField::CreateName => {
                    if let LobbyView::List { ref mut create_name, .. } = self.view {
                        create_name.pop();
                    }
                }
                FocusField::Chat => {
                    if let LobbyView::Waiting { ref mut chat_input, .. } = self.view {
                        chat_input.pop();
                    }
                }
                FocusField::None => {}
            }
        }

        // Enter v chatu → odešli zprávu
        if input.key_just_pressed(KeyCode::Enter) && self.focus == FocusField::Chat {
            if let LobbyView::Waiting { ref mut chat_input, .. } = self.view {
                if !chat_input.is_empty() {
                    let text = std::mem::take(chat_input);
                    if let Some(net) = &self.net {
                        net.send(ClientMsg::ChatMsg { text });
                    }
                }
            }
        }

        Transition::None
    }

    fn render(&mut self, _batch: &mut SpriteBatch, _camera: &Camera) {}

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        // Pozadí
        ui.panel(Rect::new(0.0, 0.0, sw, sh), [0.04, 0.04, 0.06, 1.0]);

        // Horní lišta
        ui.panel(Rect::new(0.0, 0.0, sw, 36.0), [0.08, 0.10, 0.16, 1.0]);
        ui.label(12.0, 10.0, "MULTIPLAYER", 2.0, colors::WHITE);
        ui.label(sw - 160.0, 10.0, "ESC = zpet", 1.0, colors::GREY);

        // Sbírej akce, které chceme provést po renderování
        let mut do_connect    = false;
        let mut do_disconnect = false;
        let mut pending_msg: Option<ClientMsg> = None;
        let mut new_focus:   Option<FocusField> = None;

        match &mut self.view {
            LobbyView::List { lobbies, selected, create_name, status_msg } => {
                // ── Panel připojení ──────────────────────────────────────
                let connect_action = render_connect_panel(
                    ui, sw,
                    self.net.as_ref().map(|n| n.conn_state()),
                    self.net.is_some(),
                    &self.connect_host,
                    &self.connect_port,
                    &self.player_name,
                    &self.connect_error,
                    self.focus,
                    &mut new_focus,
                );
                match connect_action {
                    ConnectAction::Connect    => do_connect = true,
                    ConnectAction::Disconnect => do_disconnect = true,
                    ConnectAction::None       => {}
                }

                // ── Seznam lobby ──────────────────────────────────────────
                let list_x = 12.0;
                let list_y = 160.0;
                let list_w = (sw - 24.0) * 0.60;
                let list_h = sh - list_y - 80.0;
                ui.panel(Rect::new(list_x, list_y, list_w, list_h), [0.07, 0.08, 0.12, 1.0]);
                ui.border(Rect::new(list_x, list_y, list_w, list_h), 1.0, colors::BORDER);
                ui.label(list_x + 8.0, list_y + 4.0, "Dostupna lobby:", 1.0, colors::GREY);

                if lobbies.is_empty() {
                    ui.label_centered(
                        Rect::new(list_x, list_y + 30.0, list_w, list_h - 30.0),
                        "Zadna lobby k dispozici", 1.0, colors::GREY,
                    );
                } else {
                    for (i, lobby) in lobbies.iter().enumerate() {
                        let ry = list_y + 28.0 + i as f32 * 36.0;
                        if ry + 36.0 > list_y + list_h { break; }
                        let row_rect = Rect::new(list_x + 4.0, ry, list_w - 8.0, 32.0);
                        let is_sel = *selected == Some(i);
                        // Kliknutí na řádek → vyber
                        if row_rect.contains(ui.input.mouse_pos)
                            && ui.input.mouse_just_released(MouseButton::Left)
                        {
                            *selected = Some(i);
                        }
                        let bg = if is_sel {
                            [0.15, 0.25, 0.40, 1.0]
                        } else {
                            [0.09, 0.11, 0.16, 1.0]
                        };
                        ui.panel(row_rect, bg);
                        ui.label(list_x + 12.0, ry + 8.0,
                            &format!("{} [{}/{}]  map:{}", lobby.name, lobby.players, lobby.max_players, lobby.map_id),
                            1.0, if lobby.in_game { colors::GREY } else { colors::WHITE },
                        );
                    }
                }

                // ── Pravý panel – akce ────────────────────────────────────
                let btn_x = list_x + list_w + 12.0;
                let btn_w = sw - btn_x - 12.0;
                ui.panel(Rect::new(btn_x, list_y, btn_w, list_h), [0.07, 0.08, 0.12, 1.0]);
                ui.border(Rect::new(btn_x, list_y, btn_w, list_h), 1.0, colors::BORDER);
                ui.label(btn_x + 8.0, list_y + 6.0, "Akce:", 1.0, colors::GREY);

                let r_btn_y = list_y + 30.0;

                // Obnovit seznam
                if ui.button(Rect::new(btn_x + 8.0, r_btn_y, btn_w - 16.0, 36.0), colors::BTN_NORMAL) {
                    pending_msg = Some(ClientMsg::RequestLobbyList);
                }
                ui.label_shadowed(btn_x + 16.0, r_btn_y + 10.0, "Obnovit seznam", 1.0, colors::WHITE);

                // Připojit do vybraného
                if ui.button(Rect::new(btn_x + 8.0, r_btn_y + 44.0, btn_w - 16.0, 36.0), [0.15, 0.35, 0.15, 1.0]) {
                    if let Some(idx) = *selected {
                        if let Some(lobby) = lobbies.get(idx) {
                            pending_msg = Some(ClientMsg::JoinLobby { id: lobby.id });
                        }
                    }
                }
                ui.label_shadowed(btn_x + 16.0, r_btn_y + 54.0, "Pripojit do vybraneho", 1.0, colors::WHITE);

                // Vstupní pole pro jméno lobby
                let create_label_y = r_btn_y + 96.0;
                ui.label(btn_x + 8.0, create_label_y, "Jmeno lobby:", 1.0, colors::GREY);
                let name_field = Rect::new(btn_x + 8.0, create_label_y + 16.0, btn_w - 16.0, 24.0);
                let is_name_focused = self.focus == FocusField::CreateName;
                ui.panel(name_field, [0.12, 0.14, 0.20, 1.0]);
                ui.border(name_field, 1.0, if is_name_focused { [0.5, 0.7, 1.0, 1.0] } else { colors::BORDER });
                if name_field.contains(ui.input.mouse_pos) && ui.input.mouse_just_released(MouseButton::Left) {
                    new_focus = Some(FocusField::CreateName);
                }
                let name_display = if is_name_focused {
                    format!("{}_", create_name)
                } else {
                    create_name.clone()
                };
                ui.label(name_field.x + 4.0, name_field.y + 6.0, &name_display, 1.0, colors::WHITE);

                // Vytvořit lobby
                let create_btn_y = create_label_y + 48.0;
                if ui.button(Rect::new(btn_x + 8.0, create_btn_y, btn_w - 16.0, 36.0), [0.15, 0.25, 0.45, 1.0]) {
                    pending_msg = Some(ClientMsg::CreateLobby {
                        name:        create_name.clone(),
                        max_players: 4,
                        map_id:      "default".into(),
                    });
                }
                ui.label_shadowed(btn_x + 16.0, create_btn_y + 10.0, "Vytvorit lobby", 1.0, colors::WHITE);

                // Status
                ui.label(12.0, sh - 22.0, status_msg, 1.0, colors::GREY);
            }

            LobbyView::Waiting { state, my_id, ready, chat_input, chat_log } => {
                let pad   = 12.0;
                let col_w = (sw - pad * 3.0) * 0.5;
                let top   = 46.0;
                let avail_h = sh - top - pad;

                // ── Levý panel – hráči ────────────────────────────────────
                ui.panel(Rect::new(pad, top, col_w, avail_h * 0.55), [0.07, 0.08, 0.12, 1.0]);
                ui.border(Rect::new(pad, top, col_w, avail_h * 0.55), 1.0, colors::BORDER);
                ui.label(pad + 8.0, top + 6.0,
                    &format!("Lobby: {}  [{}]", state.name, state.map_id), 1.0, colors::WHITE);

                for (i, p) in state.players.iter().enumerate() {
                    let py      = top + 30.0 + i as f32 * 28.0;
                    let is_host = p.id == state.host_id;
                    let is_me   = p.id == *my_id;
                    let color   = if p.ready { [0.2, 0.8, 0.2, 1.0] } else { [0.8, 0.8, 0.8, 1.0] };
                    ui.label(pad + 16.0, py,
                        &format!("{} [tym {}]{}{}",
                            p.name, p.team,
                            if is_host { " host" } else { "" },
                            if is_me   { " <--"  } else { "" }),
                        1.0, color,
                    );
                }

                // ── Pravý panel – chat ────────────────────────────────────
                let cx     = pad * 2.0 + col_w;
                let chat_h = avail_h * 0.55;
                ui.panel(Rect::new(cx, top, col_w, chat_h), [0.06, 0.06, 0.10, 1.0]);
                ui.border(Rect::new(cx, top, col_w, chat_h), 1.0, colors::BORDER);
                ui.label(cx + 8.0, top + 6.0, "Chat:", 1.0, colors::GREY);

                let line_h    = 14.0;
                let max_lines = ((chat_h - 44.0) / line_h) as usize;
                let start     = chat_log.len().saturating_sub(max_lines);
                for (i, line) in chat_log[start..].iter().enumerate() {
                    ui.label(cx + 8.0, top + 24.0 + i as f32 * line_h, line, 1.0, colors::WHITE);
                }

                // Chat vstupní pole
                let chat_field = Rect::new(cx + 4.0, top + chat_h - 28.0, col_w - 8.0, 22.0);
                let chat_focused = self.focus == FocusField::Chat;
                ui.panel(chat_field, [0.12, 0.14, 0.20, 1.0]);
                ui.border(chat_field, 1.0, if chat_focused { [0.5, 0.7, 1.0, 1.0] } else { colors::BORDER });
                if chat_field.contains(ui.input.mouse_pos) && ui.input.mouse_just_released(MouseButton::Left) {
                    new_focus = Some(FocusField::Chat);
                }
                let chat_display = if chat_focused {
                    format!("{}_", chat_input)
                } else if chat_input.is_empty() {
                    "Klikni a pis...".into()
                } else {
                    chat_input.clone()
                };
                let chat_text_color = if chat_input.is_empty() && !chat_focused {
                    colors::GREY
                } else {
                    colors::WHITE
                };
                ui.label(chat_field.x + 4.0, chat_field.y + 5.0, &chat_display, 1.0, chat_text_color);

                // ── Dolní panel – tlačítka ────────────────────────────────
                let btn_y = top + avail_h * 0.55 + pad;
                let btn_h = avail_h * 0.35;
                ui.panel(Rect::new(pad, btn_y, sw - pad * 2.0, btn_h), [0.07, 0.08, 0.12, 1.0]);
                ui.border(Rect::new(pad, btn_y, sw - pad * 2.0, btn_h), 1.0, colors::BORDER);

                let is_host  = *my_id == state.host_id;
                let rb       = Rect::new(pad + 8.0, btn_y + 12.0, 200.0, 40.0);
                let ready_col = if *ready { [0.1, 0.5, 0.1, 1.0] } else { [0.2, 0.2, 0.2, 1.0] };
                if ui.button(rb, ready_col) {
                    *ready = !*ready;
                    pending_msg = Some(ClientMsg::SetReady { ready: *ready });
                }
                ui.label_shadowed(rb.x + 12.0, rb.y + 12.0,
                    if *ready { "Pripraveni!" } else { "Pripraveni?" }, 1.0, colors::WHITE);

                if is_host {
                    let sb        = Rect::new(pad + 220.0, btn_y + 12.0, 200.0, 40.0);
                    let all_ready = state.players.iter().all(|p| p.ready);
                    let col       = if all_ready { [0.1, 0.6, 0.1, 1.0] } else { [0.2, 0.2, 0.2, 1.0] };
                    if ui.button(sb, col) && all_ready {
                        pending_msg = Some(ClientMsg::StartGame);
                    }
                    ui.label_shadowed(sb.x + 12.0, sb.y + 12.0, "Spustit hru", 1.0, colors::WHITE);
                }

                let leave_btn = Rect::new(sw - pad - 120.0, btn_y + 12.0, 110.0, 40.0);
                if ui.button(leave_btn, colors::BTN_DANGER) {
                    pending_msg = Some(ClientMsg::LeaveLobby);
                }
                ui.label_shadowed(leave_btn.x + 12.0, leave_btn.y + 12.0, "Odejit", 1.0, colors::WHITE);
            }
        }

        // ── Zpracuj akce po skončení match (borrows uvolněny) ─────────────────

        if do_connect {
            match self.connect_port.parse::<u16>() {
                Ok(port) => {
                    self.net = Some(NetClient::connect(
                        &self.connect_host, port, self.player_name.clone(),
                    ));
                    self.connect_error.clear();
                }
                Err(_) => {
                    self.connect_error = "Neplatny port".into();
                }
            }
        } else if do_disconnect {
            self.net = None;
            self.connect_error.clear();
        }

        if let Some(msg) = pending_msg {
            if let Some(net) = &self.net {
                net.send(msg);
            }
        }

        if let Some(f) = new_focus {
            self.focus = f;
        }
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.white_bg.as_ref().expect("LobbyScreen::init not called")
    }
}

// ── Connect panel ─────────────────────────────────────────────────────────────

enum ConnectAction { None, Connect, Disconnect }

fn render_connect_panel(
    ui:          &mut UiCtx,
    sw:          f32,
    net_state:   Option<ConnState>,
    has_net:     bool,
    host:        &str,
    port:        &str,
    name:        &str,
    error:       &str,
    focus:       FocusField,
    new_focus:   &mut Option<FocusField>,
) -> ConnectAction {
    let py = 44.0;
    let ph = 110.0;
    ui.panel(Rect::new(0.0, py, sw, ph), [0.06, 0.07, 0.11, 1.0]);
    ui.border(Rect::new(0.0, py, sw, ph), 1.0, colors::BORDER);

    // ── Adresa serveru ──────────────────────────────────────────────────
    ui.label(12.0, py + 8.0, "Adresa serveru:", 1.0, colors::GREY);
    let host_field = Rect::new(160.0, py + 4.0, 220.0, 24.0);
    let host_foc   = focus == FocusField::Host;
    ui.panel(host_field, [0.12, 0.14, 0.20, 1.0]);
    ui.border(host_field, 1.0, if host_foc { [0.5, 0.7, 1.0, 1.0] } else { colors::BORDER });
    if host_field.contains(ui.input.mouse_pos) && ui.input.mouse_just_released(MouseButton::Left) {
        *new_focus = Some(FocusField::Host);
    }
    let host_disp = if host_foc { format!("{host}_") } else { host.to_string() };
    ui.label(host_field.x + 4.0, host_field.y + 6.0, &host_disp, 1.0, colors::WHITE);

    // ── Port ────────────────────────────────────────────────────────────
    ui.label(395.0, py + 8.0, "Port:", 1.0, colors::GREY);
    let port_field = Rect::new(435.0, py + 4.0, 70.0, 24.0);
    let port_foc   = focus == FocusField::Port;
    ui.panel(port_field, [0.12, 0.14, 0.20, 1.0]);
    ui.border(port_field, 1.0, if port_foc { [0.5, 0.7, 1.0, 1.0] } else { colors::BORDER });
    if port_field.contains(ui.input.mouse_pos) && ui.input.mouse_just_released(MouseButton::Left) {
        *new_focus = Some(FocusField::Port);
    }
    let port_disp = if port_foc { format!("{port}_") } else { port.to_string() };
    ui.label(port_field.x + 4.0, port_field.y + 6.0, &port_disp, 1.0, colors::WHITE);

    // ── Jméno hráče ─────────────────────────────────────────────────────
    ui.label(12.0, py + 38.0, "Jmeno:", 1.0, colors::GREY);
    let name_field = Rect::new(80.0, py + 34.0, 180.0, 24.0);
    let name_foc   = focus == FocusField::PlayerName;
    ui.panel(name_field, [0.12, 0.14, 0.20, 1.0]);
    ui.border(name_field, 1.0, if name_foc { [0.5, 0.7, 1.0, 1.0] } else { colors::BORDER });
    if name_field.contains(ui.input.mouse_pos) && ui.input.mouse_just_released(MouseButton::Left) {
        *new_focus = Some(FocusField::PlayerName);
    }
    let name_disp = if name_foc { format!("{name}_") } else { name.to_string() };
    ui.label(name_field.x + 4.0, name_field.y + 6.0, &name_disp, 1.0, colors::WHITE);

    // ── Stav připojení ──────────────────────────────────────────────────
    let state_text = match &net_state {
        Some(ConnState::Connected)           => ("Pripojeno",  [0.3, 0.9, 0.3, 1.0]),
        Some(ConnState::Connecting)          => ("Pripojuji.", [0.9, 0.7, 0.1, 1.0]),
        Some(ConnState::Disconnected(_))     => ("Odpojeno",   [0.9, 0.3, 0.3, 1.0]),
        None                                 => ("Nepripojeno", colors::GREY),
    };
    ui.label(280.0, py + 40.0, state_text.0, 1.0, state_text.1);

    if !error.is_empty() {
        ui.label(12.0, py + 64.0, &format!("Chyba: {error}"), 1.0, [0.9, 0.3, 0.3, 1.0]);
    }

    // ── Tlačítko Připojit / Odpojit ─────────────────────────────────────
    let btn = Rect::new(sw - 160.0, py + 8.0, 148.0, 40.0);
    let btn_col = if has_net { colors::BTN_DANGER } else { [0.15, 0.35, 0.55, 1.0] };
    if ui.button(btn, btn_col) {
        return if has_net { ConnectAction::Disconnect } else { ConnectAction::Connect };
    }
    ui.label_shadowed(btn.x + 12.0, btn.y + 12.0,
        if has_net { "Odpojit" } else { "Pripojit" }, 1.0, colors::WHITE);

    ConnectAction::None
}
