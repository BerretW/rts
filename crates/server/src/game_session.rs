//! Autoritativní herní session – tick loop na serveru.
//!
//! Každý GameSession běží jako samostatný tokio task.
//! Přijímá PlayerInput od klientů přes mpsc kanál.
//! Broadcastuje GameState přes broadcast kanál.

use std::path::PathBuf;
use std::time::Duration;

use glam::Vec2;
use hecs::World;
use tokio::sync::mpsc;
use tokio::time;

use net::{EntitySnapshot, PlayerAction, ServerMsg};

use crate::lobby::LobbyPlayer;
use crate::scripting::{LuaRuntime, ScriptCmd, UnitInfo};
use crate::systems::*;
use crate::world::*;

const TICK_RATE: u8       = 20;
const TICK_MS:   u64      = 1000 / TICK_RATE as u64;

// ── Vstup do session ──────────────────────────────────────────────────────────

pub enum SessionInput {
    PlayerActions { client_id: u64, tick: u64, actions: Vec<PlayerAction> },
    ScriptEvent   { client_id: u64, name: String, args_json: String },
}

// ── Handle pro komunikaci se session ─────────────────────────────────────────

pub struct GameSessionHandle {
    pub input_tx: mpsc::UnboundedSender<SessionInput>,
}

impl GameSessionHandle {
    pub fn send_input(&self, client_id: u64, tick: u64, actions: Vec<PlayerAction>) {
        let _ = self.input_tx.send(SessionInput::PlayerActions { client_id, tick, actions });
    }

    pub fn send_script_event(&self, client_id: u64, name: String, args_json: String) {
        let _ = self.input_tx.send(SessionInput::ScriptEvent { client_id, name, args_json });
    }
}

// ── GameSession ───────────────────────────────────────────────────────────────

pub struct GameSession;

impl GameSession {
    /// Spustí herní session na dedikovaném OS threadu.
    /// Lua (Rc interně) není Send – musíme použít vlastní thread.
    pub fn start(
        players:       Vec<LobbyPlayer>,
        map_id:        String,
        resources_dir: PathBuf,
        assets_dir:    PathBuf,
    ) -> GameSessionHandle {
        let (input_tx, input_rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime selhalo");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let result = run_session(players, map_id, resources_dir, assets_dir, input_rx).await;
                if let Err(e) = result {
                    log::error!("game session selhala: {e}");
                }
            });
        });

        GameSessionHandle { input_tx }
    }
}

// ── Hlavní smyčka ─────────────────────────────────────────────────────────────

async fn run_session(
    players:       Vec<LobbyPlayer>,
    map_id:        String,
    resources_dir: PathBuf,
    assets_dir:    PathBuf,
    mut input_rx:  mpsc::UnboundedReceiver<SessionInput>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("GameSession start: mapa={map_id}, hráčů={}", players.len());

    // Lua runtime – inicializujeme přímo (Lua je !Send, jsme na svém threadu)
    let lua = LuaRuntime::new(&resources_dir, assets_dir)
        .map_err(|e| format!("Lua init: {e}"))?;

    // ECS world + mapa
    let mut world = World::new();
    let map = build_map(&map_id);

    // Serializuj tiles pro klienta
    let map_tiles: Vec<u8> = (0..map.height)
        .flat_map(|y| (0..map.width).map(move |x| (x, y)))
        .map(|(x, y)| map.get(x, y).unwrap_or(TileKind::Grass).to_byte())
        .collect();

    let base_positions = spawn_initial_units(&mut world, &players);

    // Oznám klientům start
    for (i, p) in players.iter().enumerate() {
        let (base_x, base_y) = base_positions.get(i).copied().unwrap_or((640.0, 640.0));
        let _ = p.handle.tx.send(ServerMsg::GameStart {
            map_id:    map_id.clone(),
            your_team: i as u8,
            tick_rate: TICK_RATE,
            map_tiles: map_tiles.clone(),
            map_w:     map.width,
            map_h:     map.height,
            base_x,
            base_y,
        });
    }

    let mut tick: u64 = 0;
    let dt = TICK_MS as f32 / 1000.0;
    let mut interval = time::interval(Duration::from_millis(TICK_MS));

    loop {
        interval.tick().await;
        tick += 1;

        // Shromáždí vstupy klientů za tento tick
        let mut pending: Vec<(u64, Vec<PlayerAction>)> = Vec::new();
        while let Ok(input) = input_rx.try_recv() {
            match input {
                SessionInput::PlayerActions { client_id, actions, .. } => {
                    pending.push((client_id, actions));
                }
                SessionInput::ScriptEvent { client_id, name, args_json } => {
                    if let Err(e) = lua.trigger_network_event(&name, client_id, &args_json) {
                        log::warn!("trigger_network_event '{}': {e}", name);
                    }
                }
            }
        }

        // Aplikuj akce hráčů; sesbíráme použití schopností pro Lua zpracování
        // (team, unit_id, ability_id, target_id, tx, ty)
        let mut ability_events: Vec<(u8, u64, String, Option<u64>, f32, f32)> = Vec::new();
        for (client_id, actions) in pending {
            let team = players.iter().enumerate()
                .find(|(_, p)| p.handle.id == client_id)
                .map(|(i, _)| i as u8)
                .unwrap_or(0);
            apply_player_actions(&mut world, team, actions, &mut ability_events);
        }

        // Systémy (blokují – herní logika je sync)
        movement_system(&mut world, &map, dt);
        patrol_system(&mut world);
        attack_system(&mut world, dt);
        ability_cooldown_system(&mut world, dt);
        let produced = production_system(&mut world, dt);
        let ai_events = ai_tick_system(&mut world, dt);
        let dead = cleanup_dead(&mut world);

        // Spawn vyrobených jednotek
        for done in produced {
            let e = spawn_unit_by_kind(&mut world, &done.kind_id, done.rally, done.team);
            if let Some(info) = unit_info(&world, e) {
                if let Err(e) = lua.hook_unit_spawned(&info) {
                    log::warn!("on_unit_spawned: {e}");
                }
            }
        }

        // AI + query cache
        let all = collect_all_infos(&world);
        let _ = lua.push_query_results(&all);
        let _ = lua.push_unit_cache(&all);

        // Zpracuj použití schopností přes Lua
        for (_, unit_id, ability_id, target_id, tx, ty) in ability_events {
            if let Some(entity) = hecs::Entity::from_bits(unit_id) {
                if let Some(info) = unit_info(&world, entity) {
                    if let Err(e) = lua.hook_ability_used(&info, &ability_id, target_id, tx, ty) {
                        log::warn!("on_ability_used: {e}");
                    }
                }
            }
        }

        for ev in ai_events {
            if let Some(entity) = hecs::Entity::from_bits(ev.entity_id) {
                if let Some(info) = unit_info(&world, entity) {
                    let _ = lua.hook_ai_tick(&info, &ev.script_id, dt);
                }
            }
        }

        // Died hooks
        for d in &dead {
            let stub = UnitInfo {
                entity_id: d.id, x: d.pos.x, y: d.pos.y,
                hp: 0, hp_max: 1, damage: 0, pierce: 0, armor: 0, attack_range: 0.0,
                team: d.team, kind_id: d.kind_id.clone(),
            };
            let _ = lua.hook_unit_died(&stub);
        }

        // Globální tick
        let _ = lua.hook_game_tick(dt);

        // Drain + aplikuj Lua příkazy
        match lua.drain_commands() {
            Ok(cmds) => { for cmd in cmds { apply_cmd(&mut world, cmd); } }
            Err(e)   => log::warn!("drain_commands: {e}"),
        }

        // Odešli TriggerClientEvent volání klientům
        match lua.drain_client_events() {
            Ok(events) => {
                for ev in events {
                    let msg = ServerMsg::ScriptEvent { name: ev.name, args_json: ev.args_json };
                    if ev.target < 0 {
                        // broadcast
                        for p in &players { let _ = p.handle.tx.send(msg.clone()); }
                    } else {
                        let target_id = ev.target as u64;
                        if let Some(p) = players.iter().find(|p| p.handle.id == target_id) {
                            let _ = p.handle.tx.send(msg);
                        }
                    }
                }
            }
            Err(e) => log::warn!("drain_client_events: {e}"),
        }

        // Broadcastni stav (každých 5 ticků = 4 Hz stačí pro plynulost na LAN)
        if tick % 5 == 0 {
            let snapshot = build_snapshot(&world, tick);
            for p in &players {
                let _ = p.handle.tx.send(snapshot.clone());
            }
        }

        // Konec hry pokud jsou všichni mrtví (všechny town_hally týmu ≠ 0 zničeny)
        if tick % 100 == 0 {
            if let Some(winner) = check_winner(&world, players.len() as u8) {
                let msg = ServerMsg::GameOver { winner_team: Some(winner), reason: "Všichni nepřátelé poraženi".into() };
                for p in &players { let _ = p.handle.tx.send(msg.clone()); }
                break;
            }
        }
    }

    log::info!("GameSession tick={tick} ukončena");
    Ok(())
}

// ── Pomocné funkce ────────────────────────────────────────────────────────────

fn apply_player_actions(
    world:          &mut World,
    team:           u8,
    actions:        Vec<PlayerAction>,
    ability_events: &mut Vec<(u8, u64, String, Option<u64>, f32, f32)>,
) {
    for action in actions {
        match action {
            PlayerAction::MoveUnits { unit_ids, target_x, target_y } => {
                for (i, uid) in unit_ids.iter().enumerate() {
                    if let Some(e) = hecs::Entity::from_bits(*uid) {
                        if let Ok(t) = world.get::<&Team>(e) {
                            if t.0 != team { continue; }
                        }
                        let ox = (i as i32 % 5 - 2) as f32 * TILE_SIZE;
                        let oy = (i as i32 / 5 - 1) as f32 * TILE_SIZE;
                        let target = Vec2::new(target_x + ox, target_y + oy);
                        let flags = world.get::<&MoveFlags>(e).ok()
                            .map(|f| (*f).clone()).unwrap_or_default();
                        let speed = world.get::<&AttackStats>(e).ok()
                            .map(|_| 128.0f32).unwrap_or(128.0);
                        let _ = world.remove_one::<MoveOrder>(e);
                        let _ = world.remove_one::<AttackOrder>(e);
                        let _ = world.insert_one(e, MoveOrder { target, speed, flags });
                        // Potlač AI na ~5 sekund (100 ticků po 50ms)
                        if let Ok(mut ai) = world.get::<&mut AiController>(e) {
                            ai.player_override = 100;
                        }
                    }
                }
            }
            PlayerAction::AttackUnit { attacker_ids, target_id } => {
                let target = match hecs::Entity::from_bits(target_id) { Some(e) => e, None => continue };
                for uid in attacker_ids {
                    if let Some(e) = hecs::Entity::from_bits(uid) {
                        let _ = world.remove_one::<MoveOrder>(e);
                        let _ = world.remove_one::<AttackOrder>(e);
                        let _ = world.insert_one(e, AttackOrder { target });
                        if let Ok(mut ai) = world.get::<&mut AiController>(e) {
                            ai.player_override = 200; // útok potlačí AI na ~10s
                        }
                    }
                }
            }
            PlayerAction::StopUnits { unit_ids } => {
                for uid in unit_ids {
                    if let Some(e) = hecs::Entity::from_bits(uid) {
                        let _ = world.remove_one::<MoveOrder>(e);
                        let _ = world.remove_one::<AttackOrder>(e);
                        if let Ok(mut v) = world.get::<&mut Velocity>(e) { v.0 = Vec2::ZERO; }
                        if let Ok(mut ai) = world.get::<&mut AiController>(e) {
                            ai.player_override = 40; // krátká pauza
                        }
                    }
                }
            }
            PlayerAction::TrainUnit { building_id, kind_id } => {
                if let Some(e) = hecs::Entity::from_bits(building_id) {
                    // Ověř, že budova patří danému týmu
                    if let Ok(t) = world.get::<&Team>(e) { if t.0 != team { continue; } }
                    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(e) {
                        let bt = unit_build_time(&kind_id);
                        if pq.current.is_none() { pq.start(kind_id, bt); }
                        else { pq.enqueue(kind_id); }
                    }
                }
            }
            PlayerAction::SpawnUnit { kind_id, x, y } => {
                spawn_unit_by_kind(world, &kind_id, Vec2::new(x, y), team);
            }
            PlayerAction::PatrolUnit { unit_ids, target_x, target_y } => {
                let target = Vec2::new(target_x, target_y);
                for uid in unit_ids {
                    if let Some(e) = hecs::Entity::from_bits(uid) {
                        if let Ok(t) = world.get::<&Team>(e) { if t.0 != team { continue; } }
                        let pos = world.get::<&Position>(e).ok().map(|p| p.0).unwrap_or_default();
                        let flags = world.get::<&MoveFlags>(e).ok()
                            .map(|f| (*f).clone()).unwrap_or_default();
                        let _ = world.remove_one::<MoveOrder>(e);
                        let _ = world.remove_one::<AttackOrder>(e);
                        let _ = world.remove_one::<PatrolOrder>(e);
                        let _ = world.insert_one(e, PatrolOrder { point_a: pos, point_b: target, going_b: true });
                        let _ = world.insert_one(e, MoveOrder { target, speed: 128.0, flags });
                        // Neblokujeme AI – patrol se má chovat jako autonomní pohyb
                    }
                }
            }
            PlayerAction::UseAbility { unit_id, ability_id, target_id, target_x, target_y } => {
                ability_events.push((team, unit_id, ability_id, target_id, target_x, target_y));
            }
            PlayerAction::CancelProduction { building_id } => {
                if let Some(e) = hecs::Entity::from_bits(building_id) {
                    if let Ok(t) = world.get::<&Team>(e) { if t.0 != team { continue; } }
                    if let Ok(mut pq) = world.get::<&mut ProductionQueue>(e) {
                        pq.current = None;
                    }
                }
            }
        }
    }
}

fn apply_cmd(world: &mut World, cmd: ScriptCmd) {
    match cmd {
        ScriptCmd::MoveUnit { entity_id, target_x, target_y, speed } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                let flags = world.get::<&MoveFlags>(e).ok().map(|f| (*f).clone()).unwrap_or_default();
                let _ = world.remove_one::<MoveOrder>(e);
                let _ = world.insert_one(e, MoveOrder { target: Vec2::new(target_x, target_y), speed, flags });
            }
        }
        ScriptCmd::AttackUnit { attacker_id, target_id } => {
            if let (Some(a), Some(t)) = (hecs::Entity::from_bits(attacker_id), hecs::Entity::from_bits(target_id)) {
                let _ = world.remove_one::<AttackOrder>(a);
                let _ = world.insert_one(a, AttackOrder { target: t });
            }
        }
        ScriptCmd::StopUnit { entity_id } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                let _ = world.remove_one::<MoveOrder>(e);
                let _ = world.remove_one::<AttackOrder>(e);
                if let Ok(mut v) = world.get::<&mut Velocity>(e) { v.0 = Vec2::ZERO; }
            }
        }
        ScriptCmd::SetHealth { entity_id, hp } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                if let Ok(mut h) = world.get::<&mut Health>(e) { h.current = hp.clamp(0, h.max); }
            }
        }
        ScriptCmd::KillUnit { entity_id } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                if let Ok(mut h) = world.get::<&mut Health>(e) { h.current = 0; }
            }
        }
        ScriptCmd::SpawnUnit { kind_id, x, y, team } => {
            spawn_unit_by_kind(world, &kind_id, Vec2::new(x, y), team);
        }
        ScriptCmd::TrainUnit { building_id, kind_id, build_time } => {
            if let Some(e) = hecs::Entity::from_bits(building_id) {
                if let Ok(mut pq) = world.get::<&mut ProductionQueue>(e) {
                    let t = if build_time > 0.0 { build_time } else { 30.0 };
                    if pq.current.is_none() { pq.start(kind_id, t); }
                    else { pq.enqueue(kind_id); }
                }
            }
        }
        ScriptCmd::SetRally { building_id, x, y } => {
            if let Some(e) = hecs::Entity::from_bits(building_id) {
                if let Ok(mut pq) = world.get::<&mut ProductionQueue>(e) { pq.rally = Vec2::new(x, y); }
            }
        }
        ScriptCmd::SetAi { entity_id, script_id, tick_interval } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                let _ = world.remove_one::<AiController>(e);
                let _ = world.insert_one(e, AiController::new(script_id, tick_interval.max(0.1)));
            }
        }
        ScriptCmd::SetAbilityCooldown { entity_id, ability_id, cooldown } => {
            if let Some(e) = hecs::Entity::from_bits(entity_id) {
                // Přidej AbilityCooldowns pokud neexistuje
                if world.get::<&AbilityCooldowns>(e).is_err() {
                    let _ = world.insert_one(e, AbilityCooldowns::new());
                }
                if let Ok(mut cd) = world.get::<&mut AbilityCooldowns>(e) {
                    cd.set(ability_id, cooldown);
                }
            }
        }
    }
}

fn unit_info(world: &World, entity: hecs::Entity) -> Option<UnitInfo> {
    let pos  = world.get::<&Position>(entity).ok()?;
    let hp   = world.get::<&Health>(entity).ok()?;
    let team = world.get::<&Team>(entity).ok()?;
    let kind = world.get::<&UnitKindId>(entity).ok()?;
    let (damage, pierce, armor, attack_range) =
        if let Ok(s) = world.get::<&AttackStats>(entity) {
            (s.damage, s.pierce, s.armor, s.range)
        } else { (0, 0, 0, 0.0) };
    Some(UnitInfo {
        entity_id: entity.to_bits().into(),
        x: pos.0.x, y: pos.0.y,
        hp: hp.current, hp_max: hp.max,
        damage, pierce, armor, attack_range,
        team: team.0, kind_id: kind.0.clone(),
    })
}

fn collect_all_infos(world: &World) -> Vec<UnitInfo> {
    world.query::<(&Position, &Health, &Team)>().iter()
        .filter(|(_, (_, hp, _))| hp.is_alive())
        .filter_map(|(e, _)| unit_info(world, e))
        .collect()
}

fn build_snapshot(world: &World, tick: u64) -> ServerMsg {
    let entities: Vec<EntitySnapshot> = world
        .query::<(&Position, &Health, &Team, &UnitKindId)>().iter()
        .filter(|(_, (_, hp, ..))| hp.is_alive())
        .map(|(e, (pos, hp, team, kind))| {
            let (prod_kind, prod_progress, prod_queue_len) =
                if let Ok(pq) = world.get::<&ProductionQueue>(e) {
                    let pk  = pq.current.as_ref().map(|(k, _, _)| k.clone());
                    let pp  = pq.progress();
                    let pql = pq.queue.len().min(255) as u8;
                    (pk, pp, pql)
                } else {
                    (None, 0.0, 0)
                };
            EntitySnapshot {
                id: e.to_bits().into(),
                x: pos.0.x, y: pos.0.y,
                hp: hp.current, hp_max: hp.max,
                team: team.0,
                kind: kind.0.clone(),
                prod_kind,
                prod_progress,
                prod_queue_len,
            }
        }).collect();
    ServerMsg::GameState { tick, entities }
}

fn check_winner(world: &World, num_teams: u8) -> Option<u8> {
    let mut alive_teams: std::collections::HashSet<u8> = std::collections::HashSet::new();
    for (_, (team, hp)) in world.query::<(&Team, &Health)>().iter() {
        if hp.is_alive() { alive_teams.insert(team.0); }
    }
    if alive_teams.len() == 1 { alive_teams.into_iter().next() }
    else if alive_teams.is_empty() { Some(0) }
    else { None }
}

/// Čas výroby jednotky v sekundách.
fn unit_build_time(kind_id: &str) -> f32 {
    match kind_id {
        "peasant" | "peon"              => 15.0,
        "footman" | "grunt"             => 20.0,
        "archer"  | "troll_axethrower"  => 20.0,
        "knight"  | "ogre"              => 30.0,
        "mage"    | "death_knight"      => 35.0,
        "gryphon_rider" | "dragon"      => 45.0,
        _                               => 20.0,
    }
}

fn spawn_unit_by_kind(world: &mut World, kind_id: &str, pos: Vec2, team: u8) -> hecs::Entity {
    struct UDef { hp: i32, dmg: i32, pierce: i32, armor: i32, range: f32, cd: f32, speed: f32, ai: &'static str }
    let d = match kind_id {
        "peasant"          => UDef { hp:30,  dmg:3,  pierce:0,  armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai"  },
        "footman"          => UDef { hp:60,  dmg:6,  pierce:3,  armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai"   },
        "archer"           => UDef { hp:40,  dmg:4,  pierce:6,  armor:0, range:160., cd:1.2, speed:128., ai:"ranged_ai"  },
        "knight"           => UDef { hp:100, dmg:10, pierce:4,  armor:5, range:0.,   cd:1.0, speed:192., ai:"melee_ai"   },
        "mage"             => UDef { hp:35,  dmg:0,  pierce:12, armor:0, range:192., cd:2.0, speed:96.,  ai:"ranged_ai"  },
        "gryphon_rider"    => UDef { hp:100, dmg:16, pierce:5,  armor:5, range:0.,   cd:1.0, speed:384., ai:"melee_ai"   },
        "peon"             => UDef { hp:30,  dmg:3,  pierce:0,  armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai"  },
        "grunt"            => UDef { hp:70,  dmg:8,  pierce:2,  armor:2, range:0.,   cd:1.0, speed:128., ai:"melee_ai"   },
        "troll_axethrower" => UDef { hp:40,  dmg:4,  pierce:6,  armor:0, range:160., cd:1.2, speed:128., ai:"ranged_ai"  },
        "ogre"             => UDef { hp:100, dmg:10, pierce:2,  armor:5, range:0.,   cd:1.3, speed:96.,  ai:"melee_ai"   },
        "death_knight"     => UDef { hp:60,  dmg:0,  pierce:10, armor:2, range:192., cd:2.0, speed:96.,  ai:"ranged_ai"  },
        "dragon"           => UDef { hp:100, dmg:16, pierce:5,  armor:5, range:0.,   cd:1.0, speed:384., ai:"melee_ai"   },
        _                  => UDef { hp:30,  dmg:3,  pierce:0,  armor:0, range:0.,   cd:1.5, speed:128., ai:"worker_ai"  },
    };
    // Létající jednotky mohou přes vodu
    let flags = match kind_id {
        "gryphon_rider" | "dragon" => MoveFlags { can_fly: true, ..Default::default() },
        _                          => MoveFlags::default(),
    };
    world.spawn((
        Position(pos), Velocity(Vec2::ZERO),
        Team(team), Health::new(d.hp),
        UnitKindId(kind_id.to_string()),
        flags,
        AttackStats { damage: d.dmg, pierce: d.pierce, armor: d.armor, range: d.range,
                      cooldown: d.cd, cooldown_left: 0.0 },
    ))
}

/// Spawne budovu (bez AI, s produkční frontou).
fn spawn_building(world: &mut World, kind_id: &str, pos: Vec2, team: u8, hp: i32, queue_cap: usize) -> hecs::Entity {
    world.spawn((
        Position(pos), Velocity(Vec2::ZERO),
        Team(team), Health::new(hp),
        UnitKindId(kind_id.to_string()),
        ProductionQueue { current: None, capacity: queue_cap, queue: Vec::new(), rally: pos + Vec2::new(0., 96.) },
    ))
}

/// Vrátí seznam (base_x, base_y) pro každého hráče.
fn spawn_initial_units(world: &mut World, players: &[LobbyPlayer]) -> Vec<(f32, f32)> {
    // Definice spawn pozic pro 2 hráče (rohová symetrie)
    let starts: &[(f32, f32)] = &[
        (6. * TILE_SIZE,  6. * TILE_SIZE),   // team 0 – levý horní roh
        (54. * TILE_SIZE, 54. * TILE_SIZE),  // team 1 – pravý dolní roh
    ];

    let mut bases = Vec::new();

    for (i, _) in players.iter().enumerate() {
        let team  = i as u8;
        let start = starts.get(i).copied().unwrap_or((
            i as f32 * 28. * TILE_SIZE + 5. * TILE_SIZE,
            5. * TILE_SIZE,
        ));
        let base = Vec2::new(start.0, start.1);
        bases.push((base.x, base.y));

        let (th_kind, barracks_kind, worker_kind, fighter_kind) = if team == 0 {
            ("town_hall", "barracks", "peasant", "footman")
        } else {
            ("great_hall", "orc_barracks", "peon", "grunt")
        };

        // ── Základna ────────────────────────────────────────────────────
        spawn_building(world, th_kind,      base,                                  team, 1200, 5);
        spawn_building(world, barracks_kind, base + Vec2::new(5.*TILE_SIZE, 0.),   team, 800,  5);
        spawn_building(world, "farm",        base + Vec2::new(0., 5.*TILE_SIZE),   team, 400,  0);
        spawn_building(world, "farm",        base + Vec2::new(3.*TILE_SIZE, 5.*TILE_SIZE), team, 400, 0);

        // ── Pracovníci (6 ks v trojúhelníku kolem TH) ───────────────────
        let offsets: &[(f32, f32)] = &[
            (2., 2.), (3., 2.), (4., 2.),
            (2., 3.), (3., 3.), (4., 3.),
        ];
        for &(ox, oy) in offsets {
            spawn_unit_by_kind(world, worker_kind,
                base + Vec2::new(ox * TILE_SIZE, oy * TILE_SIZE), team);
        }

        // ── Počáteční vojsko (4 vojáci před základnou) ──────────────────
        for j in 0..4i32 {
            let off = Vec2::new((j - 1) as f32 * 2. * TILE_SIZE, 8. * TILE_SIZE);
            spawn_unit_by_kind(world, fighter_kind, base + off, team);
        }
    }

    bases
}

fn build_map(_map_id: &str) -> TileMap {
    let w = 64u32;
    let h = 64u32;
    let mut map = TileMap::new_filled(w, h, TileKind::Grass);

    // ── Příroda kolem základen ───────────────────────────────────────────────
    // Les za základnou team 0 (horní levý roh)
    for y in 0u32..6  { for x in 14u32..24 { map.set(x, y, TileKind::Forest); } }
    for y in 14u32..24 { for x in 0u32..6  { map.set(x, y, TileKind::Forest); } }
    // Skála vlevo od základny team 0
    for y in 9u32..13 { map.set(0, y, TileKind::Rock); map.set(1, y, TileKind::Rock); }

    // Les za základnou team 1 (dolní pravý roh)
    for y in 58u32..64 { for x in 40u32..50 { map.set(x, y, TileKind::Forest); } }
    for y in 40u32..50 { for x in 58u32..64 { map.set(x, y, TileKind::Forest); } }
    // Skála vpravo od základny team 1
    for y in 51u32..55 { map.set(62, y, TileKind::Rock); map.set(63, y, TileKind::Rock); }

    // ── Diagonální pruh skály přes střed ────────────────────────────────────
    // Svislá skálová zeď x=28..36, y=20..44 s dvěma průchody
    for y in 20u32..44 {
        if (y >= 27 && y <= 30) || (y >= 36 && y <= 39) {
            continue; // průchod
        }
        for x in 28u32..36 {
            map.set(x, y, TileKind::Rock);
        }
    }

    // ── Řeka přes střed (horizontální) ──────────────────────────────────────
    // Existuje jen vlevo a vpravo od skalní zdi
    for x in 0u32..28 {
        map.set(x, 32, TileKind::Water);
        map.set(x, 33, TileKind::DeepWater);
        map.set(x, 34, TileKind::Water);
    }
    for x in 36u32..64 {
        map.set(x, 32, TileKind::Water);
        map.set(x, 33, TileKind::DeepWater);
        map.set(x, 34, TileKind::Water);
    }
    // Mosty
    for y in 32u32..=34 {
        map.set(12, y, TileKind::Bridge); map.set(13, y, TileKind::Bridge);
        map.set(50, y, TileKind::Bridge); map.set(51, y, TileKind::Bridge);
    }

    // ── Lesy uprostřed (mezi průchody) ──────────────────────────────────────
    for y in 22u32..27 { for x in 18u32..26 { map.set(x, y, TileKind::Forest); } }
    for y in 37u32..42 { for x in 38u32..46 { map.set(x, y, TileKind::Forest); } }

    // ── Nálezové doly (Dirt – dekorativní) ─────────────────────────────────
    for &(dx, dy) in &[(10u32,10u32),(11,10),(10,11),(53u32,53u32),(52,53),(53,52)] {
        map.set(dx, dy, TileKind::Dirt);
    }

    map
}
