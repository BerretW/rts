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
    matches!(kind,
        "town_hall" | "great_hall" | "barracks" | "orc_barracks" |
        "farm"      | "pig_farm"   | "stable"   | "ogre_mound"   |
        "mage_tower"| "altar"      | "gryphon_aviary" | "dragon_roost" |
        "blacksmith"| "church"     | "orc_blacksmith" | "watch_tower"  |
        "lumbermill"| "orc_lumbermill"
    )
}

fn is_worker(kind: &str) -> bool {
    matches!(kind, "peasant" | "peon")
}

/// Co daná budova může vyrábět (pro UI tlačítka trénování)
fn produces_for_kind(kind: &str) -> &'static [&'static str] {
    match kind {
        "town_hall"      | "great_hall"    => &["peasant"],
        "barracks"                         => &["footman", "archer"],
        "orc_barracks"                     => &["grunt", "troll_axethrower"],
        "stable"                           => &["knight"],
        "ogre_mound"                       => &["ogre"],
        "mage_tower"                       => &["mage"],
        "altar"                            => &["death_knight"],
        "gryphon_aviary"                   => &["gryphon_rider"],
        "dragon_roost"                     => &["dragon"],
        _ => &[],
    }
}

/// Zkrácený název jednotky pro UI tlačítka
fn unit_short_name(kind: &str) -> &'static str {
    match kind {
        "peasant"          => "Peasant",
        "peon"             => "Peon",
        "footman"          => "Footman",
        "grunt"            => "Grunt",
        "archer"           => "Archer",
        "troll_axethrower" => "T.Axe",
        "knight"           => "Knight",
        "ogre"             => "Ogre",
        "mage"             => "Mage",
        "death_knight"     => "D.Knight",
        "gryphon_rider"    => "Gryphon",
        "dragon"           => "Dragon",
        _                  => "?",
    }
}

/// Schopnosti dané jednotky (ability_id seznam)
fn abilities_for_kind(kind: &str) -> &'static [&'static str] {
    match kind {
        "mage"         => &["patrol", "holy_light"],
        "death_knight" => &["patrol", "death_coil"],
        "dragon"       => &["patrol", "blizzard"],
        "footman" | "archer" | "knight" | "grunt" |
        "troll_axethrower" | "ogre" | "gryphon_rider" |
        "elven_destroyer"  => &["patrol"],
        _ => &[],
    }
}

/// Zobrazovaný název schopnosti pro UI tlačítko
fn ability_label(id: &str) -> &'static str {
    match id {
        "patrol"     => "Patrol[P]",
        "holy_light" => "HolyL[H]",
        "death_coil" => "DeathC[D]",
        "blizzard"   => "Blizzrd[B]",
        _            => "?",
    }
}

/// Typ cíle schopnosti
#[derive(Clone, PartialEq, Eq)]
enum AbilityTarget { None, Point, Unit }

fn ability_target_type(id: &str) -> AbilityTarget {
    match id {
        "patrol" | "blizzard"             => AbilityTarget::Point,
        "holy_light" | "death_coil"       => AbilityTarget::Unit,
        _                                 => AbilityTarget::None,
    }
}

// ── MultiplayerScreen ─────────────────────────────────────────────────────────

pub struct MultiplayerScreen {
    net:      NetClient,
    map_id:   String,
    my_team:  u8,

    map_tiles: Vec<u8>,
    map_w:     u32,
    map_h:     u32,

    entities:  Vec<EntitySnapshot>,
    tick:      u64,

    selection: Vec<u64>,
    drag_start: Option<Vec2>,
    drag_end:   Option<Vec2>,

    game_over: Option<String>,

    /// Schopnost čekající na kliknutí cíle (None = žádná)
    pending_ability: Option<String>,

    /// Akce sesbírané z render_ui (tlačítka), odesílají se v update() příštího snímku.
    /// Protože tlačítka jsou v render_ui, ukládáme je sem a odesíláme na začátku update.
    queued_actions: Vec<PlayerAction>,

    white_bg:  Option<engine::wgpu::BindGroup>,
}

const PANEL_H: f32 = 128.0;

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
        camera.position = Vec2::new(base_x, base_y);
        camera.zoom     = 1.0;

        Self {
            net, map_id, my_team,
            map_tiles, map_w, map_h,
            entities:         Vec::new(),
            tick:             0,
            selection:        Vec::new(),
            drag_start:       None,
            drag_end:         None,
            game_over:        None,
            pending_ability:  None,
            queued_actions:   Vec::new(),
            white_bg:         None,
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

    /// Vrátí true pokud jakákoli vybraná vlastní jednotka má danou schopnost.
    fn selection_has_ability(&self, ability_id: &str) -> bool {
        self.selection.iter().any(|id| {
            self.entities.iter()
                .find(|e| e.id == *id && e.team == self.my_team)
                .map(|e| abilities_for_kind(&e.kind).contains(&ability_id))
                .unwrap_or(false)
        })
    }

    /// Je vybraná entita budova vlastního týmu?
    fn selected_building(&self) -> Option<&EntitySnapshot> {
        self.selection.first().and_then(|id| {
            self.entities.iter().find(|e| e.id == *id && e.team == self.my_team && is_building(&e.kind))
        })
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

        // Odešli akce sesbírané v minulém render_ui (tlačítka trénování / schopností)
        if !self.queued_actions.is_empty() {
            let actions = std::mem::take(&mut self.queued_actions);
            self.net.send(ClientMsg::PlayerInput { tick: self.tick, actions });
        }

        // ESC → zrušení čekající schopnosti, nebo zpět do menu
        if input.key_just_pressed(KeyCode::Escape) {
            if self.pending_ability.is_some() {
                self.pending_ability = None;
                return Transition::None;
            }
            self.net.send(ClientMsg::LeaveLobby);
            use super::main_menu::MainMenuScreen;
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        // ── Kamera ──────────────────────────────────────────────────────────
        let mut pan = Vec2::ZERO;
        if input.key_held(KeyCode::KeyW) || input.key_held(KeyCode::ArrowUp)    { pan.y -= 1.0; }
        if input.key_held(KeyCode::KeyS) || input.key_held(KeyCode::ArrowDown)  { pan.y += 1.0; }
        if input.key_held(KeyCode::KeyA) || input.key_held(KeyCode::ArrowLeft)  { pan.x -= 1.0; }
        if input.key_held(KeyCode::KeyD) || input.key_held(KeyCode::ArrowRight) { pan.x += 1.0; }
        if pan != Vec2::ZERO {
            camera.pan(pan.normalize() * CAM_SPEED * dt / camera.zoom);
        }
        if input.scroll_delta != 0.0 {
            let factor = if input.scroll_delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
            camera.zoom_around(factor, input.mouse_pos);
        }

        // ── Zkratky schopností ───────────────────────────────────────────────
        if !self.selection.is_empty() && self.pending_ability.is_none() {
            if input.key_just_pressed(KeyCode::KeyP) && self.selection_has_ability("patrol") {
                self.pending_ability = Some("patrol".into());
            }
            if input.key_just_pressed(KeyCode::KeyH) && self.selection_has_ability("holy_light") {
                self.pending_ability = Some("holy_light".into());
            }
            if input.key_just_pressed(KeyCode::KeyD) && self.selection_has_ability("death_coil") {
                self.pending_ability = Some("death_coil".into());
            }
            if input.key_just_pressed(KeyCode::KeyB) && self.selection_has_ability("blizzard") {
                self.pending_ability = Some("blizzard".into());
            }
        }

        // ── Výběr jednotek (LMB) ─────────────────────────────────────────────
        // Klik nevybírá pokud čekáme na cíl schopnosti
        if self.pending_ability.is_none() {
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
                        let world_pos = camera.screen_to_world(input.mouse_pos);
                        if let Some(id) = self.entity_at_world(world_pos, 20.0) {
                            self.selection = vec![id];
                        } else {
                            self.selection.clear();
                        }
                    } else {
                        self.selection = self.entities_in_box(start, end);
                    }
                } else {
                    self.drag_start = None;
                    self.drag_end   = None;
                }
            }
        }

        // ── Pravé tlačítko ───────────────────────────────────────────────────
        if input.mouse_just_released(MouseButton::Right) && !self.selection.is_empty() {
            let world_pos = camera.screen_to_world(input.mouse_pos);

            if let Some(ability_id) = self.pending_ability.take() {
                // Zpracuj schopnost podle typu cíle
                match ability_target_type(&ability_id) {
                    AbilityTarget::None => {}
                    AbilityTarget::Point => {
                        if ability_id == "patrol" {
                            let my_units: Vec<u64> = self.selection.iter().copied()
                                .filter(|id| {
                                    self.entities.iter()
                                        .find(|e| e.id == *id)
                                        .map(|e| e.team == self.my_team && !is_building(&e.kind))
                                        .unwrap_or(false)
                                }).collect();
                            if !my_units.is_empty() {
                                self.net.send(ClientMsg::PlayerInput {
                                    tick: self.tick,
                                    actions: vec![PlayerAction::PatrolUnit {
                                        unit_ids: my_units,
                                        target_x: world_pos.x,
                                        target_y: world_pos.y,
                                    }],
                                });
                            }
                        } else {
                            // Point-target ability pro první vybranou vlastní jednotku
                            if let Some(&uid) = self.selection.iter().find(|id| {
                                self.entities.iter()
                                    .find(|e| e.id == **id)
                                    .map(|e| e.team == self.my_team)
                                    .unwrap_or(false)
                            }) {
                                self.net.send(ClientMsg::PlayerInput {
                                    tick: self.tick,
                                    actions: vec![PlayerAction::UseAbility {
                                        unit_id: uid, ability_id,
                                        target_id: None,
                                        target_x: world_pos.x, target_y: world_pos.y,
                                    }],
                                });
                            }
                        }
                    }
                    AbilityTarget::Unit => {
                        // Kliknutí na cílovou entitu
                        let target_id = self.entity_at_world(world_pos, 20.0);
                        if let Some(&uid) = self.selection.iter().find(|id| {
                            self.entities.iter()
                                .find(|e| e.id == **id)
                                .map(|e| e.team == self.my_team)
                                .unwrap_or(false)
                        }) {
                            self.net.send(ClientMsg::PlayerInput {
                                tick: self.tick,
                                actions: vec![PlayerAction::UseAbility {
                                    unit_id: uid, ability_id,
                                    target_id,
                                    target_x: world_pos.x, target_y: world_pos.y,
                                }],
                            });
                        }
                    }
                }
            } else {
                // Normální pohyb / útok
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
        }

        // Stop – mezerník
        if input.key_just_pressed(KeyCode::Space) && !self.selection.is_empty() {
            self.net.send(ClientMsg::PlayerInput {
                tick: self.tick,
                actions: vec![PlayerAction::StopUnits { unit_ids: self.selection.clone() }],
            });
        }

        Transition::None
    }

    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera) {
        let vp = camera.viewport();

        // ── Tilemap ───────────────────────────────────────────────────────────
        if self.map_w > 0 && self.map_h > 0 {
            let world_tl = camera.screen_to_world(Vec2::ZERO);
            let world_br = camera.screen_to_world(vp);
            let tx0 = ((world_tl.x / TILE_SIZE).floor() as i32).clamp(0, self.map_w as i32 - 1) as u32;
            let ty0 = ((world_tl.y / TILE_SIZE).floor() as i32).clamp(0, self.map_h as i32 - 1) as u32;
            let tx1 = ((world_br.x / TILE_SIZE).ceil() as i32 + 1).clamp(0, self.map_w as i32) as u32;
            let ty1 = ((world_br.y / TILE_SIZE).ceil() as i32 + 1).clamp(0, self.map_h as i32) as u32;

            for ty in ty0..ty1 {
                for tx in tx0..tx1 {
                    let idx  = (ty * self.map_w + tx) as usize;
                    let kind = self.map_tiles.get(idx).copied().unwrap_or(0) as usize;
                    let color = TILE_COLORS[kind.min(TILE_COLORS.len() - 1)];
                    batch.draw(
                        Rect::new(tx as f32 * TILE_SIZE, ty as f32 * TILE_SIZE, TILE_SIZE, TILE_SIZE),
                        UvRect::FULL, color,
                    );
                }
            }
        }

        // ── Entity ────────────────────────────────────────────────────────────
        let selected_set: std::collections::HashSet<u64> = self.selection.iter().copied().collect();

        for e in &self.entities {
            let col  = team_color(e.team);
            let size = if is_building(&e.kind) { 64.0 } else { 28.0 };
            let rect = Rect::new(e.x - size * 0.5, e.y - size * 0.5, size, size);

            batch.draw(rect, UvRect::FULL, col);

            // Tmavší okraj
            let border = darken(col, 0.5);
            let t = 2.0;
            batch.draw(Rect::new(rect.x, rect.y,              rect.w, t),  UvRect::FULL, border);
            batch.draw(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t),  UvRect::FULL, border);
            batch.draw(Rect::new(rect.x, rect.y,              t, rect.h),  UvRect::FULL, border);
            batch.draw(Rect::new(rect.x + rect.w - t, rect.y, t, rect.h), UvRect::FULL, border);

            if is_building(&e.kind) {
                let inner = shrink(rect, 8.0);
                batch.draw(inner, UvRect::FULL, darken(col, 0.6));
            } else if is_worker(&e.kind) {
                let dot = Rect::new(rect.x + rect.w * 0.3, rect.y + rect.h * 0.3,
                                    rect.w * 0.4, rect.h * 0.4);
                batch.draw(dot, UvRect::FULL, [1.0, 1.0, 0.8, 0.9]);
            }

            // Výběrový rámeček
            if selected_set.contains(&e.id) {
                let sel = expand(rect, 4.0);
                let sc  = [0.0, 1.0, 0.0, 1.0];
                batch.draw(Rect::new(sel.x, sel.y,              sel.w, 2.0), UvRect::FULL, sc);
                batch.draw(Rect::new(sel.x, sel.y + sel.h - 2.0, sel.w, 2.0), UvRect::FULL, sc);
                batch.draw(Rect::new(sel.x, sel.y,              2.0, sel.h), UvRect::FULL, sc);
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

            // Indikátor výroby pod budovou
            if let Some(ref _pk) = e.prod_kind {
                let bar  = Rect::new(rect.x, rect.y + rect.h + 2.0, rect.w, 4.0);
                batch.draw(bar, UvRect::FULL, [0.1, 0.1, 0.3, 1.0]);
                batch.draw(Rect::new(bar.x, bar.y, bar.w * e.prod_progress, bar.h),
                           UvRect::FULL, [0.3, 0.6, 1.0, 1.0]);
            }
        }

        // ── Drag-box ─────────────────────────────────────────────────────────
        if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
            let min_x = start.x.min(end.x);
            let min_y = start.y.min(end.y);
            let max_x = start.x.max(end.x);
            let max_y = start.y.max(end.y);
            let box_r = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            batch.draw(box_r, UvRect::FULL, [0.0, 1.0, 0.0, 0.25]);
            let bl = [0.0, 1.0, 0.0, 0.9];
            let t  = 1.0;
            batch.draw(Rect::new(box_r.x, box_r.y,              box_r.w, t), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x, box_r.y + box_r.h,    box_r.w, t), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x, box_r.y,              t, box_r.h), UvRect::FULL, bl);
            batch.draw(Rect::new(box_r.x + box_r.w, box_r.y,    t, box_r.h), UvRect::FULL, bl);
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
            crate::net::ConnState::Connected       => ("Online",    [0.3, 0.9, 0.3, 1.0]),
            crate::net::ConnState::Connecting      => ("Pripojuji", [0.9, 0.8, 0.1, 1.0]),
            crate::net::ConnState::Disconnected(_) => ("Offline",   [0.9, 0.3, 0.3, 1.0]),
        };
        ui.label(sw * 0.5 - 24.0, 8.0, conn.0, 1.0, conn.1);

        // Indikátor čekající schopnosti
        if let Some(ref ab) = self.pending_ability {
            let msg = format!(">> {} – kliknete na cil (ESC=zrusit)", ability_label(ab));
            ui.label_shadowed(sw * 0.5 - 150.0, 8.0, &msg, 1.0, [1.0, 0.9, 0.2, 1.0]);
        }

        ui.label(sw - 220.0, 8.0,
            &format!("Entity: {}  Vybrano: {}", self.entities.len(), self.selection.len()),
            1.0, colors::GREY);
        ui.label(sw - 100.0, 8.0, "ESC=menu", 1.0, colors::GREY);

        // ── Dolní panel ───────────────────────────────────────────────────────
        if !self.selection.is_empty() {
            let py = sh - PANEL_H;
            ui.panel(Rect::new(0.0, py, sw, PANEL_H), [0.06, 0.07, 0.10, 0.92]);
            ui.border(Rect::new(0.0, py, sw, PANEL_H), 1.0, colors::BORDER);

            // ── Portréty (max 8) ─────────────────────────────────────────────
            let portrait_w = 76.0;
            let portrait_h = 76.0;
            let portrait_x0 = 8.0;
            let portrait_y0 = py + 8.0;

            for (slot, id) in self.selection.iter().take(8).enumerate() {
                if let Some(e) = self.entities.iter().find(|e| e.id == *id) {
                    let sx = portrait_x0 + slot as f32 * (portrait_w + 4.0);
                    let sy = portrait_y0;
                    let portrait = Rect::new(sx, sy, portrait_w, portrait_h);
                    ui.panel(portrait, darken(team_color(e.team), 0.6));
                    ui.border(portrait, 1.0, team_color(e.team));
                    ui.label(sx + 4.0, sy + 4.0, unit_short_name(&e.kind), 1.0, colors::WHITE);
                    let frac = (e.hp as f32 / e.hp_max as f32).clamp(0.0, 1.0);
                    ui.panel(Rect::new(sx, sy + 62.0, portrait_w, 8.0), [0.2, 0.05, 0.05, 1.0]);
                    let fg = if frac > 0.5 { colors::HEALTH_HI }
                             else if frac > 0.25 { colors::HEALTH_MID }
                             else { colors::HEALTH_LO };
                    ui.panel(Rect::new(sx, sy + 62.0, portrait_w * frac, 8.0), fg);
                    ui.label(sx + 4.0, sy + 50.0,
                        &format!("{}/{}", e.hp, e.hp_max), 1.0, colors::GREY);
                }
            }

            // ── Oddělovač ────────────────────────────────────────────────────
            let div_x = portrait_x0 + 8.0 * (portrait_w + 4.0) + 4.0;
            ui.panel(Rect::new(div_x, py + 4.0, 1.0, PANEL_H - 8.0), colors::BORDER);

            // ── Příkazová karta (vpravo od portétů) ──────────────────────────
            let cmd_x  = div_x + 8.0;
            let cmd_y  = py + 8.0;
            let btn_w  = 88.0;
            let btn_h  = 28.0;
            let gap    = 4.0;
            let cols   = 3usize;

            let first_id = *self.selection.first().unwrap();
            let first_e  = self.entities.iter().find(|e| e.id == first_id).cloned();

            if let Some(ref e) = first_e {
                let is_mine = e.team == self.my_team;

                if is_building(&e.kind) && is_mine {
                    // ── BUDOVA: produkce ──────────────────────────────────────

                    // Aktuální výroba
                    if let Some(ref pk) = e.prod_kind.clone() {
                        ui.label(cmd_x, cmd_y, "Vyrabi:", 1.0, colors::GREY);
                        ui.label(cmd_x + 56.0, cmd_y, unit_short_name(pk), 1.0, colors::WHITE);

                        let pb_rect = Rect::new(cmd_x, cmd_y + 12.0, btn_w * cols as f32 + gap * (cols - 1) as f32, 10.0);
                        ui.progress_bar(pb_rect, e.prod_progress,
                            [0.1, 0.1, 0.2, 1.0], [0.3, 0.6, 1.0, 1.0]);

                        if e.prod_queue_len > 0 {
                            ui.label(cmd_x, cmd_y + 24.0,
                                &format!("Fronta: {}", e.prod_queue_len), 1.0, colors::GREY);
                        }

                        // Zrušit výrobu
                        let cancel_rect = Rect::new(cmd_x, cmd_y + 38.0, btn_w, btn_h);
                        if ui.button_text(cancel_rect, "Zrusit", 1.0) {
                            self.queued_actions.push(PlayerAction::CancelProduction {
                                building_id: e.id,
                            });
                        }
                    } else {
                        ui.label(cmd_x, cmd_y, "Neni aktivni vyroba", 1.0, colors::GREY);
                    }

                    // Tlačítka trénování
                    let produces = produces_for_kind(&e.kind);
                    let train_row_y = cmd_y + 72.0;
                    for (i, &kind_id) in produces.iter().enumerate() {
                        let col = i % cols;
                        let row = i / cols;
                        let bx  = cmd_x + col as f32 * (btn_w + gap);
                        let by  = train_row_y + row as f32 * (btn_h + gap);
                        if ui.button_text(Rect::new(bx, by, btn_w, btn_h), unit_short_name(kind_id), 1.0) {
                            self.queued_actions.push(PlayerAction::TrainUnit {
                                building_id: e.id,
                                kind_id: kind_id.to_string(),
                            });
                        }
                    }

                } else if !is_building(&e.kind) {
                    // ── JEDNOTKY: schopnosti ──────────────────────────────────

                    // Sbírej unikátní schopnosti ze všech vybraných vlastních jednotek
                    let mut shown: Vec<&'static str> = Vec::new();
                    for id in &self.selection {
                        if let Some(en) = self.entities.iter().find(|en| en.id == *id && en.team == self.my_team) {
                            for &ab in abilities_for_kind(&en.kind) {
                                if !shown.contains(&ab) { shown.push(ab); }
                            }
                        }
                    }

                    if shown.is_empty() {
                        ui.label(cmd_x, cmd_y, "Zadne schopnosti", 1.0, colors::GREY);
                    }

                    let my_sel   = self.selection.clone();
                    let is_pending_ab = self.pending_ability.clone();

                    for (i, &ab_id) in shown.iter().enumerate() {
                        let col = i % cols;
                        let row = i / cols;
                        let bx  = cmd_x + col as f32 * (btn_w + gap);
                        let by  = cmd_y + row as f32 * (btn_h + gap);
                        let label = ability_label(ab_id);

                        // Zvýrazni aktivní schopnost
                        let btn_color = if is_pending_ab.as_deref() == Some(ab_id) {
                            [0.6, 0.5, 0.1, 1.0]  // zlatá = čeká na cíl
                        } else {
                            colors::BTN_NORMAL
                        };
                        let clicked = ui.button(Rect::new(bx, by, btn_w, btn_h), btn_color);
                        ui.label_centered(Rect::new(bx, by, btn_w, btn_h), label, 1.0, colors::WHITE);

                        if clicked {
                            match ability_target_type(ab_id) {
                                AbilityTarget::None => {
                                    // Okamžitá schopnost bez cíle
                                    if let Some(&uid) = my_sel.first() {
                                        self.queued_actions.push(PlayerAction::UseAbility {
                                            unit_id: uid,
                                            ability_id: ab_id.to_string(),
                                            target_id: None,
                                            target_x: 0.0, target_y: 0.0,
                                        });
                                    }
                                }
                                AbilityTarget::Point | AbilityTarget::Unit => {
                                    // Přepni do režimu cílování
                                    self.pending_ability = Some(ab_id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Tipy pro ovládání ────────────────────────────────────────────────
        if self.tick < 200 {
            let alpha = if self.tick < 160 { 1.0 } else { (200 - self.tick) as f32 / 40.0 };
            let col = [0.9, 0.9, 0.7, alpha];
            let cx = sw * 0.5 - 200.0;
            let cy = sh * 0.5 - 60.0;
            ui.label(cx, cy +  0.0, "WASD / Sipky = pohyb kamery", 1.0, col);
            ui.label(cx, cy + 14.0, "Kolecko = zoom", 1.0, col);
            ui.label(cx, cy + 28.0, "Leve tl. = vybrat jednotku/box", 1.0, col);
            ui.label(cx, cy + 42.0, "Prave tl. = pohyb / utok / cil", 1.0, col);
            ui.label(cx, cy + 56.0, "P=Patrol  H=HolyLight  D=DeathCoil  B=Blizzard", 1.0, col);
            ui.label(cx, cy + 70.0, "Mezera = stop  ESC = menu", 1.0, col);
        }

        // ── Game over ────────────────────────────────────────────────────────
        if let Some(ref text) = self.game_over {
            let bw = 520.0f32.min(sw - 40.0);
            let bh = 90.0;
            let bx = (sw - bw) * 0.5;
            let by = sh * 0.38;
            ui.panel(Rect::new(bx, by, bw, bh), [0.05, 0.05, 0.08, 0.96]);
            ui.border(Rect::new(bx, by, bw, bh), 2.0, [0.7, 0.7, 0.2, 1.0]);
            ui.label_centered(Rect::new(bx, by,             bw, bh * 0.45), "KONEC HRY", 2.0, [1.0, 0.9, 0.3, 1.0]);
            ui.label_centered(Rect::new(bx, by + bh * 0.48, bw, bh * 0.35), text, 1.0, colors::WHITE);
            ui.label_centered(Rect::new(bx, by + bh + 4.0,  bw, 18.0), "ESC = zpet do menu", 1.0, colors::GREY);
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
