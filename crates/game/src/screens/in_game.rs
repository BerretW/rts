/// Hlavní herní obrazovka – singleplayer.

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
use crate::systems::*;
use crate::scripting::{LuaRuntime, MoveParams, ScriptCmd, UnitInfo};

use super::{Screen, Transition};

const SHEET_COLS: u32 = 8;
const SHEET_ROWS: u32 = 8;
const CAM_PAN_SPEED: f32 = 400.0;
const ZOOM_FACTOR:   f32 = 1.15;

// ── Výsledek hry ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum GameResult { Win, Lose }

// ── Volby výroby pro každý typ budovy ────────────────────────────────────────

struct TrainOption {
    kind_id: &'static str,
    name:    &'static str,
    gold:    u32,
    lumber:  u32,
    time:    f32,
}

fn building_train_options(kind_id: &str) -> &'static [TrainOption] {
    match kind_id {
        "town_hall" | "keep" | "castle" => &[
            TrainOption { kind_id: "peasant",  name: "Peasant",  gold: 400, lumber:   0, time: 15.0 },
        ],
        "great_hall" | "stronghold" | "fortress" => &[
            TrainOption { kind_id: "peon",     name: "Peon",     gold: 400, lumber:   0, time: 15.0 },
        ],
        "barracks" => &[
            TrainOption { kind_id: "footman",  name: "Footman",  gold: 600, lumber:   0, time: 20.0 },
            TrainOption { kind_id: "archer",   name: "Archer",   gold: 500, lumber:  50, time: 20.0 },
            TrainOption { kind_id: "knight",   name: "Knight",   gold: 800, lumber: 100, time: 30.0 },
        ],
        "orc_barracks" => &[
            TrainOption { kind_id: "grunt",    name: "Grunt",    gold: 600, lumber:   0, time: 20.0 },
            TrainOption { kind_id: "troll_axethrower", name: "Troll", gold: 500, lumber: 50, time: 20.0 },
            TrainOption { kind_id: "ogre",     name: "Ogre",     gold: 800, lumber: 100, time: 30.0 },
        ],
        _ => &[],
    }
}

// ── Stav herní obrazovky ─────────────────────────────────────────────────────

pub struct InGameScreen {
    world:      World,
    map:        TileMap,
    lua:        LuaRuntime,

    gold:       u32,
    lumber:     u32,
    oil:        u32,

    sprite_bg:  Option<engine::wgpu::BindGroup>,

    // Výběr
    drag_start:      Option<Vec2>,
    select_box:      Option<Rect>,
    selected_entity: Option<hecs::Entity>,

    // Info panel vybrané entity
    selected_hp:    Option<(i32, i32)>,
    selected_color: [f32; 4],

    // Stav hry
    paused:      bool,
    game_result: Option<GameResult>,

    // Jídlo (populace)
    food_used: u32,
    food_max:  u32,

    // Flag: vrátit se na hlavní menu (nastaven z render_ui)
    pending_to_menu: bool,
}

impl InGameScreen {
    pub fn new() -> Self {
        let lua = LuaRuntime::new().expect("Lua init selhala");

        let resources_dir = locate_resources_dir();
        if resources_dir.join("[base]").exists() || resources_dir.join("fxmanifest.lua").exists() {
            if let Err(e) = lua.load_resources(&resources_dir) {
                log::error!("scripting: chyba při načítání resources: {e}");
            }
        } else {
            let scripts_dir = locate_scripts_dir();
            if let Err(e) = lua.load_scripts(&scripts_dir) {
                log::error!("scripting: chyba při načítání skriptů: {e}");
            }
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
            sprite_bg:       None,
            drag_start:      None,
            select_box:      None,
            selected_entity: None,
            selected_hp:     None,
            selected_color:  [1.0; 4],
            paused:          false,
            game_result:     None,
            food_used:       0,
            food_max:        10,
            pending_to_menu: false,
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
        // ── Přechod na hlavní menu (nastaven z render_ui) ────────────────────
        if self.pending_to_menu {
            self.pending_to_menu = false;
            use super::main_menu::MainMenuScreen;
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        // ── Pauza ────────────────────────────────────────────────────────────
        if input.key_just_pressed(KeyCode::KeyP) {
            self.paused = !self.paused;
        }
        if input.key_just_pressed(KeyCode::Escape) {
            if self.paused {
                use super::main_menu::MainMenuScreen;
                return Transition::To(Box::new(MainMenuScreen::new()));
            } else {
                self.paused = true;
            }
        }
        if self.paused || self.game_result.is_some() {
            handle_camera(dt, input, camera);
            return Transition::None;
        }

        handle_camera(dt, input, camera);
        handle_selection(input, camera, &mut self.world,
                         &mut self.drag_start, &mut self.select_box,
                         &mut self.selected_entity);
        handle_right_click(input, camera, &mut self.world, &self.lua);

        // ── Herní systémy ────────────────────────────────────────────────────
        let arrived         = movement_system(&mut self.world, &self.map, dt);
        let attack_events   = attack_system(&mut self.world, &self.map, dt);
        let production_done = production_system(&mut self.world, dt);
        let ai_ticks        = ai_tick_system(&mut self.world, dt);
        let harvested       = harvest_system(&mut self.world, dt);

        // Sklizené suroviny
        for h in harvested {
            if h.team == 0 {
                let prev = (self.gold, self.lumber, self.oil);
                self.gold   += h.gold;
                self.lumber += h.lumber;
                if (self.gold, self.lumber, self.oil) != prev {
                    if let Err(e) = self.lua.hook_resource_changed(self.gold, self.lumber, self.oil) {
                        log::error!("on_resource_changed: {e}");
                    }
                }
            }
        }

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
            let attacker_info = id_to_entity(ev.attacker_id).and_then(|e| unit_info(&self.world, e));
            let target_info   = id_to_entity(ev.target_id)  .and_then(|e| unit_info(&self.world, e));
            if let (Some(a), Some(t)) = (attacker_info, target_info) {
                let _ = self.lua.hook_unit_attack(&a, &t, ev.damage);
                let _ = self.lua.hook_unit_hit(&t, ev.damage, ev.attacker_id);
            }
        }

        // Dokončená výroba → spawn + hooky
        for done in production_done {
            let spawned = spawn_unit_by_kind(&mut self.world, &done.kind_id, done.rally, done.team);
            if let Some(info) = unit_info(&self.world, spawned) {
                let _ = self.lua.hook_unit_spawned(&info);
                let _ = self.lua.hook_unit_trained(&info, done.building_id);
            }
        }

        // Query cache pro Lua
        let all_units = collect_all_unit_infos(&self.world);
        let _ = self.lua.push_query_results(all_units.clone());
        let _ = self.lua.push_unit_cache(&all_units);

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

        // Globální tick
        if let Err(e) = self.lua.hook_game_tick(dt) {
            log::error!("on_game_tick: {e}");
        }

        // Cleanup mrtvých
        let dead = cleanup_dead(&mut self.world);
        for d in dead {
            let stub = UnitInfo {
                entity_id: d.id, x: d.pos.x, y: d.pos.y,
                hp: 0, hp_max: 1, damage: 0, pierce: 0, armor: 0,
                attack_range: 0.0, team: d.team, kind_id: d.kind_id,
            };
            let _ = self.lua.hook_unit_died(&stub);
        }

        // Zpracuj příkazy z Lua
        match self.lua.drain_commands() {
            Ok(cmds) => { for cmd in cmds { self.apply_cmd(cmd); } }
            Err(e) => log::error!("drain_commands: {e}"),
        }

        // Fog of war
        for (_e, (pos, sight)) in self.world.query_mut::<(&Position, &Sight)>() {
            self.map.reveal_circle(pos.0, sight.0);
        }

        // Refresh info panelu
        self.selected_hp    = None;
        self.selected_color = [1.0; 4];
        if let Some(sel_e) = self.selected_entity {
            if let (Ok(hp), Ok(sprite)) = (
                self.world.get::<&Health>(sel_e),
                self.world.get::<&Sprite>(sel_e),
            ) {
                self.selected_hp    = Some((hp.current, hp.max));
                self.selected_color = sprite.color;
            } else {
                // Entita zmizela
                self.selected_entity = None;
            }
        }
        if self.selected_entity.is_none() {
            for (_e, (hp, sprite, _sel)) in self.world.query::<(&Health, &Sprite, &Selected)>().iter() {
                self.selected_hp    = Some((hp.current, hp.max));
                self.selected_color = sprite.color;
                break;
            }
        }

        // Populace
        (self.food_used, self.food_max) = count_food(&self.world);

        // Kontrola výhry/prohry
        if self.game_result.is_none() {
            self.game_result = check_game_result(&self.world);
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

        // Výběrový rámeček (selected entities)
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

        // Drag selection box
        if let Some(sbox) = self.select_box {
            let tw  = 1.5 / camera.zoom;
            let uv  = UvRect::FULL;
            let col = [0.2, 1.0, 0.2, 0.7];
            batch.draw(Rect::new(sbox.x, sbox.y, sbox.w, tw), uv, col);
            batch.draw(Rect::new(sbox.x, sbox.y+sbox.h-tw, sbox.w, tw), uv, col);
            batch.draw(Rect::new(sbox.x, sbox.y, tw, sbox.h), uv, col);
            batch.draw(Rect::new(sbox.x+sbox.w-tw, sbox.y, tw, sbox.h), uv, col);
        }
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        // Resource bar (nahoře)
        ui.resource_bar(self.gold, self.lumber, self.oil);
        // Populace
        let food_str = format!("{}/{}", self.food_used, self.food_max);
        ui.label_shadowed(sw - 160.0, 8.0, &food_str, 1.0, colors::WHITE);

        // Health bary ve světě
        let positions: Vec<(Vec2, f32, f32)> = self.world
            .query::<(&Position, &Health, &Sprite)>()
            .iter()
            .map(|(_, (p, h, s))| (p.0, h.fraction(), s.size.y))
            .collect();
        let cam = dummy_camera();
        for (pos, frac, size) in positions {
            ui.health_bar_world(pos, size, frac, cam);
        }

        // Minimap
        ui.minimap_placeholder(self.map.width, self.map.height);

        // ── Spodní panel (info + výroba) ────────────────────────────────────
        let panel_h = 96.0;
        let panel_y = sh - panel_h;

        // Zkontrolujeme, zda je vybraná budova (má ProductionQueue)
        let selected_building: Option<(hecs::Entity, String)> = self.selected_entity
            .and_then(|e| {
                if self.world.get::<&ProductionQueue>(e).is_ok() {
                    let kind = self.world.get::<&UnitKindId>(e)
                        .map(|k| k.0.clone())
                        .unwrap_or_default();
                    Some((e, kind))
                } else {
                    None
                }
            });

        if let Some((bld_entity, bld_kind)) = selected_building {
            self.render_building_panel(ui, bld_entity, &bld_kind.clone(), panel_y, sw, sh);
        } else if let Some((cur, max)) = self.selected_hp {
            let frac = cur as f32 / max as f32;
            ui.info_panel(self.selected_color, frac, max);
        }

        // ── Pauza overlay ───────────────────────────────────────────────────
        if self.paused {
            ui.panel(Rect::new(0.0, 0.0, sw, sh), [0.0, 0.0, 0.0, 0.55]);
            let bw = 220.0; let bh = 44.0;
            let bx = (sw - bw) * 0.5;
            ui.label_shadowed(bx, sh * 0.33, "PAUZA  (P = pokracovat)", 2.0, colors::WHITE);
            if ui.button_text(Rect::new(bx, sh * 0.47, bw, bh), "Pokracovat", 1.5) {
                self.paused = false;
            }
            if ui.button_text(Rect::new(bx, sh * 0.47 + bh + 10.0, bw, bh), "Hlavni menu", 1.5) {
                self.pending_to_menu = true;
            }
        }

        // ── Výsledek hry ────────────────────────────────────────────────────
        if let Some(result) = self.game_result {
            ui.panel(Rect::new(0.0, 0.0, sw, sh), [0.0, 0.0, 0.0, 0.65]);
            let (text, col) = match result {
                GameResult::Win  => ("VYHRAL JSI!", [0.2, 1.0, 0.2, 1.0]),
                GameResult::Lose => ("PROHRALS...", [1.0, 0.2, 0.2, 1.0]),
            };
            let tw = text.chars().count() as f32 * 16.0;
            ui.label_shadowed((sw - tw) * 0.5, sh * 0.4, text, 2.0, col);
            ui.label_shadowed((sw - 240.0) * 0.5, sh * 0.55,
                "Stiskni ESC pro hlavni menu", 1.0, colors::WHITE);
        }
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.sprite_bg.as_ref().expect("InGameScreen::init not called")
    }
}

// ── Panel budovy: výroba ───────────────────────────────────────────────────────

impl InGameScreen {
    fn render_building_panel(&mut self, ui: &mut UiCtx, bld_entity: hecs::Entity,
                              bld_kind: &str, panel_y: f32, sw: f32, _sh: f32) {
        let ph = 96.0;
        ui.panel(Rect::new(0.0, panel_y, sw, ph), colors::BG_DARK);
        ui.border(Rect::new(0.0, panel_y, sw, ph), 1.0, colors::BORDER);

        // ── Načti data (immutable scope) ────────────────────────────────────
        let (bld_color, prod_info, prod_queue_len) = {
            let bld_color = self.world.get::<&Sprite>(bld_entity)
                .map(|s| s.color).unwrap_or(colors::WHITE);
            let (prod_info, prod_queue_len) =
                if let Ok(pq) = self.world.get::<&ProductionQueue>(bld_entity) {
                    let info = pq.current.as_ref().map(|(k, t)| (k.clone(), *t));
                    (info, pq.queue.len())
                } else {
                    (None, 0)
                };
            (bld_color, prod_info, prod_queue_len)
        };

        // ── Ikonka + název budovy ────────────────────────────────────────────
        ui.panel(Rect::new(8.0, panel_y + 8.0, 64.0, 64.0), bld_color);
        ui.border(Rect::new(8.0, panel_y + 8.0, 64.0, 64.0), 1.0, colors::BORDER);
        ui.label_shadowed(82.0, panel_y + 8.0, bld_kind, 1.0, colors::WHITE);

        // ── Progress výroby ─────────────────────────────────────────────────
        if let Some((ref kind, timer)) = prod_info {
            let total = 20.0f32;
            let progress = 1.0 - (timer / total).clamp(0.0, 1.0);
            ui.label_shadowed(82.0, panel_y + 22.0,
                &format!("Vyrabi: {}", kind), 1.0, colors::GREY);
            ui.progress_bar(
                Rect::new(82.0, panel_y + 38.0, 200.0, 12.0),
                progress, [0.1, 0.1, 0.1, 0.9], colors::BTN_NORMAL,
            );
            ui.label(82.0, panel_y + 56.0,
                &format!("Fronta: {}", prod_queue_len), 1.0, colors::GREY);
        } else {
            ui.label_shadowed(82.0, panel_y + 22.0, "Ceka na rozkaz", 1.0, colors::GREY);
        }

        // ── Tlačítka výroby ─────────────────────────────────────────────────
        let options = building_train_options(bld_kind);
        let btn_w = 88.0; let btn_h = 40.0;
        let start_x = sw - (options.len() as f32 * (btn_w + 4.0)) - 8.0;
        let gold   = self.gold;
        let lumber = self.lumber;
        let mut clicked_train: Option<(String, u32, u32, f32)> = None;

        for (i, opt) in options.iter().enumerate() {
            let bx = start_x + i as f32 * (btn_w + 4.0);
            let by = panel_y + (ph - btn_h) * 0.5;
            let rect = Rect::new(bx, by, btn_w, btn_h);
            let can_afford = gold >= opt.gold && lumber >= opt.lumber;
            let btn_color = if can_afford { colors::BTN_NORMAL } else { [0.25, 0.25, 0.25, 0.8] };
            if ui.button(rect, btn_color) && can_afford {
                clicked_train = Some((opt.kind_id.to_string(), opt.gold, opt.lumber, opt.time));
            }
            ui.label_centered(Rect::new(bx, by, btn_w, 18.0), opt.name, 1.0, colors::WHITE);
            let cost_str = if opt.lumber > 0 {
                format!("{}g {}l", opt.gold, opt.lumber)
            } else {
                format!("{}g", opt.gold)
            };
            ui.label_centered(Rect::new(bx, by + 22.0, btn_w, 14.0), &cost_str, 1.0, colors::GOLD);
        }

        // ── Zpracuj kliknutí (mutable přístup po uvolnění immutable borrows) ─
        if let Some((kind_id, gold_cost, lumber_cost, build_time)) = clicked_train {
            self.gold   -= gold_cost;
            self.lumber -= lumber_cost;
            if let Ok(mut pq) = self.world.get::<&mut ProductionQueue>(bld_entity) {
                if pq.current.is_none() {
                    pq.current = Some((kind_id, build_time));
                } else {
                    pq.enqueue(kind_id);
                }
            }
            let _ = self.lua.hook_resource_changed(self.gold, self.lumber, self.oil);
        }
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

    let kind_id: String = if let Ok(k) = world.get::<&UnitKindId>(entity) {
        k.0.clone()
    } else if let Ok(u) = world.get::<&Unit>(entity) {
        unit_kind_str(u.0)
    } else {
        "unknown".into()
    };

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

// ── Win / Lose ────────────────────────────────────────────────────────────────

fn check_game_result(world: &World) -> Option<GameResult> {
    // Zjisti, zda existují budovy nebo jednotky pro každý tým
    let mut player_alive = false;
    let mut enemy_alive  = false;

    for (_, (team, hp)) in world.query::<(&Team, &Health)>().iter() {
        if hp.current > 0 {
            if team.0 == 0 { player_alive = true; }
            if team.0 == 1 { enemy_alive  = true; }
        }
    }

    if !player_alive { return Some(GameResult::Lose); }
    if !enemy_alive  { return Some(GameResult::Win);  }
    None
}

// ── Populace ──────────────────────────────────────────────────────────────────

fn count_food(world: &World) -> (u32, u32) {
    let mut used = 0u32;
    let mut max  = 0u32;
    for (_, (team, kind, _hp)) in world.query::<(&Team, &UnitKindId, &Health)>().iter() {
        if team.0 != 0 { continue; }
        match kind.0.as_str() {
            // Farmy poskytují jídlo
            "farm" | "pig_farm" => max += 4,
            // Základny poskytují základní kapacitu
            "town_hall" | "keep" | "castle" | "great_hall" | "stronghold" | "fortress" => max += 5,
            // Budovy nekonzumují jídlo
            k if is_building_kind(k) => {}
            // Všechny bojové/dělnické jednotky spotřebovávají jídlo
            _ => used += 1,
        }
    }
    if max == 0 { max = 5; } // záloha – vždy alespoň 5
    (used, max.min(200))
}

fn is_building_kind(kind: &str) -> bool {
    matches!(kind,
        "town_hall" | "keep" | "castle" | "great_hall" | "stronghold" | "fortress" |
        "farm" | "pig_farm" | "barracks" | "orc_barracks" |
        "lumbermill" | "blacksmith" | "tower" | "church" | "stables" |
        "gold_mine" | "oil_platform"
    )
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
                    let _ = self.world.remove_one::<HarvestOrder>(e);
                    if let Ok(mut vel) = self.world.get::<&mut Velocity>(e) { vel.0 = Vec2::ZERO; }
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
                    if let Ok(mut h) = self.world.get::<&mut Health>(e) { h.current = 0; }
                }
            }
            ScriptCmd::AddResources { gold, lumber, oil } => {
                let prev = (self.gold, self.lumber, self.oil);
                self.gold   = (self.gold   as i32 + gold)  .max(0) as u32;
                self.lumber = (self.lumber as i32 + lumber).max(0) as u32;
                self.oil    = (self.oil    as i32 + oil)   .max(0) as u32;
                if (self.gold, self.lumber, self.oil) != prev {
                    let _ = self.lua.hook_resource_changed(self.gold, self.lumber, self.oil);
                }
            }
            ScriptCmd::SpawnUnit { kind_id, x, y, team } => {
                let e = spawn_unit_by_kind(&mut self.world, &kind_id, Vec2::new(x, y), team);
                if let Some(info) = unit_info(&self.world, e) {
                    let _ = self.lua.hook_unit_spawned(&info);
                }
            }
            ScriptCmd::TrainUnit { building_id, kind_id, build_time } => {
                if let Some(e) = id_to_entity(building_id) {
                    if let Ok(mut pq) = self.world.get::<&mut ProductionQueue>(e) {
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

/// Spawnuje jednotku/budovu podle string kind_id.
pub fn spawn_unit_by_kind(world: &mut World, kind_id: &str, pos: Vec2, team: u8) -> hecs::Entity {
    let color = team_color(team);

    struct UnitDef {
        col: u32, row: u32, size: f32, hp: i32,
        damage: i32, pierce: i32, armor: i32, range: f32, cd: f32,
        speed: f32, ai: &'static str, sight: u32,
        is_building: bool,
    }

    let d = match kind_id {
        // ── Lidé – bojové jednotky ──────────────────────────────────────────
        "peasant" => UnitDef { col:1, row:0, size:32., hp:30,  damage:3,  pierce:0, armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai", sight:4, is_building:false },
        "footman" => UnitDef { col:2, row:0, size:32., hp:60,  damage:6,  pierce:3, armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai",  sight:4, is_building:false },
        "archer"  => UnitDef { col:3, row:0, size:32., hp:40,  damage:3,  pierce:6, armor:0, range:128., cd:1.0, speed:128., ai:"ranged_ai", sight:5, is_building:false },
        "knight"  => UnitDef { col:4, row:0, size:32., hp:90,  damage:8,  pierce:4, armor:4, range:0.,   cd:1.0, speed:192., ai:"melee_ai",  sight:4, is_building:false },
        "mage"    => UnitDef { col:5, row:0, size:32., hp:35,  damage:0,  pierce:9, armor:0, range:160., cd:1.5, speed:128., ai:"ranged_ai", sight:9, is_building:false },
        // ── Orci – bojové jednotky ──────────────────────────────────────────
        "peon"    => UnitDef { col:1, row:1, size:32., hp:30,  damage:3,  pierce:0, armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai", sight:4, is_building:false },
        "grunt"   => UnitDef { col:2, row:1, size:32., hp:60,  damage:8,  pierce:2, armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai",  sight:4, is_building:false },
        "troll_axethrower" => UnitDef { col:3, row:1, size:32., hp:40, damage:3, pierce:6, armor:0, range:128., cd:1.0, speed:128., ai:"ranged_ai", sight:5, is_building:false },
        "ogre"    => UnitDef { col:4, row:1, size:32., hp:90,  damage:10, pierce:2, armor:4, range:0.,   cd:1.3, speed:128., ai:"melee_ai",  sight:4, is_building:false },
        "death_knight" => UnitDef { col:5, row:1, size:32., hp:60, damage:0, pierce:9, armor:0, range:160., cd:1.5, speed:192., ai:"ranged_ai", sight:9, is_building:false },
        // ── Lidé – budovy ───────────────────────────────────────────────────
        "town_hall" | "keep" | "castle" =>
            UnitDef { col:0, row:0, size:64., hp:1200, damage:0, pierce:0, armor:5, range:0., cd:0., speed:0., ai:"", sight:6, is_building:true },
        "barracks" =>
            UnitDef { col:0, row:2, size:64., hp:600,  damage:0, pierce:0, armor:3, range:0., cd:0., speed:0., ai:"", sight:4, is_building:true },
        "farm" =>
            UnitDef { col:0, row:4, size:32., hp:400,  damage:0, pierce:0, armor:2, range:0., cd:0., speed:0., ai:"", sight:3, is_building:true },
        // ── Orci – budovy ───────────────────────────────────────────────────
        "great_hall" | "stronghold" | "fortress" =>
            UnitDef { col:0, row:1, size:64., hp:1200, damage:0, pierce:0, armor:5, range:0., cd:0., speed:0., ai:"", sight:6, is_building:true },
        "orc_barracks" =>
            UnitDef { col:0, row:3, size:64., hp:600,  damage:0, pierce:0, armor:3, range:0., cd:0., speed:0., ai:"", sight:4, is_building:true },
        "pig_farm" =>
            UnitDef { col:0, row:5, size:32., hp:400,  damage:0, pierce:0, armor:2, range:0., cd:0., speed:0., ai:"", sight:3, is_building:true },
        // ── Zdroje ──────────────────────────────────────────────────────────
        "gold_mine" =>
            UnitDef { col:7, row:0, size:64., hp:999999, damage:0, pierce:0, armor:99, range:0., cd:0., speed:0., ai:"", sight:2, is_building:true },
        // ── Fallback ─────────────────────────────────────────────────────────
        _ => UnitDef { col:1, row:0, size:32., hp:30, damage:3, pierce:0, armor:0, range:0., cd:1.5, speed:128., ai:"worker_ai", sight:4, is_building:false },
    };

    let flags = MoveFlags {
        can_swim: false, can_fly: false,
        speed_water: 0.0, speed_forest: 0.75, speed_road: 1.0,
    };

    if d.is_building {
        // Budova – bez pohybu, s ProductionQueue pro základny
        let has_queue = matches!(kind_id,
            "town_hall" | "keep" | "castle" |
            "great_hall" | "stronghold" | "fortress" |
            "barracks" | "orc_barracks"
        );
        let is_gold = kind_id == "gold_mine";

        let mut builder = hecs::EntityBuilder::new();
        builder.add(Position(pos));
        builder.add(Velocity(Vec2::ZERO));
        builder.add(Sprite { col: d.col, row: d.row, size: Vec2::splat(d.size), color });
        builder.add(Team(team));
        builder.add(Health::new(d.hp));
        builder.add(UnitKindId(kind_id.to_string()));
        builder.add(Sight(d.sight));
        builder.add(IsBuilding);
        if d.armor > 0 {
            builder.add(AttackStats {
                damage: 0, pierce: 0, armor: d.armor,
                range: 0.0, cooldown: 0.0, cooldown_left: 0.0,
            });
        }
        if has_queue {
            let mut pq = ProductionQueue::new(5);
            pq.rally = pos + Vec2::new(d.size + TILE_SIZE, 0.0);
            builder.add(pq);
        }
        if is_gold {
            builder.add(ResourceSource { kind: HarvestKind::Gold, remaining: -1 });
        }
        world.spawn(builder.build())
    } else {
        // Pohyblivá jednotka
        let mut builder = hecs::EntityBuilder::new();
        builder.add(Position(pos));
        builder.add(Velocity(Vec2::ZERO));
        builder.add(Sprite { col: d.col, row: d.row, size: Vec2::splat(d.size), color });
        builder.add(Team(team));
        builder.add(Health::new(d.hp));
        builder.add(UnitKindId(kind_id.to_string()));
        builder.add(Sight(d.sight));
        builder.add(flags);
        builder.add(AttackStats {
            damage: d.damage, pierce: d.pierce, armor: d.armor,
            range: d.range, cooldown: d.cd, cooldown_left: 0.0,
        });
        if !d.ai.is_empty() {
            builder.add(AiController::new(d.ai, 0.5));
        }
        world.spawn(builder.build())
    }
}

fn team_color(team: u8) -> [f32; 4] {
    match team {
        0 => [0.20, 0.45, 1.00, 1.0],
        1 => [0.80, 0.20, 0.10, 1.0],
        2 => [0.10, 0.70, 0.20, 1.0],
        _ => [0.70, 0.70, 0.10, 1.0],
    }
}

// ── Výběr jednotek ────────────────────────────────────────────────────────────

fn handle_selection(
    input: &Input, camera: &Camera, world: &mut World,
    drag_start: &mut Option<Vec2>, select_box: &mut Option<Rect>,
    selected_entity: &mut Option<hecs::Entity>,
) {
    let mw = camera.screen_to_world(input.mouse_pos);

    if input.mouse_just_pressed(MouseButton::Left) {
        *drag_start = Some(mw);
    }

    if input.mouse_held(MouseButton::Left) {
        if let Some(start) = *drag_start {
            let delta = (mw - start).length();
            if delta > 4.0 {
                let (x0, y0) = (start.x.min(mw.x), start.y.min(mw.y));
                let (x1, y1) = (start.x.max(mw.x), start.y.max(mw.y));
                *select_box = Some(Rect::new(x0, y0, x1-x0, y1-y0));
            }
        }
    }

    if input.mouse_just_released(MouseButton::Left) {
        let start = drag_start.take().unwrap_or(mw);
        let delta = (mw - start).length();

        // Zruš předchozí výběr
        let prev: Vec<_> = world.query::<()>().with::<&Selected>().iter().map(|(e,_)| e).collect();
        for e in prev { let _ = world.remove_one::<Selected>(e); }
        *selected_entity = None;

        if let Some(sbox) = select_box.take() {
            // Drag-box výběr – jen hráčovy jednotky
            let sel: Vec<_> = world.query::<(&Position, &Team)>().iter()
                .filter(|(_, (p, t))| t.0 == 0 && sbox.contains(p.0))
                .map(|(e, _)| e)
                .collect();
            for e in &sel { let _ = world.insert_one(*e, Selected); }
            if let Some(&first) = sel.first() { *selected_entity = Some(first); }
        } else if delta < 6.0 {
            // Single-click – vyber entitu pod kurzorem (hráčova přednost)
            let clicked: Option<hecs::Entity> = find_entity_at_for_team(world, mw, 0)
                .or_else(|| find_entity_at_any(world, mw));
            if let Some(e) = clicked {
                let _ = world.insert_one(e, Selected);
                *selected_entity = Some(e);
            }
        }

        *drag_start = None;
    }
}

// ── Pravé tlačítko – kontext ──────────────────────────────────────────────────

fn handle_right_click(input: &Input, camera: &Camera, world: &mut World, lua: &LuaRuntime) {
    if !input.mouse_just_pressed(MouseButton::Right) { return; }
    let target = camera.screen_to_world(input.mouse_pos);

    // Zjisti, co je na místě kliknutí
    let target_entity = find_entity_at_any(world, target);

    let selected: Vec<hecs::Entity> = world.query::<()>().with::<&Selected>()
        .iter().map(|(e,_)| e).collect();

    for (idx, &sel_e) in selected.iter().enumerate() {
        // Ofset pro skupinu
        let ox = (idx as i32 % 5 - 2) as f32 * TILE_SIZE;
        let oy = (idx as i32 / 5 - 1) as f32 * TILE_SIZE;
        let t  = target + Vec2::new(ox, oy);

        if let Some(tgt_e) = target_entity {
            // Je cíl nepřítel?
            let sel_team = world.get::<&Team>(sel_e).map(|t| t.0).unwrap_or(0);
            let tgt_team = world.get::<&Team>(tgt_e).map(|t| t.0).unwrap_or(0);
            let is_enemy    = sel_team != tgt_team;
            let is_resource = world.get::<&ResourceSource>(tgt_e).is_ok();

            if is_enemy && world.get::<&AttackStats>(sel_e).is_ok() {
                // Útok
                let _ = world.remove_one::<MoveOrder>(sel_e);
                let _ = world.remove_one::<AttackOrder>(sel_e);
                let _ = world.insert_one(sel_e, AttackOrder { target: tgt_e });
                continue;
            }

            if is_resource {
                // Sklizeň – jen pro dělníky
                let kind_id = world.get::<&UnitKindId>(sel_e).map(|k| k.0.clone()).unwrap_or_default();
                if is_worker_kind(&kind_id) {
                    let source_pos = world.get::<&Position>(tgt_e).map(|p| p.0).unwrap_or(target);
                    let res_kind   = world.get::<&ResourceSource>(tgt_e)
                        .map(|r| r.kind.clone()).unwrap_or(HarvestKind::Gold);
                    let depot_pos  = find_nearest_depot(world, sel_e, source_pos);
                    let _ = world.remove_one::<MoveOrder>(sel_e);
                    let _ = world.remove_one::<HarvestOrder>(sel_e);
                    let _ = world.insert_one(sel_e, HarvestOrder {
                        source:    source_pos,
                        depot:     depot_pos,
                        kind:      res_kind,
                        state:     HarvestState::GoingToSource,
                        carried:   0,
                        max_carry: 10,
                        timer:     0.0,
                    });
                    let _ = world.insert_one(sel_e, MoveOrder {
                        target: source_pos,
                        speed:  128.0,
                        flags:  MoveFlags::default(),
                    });
                    continue;
                }
            }
        }

        // Pohyb + Lua hook
        let Some(info) = unit_info(world, sel_e) else { continue };
        let default_params = MoveParams::default();

        match lua.hook_move_order(&info, t.x, t.y, default_params) {
            Ok(Some(ScriptCmd::MoveUnit { target_x, target_y, params, .. })) => {
                let flags = MoveFlags::from(params.clone());
                let _ = world.remove_one::<MoveOrder>(sel_e);
                let _ = world.remove_one::<AttackOrder>(sel_e);
                let _ = world.insert_one(sel_e, MoveOrder {
                    target: Vec2::new(target_x, target_y),
                    speed:  params.speed,
                    flags,
                });
            }
            Ok(None) => {}
            Ok(Some(_)) => {}
            Err(e) => log::error!("on_move_order: {e}"),
        }
    }
}

// ── Pomocné funkce ────────────────────────────────────────────────────────────

fn find_entity_at_for_team(world: &World, pos: Vec2, team: u8) -> Option<hecs::Entity> {
    world.query::<(&Position, &Sprite, &Team)>()
        .iter()
        .find(|(_, (p, s, t))| {
            t.0 == team && Rect::new(p.0.x - s.size.x*0.5, p.0.y - s.size.y*0.5,
                                     s.size.x, s.size.y).contains(pos)
        })
        .map(|(e, _)| e)
}

fn find_entity_at_any(world: &World, pos: Vec2) -> Option<hecs::Entity> {
    world.query::<(&Position, &Sprite)>()
        .iter()
        .find(|(_, (p, s))| {
            Rect::new(p.0.x - s.size.x*0.5, p.0.y - s.size.y*0.5,
                      s.size.x, s.size.y).contains(pos)
        })
        .map(|(e, _)| e)
}

fn find_nearest_depot(world: &World, _worker: hecs::Entity, near: Vec2) -> Vec2 {
    // Nejbližší budova (Town Hall / Great Hall) týmu 0
    world.query::<(&Position, &Team, &IsBuilding, &UnitKindId)>()
        .iter()
        .filter(|(_, (_, t, _, k))| {
            t.0 == 0 && matches!(k.0.as_str(),
                "town_hall" | "keep" | "castle" | "great_hall" | "stronghold" | "fortress")
        })
        .min_by(|(_, (p1, ..)), (_, (p2, ..))| {
            let d1 = (p1.0 - near).length_squared();
            let d2 = (p2.0 - near).length_squared();
            d1.partial_cmp(&d2).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, (p, ..))| p.0)
        .unwrap_or(near)
}

fn is_worker_kind(kind: &str) -> bool {
    matches!(kind, "peasant" | "peon")
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

fn locate_resources_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.parent().unwrap_or(std::path::Path::new(".")).join("resources");
        if c.exists() { return c; }
    }
    std::path::PathBuf::from("resources")
}

fn locate_scripts_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.parent().unwrap_or(std::path::Path::new(".")).join("scripts");
        if c.exists() { return c; }
    }
    std::path::PathBuf::from("scripts")
}

// ── Demo scéna ────────────────────────────────────────────────────────────────

fn spawn_demo_units(world: &mut World, lua: &LuaRuntime) {
    let p0_base = Vec2::new(5.0 * TILE_SIZE, 35.0 * TILE_SIZE);
    let p1_base = Vec2::new(50.0 * TILE_SIZE, 5.0 * TILE_SIZE);

    // Zlatý důl – uprostřed mapy
    let mine_pos = Vec2::new(30.0 * TILE_SIZE, 30.0 * TILE_SIZE);
    spawn_unit_by_kind(world, "gold_mine", mine_pos, 255);

    // ── Hráč (team 0) ────────────────────────────────────────────────────────
    let th0 = spawn_unit_by_kind(world, "town_hall", p0_base, 0);
    // Rally point vedle základny
    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(th0) {
        pq.rally = p0_base + Vec2::new(80.0, 0.0);
    }

    // Peons (dělníci)
    let peon0 = spawn_unit_by_kind(world, "peasant", p0_base + Vec2::new(2.0*TILE_SIZE, 3.0*TILE_SIZE), 0);
    let peon1 = spawn_unit_by_kind(world, "peasant", p0_base + Vec2::new(3.0*TILE_SIZE, 3.0*TILE_SIZE), 0);

    // Automaticky přiřaď dělníky k těžbě zlata
    for &peon in &[peon0, peon1] {
        let _ = world.insert_one(peon, HarvestOrder {
            source:    mine_pos,
            depot:     p0_base,
            kind:      HarvestKind::Gold,
            state:     HarvestState::GoingToSource,
            carried:   0,
            max_carry: 10,
            timer:     0.0,
        });
        let _ = world.insert_one(peon, MoveOrder {
            target: mine_pos, speed: 128.0, flags: MoveFlags::default(),
        });
    }

    // Bojové jednotky hráče
    spawn_unit_by_kind(world, "footman", p0_base + Vec2::new(3.0*TILE_SIZE, 4.0*TILE_SIZE), 0);
    spawn_unit_by_kind(world, "archer",  p0_base + Vec2::new(4.0*TILE_SIZE, 3.5*TILE_SIZE), 0);

    // ── Nepřítel (team 1) ─────────────────────────────────────────────────────
    let gh1 = spawn_unit_by_kind(world, "great_hall", p1_base, 1);
    // Barracks nepřítele
    let orc_barrack = spawn_unit_by_kind(world, "orc_barracks",
        p1_base + Vec2::new(4.0*TILE_SIZE, 0.0), 1);

    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(gh1) {
        pq.rally = p1_base + Vec2::new(80.0, 0.0);
    }
    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(orc_barrack) {
        pq.rally = p1_base + Vec2::new(2.0*TILE_SIZE, 4.0*TILE_SIZE);
    }

    // Počáteční vojaci nepřítele
    spawn_unit_by_kind(world, "grunt",  p1_base + Vec2::new(2.0*TILE_SIZE, 3.0*TILE_SIZE), 1);
    spawn_unit_by_kind(world, "grunt",  p1_base + Vec2::new(3.0*TILE_SIZE, 3.0*TILE_SIZE), 1);
    spawn_unit_by_kind(world, "troll_axethrower", p1_base + Vec2::new(4.0*TILE_SIZE, 2.5*TILE_SIZE), 1);

    // on_unit_spawned pro všechny entity
    let entities: Vec<hecs::Entity> = world.query::<()>().iter().map(|(e, _)| e).collect();
    for entity in entities {
        if let Some(info) = unit_info(world, entity) {
            let _ = lua.hook_unit_spawned(&info);
        }
    }
}

fn create_demo_map() -> TileMap {
    use TileKind::*;
    let w = 64u32; let h = 64u32;
    let mut map = TileMap::new_filled(w, h, Grass);
    // Vodní překážka
    for y in 20..28 { for x in 0..w { map.set(x, y, Water); } }
    for y in 22..26 { for x in 0..w { map.set(x, y, DeepWater); } }
    for y in 20..28 { map.set(32, y, Bridge); map.set(33, y, Bridge); }
    // Les
    for y in 5..12  { for x in 10..30 { map.set(x, y, Forest); } }
    // Skály
    for &(x,y) in &[(40u32,15u32),(41,15),(40,16),(50,10),(51,10),(52,10)] { map.set(x,y,Rock); }
    // Předexponovaná oblast hráče
    for y in 30..50 { for x in 0..20 {
        if let Some(t) = map.get_mut(x, y) { t.visible = true; t.explored = true; }
    }}
    map
}
