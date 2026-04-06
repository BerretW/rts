/// Hlavní herní obrazovka.

use glam::Vec2;
use hecs::World;

use engine::{
    Rect, UvRect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    tilemap::{TileKind, TileMap, TILE_SIZE},
    ui::UiCtx,
};
use engine::winit::keyboard::KeyCode;
use engine::winit::event::MouseButton;

use crate::components::*;
use crate::systems::*;
use crate::scripting::{LuaRuntime, MoveParams, ScriptCmd, UnitInfo};

use super::{Screen, Transition};

const SHEET_COLS: u32 = 8;
const SHEET_ROWS: u32 = 8;
const CAM_PAN_SPEED: f32 = 400.0;
const ZOOM_FACTOR:   f32 = 1.15;

// ── Stav herní obrazovky ─────────────────────────────────────────────────────

pub struct InGameScreen {
    world:      World,
    map:        TileMap,
    lua:        LuaRuntime,

    // Suroviny hráče
    gold:       u32,
    lumber:     u32,
    oil:        u32,

    // GPU
    sprite_bg:  Option<engine::wgpu::BindGroup>,

    // Výběr
    drag_start: Option<Vec2>,
    select_box: Option<Rect>,

    // Panel informací
    selected_hp:     Option<(i32, i32)>,
    selected_color:  [f32; 4],
}

impl InGameScreen {
    pub fn new() -> Self {
        let lua = LuaRuntime::new().expect("Lua init selhala");

        // Načti scripty ze složky scripts/ vedle exe nebo v rootu projektu
        let scripts_dir = locate_scripts_dir();
        if let Err(e) = lua.load_scripts(&scripts_dir) {
            log::error!("scripting: chyba při načítání skriptů: {e}");
        }

        let mut world = World::new();
        let map   = create_demo_map();
        spawn_demo_units(&mut world);

        Self {
            world,
            map,
            lua,
            gold:   2000,
            lumber: 1000,
            oil:    0,
            sprite_bg:      None,
            drag_start:     None,
            select_box:     None,
            selected_hp:    None,
            selected_color: [1.0; 4],
        }
    }
}

impl Screen for InGameScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        let bg  = tex.create_bind_group(ctx, &batch.texture_bind_group_layout);
        self.sprite_bg = Some(bg);
    }

    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera) -> Transition {
        handle_camera(dt, input, camera);
        handle_selection(input, camera, &mut self.world,
                         &mut self.drag_start, &mut self.select_box);

        // Pohybové rozkazy – projdou Lua hook on_move_order
        handle_move_orders(input, camera, &mut self.world, &self.lua);

        // Pohybový systém – vrátí entity, které dorazily
        let arrived = movement_system(&mut self.world, &self.map, dt);

        // on_unit_arrived hooky
        for entity in arrived {
            let info = unit_info(&self.world, entity);
            if let Some(info) = info {
                if let Err(e) = self.lua.hook_unit_arrived(&info) {
                    log::error!("on_unit_arrived: {e}");
                }
            }
        }

        // Cleanup mrtvých – vrátí jejich bits
        let dead_ids = cleanup_dead(&mut self.world);

        // on_unit_died hooky (entity už neexistují, předáme uloženou info – stub)
        for id in dead_ids {
            let stub = UnitInfo {
                entity_id: id,
                x: 0.0, y: 0.0, hp: 0, hp_max: 1,
                team: 0,
                kind_id: "unknown".into(),
            };
            if let Err(e) = self.lua.hook_unit_died(&stub) {
                log::error!("on_unit_died: {e}");
            }
        }

        // Zpracuj příkazy nagenerované Lua skripty
        match self.lua.drain_commands() {
            Ok(cmds) => {
                for cmd in cmds {
                    self.apply_cmd(cmd);
                }
            }
            Err(e) => log::error!("drain_commands: {e}"),
        }

        // Fog of war
        for (_e, (pos, team)) in self.world.query_mut::<(&Position, &Team)>() {
            if team.0 == 0 {
                self.map.reveal_circle(pos.0, 5);
            }
        }

        // Refresh info panelu
        self.selected_hp    = None;
        self.selected_color = [1.0; 4];
        for (_e, (hp, sprite, _sel)) in self.world
            .query::<(&Health, &Sprite, &Selected)>().iter()
        {
            self.selected_hp    = Some((hp.current, hp.max));
            self.selected_color = sprite.color;
            break;
        }

        if input.key_just_pressed(KeyCode::Escape) {
            use super::main_menu::MainMenuScreen;
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        Transition::None
    }

    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera) {
        let view_half = camera.viewport() * 0.5 / camera.zoom;
        let view_rect = Rect::new(
            camera.position.x - view_half.x,
            camera.position.y - view_half.y,
            view_half.x * 2.0,
            view_half.y * 2.0,
        );

        // Terrain
        for (tx, ty) in self.map.visible_tiles(view_rect) {
            let tile = match self.map.get(tx, ty) { Some(t) => t, None => continue };
            let dst  = self.map.tile_rect(tx, ty);
            let uv   = UvRect::from_tile(tile.kind.sheet_pos().0, tile.kind.sheet_pos().1,
                                         SHEET_COLS, SHEET_ROWS);
            let color = if tile.visible {
                tile_color(tile.kind)
            } else if tile.explored {
                darken(tile_color(tile.kind), 0.4)
            } else {
                continue;
            };
            batch.draw(dst, uv, color);
        }

        // Entity
        for (_e, (pos, sprite)) in self.world.query::<(&Position, &Sprite)>().iter() {
            let half = sprite.size * 0.5;
            let dst  = Rect::new(pos.0.x - half.x, pos.0.y - half.y, sprite.size.x, sprite.size.y);
            let uv   = UvRect::from_tile(sprite.col, sprite.row, SHEET_COLS, SHEET_ROWS);
            batch.draw(dst, uv, sprite.color);
        }

        // Označení vybrané entity
        for (_e, (pos, sprite, _sel)) in self.world.query::<(&Position, &Sprite, &Selected)>().iter() {
            let half = sprite.size * 0.5 + Vec2::splat(3.0);
            let rect = Rect::new(pos.0.x - half.x, pos.0.y - half.y, half.x*2.0, half.y*2.0);
            let tw   = 2.0 / camera.zoom;
            let uv   = UvRect::FULL;
            let col  = [0.2, 1.0, 0.2, 0.9];
            batch.draw(Rect::new(rect.x, rect.y, rect.w, tw), uv, col);
            batch.draw(Rect::new(rect.x, rect.y+rect.h-tw, rect.w, tw), uv, col);
            batch.draw(Rect::new(rect.x, rect.y, tw, rect.h), uv, col);
            batch.draw(Rect::new(rect.x+rect.w-tw, rect.y, tw, rect.h), uv, col);
        }

        // Selection box
        if let Some(sbox) = self.select_box {
            let tw = 1.5 / camera.zoom;
            let uv = UvRect::FULL;
            let col = [0.2, 1.0, 0.2, 0.7];
            batch.draw(Rect::new(sbox.x, sbox.y, sbox.w, tw), uv, col);
            batch.draw(Rect::new(sbox.x, sbox.y+sbox.h-tw, sbox.w, tw), uv, col);
            batch.draw(Rect::new(sbox.x, sbox.y, tw, sbox.h), uv, col);
            batch.draw(Rect::new(sbox.x+sbox.w-tw, sbox.y, tw, sbox.h), uv, col);
        }
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        ui.resource_bar(self.gold, self.lumber, self.oil);

        let positions: Vec<(Vec2, f32, f32)> = self.world
            .query::<(&Position, &Health, &Sprite)>()
            .iter()
            .map(|(_, (p, h, s))| (p.0, h.current as f32 / h.max as f32, s.size.y))
            .collect();
        let cam = dummy_camera();
        for (pos, frac, size) in positions {
            ui.health_bar_world(pos, size, frac, cam);
        }

        ui.minimap_placeholder(self.map.width, self.map.height);

        if let Some((cur, max)) = self.selected_hp {
            let frac = cur as f32 / max as f32;
            ui.info_panel(self.selected_color, frac, max);
        }

        let sw = ui.screen.x;
        ui.panel(Rect::new(sw - 90.0, 4.0, 80.0, 20.0), [0.1, 0.1, 0.12, 0.8]);
        ui.panel(Rect::new(sw - 85.0, 8.0, 12.0, 12.0), [0.5, 0.3, 0.3, 1.0]);
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.sprite_bg.as_ref().expect("InGameScreen::init not called")
    }
}

// ── Aplikace ScriptCmd ────────────────────────────────────────────────────────

impl InGameScreen {
    fn apply_cmd(&mut self, cmd: ScriptCmd) {
        match cmd {
            ScriptCmd::MoveUnit { entity_id, target_x, target_y, params } => {
                let entity = std::num::NonZeroU64::new(entity_id).map(hecs::Entity::from_bits);
                if let Some(entity) = entity {
                    let flags = crate::components::MoveFlags::from(params.clone());
                    let _ = self.world.remove_one::<MoveOrder>(entity);
                    let _ = self.world.insert_one(entity, MoveOrder {
                        target: Vec2::new(target_x, target_y),
                        speed:  params.speed,
                        flags,
                    });
                }
            }
            ScriptCmd::SetHealth { entity_id, hp } => {
                let entity = std::num::NonZeroU64::new(entity_id).map(hecs::Entity::from_bits);
                if let Some(entity) = entity {
                    if let Ok(mut h) = self.world.get::<&mut Health>(entity) {
                        h.current = hp.clamp(0, h.max);
                    }
                }
            }
            ScriptCmd::KillUnit { entity_id } => {
                let entity = std::num::NonZeroU64::new(entity_id).map(hecs::Entity::from_bits);
                if let Some(entity) = entity {
                    if let Ok(mut h) = self.world.get::<&mut Health>(entity) {
                        h.current = 0;
                    }
                }
            }
            ScriptCmd::AddResources { gold, lumber, oil } => {
                self.gold   = (self.gold   as i32 + gold)  .max(0) as u32;
                self.lumber = (self.lumber as i32 + lumber).max(0) as u32;
                self.oil    = (self.oil    as i32 + oil)   .max(0) as u32;
            }
            ScriptCmd::SpawnUnit { kind_id, x, y, team } => {
                spawn_unit_by_kind(&mut self.world, &kind_id, Vec2::new(x, y), team);
            }
        }
    }
}

// ── Pomocné funkce ────────────────────────────────────────────────────────────

/// Sestaví UnitInfo snapshot pro Lua hook.
fn unit_info(world: &World, entity: hecs::Entity) -> Option<UnitInfo> {
    let pos  = world.get::<&Position>(entity).ok()?;
    let hp   = world.get::<&Health>(entity).ok()?;
    let team = world.get::<&Team>(entity).ok()?;
    let unit = world.get::<&Unit>(entity).ok()?;
    Some(UnitInfo {
        entity_id: entity.to_bits().into(),
        x:         pos.0.x,
        y:         pos.0.y,
        hp:        hp.current,
        hp_max:    hp.max,
        team:      team.0,
        kind_id:   unit_kind_str(unit.0),
    })
}

fn unit_kind_str(k: UnitKind) -> String {
    match k {
        UnitKind::Peon     => "peon",
        UnitKind::Grunt    => "grunt",
        UnitKind::Archer   => "archer",
        UnitKind::Catapult => "catapult",
        UnitKind::TownHall => "town_hall",
        UnitKind::Barracks => "barracks",
    }.to_string()
}

/// Spawnuje jednotku podle string kind_id (z Lua SpawnUnit příkazu).
fn spawn_unit_by_kind(world: &mut World, kind_id: &str, pos: Vec2, team: u8) {
    let (col, row, size, hp, kind) = match kind_id {
        "peon"   => (1u32, 1u32, 32.0f32, 30i32, UnitKind::Peon),
        "grunt"  => (2,    1,    32.0,    60,    UnitKind::Grunt),
        "archer" => (3,    0,    32.0,    40,    UnitKind::Archer),
        _        => (1,    0,    32.0,    30,    UnitKind::Peon),
    };
    let color = if team == 0 { [0.20, 0.45, 1.0, 1.0] } else { [0.80, 0.20, 0.10, 1.0] };
    world.spawn((
        Position(pos), Velocity(Vec2::ZERO),
        Sprite { col, row, size: Vec2::splat(size), color },
        Team(team),
        Health::new(hp),
        Unit(kind),
    ));
}

/// Pohybové rozkazy přes Lua hook.
fn handle_move_orders(input: &Input, camera: &Camera, world: &mut World, lua: &LuaRuntime) {
    if !input.mouse_just_pressed(MouseButton::Right) { return; }

    let target = camera.screen_to_world(input.mouse_pos);
    let sel: Vec<_> = world.query::<()>().with::<&Selected>().iter()
        .map(|(e, _)| e).collect();

    for (i, &entity) in sel.iter().enumerate() {
        let ox = (i as i32 % 5 - 2) as f32 * TILE_SIZE;
        let oy = (i as i32 / 5 - 1) as f32 * TILE_SIZE;
        let t  = target + Vec2::new(ox, oy);

        // Sestav UnitInfo snapshot
        let Some(info) = unit_info(world, entity) else { continue };

        // Výchozí parametry – Lua on_move_order je může přepsat
        let default_params = MoveParams::default();

        match lua.hook_move_order(&info, t.x, t.y, default_params) {
            Ok(Some(ScriptCmd::MoveUnit { target_x, target_y, params, .. })) => {
                let flags = crate::components::MoveFlags::from(params.clone());
                let _ = world.remove_one::<MoveOrder>(entity);
                let _ = world.insert_one(entity, MoveOrder {
                    target: Vec2::new(target_x, target_y),
                    speed:  params.speed,
                    flags,
                });
            }
            Ok(None) => {
                // Hook pohyb zablokoval
                log::debug!("on_move_order zablokoval pohyb entity {:?}", entity);
            }
            Ok(Some(_)) => {} // jiný cmd typ – ignoruj
            Err(e) => log::error!("on_move_order: {e}"),
        }
    }
}

fn dummy_camera() -> &'static engine::camera::Camera {
    static DUMMY: std::sync::OnceLock<engine::camera::Camera> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| engine::camera::Camera::new(1280.0, 720.0))
}

fn handle_camera(dt: f32, input: &Input, camera: &mut Camera) {
    let mut dir = Vec2::ZERO;
    if input.key_held(KeyCode::ArrowLeft)  || input.key_held(KeyCode::KeyA) { dir.x -= 1.0; }
    if input.key_held(KeyCode::ArrowRight) || input.key_held(KeyCode::KeyD) { dir.x += 1.0; }
    if input.key_held(KeyCode::ArrowUp)    || input.key_held(KeyCode::KeyW) { dir.y -= 1.0; }
    if input.key_held(KeyCode::ArrowDown)  || input.key_held(KeyCode::KeyS) { dir.y += 1.0; }
    if dir != Vec2::ZERO {
        camera.pan(dir.normalize() * CAM_PAN_SPEED * dt / camera.zoom);
    }
    if input.scroll_delta != 0.0 {
        let factor = if input.scroll_delta > 0.0 { ZOOM_FACTOR } else { 1.0 / ZOOM_FACTOR };
        camera.zoom_around(factor, input.mouse_pos);
    }
    if input.mouse_held(MouseButton::Middle) {
        camera.pan(-input.mouse_delta / camera.zoom);
    }
}

fn handle_selection(
    input: &Input, camera: &Camera, world: &mut World,
    drag_start: &mut Option<Vec2>, select_box: &mut Option<Rect>,
) {
    let mw = camera.screen_to_world(input.mouse_pos);
    if input.mouse_just_pressed(MouseButton::Left) { *drag_start = Some(mw); }
    if input.mouse_held(MouseButton::Left) {
        if let Some(start) = *drag_start {
            let (x0, y0) = (start.x.min(mw.x), start.y.min(mw.y));
            let (x1, y1) = (start.x.max(mw.x), start.y.max(mw.y));
            *select_box = Some(Rect::new(x0, y0, x1-x0, y1-y0));
        }
    }
    if input.mouse_just_released(MouseButton::Left) {
        if let Some(sbox) = select_box.take() {
            let prev: Vec<_> = world.query::<()>().with::<&Selected>().iter()
                .map(|(e,_)| e).collect();
            for e in prev { let _ = world.remove_one::<Selected>(e); }
            let sel: Vec<_> = world.query::<&Position>().iter()
                .filter(|(_,p)| sbox.contains(p.0)).map(|(e,_)| e).collect();
            for e in sel  { let _ = world.insert_one(e, Selected); }
        }
        *drag_start = None;
        *select_box = None;
    }
}

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

fn darken(c: [f32; 4], f: f32) -> [f32; 4] {
    [c[0]*f, c[1]*f, c[2]*f, c[3]]
}

/// Zkusí najít složku `scripts/` – nejprve vedle exe, pak od working directory.
fn locate_scripts_dir() -> std::path::PathBuf {
    // 1) Vedle spustitelného souboru
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent().unwrap_or(std::path::Path::new(".")).join("scripts");
        if candidate.exists() { return candidate; }
    }
    // 2) Pracovní adresář (cargo run z rootu projektu)
    std::path::PathBuf::from("scripts")
}

fn create_demo_map() -> TileMap {
    use TileKind::*;
    let w = 64u32; let h = 64u32;
    let mut map = TileMap::new_filled(w, h, Grass);
    for y in 20..28 { for x in 0..w { map.set(x, y, Water); } }
    for y in 22..26 { for x in 0..w { map.set(x, y, DeepWater); } }
    for y in 20..28 { map.set(32, y, Bridge); map.set(33, y, Bridge); }
    for y in 5..12  { for x in 10..30 { map.set(x, y, Forest); } }
    for &(x,y) in &[(40u32,15u32),(41,15),(40,16),(50,10),(51,10),(52,10)] { map.set(x,y,Rock); }
    for y in 30..50 { for x in 0..20 {
        if let Some(t) = map.get_mut(x, y) { t.visible = true; t.explored = true; }
    }}
    map
}

fn spawn_demo_units(world: &mut World) {
    let defs = vec![
        (Vec2::new( 5.0*TILE_SIZE, 35.0*TILE_SIZE), 0u8, 0u32, 1u32, UnitKind::TownHall, 64.0, [0.20,0.45,1.0,1.0f32]),
        (Vec2::new( 8.0*TILE_SIZE, 38.0*TILE_SIZE), 0,   1,    0,    UnitKind::Peon,     32.0, [0.20,0.45,1.0,1.0]),
        (Vec2::new( 9.0*TILE_SIZE, 38.0*TILE_SIZE), 0,   1,    0,    UnitKind::Peon,     32.0, [0.20,0.45,1.0,1.0]),
        (Vec2::new( 7.0*TILE_SIZE, 40.0*TILE_SIZE), 0,   2,    0,    UnitKind::Grunt,    32.0, [0.20,0.45,1.0,1.0]),
        (Vec2::new(50.0*TILE_SIZE,  5.0*TILE_SIZE), 1,   0,    1,    UnitKind::TownHall, 64.0, [0.80,0.20,0.10,1.0]),
        (Vec2::new(52.0*TILE_SIZE,  8.0*TILE_SIZE), 1,   2,    1,    UnitKind::Grunt,    32.0, [0.80,0.20,0.10,1.0]),
    ];
    for (pos, team, col, row, kind, sz, color) in defs {
        world.spawn((
            Position(pos), Velocity(Vec2::ZERO),
            Sprite { col, row, size: Vec2::splat(sz), color },
            Team(team),
            Health::new(if sz > 32.0 { 1000 } else { 100 }),
            Unit(kind),
        ));
    }
}
