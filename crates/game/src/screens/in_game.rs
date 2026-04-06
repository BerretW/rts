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

    gold:       u32,
    lumber:     u32,
    oil:        u32,

    sprite_bg:  Option<engine::wgpu::BindGroup>,

    drag_start: Option<Vec2>,
    select_box: Option<Rect>,

    selected_hp:    Option<(i32, i32)>,
    selected_color: [f32; 4],
}

impl InGameScreen {
    pub fn new() -> Self {
        let lua = LuaRuntime::new().expect("Lua init selhala");

        let scripts_dir = locate_scripts_dir();
        if let Err(e) = lua.load_scripts(&scripts_dir) {
            log::error!("scripting: chyba při načítání skriptů: {e}");
        }

        let mut world = World::new();
        let map = create_demo_map();
        spawn_demo_units(&mut world, &lua);

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
        handle_move_orders(input, camera, &mut self.world, &self.lua);

        // ── Herní systémy ────────────────────────────────────────────────────
        let arrived        = movement_system(&mut self.world, &self.map, dt);
        let attack_events  = attack_system(&mut self.world, &self.map, dt);
        let production_done = production_system(&mut self.world, dt);
        let ai_ticks       = ai_tick_system(&mut self.world, dt);

        // on_unit_arrived
        for entity in arrived {
            if let Some(info) = unit_info(&self.world, entity) {
                if let Err(e) = self.lua.hook_unit_arrived(&info) {
                    log::error!("on_unit_arrived: {e}");
                }
            }
        }

        // on_unit_attack + on_unit_hit
        for ev in attack_events {
            let attacker_info = id_to_entity(ev.attacker_id)
                .and_then(|e| unit_info(&self.world, e));
            let target_info = id_to_entity(ev.target_id)
                .and_then(|e| unit_info(&self.world, e));
            if let (Some(a), Some(t)) = (attacker_info, target_info) {
                if let Err(e) = self.lua.hook_unit_attack(&a, &t, ev.damage) {
                    log::error!("on_unit_attack: {e}");
                }
                if let Err(e) = self.lua.hook_unit_hit(&t, ev.damage, ev.attacker_id) {
                    log::error!("on_unit_hit: {e}");
                }
            }
        }

        // Dokončené výroby – spawn jednotky + on_unit_trained
        for done in production_done {
            let spawned_entity = spawn_unit_by_kind(
                &mut self.world, &done.kind_id, done.rally, done.team,
            );
            // Trigger on_unit_spawned
            if let Some(info) = unit_info(&self.world, spawned_entity) {
                if let Err(e) = self.lua.hook_unit_spawned(&info) {
                    log::error!("on_unit_spawned: {e}");
                }
                if let Err(e) = self.lua.hook_unit_trained(&info, done.building_id) {
                    log::error!("on_unit_trained: {e}");
                }
            }
        }

        // ── Naplň query cache před AI tickem ─────────────────────────────────
        let all_units = collect_all_unit_infos(&self.world);
        if let Err(e) = self.lua.push_query_results(all_units.clone()) {
            log::error!("push_query_results: {e}");
        }
        if let Err(e) = self.lua.push_unit_cache(&all_units) {
            log::error!("push_unit_cache: {e}");
        }

        // AI ticky
        for tick in ai_ticks {
            if let Some(entity) = id_to_entity(tick.entity_id) {
                if let Some(info) = unit_info(&self.world, entity) {
                    if let Err(e) = self.lua.hook_ai_tick(&info, &tick.script_id, dt) {
                        log::error!("on_ai_tick [{}]: {e}", tick.script_id);
                    }
                }
            }
        }

        // Globální herní tick
        if let Err(e) = self.lua.hook_game_tick(dt) {
            log::error!("on_game_tick: {e}");
        }

        // Cleanup mrtvých – snapshoty před despawnem
        let dead = cleanup_dead(&mut self.world);
        for d in dead {
            let stub = UnitInfo {
                entity_id:    d.id,
                x:            d.pos.x,
                y:            d.pos.y,
                hp:           0,
                hp_max:       1,
                damage:       0,
                pierce:       0,
                armor:        0,
                attack_range: 0.0,
                team:         d.team,
                kind_id:      d.kind_id,
            };
            if let Err(e) = self.lua.hook_unit_died(&stub) {
                log::error!("on_unit_died: {e}");
            }
        }

        // Zpracuj příkazy z Lua skriptů
        match self.lua.drain_commands() {
            Ok(cmds) => {
                for cmd in cmds { self.apply_cmd(cmd); }
            }
            Err(e) => log::error!("drain_commands: {e}"),
        }

        // Fog of war
        for (_e, (pos, sight)) in self.world.query_mut::<(&Position, &Sight)>() {
            self.map.reveal_circle(pos.0, sight.0);
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

        // Výběr
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
            .map(|(_, (p, h, s))| (p.0, h.fraction(), s.size.y))
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

// ── ID ↔ Entity ───────────────────────────────────────────────────────────────

fn id_to_entity(id: u64) -> Option<hecs::Entity> {
    hecs::Entity::from_bits(id)
}

// ── UnitInfo snapshot ─────────────────────────────────────────────────────────

fn unit_info(world: &World, entity: hecs::Entity) -> Option<UnitInfo> {
    let pos  = world.get::<&Position>(entity).ok()?;
    let hp   = world.get::<&Health>(entity).ok()?;
    let team = world.get::<&Team>(entity).ok()?;

    // Preferuj UnitKindId (nový systém), fallback na enum Unit
    let kind_id: String = if let Ok(k) = world.get::<&UnitKindId>(entity) {
        k.0.clone()
    } else if let Ok(u) = world.get::<&Unit>(entity) {
        unit_kind_str(u.0)
    } else {
        "unknown".into()
    };

    // Bojové statistiky (volitelné)
    let (damage, pierce, armor, attack_range) =
        if let Ok(s) = world.get::<&AttackStats>(entity) {
            (s.damage, s.pierce, s.armor, s.range)
        } else {
            (0, 0, 0, 0.0)
        };

    Some(UnitInfo {
        entity_id:    entity.to_bits().into(),
        x:            pos.0.x,
        y:            pos.0.y,
        hp:           hp.current,
        hp_max:       hp.max,
        damage,
        pierce,
        armor,
        attack_range,
        team:         team.0,
        kind_id,
    })
}

/// Sestaví snapshot všech živých entit pro Lua query.
fn collect_all_unit_infos(world: &World) -> Vec<UnitInfo> {
    world.query::<(&Position, &Health, &Team)>()
        .iter()
        .filter(|(_, (_, hp, _))| hp.is_alive())
        .filter_map(|(e, _)| unit_info(world, e))
        .collect()
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

// ── Aplikace ScriptCmd ────────────────────────────────────────────────────────

impl InGameScreen {
    fn apply_cmd(&mut self, cmd: ScriptCmd) {
        match cmd {
            ScriptCmd::MoveUnit { entity_id, target_x, target_y, params } => {
                if let Some(e) = id_to_entity(entity_id) {
                    let flags = MoveFlags::from(params.clone());
                    let _ = self.world.remove_one::<MoveOrder>(e);
                    let _ = self.world.insert_one(e, MoveOrder {
                        target: Vec2::new(target_x, target_y),
                        speed:  params.speed,
                        flags,
                    });
                }
            }

            ScriptCmd::AttackUnit { attacker_id, target_id } => {
                if let (Some(attacker), Some(target)) =
                    (id_to_entity(attacker_id), id_to_entity(target_id))
                {
                    let _ = self.world.remove_one::<AttackOrder>(attacker);
                    let _ = self.world.insert_one(attacker, AttackOrder { target });
                }
            }

            ScriptCmd::StopUnit { entity_id } => {
                if let Some(e) = id_to_entity(entity_id) {
                    let _ = self.world.remove_one::<MoveOrder>(e);
                    let _ = self.world.remove_one::<AttackOrder>(e);
                    if let Ok(mut vel) = self.world.get::<&mut Velocity>(e) {
                        vel.0 = Vec2::ZERO;
                    }
                }
            }

            ScriptCmd::SetHealth { entity_id, hp } => {
                if let Some(e) = id_to_entity(entity_id) {
                    if let Ok(mut h) = self.world.get::<&mut Health>(e) {
                        h.current = hp.clamp(0, h.max);
                    }
                }
            }

            ScriptCmd::KillUnit { entity_id } => {
                if let Some(e) = id_to_entity(entity_id) {
                    if let Ok(mut h) = self.world.get::<&mut Health>(e) {
                        h.current = 0;
                    }
                }
            }

            ScriptCmd::AddResources { gold, lumber, oil } => {
                let prev_gold   = self.gold;
                let prev_lumber = self.lumber;
                let prev_oil    = self.oil;
                self.gold   = (self.gold   as i32 + gold)  .max(0) as u32;
                self.lumber = (self.lumber as i32 + lumber).max(0) as u32;
                self.oil    = (self.oil    as i32 + oil)   .max(0) as u32;
                if self.gold != prev_gold || self.lumber != prev_lumber || self.oil != prev_oil {
                    if let Err(e) = self.lua.hook_resource_changed(self.gold, self.lumber, self.oil) {
                        log::error!("on_resource_changed: {e}");
                    }
                }
            }

            ScriptCmd::SpawnUnit { kind_id, x, y, team } => {
                let e = spawn_unit_by_kind(&mut self.world, &kind_id, Vec2::new(x, y), team);
                if let Some(info) = unit_info(&self.world, e) {
                    if let Err(err) = self.lua.hook_unit_spawned(&info) {
                        log::error!("on_unit_spawned: {err}");
                    }
                }
            }

            ScriptCmd::TrainUnit { building_id, kind_id, build_time } => {
                if let Some(e) = id_to_entity(building_id) {
                    if let Ok(mut pq) = self.world.get::<&mut ProductionQueue>(e) {
                        // build_time == 0 znamená použít výchozí
                        let time = if build_time > 0.0 { build_time } else { 30.0 };
                        if pq.current.is_none() {
                            pq.current = Some((kind_id, time));
                        } else {
                            pq.enqueue(kind_id);
                        }
                    }
                }
            }

            ScriptCmd::SetRally { building_id, x, y } => {
                if let Some(e) = id_to_entity(building_id) {
                    if let Ok(mut pq) = self.world.get::<&mut ProductionQueue>(e) {
                        pq.rally = Vec2::new(x, y);
                    }
                }
            }

            ScriptCmd::SetAi { entity_id, script_id, tick_interval } => {
                if let Some(e) = id_to_entity(entity_id) {
                    let ctrl = AiController::new(script_id, tick_interval.max(0.1));
                    let _ = self.world.remove_one::<AiController>(e);
                    let _ = self.world.insert_one(e, ctrl);
                }
            }

            ScriptCmd::SetAiState { entity_id, state_json } => {
                if let Some(e) = id_to_entity(entity_id) {
                    if let Ok(mut ctrl) = self.world.get::<&mut AiController>(e) {
                        ctrl.state_json = state_json;
                    }
                }
            }
        }
    }
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

/// Spawnuje jednotku podle string kind_id. Vrátí vytvořenou entitu.
fn spawn_unit_by_kind(world: &mut World, kind_id: &str, pos: Vec2, team: u8) -> hecs::Entity {
    let color = team_color(team);

    struct UnitDef {
        col: u32, row: u32, size: f32, hp: i32,
        damage: i32, pierce: i32, armor: i32, range: f32, cd: f32,
        speed: f32, ai: &'static str, sight: u32,
    }

    let d = match kind_id {
        "peasant" => UnitDef { col:1, row:0, size:32., hp:30,  damage:3,  pierce:0, armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai", sight:4 },
        "footman" => UnitDef { col:2, row:0, size:32., hp:60,  damage:6,  pierce:3, armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai",  sight:4 },
        "archer"  => UnitDef { col:3, row:0, size:32., hp:40,  damage:3,  pierce:6, armor:0, range:128., cd:1.0, speed:128., ai:"ranged_ai", sight:5 },
        "knight"  => UnitDef { col:4, row:0, size:32., hp:90,  damage:8,  pierce:4, armor:4, range:0.,   cd:1.0, speed:192., ai:"melee_ai",  sight:4 },
        "mage"    => UnitDef { col:5, row:0, size:32., hp:35,  damage:0,  pierce:9, armor:0, range:160., cd:1.5, speed:128., ai:"ranged_ai", sight:9 },
        "peon"    => UnitDef { col:1, row:1, size:32., hp:30,  damage:3,  pierce:0, armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai", sight:4 },
        "grunt"   => UnitDef { col:2, row:1, size:32., hp:60,  damage:8,  pierce:2, armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai",  sight:4 },
        "troll_axethrower" => UnitDef { col:3, row:1, size:32., hp:40, damage:3, pierce:6, armor:0, range:128., cd:1.0, speed:128., ai:"ranged_ai", sight:5 },
        "ogre"    => UnitDef { col:4, row:1, size:32., hp:90,  damage:10, pierce:2, armor:4, range:0.,   cd:1.3, speed:128., ai:"melee_ai",  sight:4 },
        "death_knight" => UnitDef { col:5, row:1, size:32., hp:60, damage:0, pierce:9, armor:0, range:160., cd:1.5, speed:192., ai:"ranged_ai", sight:9 },
        _         => UnitDef { col:1, row:0, size:32., hp:30,  damage:3,  pierce:0, armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai", sight:4 },
    };

    let flags = MoveFlags {
        can_swim: false, can_fly: false,
        speed_water: 0.0, speed_forest: 0.75, speed_road: 1.0,
    };

    world.spawn((
        Position(pos),
        Velocity(Vec2::ZERO),
        Sprite { col: d.col, row: d.row, size: Vec2::splat(d.size), color },
        Team(team),
        Health::new(d.hp),
        UnitKindId(kind_id.to_string()),
        Sight(d.sight),
        flags.clone(),
        AttackStats {
            damage:       d.damage,
            pierce:       d.pierce,
            armor:        d.armor,
            range:        d.range,
            cooldown:     d.cd,
            cooldown_left: 0.0,
        },
        AiController::new(d.ai, 0.5),
    ))
}

fn team_color(team: u8) -> [f32; 4] {
    match team {
        0 => [0.20, 0.45, 1.00, 1.0],
        1 => [0.80, 0.20, 0.10, 1.0],
        2 => [0.10, 0.70, 0.20, 1.0],
        _ => [0.70, 0.70, 0.10, 1.0],
    }
}

// ── Pohybové rozkazy ──────────────────────────────────────────────────────────

fn handle_move_orders(input: &Input, camera: &Camera, world: &mut World, lua: &LuaRuntime) {
    if !input.mouse_just_pressed(MouseButton::Right) { return; }

    let target = camera.screen_to_world(input.mouse_pos);
    let sel: Vec<_> = world.query::<()>().with::<&Selected>().iter()
        .map(|(e, _)| e).collect();

    for (i, &entity) in sel.iter().enumerate() {
        let ox = (i as i32 % 5 - 2) as f32 * TILE_SIZE;
        let oy = (i as i32 / 5 - 1) as f32 * TILE_SIZE;
        let t  = target + Vec2::new(ox, oy);

        let Some(info) = unit_info(world, entity) else { continue };
        let default_params = MoveParams::default();

        match lua.hook_move_order(&info, t.x, t.y, default_params) {
            Ok(Some(ScriptCmd::MoveUnit { target_x, target_y, params, .. })) => {
                let flags = MoveFlags::from(params.clone());
                let _ = world.remove_one::<MoveOrder>(entity);
                let _ = world.insert_one(entity, MoveOrder {
                    target: Vec2::new(target_x, target_y),
                    speed:  params.speed,
                    flags,
                });
            }
            Ok(None) => {
                log::debug!("on_move_order zablokoval pohyb {:?}", entity);
            }
            Ok(Some(_)) => {}
            Err(e) => log::error!("on_move_order: {e}"),
        }
    }
}

// ── Demo scéna ────────────────────────────────────────────────────────────────

fn spawn_demo_units(world: &mut World, lua: &LuaRuntime) {
    // Hráč (team 0) – základna + jednotky
    let p0_base = Vec2::new(5.0 * TILE_SIZE, 35.0 * TILE_SIZE);
    let p1_base = Vec2::new(50.0 * TILE_SIZE, 5.0 * TILE_SIZE);

    // Town Hall hráče 0 (budova – bez AI, s ProductionQueue)
    let th0 = world.spawn((
        Position(p0_base),
        Velocity(Vec2::ZERO),
        Sprite { col: 0, row: 0, size: Vec2::splat(64.0), color: [0.20, 0.45, 1.0, 1.0] },
        Team(0u8),
        Health::new(1200),
        UnitKindId("town_hall".into()),
        Sight(6u32),
        ProductionQueue::new(5),
    ));
    // Rally point – vedle základny
    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(th0) {
        pq.rally = p0_base + Vec2::new(80.0, 0.0);
    }

    // Pěší hráče 0
    for i in 0..2 {
        let pos = p0_base + Vec2::new((i as f32 + 2.0) * TILE_SIZE, TILE_SIZE * 3.0);
        spawn_unit_by_kind(world, "peon", pos, 0);
    }
    spawn_unit_by_kind(world, "footman", p0_base + Vec2::new(3.0 * TILE_SIZE, 4.0 * TILE_SIZE), 0);
    spawn_unit_by_kind(world, "archer",  p0_base + Vec2::new(4.0 * TILE_SIZE, 3.5 * TILE_SIZE), 0);

    // Town Hall nepřítele (team 1)
    world.spawn((
        Position(p1_base),
        Velocity(Vec2::ZERO),
        Sprite { col: 0, row: 0, size: Vec2::splat(64.0), color: [0.80, 0.20, 0.10, 1.0] },
        Team(1u8),
        Health::new(1200),
        UnitKindId("great_hall".into()),
        Sight(6u32),
        ProductionQueue::new(5),
    ));

    // Jednotky nepřítele
    spawn_unit_by_kind(world, "grunt",  p1_base + Vec2::new(2.0 * TILE_SIZE, 3.0 * TILE_SIZE), 1);
    spawn_unit_by_kind(world, "grunt",  p1_base + Vec2::new(3.0 * TILE_SIZE, 3.0 * TILE_SIZE), 1);
    spawn_unit_by_kind(world, "troll_axethrower", p1_base + Vec2::new(4.0 * TILE_SIZE, 2.5 * TILE_SIZE), 1);

    // Triggerneme on_unit_spawned pro všechny takto vzniklé entity
    let entities: Vec<hecs::Entity> = world.query::<()>().iter().map(|(e, _)| e).collect();
    for entity in entities {
        if let Some(info) = unit_info(world, entity) {
            if let Err(e) = lua.hook_unit_spawned(&info) {
                log::error!("on_unit_spawned (init): {e}");
            }
        }
    }
}

// ── Kamera ────────────────────────────────────────────────────────────────────

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

// ── Výběr ─────────────────────────────────────────────────────────────────────

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

// ── Mapa ──────────────────────────────────────────────────────────────────────

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

fn locate_scripts_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.parent().unwrap_or(std::path::Path::new(".")).join("scripts");
        if c.exists() { return c; }
    }
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
