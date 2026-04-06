/// Developer editor – umožňuje malovat terén, umísťovat jednotky/budovy
/// a inspektovat entity přímo ve hře bez restartu.
///
/// Layout (1280×720):
/// ┌─────────────────────────────────────────────────────────────────┐
/// │ Toolbar  40px  │ [TILES] [UNITS] [SELECT] [ERASE] │ cursor info │
/// ├──────────┬──────────────────────────────────────────────────────┤
/// │ Sidebar  │                                                      │
/// │ 140px    │              MAP VIEW                                │
/// │          │                                                      │
/// │  palette │                                                      │
/// │          │                                                      │
/// ├──────────┴──────────────────────────────────────────────────────┤
/// │ Inspector  80px │ info o hoveru / vybrané entitě                │
/// └─────────────────────────────────────────────────────────────────┘

use glam::Vec2;
use hecs::World;

use engine::{
    Rect, UvRect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    tilemap::{TileKind, TileMap, TILE_SIZE},
    ui::{UiCtx, colors},
};
use engine::winit::keyboard::KeyCode;
use engine::winit::event::MouseButton;

use crate::components::*;
use super::{Screen, Transition};

// ── Layout konstanty ──────────────────────────────────────────────────────────

const SIDEBAR_W:    f32 = 140.0;
const TOOLBAR_H:    f32 = 42.0;
const INSPECTOR_H:  f32 = 88.0;
const SHEET_COLS:   u32 = 8;
const SHEET_ROWS:   u32 = 8;
const CAM_PAN_SPEED: f32 = 400.0;
const ZOOM_FACTOR:   f32 = 1.15;

// ── Mód editoru ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Tiles,   // maluj terén
    Units,   // umísti entitu
    Select,  // vyber + inspektuj entitu
    Erase,   // smaž entitu pod kurzorem
}

// ── Položka unit palety ────────────────────────────────────────────────────────

#[derive(Clone)]
struct PlaceEntry {
    kind:       UnitKind,
    team:       u8,
    hp:         i32,
    size:       f32,
    sprite_col: u32,
    sprite_row: u32,
    /// Barva ikonky v paletě (team color).
    icon_color: [f32; 4],
    /// Barva štítku – světlejší odstín pro akcentový proužek.
    accent:     [f32; 4],
}

// ── Stav editoru ──────────────────────────────────────────────────────────────

pub struct EditorScreen {
    world:      World,
    map:        TileMap,
    white_bg:   Option<engine::wgpu::BindGroup>,

    mode:           EditorMode,

    // Tile paleta
    tile_palette:   Vec<(TileKind, [f32; 4])>,
    selected_tile:  usize,

    // Unit paleta
    unit_palette:   Vec<PlaceEntry>,
    selected_unit:  usize,

    // Select mode
    selected_entity: Option<hecs::Entity>,
    inspect_snapshot: Option<EntitySnapshot>,

    // Stav pod kurzorem
    hovered_tile:   Option<(u32, u32)>,
    painting:       bool,

    // Příznak prvního snímku – centruj kameru
    first_frame: bool,
}

/// Snapshot dat vybrané entity pro inspector (kopie, nevyžaduje borrow).
#[derive(Clone)]
struct EntitySnapshot {
    pos:      Vec2,
    hp:       i32,
    hp_max:   i32,
    team:     u8,
    kind:     UnitKind,
    size:     f32,
    color:    [f32; 4],
}

// ── Konstruktor ───────────────────────────────────────────────────────────────

impl EditorScreen {
    pub fn new() -> Self {
        let mut map = TileMap::new_filled(64, 64, TileKind::Grass);
        // Odhal celou mapu – editor nemá fog of war
        for y in 0..64 {
            for x in 0..64 {
                if let Some(t) = map.get_mut(x, y) {
                    t.visible  = true;
                    t.explored = true;
                }
            }
        }

        Self {
            world:   World::new(),
            map,
            white_bg: None,

            mode:          EditorMode::Tiles,

            tile_palette:  build_tile_palette(),
            selected_tile: 0,

            unit_palette:  build_unit_palette(),
            selected_unit: 0,

            selected_entity:  None,
            inspect_snapshot: None,

            hovered_tile: None,
            painting:     false,
            first_frame:  true,
        }
    }

    // ── Oblast mapy (bez sidebaru + toolbaru + inspektoru) ────────────────

    fn map_area(screen: Vec2) -> Rect {
        Rect::new(
            SIDEBAR_W,
            TOOLBAR_H,
            screen.x - SIDEBAR_W,
            screen.y - TOOLBAR_H - INSPECTOR_H,
        )
    }

    fn in_map_area(screen: Vec2, pos: Vec2) -> bool {
        Self::map_area(screen).contains(pos)
    }

    // ── Tile pod kurzorem ─────────────────────────────────────────────────

    fn world_to_tile(world_pos: Vec2) -> (u32, u32) {
        (
            (world_pos.x / TILE_SIZE).max(0.0) as u32,
            (world_pos.y / TILE_SIZE).max(0.0) as u32,
        )
    }

    // ── Aplikuj aktuální tile nástroj ─────────────────────────────────────

    fn paint_tile(&mut self, tx: u32, ty: u32) {
        let kind = self.tile_palette[self.selected_tile].0;
        self.map.set(tx, ty, kind);
        if let Some(t) = self.map.get_mut(tx, ty) {
            t.visible  = true;
            t.explored = true;
        }
    }

    // ── Umísti jednotku ───────────────────────────────────────────────────

    fn place_unit(&mut self, world_pos: Vec2) {
        let entry = &self.unit_palette[self.selected_unit];
        self.world.spawn((
            Position(world_pos),
            Velocity(Vec2::ZERO),
            Sprite {
                col:   entry.sprite_col,
                row:   entry.sprite_row,
                size:  Vec2::splat(entry.size),
                color: entry.icon_color,
            },
            Team(entry.team),
            Health::new(entry.hp),
            Unit(entry.kind),
        ));
    }

    // ── Vymaž entitu nejblíže kurzoru ─────────────────────────────────────

    fn erase_at(&mut self, world_pos: Vec2) {
        let radius = TILE_SIZE;
        let to_remove: Vec<hecs::Entity> = self.world
            .query::<&Position>()
            .iter()
            .filter(|(_, p)| (p.0 - world_pos).length() < radius)
            .map(|(e, _)| e)
            .collect();
        for e in to_remove {
            if Some(e) == self.selected_entity {
                self.selected_entity  = None;
                self.inspect_snapshot = None;
            }
            let _ = self.world.despawn(e);
        }
    }

    // ── Vyber entitu nejblíže kurzoru ─────────────────────────────────────

    fn select_at(&mut self, world_pos: Vec2) {
        let radius = TILE_SIZE * 1.5;
        let found = self.world
            .query::<(&Position, &Health, &Sprite, &Team, &Unit)>()
            .iter()
            .filter(|(_, (p, ..))| (p.0 - world_pos).length() < radius)
            .min_by_key(|(_, (p, ..))| ((p.0 - world_pos).length() * 100.0) as i64)
            .map(|(e, (pos, hp, sprite, team, unit))| {
                (e, EntitySnapshot {
                    pos:     pos.0,
                    hp:      hp.current,
                    hp_max:  hp.max,
                    team:    team.0,
                    kind:    unit.0,
                    size:    sprite.size.x,
                    color:   sprite.color,
                })
            });

        if let Some((entity, snap)) = found {
            self.selected_entity  = Some(entity);
            self.inspect_snapshot = Some(snap);
        } else {
            self.selected_entity  = None;
            self.inspect_snapshot = None;
        }
    }

    // ── Refresh snapshot vybrané entity ──────────────────────────────────

    fn refresh_snapshot(&mut self) {
        if let Some(entity) = self.selected_entity {
            let snap = self.world
                .query::<(&Position, &Health, &Sprite, &Team, &Unit)>()
                .iter()
                .find(|(e, _)| *e == entity)
                .map(|(_, (pos, hp, sprite, team, unit))| EntitySnapshot {
                    pos:     pos.0,
                    hp:      hp.current,
                    hp_max:  hp.max,
                    team:    team.0,
                    kind:    unit.0,
                    size:    sprite.size.x,
                    color:   sprite.color,
                });
            self.inspect_snapshot = snap;
            if self.inspect_snapshot.is_none() {
                self.selected_entity = None;
            }
        }
    }
}

// ── Screen impl ───────────────────────────────────────────────────────────────

impl Screen for EditorScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        let bg  = tex.create_bind_group(ctx, &batch.texture_bind_group_layout);
        self.white_bg = Some(bg);
    }

    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera) -> Transition {
        // Centruj kameru na střed mapy při prvním snímku
        if self.first_frame {
            camera.position = Vec2::new(
                self.map.width  as f32 * TILE_SIZE * 0.5,
                self.map.height as f32 * TILE_SIZE * 0.5,
            );
            camera.zoom = 1.0;
            self.first_frame = false;
        }

        let screen = camera.viewport();

        // ── Klávesové zkratky módu ────────────────────────────────────────
        if input.key_just_pressed(KeyCode::KeyT) { self.mode = EditorMode::Tiles; }
        if input.key_just_pressed(KeyCode::KeyU) { self.mode = EditorMode::Units; }
        if input.key_just_pressed(KeyCode::KeyS) { self.mode = EditorMode::Select; }
        if input.key_just_pressed(KeyCode::KeyX) { self.mode = EditorMode::Erase; }

        // Del = smaž vybranou entitu v select módu
        if input.key_just_pressed(KeyCode::Delete) {
            if let Some(e) = self.selected_entity.take() {
                let _ = self.world.despawn(e);
                self.inspect_snapshot = None;
            }
        }

        // ESC → main menu
        if input.key_just_pressed(KeyCode::Escape) {
            use super::main_menu::MainMenuScreen;
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        // ── Kamera – jen pokud myš NENÍ v sidebaru nebo toolbaru ─────────
        handle_camera_editor(dt, input, camera, screen);

        // ── Tile pod kurzorem ─────────────────────────────────────────────
        let in_map = Self::in_map_area(screen, input.mouse_pos);
        let world_pos = camera.screen_to_world(input.mouse_pos);
        let (tx, ty) = Self::world_to_tile(world_pos);
        self.hovered_tile = if in_map && tx < self.map.width && ty < self.map.height {
            Some((tx, ty))
        } else {
            None
        };

        // ── Sidebar kliknutí – výběr z palety ────────────────────────────
        let sidebar_rect = Rect::new(0.0, TOOLBAR_H, SIDEBAR_W, screen.y - TOOLBAR_H - INSPECTOR_H);
        if sidebar_rect.contains(input.mouse_pos) && input.mouse_just_pressed(MouseButton::Left) {
            let idx = ((input.mouse_pos.y - TOOLBAR_H) / PALETTE_ITEM_H) as usize;
            match self.mode {
                EditorMode::Tiles  => { if idx < self.tile_palette.len() { self.selected_tile = idx; } }
                EditorMode::Units  => { if idx < self.unit_palette.len() { self.selected_unit = idx; } }
                _ => {}
            }
        }

        // ── Toolbar kliknutí – přepnutí módu ─────────────────────────────
        let toolbar_rect = Rect::new(0.0, 0.0, screen.x, TOOLBAR_H);
        if toolbar_rect.contains(input.mouse_pos) && input.mouse_just_pressed(MouseButton::Left) {
            for (i, mode) in [EditorMode::Tiles, EditorMode::Units, EditorMode::Select, EditorMode::Erase]
                .iter().enumerate()
            {
                let btn = toolbar_btn_rect(i);
                if btn.contains(input.mouse_pos) {
                    self.mode = *mode;
                }
            }
        }

        // ── Akce na mapě ──────────────────────────────────────────────────
        if in_map {
            match self.mode {
                EditorMode::Tiles => {
                    if input.mouse_just_pressed(MouseButton::Left) { self.painting = true; }
                    if input.mouse_just_released(MouseButton::Left) { self.painting = false; }
                    if self.painting {
                        if let Some((tx, ty)) = self.hovered_tile {
                            self.paint_tile(tx, ty);
                        }
                    }
                    // RMB = eyedropper (vybere tile pod kurzorem)
                    if input.mouse_just_pressed(MouseButton::Right) {
                        if let Some((tx, ty)) = self.hovered_tile {
                            if let Some(t) = self.map.get(tx, ty) {
                                if let Some(idx) = self.tile_palette.iter().position(|(k,_)| *k == t.kind) {
                                    self.selected_tile = idx;
                                }
                            }
                        }
                    }
                }
                EditorMode::Units => {
                    if input.mouse_just_pressed(MouseButton::Left) {
                        self.place_unit(snap_to_tile_center(world_pos));
                    }
                    // RMB = smaž entitu pod kurzorem
                    if input.mouse_just_pressed(MouseButton::Right) {
                        self.erase_at(world_pos);
                    }
                }
                EditorMode::Select => {
                    if input.mouse_just_pressed(MouseButton::Left) {
                        self.select_at(world_pos);
                    }
                }
                EditorMode::Erase => {
                    if input.mouse_just_pressed(MouseButton::Left) || self.painting {
                        self.erase_at(world_pos);
                    }
                    if input.mouse_just_pressed(MouseButton::Left) { self.painting = true; }
                    if input.mouse_just_released(MouseButton::Left) { self.painting = false; }
                }
            }
        } else {
            self.painting = false;
        }

        self.refresh_snapshot();
        Transition::None
    }

    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera) {
        let vp = camera.viewport();
        let view_half = vp * 0.5 / camera.zoom;
        let view_rect = Rect::new(
            camera.position.x - view_half.x,
            camera.position.y - view_half.y,
            view_half.x * 2.0,
            view_half.y * 2.0,
        );

        // Terrain – vždy plná viditelnost
        for (tx, ty) in self.map.visible_tiles(view_rect) {
            let tile = match self.map.get(tx, ty) { Some(t) => t, None => continue };
            let dst  = self.map.tile_rect(tx, ty);
            let uv   = UvRect::from_tile(
                tile.kind.sheet_pos().0, tile.kind.sheet_pos().1,
                SHEET_COLS, SHEET_ROWS,
            );
            batch.draw(dst, uv, tile_color(tile.kind));
        }

        // Highlight dlaždice pod kurzorem
        if let Some((tx, ty)) = self.hovered_tile {
            let dst = self.map.tile_rect(tx, ty);
            let uv  = UvRect::FULL;
            let col = match self.mode {
                EditorMode::Tiles  => [1.0, 1.0, 0.3, 0.25],
                EditorMode::Units  => [0.3, 1.0, 0.3, 0.25],
                EditorMode::Select => [0.3, 0.7, 1.0, 0.20],
                EditorMode::Erase  => [1.0, 0.2, 0.2, 0.30],
            };
            batch.draw(dst, uv, col);
        }

        // Entity
        for (_e, (pos, sprite)) in self.world.query::<(&Position, &Sprite)>().iter() {
            let half = sprite.size * 0.5;
            let dst  = Rect::new(pos.0.x - half.x, pos.0.y - half.y, sprite.size.x, sprite.size.y);
            let uv   = UvRect::from_tile(sprite.col, sprite.row, SHEET_COLS, SHEET_ROWS);
            batch.draw(dst, uv, sprite.color);
        }

        // Zvýraznění vybrané entity (Select mode)
        if let Some(entity) = self.selected_entity {
            if let Ok(pos) = self.world.get::<&Position>(entity) {
                if let Ok(sprite) = self.world.get::<&Sprite>(entity) {
                    let half = sprite.size * 0.5 + Vec2::splat(4.0);
                    let rect = Rect::new(pos.0.x - half.x, pos.0.y - half.y, half.x*2.0, half.y*2.0);
                    let tw   = 2.0 / camera.zoom;
                    let uv   = UvRect::FULL;
                    let col  = [1.0, 0.9, 0.1, 1.0]; // žlutá = vybraná
                    batch.draw(Rect::new(rect.x, rect.y, rect.w, tw), uv, col);
                    batch.draw(Rect::new(rect.x, rect.y+rect.h-tw, rect.w, tw), uv, col);
                    batch.draw(Rect::new(rect.x, rect.y, tw, rect.h), uv, col);
                    batch.draw(Rect::new(rect.x+rect.w-tw, rect.y, tw, rect.h), uv, col);
                }
            }
        }

        // Preview při umísťování jednotky: ghost sprite na tile pod kurzorem
        if self.mode == EditorMode::Units {
            if let Some((tx, ty)) = self.hovered_tile {
                let cx = tx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                let cy = ty as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                let entry = &self.unit_palette[self.selected_unit];
                let half  = entry.size * 0.5;
                let dst   = Rect::new(cx - half, cy - half, entry.size, entry.size);
                let uv    = UvRect::from_tile(entry.sprite_col, entry.sprite_row, SHEET_COLS, SHEET_ROWS);
                let ghost = [entry.icon_color[0], entry.icon_color[1], entry.icon_color[2], 0.5];
                batch.draw(dst, uv, ghost);
            }
        }
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        draw_toolbar(ui, self.mode, sw);
        draw_sidebar(ui, self.mode, &self.tile_palette, self.selected_tile,
                     &self.unit_palette, self.selected_unit, sh);
        draw_inspector(ui, self.mode, &self.hovered_tile, &self.map,
                       &self.inspect_snapshot, &self.unit_palette, self.selected_unit,
                       &self.tile_palette, self.selected_tile, sw, sh);
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.white_bg.as_ref().expect("EditorScreen::init not called")
    }
}

// ── UI kreslení ───────────────────────────────────────────────────────────────

const PALETTE_ITEM_H: f32 = 36.0;

fn toolbar_btn_rect(i: usize) -> Rect {
    let x = 8.0 + i as f32 * 110.0;
    Rect::new(x, 5.0, 100.0, 32.0)
}

fn draw_toolbar(ui: &mut UiCtx, mode: EditorMode, sw: f32) {
    ui.panel(Rect::new(0.0, 0.0, sw, TOOLBAR_H), colors::BG_DARK);
    ui.border(Rect::new(0.0, 0.0, sw, TOOLBAR_H), 1.0, colors::BORDER);

    let modes: &[(EditorMode, [f32; 4], &str, &str)] = &[
        (EditorMode::Tiles,  [0.25, 0.60, 0.20, 1.0], "TILES",  "[T]"),
        (EditorMode::Units,  [0.20, 0.45, 1.00, 1.0], "UNITS",  "[U]"),
        (EditorMode::Select, [0.80, 0.75, 0.10, 1.0], "SELECT", "[S]"),
        (EditorMode::Erase,  [0.80, 0.20, 0.15, 1.0], "ERASE",  "[X]"),
    ];

    for (i, (m, accent, label, key)) in modes.iter().enumerate() {
        let rect   = toolbar_btn_rect(i);
        let active = *m == mode;
        let bg     = if active { lighten(*accent, 0.35) } else { colors::BG_MID };
        ui.panel_bordered(rect, bg, colors::BORDER);

        if active {
            ui.panel(Rect::new(rect.x, rect.y, rect.w, 3.0), *accent);
        }

        // Ikonka nalevo
        draw_mode_icon(ui, Rect::new(rect.x, rect.y, 32.0, rect.h), *m, active, *accent);

        // Text popisku
        let text_col = if active { colors::WHITE } else { colors::GREY };
        ui.label_shadowed(rect.x + 34.0, rect.y + 6.0,  label, 1.0, text_col);
        ui.label(         rect.x + 34.0, rect.y + 18.0, key,   1.0,
                          if active { *accent } else { darken(*accent, 0.8) });
    }

    // Pravý okraj – klávesa ESC
    ui.label(sw - 88.0, 14.0, "[ESC] menu", 1.0, colors::GREY);
    ui.panel(Rect::new(sw - 2.0, 0.0, 2.0, TOOLBAR_H), colors::BORDER);
}

fn draw_mode_icon(ui: &mut UiCtx, btn: Rect, mode: EditorMode, active: bool, accent: [f32; 4]) {
    let cx = btn.x + btn.w * 0.5;
    let cy = btn.y + btn.h * 0.5;
    let col = if active { accent } else { darken(accent, 0.7) };

    match mode {
        EditorMode::Tiles => {
            // 2×2 dlaždicová mřížka
            for gy in 0..2i32 { for gx in 0..2i32 {
                let px = cx - 14.0 + gx as f32 * 15.0;
                let py = cy - 10.0 + gy as f32 * 12.0;
                ui.panel(Rect::new(px, py, 12.0, 10.0), col);
                ui.border(Rect::new(px, py, 12.0, 10.0), 1.0, darken(col, 0.6));
            }}
        }
        EditorMode::Units => {
            // Kosočtverec = unit ikona
            let s = 8.0_f32;
            ui.panel(Rect::new(cx - s*1.5, cy - s*0.5, s*3.0, s), col); // vodorovný pruh
            ui.panel(Rect::new(cx - s*0.5, cy - s*1.5, s,     s*3.0), col); // svislý pruh
        }
        EditorMode::Select => {
            // Tečkovaný rámeček (4 rohy)
            let w = 22.0; let h = 16.0;
            let x = cx - w*0.5; let y = cy - h*0.5;
            let t = 3.0;
            ui.panel(Rect::new(x,       y,       8.0, t),   col);
            ui.panel(Rect::new(x+w-8.0, y,       8.0, t),   col);
            ui.panel(Rect::new(x,       y+h-t,   8.0, t),   col);
            ui.panel(Rect::new(x+w-8.0, y+h-t,   8.0, t),   col);
            ui.panel(Rect::new(x,       y,       t,   6.0), col);
            ui.panel(Rect::new(x,       y+h-6.0, t,   6.0), col);
            ui.panel(Rect::new(x+w-t,   y,       t,   6.0), col);
            ui.panel(Rect::new(x+w-t,   y+h-6.0, t,   6.0), col);
        }
        EditorMode::Erase => {
            // X tvar
            let s = 10.0_f32; let t = 3.0;
            ui.panel(Rect::new(cx - s, cy - t*0.5, s*2.0, t), col); // vodorovná
            ui.panel(Rect::new(cx - t*0.5, cy - s, t, s*2.0), col); // svislá – X jako +, vizuálně stačí
            // diagonály pomocí malých čtverců
            for d in -2i32..=2 {
                let dx = d as f32 * 4.0;
                ui.panel(Rect::new(cx + dx - 1.5, cy + dx - 1.5, 3.0, 3.0), col);
                ui.panel(Rect::new(cx + dx - 1.5, cy - dx - 1.5, 3.0, 3.0), col);
            }
        }
    }
}

fn draw_sidebar(
    ui: &mut UiCtx,
    mode: EditorMode,
    tile_palette: &[(TileKind, [f32; 4])],
    selected_tile: usize,
    unit_palette: &[PlaceEntry],
    selected_unit: usize,
    sh: f32,
) {
    let h = sh - TOOLBAR_H - INSPECTOR_H;
    ui.panel(Rect::new(0.0, TOOLBAR_H, SIDEBAR_W, h), colors::BG_DARK);
    ui.border(Rect::new(0.0, TOOLBAR_H, SIDEBAR_W, h), 1.0, colors::BORDER);

    // Nadpisový proužek s barvou + label aktivního módu
    let (header_col, header_label) = match mode {
        EditorMode::Tiles  => ([0.25, 0.60, 0.20, 1.0], "PALETTE"),
        EditorMode::Units  => ([0.20, 0.45, 1.00, 1.0], "UNITS"),
        EditorMode::Select => ([0.80, 0.75, 0.10, 1.0], "SELECT"),
        EditorMode::Erase  => ([0.80, 0.20, 0.15, 1.0], "ERASE"),
    };
    ui.panel(Rect::new(0.0, TOOLBAR_H, SIDEBAR_W, 16.0), darken(header_col, 0.5));
    ui.label(6.0, TOOLBAR_H + 4.0, header_label, 1.0, header_col);
    ui.panel(Rect::new(0.0, TOOLBAR_H + 16.0, SIDEBAR_W, 2.0), header_col);

    match mode {
        EditorMode::Tiles | EditorMode::Erase => {
            for (i, (kind, color)) in tile_palette.iter().enumerate() {
                let ry = TOOLBAR_H + 20.0 + i as f32 * PALETTE_ITEM_H;
                let item = Rect::new(4.0, ry, SIDEBAR_W - 8.0, PALETTE_ITEM_H - 4.0);

                let selected = i == selected_tile;
                let bg = if selected { lighten(*color, 0.4) } else { colors::BG_MID };
                ui.panel(item, bg);

                // Barevná ukázka dlaždice
                ui.panel(Rect::new(item.x + 2.0, item.y + 2.0, 28.0, item.h - 4.0), *color);

                if selected {
                    ui.panel(Rect::new(item.x, item.y, 3.0, item.h), [1.0, 1.0, 1.0, 0.9]);
                }

                // Název dlaždice
                let name = tile_kind_name(*kind);
                let text_col = if selected { colors::WHITE } else { colors::GREY };
                ui.label_shadowed(item.x + 36.0, item.y + (item.h - 8.0) * 0.5, name, 1.0, text_col);

                ui.border(item, 1.0, if selected { [1.0,1.0,1.0,0.5] } else { colors::BORDER });
            }
        }
        EditorMode::Units => {
            draw_team_switcher(ui, unit_palette, selected_unit);

            for (i, entry) in unit_palette.iter().enumerate() {
                let ry = TOOLBAR_H + 44.0 + i as f32 * PALETTE_ITEM_H;
                if ry + PALETTE_ITEM_H > TOOLBAR_H + sh - INSPECTOR_H { break; }
                let item = Rect::new(4.0, ry, SIDEBAR_W - 8.0, PALETTE_ITEM_H - 4.0);

                let selected = i == selected_unit;
                let bg = if selected { lighten(entry.icon_color, 0.3) } else { colors::BG_MID };
                ui.panel(item, bg);

                // Barevná ikonka jednotky
                ui.panel(Rect::new(item.x + 2.0, item.y + 2.0, 28.0, item.h - 4.0), entry.icon_color);

                if selected {
                    ui.panel(Rect::new(item.x, item.y, 3.0, item.h), entry.accent);
                }

                // Název jednotky
                let text_col = if selected { colors::WHITE } else { colors::GREY };
                ui.label_shadowed(item.x + 36.0, item.y + 3.0, unit_kind_short(entry.kind), 1.0, text_col);

                // HP bar
                let hp_frac = (entry.hp as f32 / 1000.0).clamp(0.05, 1.0);
                let hp_col  = health_color(hp_frac);
                ui.progress_bar(Rect::new(item.x + 36.0, item.y + 16.0, SIDEBAR_W - 48.0, 5.0),
                                hp_frac, [0.15,0.15,0.15,1.0], hp_col);

                // Team color strip
                let team_col = if entry.team == 0 { [0.20,0.45,1.0,1.0] } else { [0.80,0.20,0.10,1.0] };
                ui.panel(Rect::new(item.x + 36.0, item.y + item.h - 5.0, SIDEBAR_W - 48.0, 3.0), team_col);

                ui.border(item, 1.0, if selected { entry.accent } else { colors::BORDER });
            }
        }
        EditorMode::Select => {
            // Sidebar: nápověda (barevné indikátory akcí)
            draw_select_hints(ui, sh);
        }
    }
}

fn draw_team_switcher(ui: &mut UiCtx, unit_palette: &[PlaceEntry], selected_unit: usize) {
    let y = TOOLBAR_H + 20.0;
    // Team 0
    ui.panel(Rect::new(5.0, y, 60.0, 18.0), [0.10, 0.25, 0.55, 1.0]);
    ui.border(Rect::new(5.0, y, 60.0, 18.0), 1.0, [0.20, 0.45, 1.0, 1.0]);
    ui.label_centered(Rect::new(5.0, y, 60.0, 18.0), "HUMAN", 1.0, [0.60, 0.80, 1.0, 1.0]);
    // Team 1
    ui.panel(Rect::new(72.0, y, 60.0, 18.0), [0.45, 0.10, 0.10, 1.0]);
    ui.border(Rect::new(72.0, y, 60.0, 18.0), 1.0, [0.80, 0.20, 0.10, 1.0]);
    ui.label_centered(Rect::new(72.0, y, 60.0, 18.0), "ORC", 1.0, [1.0, 0.60, 0.50, 1.0]);

    // Indikátor aktuálního týmu
    if selected_unit < unit_palette.len() {
        let team = unit_palette[selected_unit].team;
        let ix = if team == 0 { 5.0 } else { 72.0 };
        ui.panel(Rect::new(ix, y + 18.0, 60.0, 3.0), [1.0, 1.0, 1.0, 0.9]);
    }
}

fn draw_select_hints(ui: &mut UiCtx, sh: f32) {
    let h = sh - TOOLBAR_H - INSPECTOR_H;
    // Klik = vybrat
    let r0 = Rect::new(6.0, TOOLBAR_H + 20.0, SIDEBAR_W - 12.0, 36.0);
    ui.panel(r0, [0.10, 0.25, 0.55, 1.0]);
    ui.panel(Rect::new(r0.x, r0.y, 4.0, r0.h), [0.20, 0.45, 1.0, 1.0]);
    ui.label_shadowed(r0.x + 8.0, r0.y + 6.0,  "LMB", 1.0, [0.20, 0.45, 1.0, 1.0]);
    ui.label(         r0.x + 8.0, r0.y + 18.0, "select", 1.0, colors::GREY);
    ui.border(r0, 1.0, [0.20, 0.45, 1.0, 1.0]);
    // Del = smazat
    let r1 = Rect::new(6.0, TOOLBAR_H + 62.0, SIDEBAR_W - 12.0, 36.0);
    ui.panel(r1, [0.45, 0.10, 0.10, 1.0]);
    ui.panel(Rect::new(r1.x, r1.y, 4.0, r1.h), [0.80, 0.20, 0.15, 1.0]);
    ui.label_shadowed(r1.x + 8.0, r1.y + 6.0,  "Del", 1.0, [0.80, 0.20, 0.15, 1.0]);
    ui.label(         r1.x + 8.0, r1.y + 18.0, "delete", 1.0, colors::GREY);
    ui.border(r1, 1.0, [0.80, 0.20, 0.15, 1.0]);
    // Dole
    let guide_y = TOOLBAR_H + h - 30.0;
    ui.panel(Rect::new(4.0, guide_y, SIDEBAR_W - 8.0, 1.0), colors::BORDER);
    ui.label(6.0, guide_y + 6.0, "Inspector:", 1.0, colors::GREY);
    ui.label(6.0, guide_y + 16.0, "see below", 1.0, darken(colors::GREY, 0.7));
}

fn draw_inspector(
    ui: &mut UiCtx,
    mode: EditorMode,
    hovered_tile: &Option<(u32, u32)>,
    map: &TileMap,
    snapshot: &Option<EntitySnapshot>,
    unit_palette: &[PlaceEntry],
    selected_unit: usize,
    tile_palette: &[(TileKind, [f32; 4])],
    selected_tile: usize,
    sw: f32,
    sh: f32,
) {
    let y = sh - INSPECTOR_H;
    ui.panel(Rect::new(0.0, y, sw, INSPECTOR_H), colors::BG_DARK);
    ui.border(Rect::new(0.0, y, sw, INSPECTOR_H), 1.0, colors::BORDER);

    // Levý panel – info o hoveru
    let info_x = 8.0;

    match mode {
        EditorMode::Tiles | EditorMode::Erase => {
            let (sel_kind, sel_col) = tile_palette[selected_tile];
            ui.panel(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), sel_col);
            ui.border(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), 2.0, [1.0,1.0,1.0,0.4]);
            ui.label_shadowed(info_x + 72.0, y + 8.0,  tile_kind_name(sel_kind), 1.0, colors::WHITE);
            ui.label(         info_x + 72.0, y + 22.0, "selected", 1.0, colors::GREY);

            if let Some((tx, ty)) = hovered_tile {
                if let Some(tile) = map.get(*tx, *ty) {
                    let hov_col = tile_color(tile.kind);
                    ui.panel(Rect::new(info_x + 72.0, y + 36.0, 48.0, 20.0), hov_col);
                    ui.border(Rect::new(info_x + 72.0, y + 36.0, 48.0, 20.0), 1.0, colors::BORDER);
                    ui.label_shadowed(info_x + 126.0, y + 38.0, tile_kind_name(tile.kind), 1.0, colors::GREY);
                }
                ui.label_shadowed(info_x + 72.0,  y + 60.0,
                    &format!("X:{} Y:{}", tx, ty), 1.0, [1.0, 0.85, 0.3, 1.0]);
            } else {
                ui.label(info_x + 72.0, y + 36.0, "hover: ---", 1.0, colors::GREY);
            }
        }
        EditorMode::Units => {
            if selected_unit < unit_palette.len() {
                let entry = &unit_palette[selected_unit];
                ui.panel(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), entry.icon_color);
                ui.border(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), 2.0, entry.accent);

                // Název jednotky
                ui.label_shadowed(info_x + 72.0, y + 8.0,  unit_kind_short(entry.kind), 1.0, colors::WHITE);
                let team_name = if entry.team == 0 { "Human" } else { "Orc" };
                let tc = if entry.team == 0 { [0.40,0.70,1.0,1.0] } else { [1.0,0.50,0.40,1.0] };
                ui.label(info_x + 72.0, y + 20.0, team_name, 1.0, tc);

                // HP
                let hp_frac = (entry.hp as f32 / 1500.0).clamp(0.05, 1.0);
                ui.progress_bar(Rect::new(info_x + 72.0, y + 34.0, 200.0, 10.0),
                                hp_frac, [0.1,0.1,0.1,1.0], health_color(hp_frac));
                ui.label(info_x + 72.0, y + 48.0,
                    &format!("HP: {}", entry.hp), 1.0, colors::GREY);
            }
        }
        EditorMode::Select => {
            if let Some(snap) = snapshot {
                ui.panel(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), snap.color);
                ui.border(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), 2.0, [1.0,0.9,0.1,1.0]);

                // Název + tým
                ui.label_shadowed(info_x + 72.0, y + 8.0, unit_kind_short(snap.kind), 1.0, colors::WHITE);
                let (team_name, tc) = if snap.team == 0 {
                    ("Human", [0.40,0.70,1.0,1.0f32])
                } else {
                    ("Orc", [1.0,0.50,0.40,1.0])
                };
                ui.label(info_x + 72.0, y + 20.0, team_name, 1.0, tc);

                // HP bar + čísla
                let hp_frac = snap.hp as f32 / snap.hp_max as f32;
                ui.progress_bar(Rect::new(info_x + 72.0, y + 34.0, 220.0, 10.0),
                                hp_frac, [0.1,0.1,0.1,1.0], health_color(hp_frac));
                ui.label_shadowed(info_x + 72.0, y + 48.0,
                    &format!("HP {}/{}", snap.hp, snap.hp_max), 1.0, colors::WHITE);

                // Pozice
                let tx = (snap.pos.x / TILE_SIZE) as i32;
                let ty = (snap.pos.y / TILE_SIZE) as i32;
                ui.label(info_x + 72.0, y + 62.0,
                    &format!("X:{} Y:{}", tx, ty), 1.0, [1.0,0.85,0.3,1.0]);

                // Entity ID (zkrácené)
                ui.label(info_x + 300.0, y + 8.0, "Del=delete", 1.0, [0.7,0.3,0.3,1.0]);
            } else {
                ui.panel(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), colors::BG_LIGHT);
                ui.border(Rect::new(info_x, y + 8.0, 64.0, INSPECTOR_H - 16.0), 1.0, colors::BORDER);
                ui.label_shadowed(info_x + 72.0, y + INSPECTOR_H*0.5 - 4.0,
                    "No entity selected", 1.0, colors::GREY);
                ui.label(info_x + 72.0, y + INSPECTOR_H*0.5 + 8.0,
                    "Click to select", 1.0, darken(colors::GREY, 0.7));
            }
        }
    }

    // Pravý kraje: klávesové zkratky jako barevné badges
    draw_shortcut_badges(ui, sw, y);
}

fn draw_shortcut_badges(ui: &mut UiCtx, sw: f32, y: f32) {
    let shortcuts: &[([f32; 4], [f32; 4], &str)] = &[
        ([0.25, 0.60, 0.20, 1.0], [0.15, 0.35, 0.12, 1.0], "T"),
        ([0.20, 0.45, 1.00, 1.0], [0.10, 0.25, 0.55, 1.0], "U"),
        ([0.80, 0.75, 0.10, 1.0], [0.45, 0.42, 0.05, 1.0], "S"),
        ([0.80, 0.20, 0.15, 1.0], [0.45, 0.10, 0.08, 1.0], "X"),
    ];
    let sx = sw - 168.0;
    ui.label(sx - 2.0, y + 6.0, "Keys:", 1.0, colors::GREY);
    for (i, (accent, bg, key)) in shortcuts.iter().enumerate() {
        let bx = sx + 44.0 + i as f32 * 30.0;
        ui.panel(Rect::new(bx, y + 4.0, 26.0, 18.0), *bg);
        ui.panel(Rect::new(bx, y + 4.0, 26.0, 3.0),  *accent);
        ui.border(Rect::new(bx, y + 4.0, 26.0, 18.0), 1.0, *accent);
        ui.label_centered(Rect::new(bx, y + 4.0, 26.0, 18.0), key, 1.0, *accent);
    }
    ui.label(sx + 44.0 + 4.0 * 30.0 + 4.0, y + 6.0, "Del=rm", 1.0, [0.7,0.3,0.3,1.0]);
}

// ── Kamera ────────────────────────────────────────────────────────────────────

fn handle_camera_editor(dt: f32, input: &Input, camera: &mut Camera, screen: Vec2) {
    let in_sidebar  = input.mouse_pos.x < SIDEBAR_W;
    let in_toolbar  = input.mouse_pos.y < TOOLBAR_H;
    let in_inspector = input.mouse_pos.y > screen.y - INSPECTOR_H;

    let mut dir = Vec2::ZERO;
    if input.key_held(KeyCode::ArrowLeft)  || input.key_held(KeyCode::KeyA) { dir.x -= 1.0; }
    if input.key_held(KeyCode::ArrowRight) || input.key_held(KeyCode::KeyD) { dir.x += 1.0; }
    if input.key_held(KeyCode::ArrowUp)    || input.key_held(KeyCode::KeyW) { dir.y -= 1.0; }
    if input.key_held(KeyCode::ArrowDown)  || input.key_held(KeyCode::KeyS) { dir.y += 1.0; }
    if dir != Vec2::ZERO {
        camera.pan(dir.normalize() * (CAM_PAN_SPEED * dt / camera.zoom));
    }
    if !in_sidebar && !in_toolbar && !in_inspector {
        if input.scroll_delta != 0.0 {
            let factor = if input.scroll_delta > 0.0 { ZOOM_FACTOR } else { 1.0 / ZOOM_FACTOR };
            camera.zoom_around(factor, input.mouse_pos);
        }
        if input.mouse_held(MouseButton::Middle) {
            camera.pan(-input.mouse_delta / camera.zoom);
        }
    }
}

// ── Palety ────────────────────────────────────────────────────────────────────

fn build_tile_palette() -> Vec<(TileKind, [f32; 4])> {
    vec![
        (TileKind::Grass,     tile_color(TileKind::Grass)),
        (TileKind::Dirt,      tile_color(TileKind::Dirt)),
        (TileKind::Forest,    tile_color(TileKind::Forest)),
        (TileKind::Water,     tile_color(TileKind::Water)),
        (TileKind::DeepWater, tile_color(TileKind::DeepWater)),
        (TileKind::Sand,      tile_color(TileKind::Sand)),
        (TileKind::Rock,      tile_color(TileKind::Rock)),
        (TileKind::Bridge,    tile_color(TileKind::Bridge)),
    ]
}

fn build_unit_palette() -> Vec<PlaceEntry> {
    vec![
        PlaceEntry { kind: UnitKind::Peon,     team: 0, hp:  30, size: 32.0, sprite_col: 1, sprite_row: 0,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.40,0.70,1.0,1.0] },
        PlaceEntry { kind: UnitKind::Grunt,    team: 0, hp:  60, size: 32.0, sprite_col: 2, sprite_row: 0,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.40,0.70,1.0,1.0] },
        PlaceEntry { kind: UnitKind::Archer,   team: 0, hp:  40, size: 32.0, sprite_col: 3, sprite_row: 0,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.50,0.80,0.50,1.0] },
        PlaceEntry { kind: UnitKind::Catapult, team: 0, hp: 110, size: 48.0, sprite_col: 4, sprite_row: 0,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.70,0.60,0.30,1.0] },
        PlaceEntry { kind: UnitKind::TownHall, team: 0, hp: 1000, size: 64.0, sprite_col: 0, sprite_row: 2,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.60,0.80,1.0,1.0] },
        PlaceEntry { kind: UnitKind::Barracks, team: 0, hp: 800, size: 48.0, sprite_col: 4, sprite_row: 2,
                     icon_color: [0.20,0.45,1.0,1.0], accent: [0.60,0.80,1.0,1.0] },
        // Orci
        PlaceEntry { kind: UnitKind::Peon,     team: 1, hp:  30, size: 32.0, sprite_col: 1, sprite_row: 1,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [1.0,0.40,0.30,1.0] },
        PlaceEntry { kind: UnitKind::Grunt,    team: 1, hp:  60, size: 32.0, sprite_col: 2, sprite_row: 1,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [1.0,0.40,0.30,1.0] },
        PlaceEntry { kind: UnitKind::Archer,   team: 1, hp:  40, size: 32.0, sprite_col: 3, sprite_row: 1,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [1.0,0.60,0.30,1.0] },
        PlaceEntry { kind: UnitKind::Catapult, team: 1, hp: 110, size: 48.0, sprite_col: 4, sprite_row: 1,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [0.90,0.70,0.20,1.0] },
        PlaceEntry { kind: UnitKind::TownHall, team: 1, hp: 1000, size: 64.0, sprite_col: 4, sprite_row: 3,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [1.0,0.50,0.40,1.0] },
        PlaceEntry { kind: UnitKind::Barracks, team: 1, hp: 800, size: 48.0, sprite_col: 0, sprite_row: 4,
                     icon_color: [0.80,0.20,0.10,1.0], accent: [1.0,0.50,0.40,1.0] },
    ]
}

// ── Utility ───────────────────────────────────────────────────────────────────

fn tile_color(kind: TileKind) -> [f32; 4] {
    match kind {
        TileKind::Grass     => [0.25, 0.60, 0.20, 1.0],
        TileKind::Dirt      => [0.55, 0.40, 0.25, 1.0],
        TileKind::Water     => [0.15, 0.35, 0.75, 1.0],
        TileKind::DeepWater => [0.08, 0.20, 0.55, 1.0],
        TileKind::Forest    => [0.10, 0.40, 0.12, 1.0],
        TileKind::Rock      => [0.45, 0.45, 0.45, 1.0],
        TileKind::Sand      => [0.80, 0.72, 0.45, 1.0],
        TileKind::Bridge    => [0.50, 0.35, 0.20, 1.0],
    }
}

fn tile_kind_name(kind: TileKind) -> &'static str {
    match kind {
        TileKind::Grass     => "Grass",
        TileKind::Dirt      => "Dirt",
        TileKind::Forest    => "Forest",
        TileKind::Water     => "Water",
        TileKind::DeepWater => "Deep Water",
        TileKind::Sand      => "Sand",
        TileKind::Rock      => "Rock",
        TileKind::Bridge    => "Bridge",
    }
}

fn unit_kind_short(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Peon     => "Peon",
        UnitKind::Grunt    => "Grunt",
        UnitKind::Archer   => "Archer",
        UnitKind::Catapult => "Catapult",
        UnitKind::TownHall => "Town Hall",
        UnitKind::Barracks => "Barracks",
    }
}

fn snap_to_tile_center(world_pos: Vec2) -> Vec2 {
    let tx = (world_pos.x / TILE_SIZE).floor();
    let ty = (world_pos.y / TILE_SIZE).floor();
    Vec2::new(tx * TILE_SIZE + TILE_SIZE * 0.5, ty * TILE_SIZE + TILE_SIZE * 0.5)
}

fn health_color(frac: f32) -> [f32; 4] {
    if frac > 0.5 { [0.15, 0.80, 0.15, 1.0] }
    else if frac > 0.25 { [0.85, 0.75, 0.10, 1.0] }
    else { [0.85, 0.15, 0.10, 1.0] }
}

fn lighten(c: [f32; 4], f: f32) -> [f32; 4] {
    [(c[0]*f).min(1.0), (c[1]*f).min(1.0), (c[2]*f).min(1.0), c[3]]
}

fn darken(c: [f32; 4], f: f32) -> [f32; 4] {
    [c[0]*f, c[1]*f, c[2]*f, c[3]]
}
