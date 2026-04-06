//! Server-side herní systémy.

use glam::Vec2;
use hecs::World;

use crate::world::*;

fn effective_speed(pos: Vec2, base: f32, flags: &MoveFlags, map: &TileMap) -> f32 {
    if flags.can_fly { return base; }
    let tx = (pos.x / TILE_SIZE) as u32;
    let ty = (pos.y / TILE_SIZE) as u32;
    let kind = map.get(tx, ty).unwrap_or(TileKind::Grass);
    match kind {
        TileKind::Water | TileKind::DeepWater => {
            if !flags.can_swim { return 0.0; }
            base * flags.speed_water
        }
        TileKind::Rock    => 0.0,
        TileKind::Forest  => base * flags.speed_forest,
        TileKind::Bridge | TileKind::Sand | TileKind::Dirt => base * flags.speed_road,
        TileKind::Grass   => base,
    }
}

pub fn movement_system(world: &mut World, map: &TileMap, dt: f32) -> Vec<hecs::Entity> {
    let mut arrived = Vec::new();

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
    for &e in &arrived { let _ = world.remove_one::<MoveOrder>(e); }
    arrived
}

pub struct AttackEvent { pub attacker_id: u64, pub target_id: u64, pub damage: i32 }

pub fn attack_system(world: &mut World, dt: f32) -> Vec<AttackEvent> {
    let mut events = Vec::new();

    for (_, stats) in world.query_mut::<&mut AttackStats>() {
        stats.cooldown_left = (stats.cooldown_left - dt).max(0.0);
    }

    let pairs: Vec<(hecs::Entity, hecs::Entity)> = world
        .query::<&AttackOrder>().iter().map(|(e, o)| (e, o.target)).collect();

    for (attacker, target) in pairs {
        let target_pos: Vec2 = {
            let g = world.get::<&Position>(target);
            match g.ok().map(|r| r.0) {
                Some(p) => p,
                None => { let _ = world.remove_one::<AttackOrder>(attacker); continue; }
            }
        };
        let attacker_pos: Vec2 = match world.get::<&Position>(attacker).ok().map(|r| r.0) {
            Some(p) => p, None => continue,
        };
        let range: f32 = { world.get::<&AttackStats>(attacker).ok()
            .map(|s| if s.range <= 0.0 { TILE_SIZE * 1.5 } else { s.range })
            .unwrap_or(TILE_SIZE * 1.5) };

        let dist = (target_pos - attacker_pos).length();

        if dist > range {
            let speed = 120.0f32;
            let flags = world.get::<&MoveFlags>(attacker).ok()
                .map(|f| (*f).clone()).unwrap_or_default();
            let _ = world.remove_one::<MoveOrder>(attacker);
            let _ = world.insert_one(attacker, MoveOrder { target: target_pos, speed, flags });
        } else {
            let _ = world.remove_one::<MoveOrder>(attacker);
            if let Ok(mut vel) = world.get::<&mut Velocity>(attacker) { vel.0 = Vec2::ZERO; }

            let can_attack = world.get::<&AttackStats>(attacker)
                .ok().map(|s| s.cooldown_left <= 0.0).unwrap_or(false);

            if can_attack {
                let (dmg, pierce, cd) = match world.get::<&AttackStats>(attacker) {
                    Ok(s) => (s.damage, s.pierce, s.cooldown), Err(_) => continue,
                };
                let armor = world.get::<&AttackStats>(target).ok().map(|s| s.armor).unwrap_or(0);
                let total = ((dmg - armor).max(1) + pierce).max(1);

                if let Ok(mut hp) = world.get::<&mut Health>(target) {
                    hp.current = (hp.current - total).max(0);
                }
                if let Ok(mut stats) = world.get::<&mut AttackStats>(attacker) {
                    stats.cooldown_left = cd;
                }
                events.push(AttackEvent {
                    attacker_id: attacker.to_bits().into(),
                    target_id:   target.to_bits().into(),
                    damage:      total,
                });
            }
        }
    }
    events
}

pub struct ProductionDone { pub building_id: u64, pub kind_id: String, pub rally: Vec2, pub team: u8 }

pub fn production_system(world: &mut World, dt: f32) -> Vec<ProductionDone> {
    let mut done = Vec::new();
    for (entity, (pq, team)) in world.query_mut::<(&mut ProductionQueue, &Team)>() {
        if pq.current.is_none() {
            if let Some(next) = pq.queue.first().cloned() {
                pq.queue.remove(0);
                let total = default_build_time(&next);
                pq.current = Some((next, total, total));
            }
        }
        if let Some((ref kind_id, ref mut timer, _total)) = pq.current {
            *timer -= dt;
            if *timer <= 0.0 {
                done.push(ProductionDone {
                    building_id: entity.to_bits().into(),
                    kind_id: kind_id.clone(),
                    rally: pq.rally,
                    team: team.0,
                });
                pq.current = None;
            }
        }
    }
    done
}

fn default_build_time(kind: &str) -> f32 {
    match kind {
        "peasant" | "peon"              => 15.0,
        "footman" | "grunt"             => 20.0,
        "archer"  | "troll_axethrower"  => 20.0,
        "knight"  | "ogre"              => 30.0,
        "mage"    | "death_knight"      => 35.0,
        "gryphon_rider" | "dragon"      => 45.0,
        _                               => 20.0,
    }
}

// ── Patrol systém ─────────────────────────────────────────────────────────────

pub fn patrol_system(world: &mut World) {
    // Sbíráme entity s PatrolOrder
    let patrolling: Vec<(hecs::Entity, Vec2, Vec2, bool)> = world
        .query::<&PatrolOrder>().iter()
        .map(|(e, p)| (e, p.point_a, p.point_b, p.going_b))
        .collect();

    for (entity, point_a, point_b, going_b) in patrolling {
        // Pokud má MoveOrder nebo AttackOrder → stále se pohybuje/útočí, nech ho
        let busy = world.get::<&MoveOrder>(entity).is_ok()
                || world.get::<&AttackOrder>(entity).is_ok();
        if busy { continue; }

        // Dorazil na waypoint – otočíme směr
        if let Ok(mut p) = world.get::<&mut PatrolOrder>(entity) {
            p.going_b = !going_b;
        }
        let target = if going_b { point_a } else { point_b };
        let flags  = world.get::<&MoveFlags>(entity).ok()
            .map(|f| (*f).clone()).unwrap_or_default();
        let _      = world.insert_one(entity, MoveOrder { target, speed: 128.0, flags });
    }
}

// ── Cooldown schopností ───────────────────────────────────────────────────────

pub fn ability_cooldown_system(world: &mut World, dt: f32) {
    for (_, cd) in world.query_mut::<&mut AbilityCooldowns>() {
        cd.tick(dt);
    }
}

pub struct AiTickEvent { pub entity_id: u64, pub script_id: String }

pub fn ai_tick_system(world: &mut World, dt: f32) -> Vec<AiTickEvent> {
    let mut ticks = Vec::new();
    for (entity, ctrl) in world.query_mut::<&mut AiController>() {
        // Pokud hráč vydal rozkaz, AI čeká
        if ctrl.player_override > 0 {
            ctrl.player_override -= 1;
            ctrl.tick_timer = ctrl.tick_interval; // resetuj timer
            continue;
        }
        ctrl.tick_timer -= dt;
        if ctrl.tick_timer <= 0.0 {
            ctrl.tick_timer = ctrl.tick_interval;
            ticks.push(AiTickEvent { entity_id: entity.to_bits().into(), script_id: ctrl.script_id.clone() });
        }
    }
    ticks
}

pub struct DeadInfo { pub id: u64, pub kind_id: String, pub team: u8, pub pos: Vec2 }

pub fn cleanup_dead(world: &mut World) -> Vec<DeadInfo> {
    let dead: Vec<(hecs::Entity, DeadInfo)> = world
        .query::<(&Health, &Position, &Team, &UnitKindId)>().iter()
        .filter(|(_, (h, ..))| !h.is_alive())
        .map(|(e, (_, pos, team, kind))| (e, DeadInfo {
            id: e.to_bits().into(), kind_id: kind.0.clone(), team: team.0, pos: pos.0,
        })).collect();

    let infos = dead.iter().map(|(_, d)| DeadInfo {
        id: d.id, kind_id: d.kind_id.clone(), team: d.team, pos: d.pos,
    }).collect();
    for (e, _) in dead { let _ = world.despawn(e); }
    infos
}
