use glam::Vec2;
use hecs::World;

use engine::tilemap::{TileKind, TileMap, TILE_SIZE};

use crate::components::{Health, MoveFlags, MoveOrder, Position, Velocity};

// ── Pohybový systém ───────────────────────────────────────────────────────────

/// Spočítá efektivní rychlost na základě druhu dlaždice pod entitou a `MoveFlags`.
fn effective_speed(pos: Vec2, base_speed: f32, flags: &MoveFlags, map: &TileMap) -> f32 {
    // Létající jednotky ignorují terén zcela.
    if flags.can_fly {
        return base_speed;
    }

    let tx = (pos.x / TILE_SIZE) as u32;
    let ty = (pos.y / TILE_SIZE) as u32;
    let kind = map.get(tx, ty).map(|t| t.kind).unwrap_or(TileKind::Grass);

    match kind {
        TileKind::Water | TileKind::DeepWater => {
            if !flags.can_swim {
                return 0.0; // nelze projít – zůstaň stát
            }
            base_speed * flags.speed_water
        }
        TileKind::Rock => 0.0, // skály vždy blokují

        TileKind::Forest                     => base_speed * flags.speed_forest,
        TileKind::Bridge | TileKind::Sand
        | TileKind::Dirt                     => base_speed * flags.speed_road,

        TileKind::Grass                      => base_speed,
    }
}

/// Pohybový systém.
///
/// Vrací seznam entit, které **právě dorazily** na cílovou pozici (MoveOrder odstraněn).
/// Volající může tyto entity použít k vyvolání Lua eventu `on_unit_reached_target`.
pub fn movement_system(world: &mut World, map: &TileMap, dt: f32) -> Vec<hecs::Entity> {
    let mut arrived: Vec<hecs::Entity> = Vec::new();

    // Fáze 1: výpočet velocity z MoveOrder + terrain
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

    // Fáze 2: aplikovat velocity na pozice
    for (_entity, (pos, vel)) in world.query_mut::<(&mut Position, &Velocity)>() {
        pos.0 += vel.0 * dt;
    }

    // Odeber splněné rozkazy
    for &e in &arrived {
        let _ = world.remove_one::<MoveOrder>(e);
    }

    arrived
}

// ── Cleanup ───────────────────────────────────────────────────────────────────

/// Odstraní mrtvé entity.
///
/// Vrací jejich ID jako `u64` (hecs bits) pro Lua event `on_unit_died`.
pub fn cleanup_dead(world: &mut World) -> Vec<u64> {
    let dead: Vec<hecs::Entity> = world
        .query::<&Health>()
        .iter()
        .filter(|(_, h)| !h.is_alive())
        .map(|(e, _)| e)
        .collect();

    let ids: Vec<u64> = dead.iter().map(|e| e.to_bits().into()).collect();

    for e in dead {
        let _ = world.despawn(e);
    }

    ids
}
