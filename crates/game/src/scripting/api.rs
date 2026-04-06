//! Registrace Rust nativních funkcí dostupných z Lua jako globální tabulka `Engine`.
//!
//! Lua může volat:
//!
//! ```lua
//! -- Pohyb jednotky
//! Engine.move_unit(entity_id, target_x, target_y, {
//!     speed        = 180.0,
//!     can_swim     = false,
//!     can_fly      = false,
//!     speed_water  = 0.0,
//!     speed_forest = 0.5,
//!     speed_road   = 1.2,
//! })
//!
//! -- Nastavit HP
//! Engine.set_health(entity_id, 50)
//!
//! -- Zabít jednotku
//! Engine.kill_unit(entity_id)
//!
//! -- Přidat/odebrat suroviny
//! Engine.add_resources({ gold = 100, lumber = -20 })
//!
//! -- Spawnovat jednotku
//! Engine.spawn_unit("peasant", x, y, team)
//!
//! -- Log
//! Engine.log("ahoj ze skriptu!")
//! ```
//!
//! Všechny příkazy se jen vloží do `__cmd_queue` – Rust je aplikuje po Lua ticku.

use mlua::prelude::*;

pub fn register(lua: &Lua) -> LuaResult<()> {
    // Inicializuj frontu příkazů
    lua.globals().set("__cmd_queue", lua.create_table()?)?;

    let engine = lua.create_table()?;

    // Engine.move_unit(entity_id, tx, ty, params_table?)
    engine.set("move_unit", lua.create_function(|lua, args: (u64, f32, f32, Option<LuaTable>)| {
        let (entity_id, target_x, target_y, params_opt) = args;

        let params = lua.create_table()?;
        // Výchozí hodnoty – Lua může přepsat libovolnou
        params.set("speed",        128.0f32)?;
        params.set("can_swim",     false)?;
        params.set("can_fly",      false)?;
        params.set("speed_water",  0.0f32)?;
        params.set("speed_forest", 0.75f32)?;
        params.set("speed_road",   1.0f32)?;

        // Přepiš klíče z tabulky dodané skriptem
        if let Some(user_params) = params_opt {
            for pair in user_params.pairs::<LuaValue, LuaValue>() {
                let (k, v) = pair?;
                params.set(k, v)?;
            }
        }

        let cmd = lua.create_table()?;
        cmd.set("cmd",       "move_unit")?;
        cmd.set("entity_id", entity_id)?;
        cmd.set("target_x",  target_x)?;
        cmd.set("target_y",  target_y)?;
        cmd.set("params",    params)?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.set_health(entity_id, hp)
    engine.set("set_health", lua.create_function(|lua, (entity_id, hp): (u64, i32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",       "set_health")?;
        cmd.set("entity_id", entity_id)?;
        cmd.set("hp",        hp)?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.kill_unit(entity_id)
    engine.set("kill_unit", lua.create_function(|lua, entity_id: u64| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",       "kill_unit")?;
        cmd.set("entity_id", entity_id)?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.add_resources({ gold, lumber, oil })
    engine.set("add_resources", lua.create_function(|lua, tbl: LuaTable| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",    "add_resources")?;
        cmd.set("gold",   tbl.get::<i32>("gold")  .unwrap_or(0))?;
        cmd.set("lumber", tbl.get::<i32>("lumber").unwrap_or(0))?;
        cmd.set("oil",    tbl.get::<i32>("oil")   .unwrap_or(0))?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.spawn_unit(kind_id, x, y, team?)
    engine.set("spawn_unit", lua.create_function(|lua, (kind_id, x, y, team): (String, f32, f32, Option<u8>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",     "spawn_unit")?;
        cmd.set("kind_id", kind_id)?;
        cmd.set("x",       x)?;
        cmd.set("y",       y)?;
        cmd.set("team",    team.unwrap_or(0))?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.log(msg)
    engine.set("log", lua.create_function(|_lua, msg: String| {
        log::info!("[Lua] {}", msg);
        Ok(())
    })?)?;

    lua.globals().set("Engine", engine)?;
    Ok(())
}

/// Vloží příkaz (tabulku) do globální fronty `__cmd_queue`.
fn push_cmd(lua: &Lua, cmd: LuaTable) -> LuaResult<()> {
    let queue: LuaTable = lua.globals().get("__cmd_queue")?;
    let len = queue.raw_len();
    queue.raw_set(len + 1, cmd)?;
    Ok(())
}
