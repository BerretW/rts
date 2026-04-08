use glam::Vec2;
use hecs::World;

use engine::tilemap::{TileKind, TileMap, TILE_SIZE};

use crate::components::{
    AttackOrder, AttackStats, AiController, Health,
    HarvestKind, HarvestOrder, HarvestState,
    MoveFlags, MoveOrder, Position, ProductionQueue, Velocity,
};

// ── Pohybový systém ───────────────────────────────────────────────────────────

fn effective_speed(pos: Vec2, base: f32, flags: &MoveFlags, map: &TileMap) -> f32 {
    if flags.can_fly { return base; }
    let tx = (pos.x / TILE_SIZE) as u32;
    let ty = (pos.y / TILE_SIZE) as u32;
    let kind = map.get(tx, ty).map(|t| t.kind).unwrap_or(TileKind::Grass);
    match kind {
        TileKind::Water | TileKind::DeepWater => {
            if !flags.can_swim { return 0.0; }
            base * flags.speed_water
        }
        TileKind::Rock                               => 0.0,
        TileKind::Forest                             => base * flags.speed_forest,
        TileKind::Bridge | TileKind::Sand | TileKind::Dirt => base * flags.speed_road,
        TileKind::Grass                              => base,
    }
}

/// Pohybový systém. Vrátí entity, které **právě dorazily** na cíl.
pub fn movement_system(world: &mut World, map: &TileMap, dt: f32) -> Vec<hecs::Entity> {
    let mut arrived: Vec<hecs::Entity> = Vec::new();

    for (entity, (pos, order, vel)) in
        world.query_mut::<(&Position, &MoveOrder, &mut Velocity)>()
    {
        let dir  = order.target - pos.0;
        let dist = dir.length();
        if dist < 2.0 {
            vel.0 = Vec2::ZERO;
            arrived.push(entity);
        } else {
            let spd = effective_speed(pos.0, order.speed, &order.flags, map);
            vel.0 = if spd > 0.0 { dir.normalize() * spd } else { Vec2::ZERO };
        }
    }

    for (_e, (pos, vel)) in world.query_mut::<(&mut Position, &Velocity)>() {
        pos.0 += vel.0 * dt;
    }

    for &e in &arrived {
        let _ = world.remove_one::<MoveOrder>(e);
    }
    arrived
}

// ── Bojový systém ─────────────────────────────────────────────────────────────

pub struct AttackEvent {
    pub attacker_id: u64,
    pub target_id:   u64,
    pub damage:      i32,
}

/// Bojový systém.
///
/// * Přesune útočníky do dosahu cíle (přidá MoveOrder pokud mimo dosah).
/// * Po dosažení dosahu útočí každý `cooldown` sekund.
/// * Vrátí seznam útočných событий pro Lua hooky (on_unit_attack, on_unit_hit).
/// * Mrtví zůstávají – odstraní je cleanup_dead.
pub fn attack_system(world: &mut World, map: &TileMap, dt: f32) -> Vec<AttackEvent> {
    let mut events = Vec::new();

    // Cooldown tick
    for (_, stats) in world.query_mut::<&mut AttackStats>() {
        stats.cooldown_left = (stats.cooldown_left - dt).max(0.0);
    }

    // Sbíráme co potřebujeme mimo query_mut (borrow checker)
    let pairs: Vec<(hecs::Entity, hecs::Entity)> = world
        .query::<&AttackOrder>()
        .iter()
        .map(|(e, o)| (e, o.target))
        .collect();

    for (attacker, target) in pairs {
        // Pozice cíle – zkopírujeme Vec2 a pustíme Ref před mutable borrowem
        let target_pos_opt: Option<Vec2> = {
            let g = world.get::<&Position>(target);
            g.ok().map(|r| r.0)
        };
        let target_pos = match target_pos_opt {
            Some(p) => p,
            None => {
                let _ = world.remove_one::<AttackOrder>(attacker);
                continue;
            }
        };

        let attacker_pos: Vec2 = match { world.get::<&Position>(attacker).ok().map(|r| r.0) } {
            Some(p) => p,
            None => continue,
        };

        let range: f32 = {
            let g = world.get::<&AttackStats>(attacker);
            g.ok().map(|s| if s.range <= 0.0 { TILE_SIZE * 1.5 } else { s.range })
                .unwrap_or(TILE_SIZE * 1.5)
        };

        let dist = (target_pos - attacker_pos).length();

        if dist > range {
            // Přesun do dosahu
            let speed = 120.0f32;
            let flags  = world.get::<&MoveFlags>(attacker)
                .ok()
                .map(|f| (*f).clone())
                .unwrap_or_default();
            let _ = world.remove_one::<MoveOrder>(attacker);
            let _ = world.insert_one(attacker, MoveOrder {
                target: target_pos,
                speed,
                flags,
            });
        } else {
            // Stop + útok
            let _ = world.remove_one::<MoveOrder>(attacker);
            if let Ok(mut vel) = world.get::<&mut Velocity>(attacker) {
                vel.0 = Vec2::ZERO;
            }

            let can_attack = world.get::<&AttackStats>(attacker)
                .map(|s| s.cooldown_left <= 0.0)
                .unwrap_or(false);

            if can_attack {
                let (atk_dmg, atk_pierce, atk_cooldown) = match world.get::<&AttackStats>(attacker) {
                    Ok(s) => (s.damage, s.pierce, s.cooldown),
                    Err(_) => continue,
                };
                let target_armor = world.get::<&AttackStats>(target)
                    .map(|s| s.armor)
                    .unwrap_or(0);

                let dmg = ((atk_dmg - target_armor).max(1) + atk_pierce).max(1);

                if let Ok(mut hp) = world.get::<&mut Health>(target) {
                    hp.current = (hp.current - dmg).max(0);
                }
                if let Ok(mut stats) = world.get::<&mut AttackStats>(attacker) {
                    stats.cooldown_left = atk_cooldown;
                }
                events.push(AttackEvent {
                    attacker_id: attacker.to_bits().into(),
                    target_id:   target.to_bits().into(),
                    damage:      dmg,
                });
            }
        }
    }

    events
}

// ── Výrobní systém ────────────────────────────────────────────────────────────

pub struct ProductionDone {
    pub building_id: u64,
    pub kind_id:     String,
    pub rally:       Vec2,
    pub team:        u8,
}

/// Systém výroby budov. Vrátí hotové výroby pro spawn a Lua hook.
pub fn production_system(world: &mut World, dt: f32) -> Vec<ProductionDone> {
    let mut done = Vec::new();

    for (entity, (pq, team)) in world.query_mut::<(&mut ProductionQueue, &crate::components::Team)>() {
        // Posuň frontu – vezmi první pokud nic nevyrábíme
        if pq.current.is_none() {
            if let Some(next) = pq.queue.first().cloned() {
                pq.queue.remove(0);
                // Doba výroby – defaults; přepsat přes Lua TrainUnit s časem
                pq.current = Some((next, 15.0));
            }
        }

        if let Some((ref kind_id, ref mut timer)) = pq.current {
            *timer -= dt;
            if *timer <= 0.0 {
                done.push(ProductionDone {
                    building_id: entity.to_bits().into(),
                    kind_id:     kind_id.clone(),
                    rally:       pq.rally,
                    team:        team.0,
                });
                pq.current = None;
            }
        }
    }

    done
}

// ── AI tick systém ────────────────────────────────────────────────────────────

pub struct AiTickEvent {
    pub entity_id: u64,
    pub script_id: String,
}

/// Snižuje timer AI controllerů a vrátí ty, které potřebují tick.
pub fn ai_tick_system(world: &mut World, dt: f32) -> Vec<AiTickEvent> {
    let mut ticks = Vec::new();
    for (entity, ctrl) in world.query_mut::<&mut AiController>() {
        ctrl.tick_timer -= dt;
        if ctrl.tick_timer <= 0.0 {
            ctrl.tick_timer = ctrl.tick_interval;
            ticks.push(AiTickEvent {
                entity_id: entity.to_bits().into(),
                script_id: ctrl.script_id.clone(),
            });
        }
    }
    ticks
}

// ── Sklizeň surovin ───────────────────────────────────────────────────────────

pub struct HarvestResult {
    pub gold:   u32,
    pub lumber: u32,
    pub team:   u8,
}

/// Spravuje stavový stroj sklizně pracovníků.
///
/// Fáze:  GoingToSource → Harvesting (čas 4 s) → GoingToDepot → GoingToSource …
/// Vrátí suroviny, které pracovníci právě odevzdali ve skladu.
pub fn harvest_system(world: &mut World, dt: f32) -> Vec<HarvestResult> {
    // ── Fáze 1: odpočítej čas u zdroje ────────────────────────────────────────
    let harvesting_done: Vec<hecs::Entity> = {
        let mut done = Vec::new();
        for (entity, harv) in world.query_mut::<&mut HarvestOrder>() {
            if harv.state == HarvestState::Harvesting {
                harv.timer -= dt;
                if harv.timer <= 0.0 {
                    done.push(entity);
                }
            }
        }
        done
    };

    // ── Fáze 2: hotová těžba → jdi do skladu ─────────────────────────────────
    for entity in harvesting_done {
        let (depot, max_carry) = {
            let h = match world.get::<&HarvestOrder>(entity) { Ok(h) => h, Err(_) => continue };
            (h.depot, h.max_carry)
        };
        if let Ok(mut h) = world.get::<&mut HarvestOrder>(entity) {
            h.carried = max_carry;
            h.state   = HarvestState::GoingToDepot;
        }
        let flags = world.get::<&MoveFlags>(entity)
            .ok().map(|f| (*f).clone()).unwrap_or_default();
        let _ = world.insert_one(entity, MoveOrder { target: depot, speed: 128.0, flags });
    }

    // ── Fáze 3: příchod ke zdroji → začni těžit ───────────────────────────────
    let source_arrivals: Vec<hecs::Entity> = world
        .query::<(&Position, &HarvestOrder)>()
        .iter()
        .filter(|(_, (pos, h))| {
            h.state == HarvestState::GoingToSource
            && (pos.0 - h.source).length() < TILE_SIZE * 1.8
        })
        .map(|(e, _)| e)
        .collect();

    for entity in source_arrivals {
        let _ = world.remove_one::<MoveOrder>(entity);
        if let Ok(mut v) = world.get::<&mut Velocity>(entity) { v.0 = Vec2::ZERO; }
        if let Ok(mut h) = world.get::<&mut HarvestOrder>(entity) {
            h.state = HarvestState::Harvesting;
            h.timer = 4.0;
        }
    }

    // ── Fáze 4: příchod do skladu → odevzdej ─────────────────────────────────
    struct DepotArrival {
        entity: hecs::Entity,
        team:   u8,
        gold:   u32,
        lumber: u32,
        source: Vec2,
    }

    let depot_arrivals: Vec<DepotArrival> = world
        .query::<(&Position, &HarvestOrder, &crate::components::Team)>()
        .iter()
        .filter(|(_, (pos, h, _))| {
            h.state == HarvestState::GoingToDepot
            && (pos.0 - h.depot).length() < TILE_SIZE * 2.0
        })
        .map(|(e, (_, h, t))| DepotArrival {
            entity: e,
            team:   t.0,
            gold:   if h.kind == HarvestKind::Gold   { h.max_carry } else { 0 },
            lumber: if h.kind == HarvestKind::Lumber { h.max_carry } else { 0 },
            source: h.source,
        })
        .collect();

    let mut results = Vec::new();
    for arr in depot_arrivals {
        results.push(HarvestResult { gold: arr.gold, lumber: arr.lumber, team: arr.team });
        if let Ok(mut h) = world.get::<&mut HarvestOrder>(arr.entity) {
            h.carried = 0;
            h.state   = HarvestState::GoingToSource;
        }
        let flags = world.get::<&MoveFlags>(arr.entity)
            .ok().map(|f| (*f).clone()).unwrap_or_default();
        let _ = world.insert_one(arr.entity,
            MoveOrder { target: arr.source, speed: 128.0, flags });
    }

    results
}

// ── Cleanup ───────────────────────────────────────────────────────────────────

pub struct DeadInfo {
    pub id:       u64,
    pub kind_id:  String,
    pub team:     u8,
    pub pos:      Vec2,
}

/// Odstraní mrtvé entity. Vrátí jejich snapshot pro Lua on_unit_died.
pub fn cleanup_dead(world: &mut World) -> Vec<DeadInfo> {
    let dead: Vec<(hecs::Entity, DeadInfo)> = world
        .query::<(&Health, &Position, &crate::components::Team, &crate::components::UnitKindId)>()
        .iter()
        .filter(|(_, (h, ..))| !h.is_alive())
        .map(|(e, (_, pos, team, kind))| (e, DeadInfo {
            id:      e.to_bits().into(),
            kind_id: kind.0.clone(),
            team:    team.0,
            pos:     pos.0,
        }))
        .collect();

    let infos: Vec<DeadInfo> = dead.iter().map(|(_, d)| DeadInfo {
        id:      d.id,
        kind_id: d.kind_id.clone(),
        team:    d.team,
        pos:     d.pos,
    }).collect();

    for (e, _) in dead {
        let _ = world.despawn(e);
    }
    infos
}
