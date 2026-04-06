//! Multiplayer herní screen – renderuje stav hry přijatý ze serveru.

use glam::Vec2;

use engine::{
    Rect, UvRect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    ui::{UiCtx, colors},
};
use engine::winit::keyboard::KeyCode;
use engine::winit::event::MouseButton;

use net::{ClientMsg, EntitySnapshot, PlayerAction, ServerMsg};

use crate::net::NetClient;

use super::{Screen, Transition};

// ── Tile barvy ────────────────────────────────────────────────────────────────

const TILE_COLORS: [[f32; 4]; 8] = [
    [0.22, 0.42, 0.15, 1.0],   // 0 Grass
    [0.45, 0.35, 0.20, 1.0],   // 1 Dirt
    [0.20, 0.45, 0.70, 1.0],   // 2 Water
    [0.12, 0.30, 0.60, 1.0],   // 3 DeepWater
    [0.10, 0.28, 0.10, 1.0],   // 4 Forest
    [0.50, 0.48, 0.46, 1.0],   // 5 Rock
    [0.70, 0.65, 0.40, 1.0],   // 6 Sand
    [0.55, 0.42, 0.22, 1.0],   // 7 Bridge
];

const TILE_SIZE: f32 = 32.0;

// ── Barvy týmů ────────────────────────────────────────────────────────────────

const TEAM_COLORS: [[f32; 4]; 8] = [
    [0.20, 0.50, 1.00, 1.0],
    [0.90, 0.20, 0.20, 1.0],
    [0.20, 0.80, 0.20, 1.0],
    [0.90, 0.80, 0.10, 1.0],
    [0.90, 0.40, 0.10, 1.0],
    [0.70, 0.20, 0.90, 1.0],
    [0.20, 0.90, 0.90, 1.0],
    [0.90, 0.90, 0.90, 1.0],
];

fn team_color(team: u8) -> [f32; 4] {
    TEAM_COLORS[team as usize % TEAM_COLORS.len()]
}

// ── Druh entity ───────────────────────────────────────────────────────────────

fn is_building(kind: &str) -> bool {
    matches!(kind, "town_hall" | "great_hall" | "barracks" | "orc_barracks" |
                   "farm" | "stable" | "blacksmith" | "church" |
                   "orc_blacksmith" | "altar")
}

fn is_worker(kind: &str) -> bool {
    matches!(kind, "peasant" | "peon")
}

// ── Výběr entit ───────────────────────────────────────────────────────────────

struct Selection {
    ids: Vec<u64>,
}

// ── MultiplayerScreen ─────────────────────────────────────────────────────────

pub struct MultiplayerScreen {
    net:      NetClient,
    map_id:   String,
    my_team:  u8,

    /// Tiles z GameStart (index = y*w+x, hodnota = TileKind byte)
    map_tiles: Vec<u8>,
    map_w:     u32,
    map_h:     u32,

    entities:  Vec<EntitySnapshot>,
    tick:      u64,

    /// Výběr vlastních jednotek
    selection: Vec<u64>,
    drag_start: Option<Vec2>,   // world pos začátku drag-boxu
    drag_end:   Option<Vec2>,

    game_over: Option<String>,

    white_bg:  Option<engine::wgpu::BindGroup>,
}

impl MultiplayerScreen {
    pub fn new(
        net:       NetClient,
        map_id:    String,
        my_team:   u8,
        _tick_rate: u8,
        map_tiles: Vec<u8>,
        map_w:     u32,
        map_h:     u32,
        base_x:    f32,
        base_y:    f32,
        camera:    &mut Camera,
    ) -> Self {
        // Nastav kameru na základnu
        camera.position = Vec2::new(base_x, base_y);
        camera.zoom     = 1.0;

        Self {
            net,
            map_id,
            my_team,
            map_tiles,
            map_w,
            map_h,
            entities:   Vec::new(),
            tick:       0,
            selection:  Vec::new(),
            drag_start: None,
            drag_end:   None,
            game_over:  None,
            white_bg:   None,
        }
    }

    fn handle_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::GameState { tick, entities } => {
                self.tick     = tick;
                self.entities = entities;
            }
            ServerMsg::GameOver { winner_team, reason } => {
                let text = match winner_team {
                    Some(t) if t == self.my_team => format!("Vyhráli jste! ({})", reason),
                    Some(t)                       => format!("Prohrali jste - vítěz tým {} ({})", t, reason),
                    None                          => format!("Remíza - {}", reason),
                };
                self.game_over = Some(text);
            }
            _ => {}
        }
    }

    fn entity_at_world(&self, pos: Vec2, radius: f32) -> Option<u64> {
        self.entities.iter().find_map(|e| {
            let dx = e.x - pos.x;
            let dy = e.y - pos.y;
            if dx*dx + dy*dy <= radius*radius { Some(e.id) } else { None }
        })
    }

    fn entities_in_box(&self, a: Vec2, b: Vec2) -> Vec<u64> {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        self.entities.iter()
            .filter(|e| e.team == self.my_team && !is_building(&e.kind)
                && e.x >= min_x && e.x <= max_x && e.y >= min_y && e.y <= max_y)
            .map(|e| e.id)
            .collect()
    }
}

const CAM_SPEED: f32 = 400.0;

impl Screen for MultiplayerScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        self.white_bg = Some(tex.create_bind_group(ctx, &batch.texture_bind_group_layout));
    }

    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera) -> Transition {
        // Síťové zprávy
        for msg in self.net.drain() {
            self.handle_msg(msg);
        }

        // ESC → zpět do menu
        if input.key_just_pressed(KeyCode::Escape) {
            self.net.send(ClientMsg::LeaveLobby);
            use super::main_menu::MainMenuScreen;
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        // ── Kamera – WASD/šipky ──────────────────────────────────────────────
        let mut pan = Vec2::ZERO;
        if input.key_held(KeyCode::KeyW) || input.key_held(KeyCode::ArrowUp)    { pan.y -= 1.0; }
        if input.key_held(KeyCode::KeyS) || input.key_held(KeyCode::ArrowDown)  { pan.y += 1.0; }
        if input.key_held(KeyCode::KeyA) || input.key_held(KeyCode::ArrowLeft)  { pan.x -= 1.0; }
        if input.key_held(KeyCode::KeyD) || input.key_held(KeyCode::ArrowRight) { pan.x += 1.0; }
        if pan != Vec2::ZERO {
            camera.pan(pan.normalize() * CAM_SPEED * dt / camera.zoom);
        }

        // Zoom kolečkem
        if input.scroll_delta != 0.0 {
            let factor = if input.scroll_delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
            camera.zoom_around(factor, input.mouse_pos);
        }

        // ── Výběr jednotek ───────────────────────────────────────────────────
        if input.mouse_just_pressed(MouseButton::Left) {
            self.drag_start = Some(camera.screen_to_world(input.mouse_pos));
        }
        if input.mouse_held(MouseButton::Left) {
            if let Some(_start) = self.drag_start {
                self.drag_end = Some(camera.screen_to_world(input.mouse_pos));
            }
        }
        if input.mouse_just_released(MouseButton::Left) {
            if let (Some(start), Some(end)) = (self.drag_start.take(), self.drag_end.take()) {
                let dist = (end - start).length();
                if dist < 8.0 {
                    // Klik – vyber entitu pod kurzorem
                    let world_pos = camera.screen_to_world(input.mouse_pos);
                    if let Some(id) = self.entity_at_world(world_pos, 20.0) {
                        self.selection = vec![id];
                    } else {
                        self.selection.clear();
                    }
                } else {
                    // Drag box
                    self.selection = self.entities_in_box(start, end);
                }
            } else {
                self.drag_start = None;
                self.drag_end   = None;
            }
        }

        // ── Pravé tlačítko – pohyb / útok ────────────────────────────────────
        if input.mouse_just_released(MouseButton::Right) && !self.selection.is_empty() {
            let world_pos = camera.screen_to_world(input.mouse_pos);
            // Útok pokud kliknutí na nepřátelskou entitu
            if let Some(target_id) = self.entity_at_world(world_pos, 20.0) {
                let is_enemy = self.entities.iter()
                    .find(|e| e.id == target_id)
                    .map(|e| e.team != self.my_team)
                    .unwrap_or(false);
                if is_enemy {
                    self.net.send(ClientMsg::PlayerInput {
                        tick: self.tick,
                        actions: vec![PlayerAction::AttackUnit {
                            attacker_ids: self.selection.clone(),
                            target_id,
                        }],
                    });
                } else {
                    // Pohyb na kliknuté místo
                    self.net.send(ClientMsg::PlayerInput {
                        tick: self.tick,
                        actions: vec![PlayerAction::MoveUnits {
                            unit_ids: self.selection.clone(),
                            target_x: world_pos.x,
                            target_y: world_pos.y,
                        }],
                    });
                }
            } else {
                self.net.send(ClientMsg::PlayerInput {
                    tick: self.tick,
                    actions: vec![PlayerAction::MoveUnits {
                        unit_ids: self.selection.clone(),
                        target_x: world_pos.x,
                        target_y: world_pos.y,
                    }],
                });
            }
        }

        Transition::None
    }

    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera) {
        let vp = camera.viewport();

        // ── Tilemap ───────────────────────────────────────────────────────────
        if self.map_w > 0 && self.map_h > 0 {
            // Viditelný rozsah dlaždic
            let world_tl = camera.screen_to_world(Vec2::ZERO);
            let world_br = camera.screen_to_world(vp);
            let tx0 = ((world_tl.x / TILE_SIZE).floor() as i32).max(0) as u32;
            let ty0 = ((world_tl.y / TILE_SIZE).floor() as i32).max(0) as u32;
            let tx1 = ((world_br.x / TILE_SIZE).ceil() as i32 + 1).min(self.map_w as i32) as u32;
            let ty1 = ((world_br.y / TILE_SIZE).ceil() as i32 + 1).min(self.map_h as i32) as u32;

            for ty in ty0..ty1 {
                for tx in tx0..tx1 {
                    let idx = (ty * self.map_w + tx) as usize;
                    let kind = self.map_tiles.get(idx).copied().unwrap_or(0) as usize;
                    let color = TILE_COLORS[kind.min(TILE_COLORS.len() - 1)];
                    let rect = Rect::new(
                        tx as f32 * TILE_SIZE,
                        ty as f32 * TILE_SIZE,
                        TILE_SIZE, TILE_SIZE,
                    );
                    batch.draw(rect, UvRect::FULL, color);
                }
            }
        }

        // ── Entity ────────────────────────────────────────────────────────────
        let selected_set: std::collections::HashSet<u64> = self.selection.iter().copied().collect();

        for e in &self.entities {
            let col = team_color(e.team);
            let size = if is_building(&e.kind) { 64.0 } else { 28.0 };

            let rect = Rect::new(e.x - size * 0.5, e.y - size * 0.5, size, size);

            // Vykreslení entity
            batch.draw(rect, UvRect::FULL, col);

            // Tmavší okraj
            let border = darken(col, 0.5);
            let t = 2.0;
            batch.draw(Rect::new(rect.x, rect.y, rect.w, t), UvRect::FULL, border);
            batch.draw(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t), UvRect::FULL, border);
            batch.draw(Rect::new(rect.x, rect.y, t, rect.h), UvRect::FULL, border);
            batch.draw(Rect::new(rect.x + rect.w - t, rect.y, t, rect.h), UvRect::FULL, border);

            // Ikona dělníka / budovy
            if is_building(&e.kind) {
                // Tmavý střed pro budovy
                let inner = shrink(rect, 8.0);
                batch.draw(inner, UvRect::FULL, darken(col, 0.6));
            } else if is_worker(&e.kind) {
                // Malý čtvereček uvnitř pro pracovníka
                let dot = Rect::new(rect.x + rect.w * 0.3, rect.y + rect.h * 0.3,
                                    rect.w * 0.4, rect.h * 0.4);
                batch.draw(dot, UvRect::FULL, [1.0, 1.0, 0.8, 0.9]);
            }

            // Výběrový kroužek (zelený rámeček)
            if selected_set.contains(&e.id) {
                let sel = expand(rect, 4.0);
                let sc  = [0.0, 1.0, 0.0, 1.0];
                batch.draw(Rect::new(sel.x, sel.y, sel.w, 2.0), UvRect::FULL, sc);
                batch.draw(Rect::new(sel.x, sel.y + sel.h - 2.0, sel.w, 2.0), UvRect::FULL, sc);
                batch.draw(Rect::new(sel.x, sel.y, 2.0, sel.h), UvRect::FULL, sc);
                batch.draw(Rect::new(sel.x + sel.w - 2.0, sel.y, 2.0, sel.h), UvRect::FULL, sc);
            }

            // Health bar
            if e.hp_max > 0 {
                let frac = (e.hp as f32 / e.hp_max as f32).clamp(0.0, 1.0);
                let bar  = Rect::new(rect.x, rect.y - 6.0, rect.w, 4.0);
                batch.draw(bar, UvRect::FULL, [0.25, 0.05, 0.05, 1.0]);
                if frac > 0.0 {
                    let fg = if frac > 0.5 { [0.1, 0.8, 0.1, 1.0] }
                             else if frac > 0.25 { [0.85, 0.65, 0.1, 1.0] }
                             else { [0.85, 0.1, 0.1, 1.0] };
                    batch.draw(Rect::new(bar.x, bar.y, bar.w * frac, bar.h), UvRect::FULL, fg);
                }
            }
        }

        // ── Drag-box ─────────────────────────────────────────────────────────
        if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
            let min_x = start.x.min(end.x);
            let min_y = start.y.min(end.y);
            let max_x = start.x.max(end.x);
            let max_y = start.y.max(end.y);
            let box_r = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            let bc = [0.0, 1.0, 0.0, 0.25];
            batch.draw(box_r, UvRect::FULL, bc);
            let bl = [0.0, 1.0, 0.0, 0.9];
            let t  = 1.0;
            batch.draw(Rect::new(box_r.x, box_r.y, box_r.w, t), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x, box_r.y + box_r.h, box_r.w, t), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x, box_r.y, t, box_r.h), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x + box_r.w, box_r.y, t, box_r.h), UvRect::FULL, bl);
        }
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        // ── Horní lišta ───────────────────────────────────────────────────────
        ui.panel(Rect::new(0.0, 0.0, sw, 28.0), [0.06, 0.07, 0.10, 0.92]);
        ui.border(Rect::new(0.0, 0.0, sw, 28.0), 1.0, colors::BORDER);

        let col = team_color(self.my_team);
        ui.panel(Rect::new(8.0, 6.0, 16.0, 16.0), col);
        ui.label_shadowed(30.0, 8.0,
            &format!("Tym {}  Tick {}  {}", self.my_team, self.tick, self.map_id),
            1.0, colors::WHITE);

        let conn = match self.net.conn_state() {
            crate::net::ConnState::Connected         => ("Online",    [0.3, 0.9, 0.3, 1.0]),
            crate::net::ConnState::Connecting        => ("Pripojuji", [0.9, 0.8, 0.1, 1.0]),
            crate::net::ConnState::Disconnected(_)   => ("Offline",   [0.9, 0.3, 0.3, 1.0]),
        };
        ui.label(sw * 0.5 - 24.0, 8.0, conn.0, 1.0, conn.1);

        ui.label(sw - 220.0, 8.0,
            &format!("Entity: {}  Vybrano: {}", self.entities.len(), self.selection.len()),
            1.0, colors::GREY);
        ui.label(sw - 100.0, 8.0, "ESC=menu", 1.0, colors::GREY);

        // ── Dolní panel – info o výběru ───────────────────────────────────────
        if !self.selection.is_empty() {
            let ph = 96.0;
            let py = sh - ph;
            ui.panel(Rect::new(0.0, py, sw, ph), [0.06, 0.07, 0.10, 0.92]);
            ui.border(Rect::new(0.0, py, sw, ph), 1.0, colors::BORDER);

            // Zobraz prvních 8 vybraných entit
            for (slot, id) in self.selection.iter().take(8).enumerate() {
                if let Some(e) = self.entities.iter().find(|e| e.id == *id) {
                    let sx = 8.0 + slot as f32 * 80.0;
                    let sy = py + 8.0;
                    let portrait = Rect::new(sx, sy, 72.0, 72.0);
                    ui.panel(portrait, darken(team_color(e.team), 0.6));
                    ui.border(portrait, 1.0, team_color(e.team));
                    ui.label(sx + 4.0, sy + 4.0, &e.kind, 1.0, colors::WHITE);
                    // HP bar
                    let frac = (e.hp as f32 / e.hp_max as f32).clamp(0.0, 1.0);
                    let hb = Rect::new(sx, sy + 58.0, 72.0, 8.0);
                    ui.panel(hb, [0.2, 0.05, 0.05, 1.0]);
                    let fg = if frac > 0.5 { colors::HEALTH_HI }
                             else if frac > 0.25 { colors::HEALTH_MID }
                             else { colors::HEALTH_LO };
                    ui.panel(Rect::new(hb.x, hb.y, hb.w * frac, hb.h), fg);
                    ui.label(sx + 4.0, sy + 48.0,
                        &format!("{}/{}", e.hp, e.hp_max), 1.0, colors::GREY);
                }
            }

            // Produktivní fronta první vybrané budovy
            if let Some(first_id) = self.selection.first() {
                if let Some(e) = self.entities.iter().find(|e| e.id == *first_id) {
                    if is_building(&e.kind) {
                        ui.label(sw - 280.0, py + 8.0, "Budova - LK=vybrat, PK=rally", 1.0, colors::GREY);
                    }
                }
            }
        }

        // ── Tipy pro ovládání (zobrazí se jen na začátku) ────────────────────
        if self.tick < 200 {
            let alpha = if self.tick < 160 { 1.0 } else { (200 - self.tick) as f32 / 40.0 };
            let col = [0.9, 0.9, 0.7, alpha];
            ui.label(sw * 0.5 - 200.0, sh * 0.5 - 50.0, "WASD / Sipky = pohyb kamery", 1.0, col);
            ui.label(sw * 0.5 - 200.0, sh * 0.5 - 36.0, "Kolecko = zoom", 1.0, col);
            ui.label(sw * 0.5 - 200.0, sh * 0.5 - 22.0, "Leve tlacitko = vybrat", 1.0, col);
            ui.label(sw * 0.5 - 200.0, sh * 0.5 - 8.0,  "Prave tlacitko = pohyb / utok", 1.0, col);
            ui.label(sw * 0.5 - 200.0, sh * 0.5 + 6.0,  "ESC = zpet do menu", 1.0, col);
        }

        // ── Game over banner ──────────────────────────────────────────────────
        if let Some(ref text) = self.game_over {
            let bw = 520.0f32.min(sw - 40.0);
            let bh = 90.0;
            let bx = (sw - bw) * 0.5;
            let by = sh * 0.38;
            ui.panel(Rect::new(bx, by, bw, bh), [0.05, 0.05, 0.08, 0.96]);
            ui.border(Rect::new(bx, by, bw, bh), 2.0, [0.7, 0.7, 0.2, 1.0]);
            ui.label_centered(Rect::new(bx, by, bw, bh * 0.45), "KONEC HRY", 2.0, [1.0, 0.9, 0.3, 1.0]);
            ui.label_centered(Rect::new(bx, by + bh * 0.48, bw, bh * 0.35), text, 1.0, colors::WHITE);
            ui.label_centered(Rect::new(bx, by + bh + 4.0, bw, 18.0), "ESC = zpet do menu", 1.0, colors::GREY);
        }
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.white_bg.as_ref().expect("MultiplayerScreen::init not called")
    }
}

// ── Rect helpers ──────────────────────────────────────────────────────────────

fn darken(c: [f32; 4], f: f32) -> [f32; 4] {
    [c[0]*f, c[1]*f, c[2]*f, c[3]]
}

fn shrink(r: Rect, by: f32) -> Rect {
    Rect::new(r.x + by, r.y + by, (r.w - by*2.0).max(0.0), (r.h - by*2.0).max(0.0))
}

fn expand(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + by*2.0, r.h + by*2.0)
}
