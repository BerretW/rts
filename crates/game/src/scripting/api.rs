//! Registrace Rust nativních funkcí dostupných z Lua jako `Engine.*`.
//!
//! Všechny příkazy jdou přes __cmd_queue (zpracuje Rust po Lua ticku).
//! Queries využívají __query_result který Rust naplní před voláním callbacku.

use mlua::prelude::*;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.globals().set("__cmd_queue", lua.create_table()?)?;
    lua.globals().set("__query_result", lua.create_table()?)?;

    let e = lua.create_table()?;

    // ── Pohyb ─────────────────────────────────────────────────────────────────

    // Engine.move_unit(entity_id, tx, ty, params?)
    e.set("move_unit", lua.create_function(|lua, (id, tx, ty, params_opt): (u64, f32, f32, Option<LuaTable>)| {
        let params = default_params_table(lua)?;
        if let Some(user) = params_opt {
            for pair in user.pairs::<LuaValue, LuaValue>() {
                let (k, v) = pair?; params.set(k, v)?;
            }
        }
        let cmd = lua.create_table()?;
        cmd.set("cmd",       "move_unit")?;
        cmd.set("entity_id", id)?;
        cmd.set("target_x",  tx)?;
        cmd.set("target_y",  ty)?;
        cmd.set("params",    params)?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.stop_unit(entity_id)
    e.set("stop_unit", lua.create_function(|lua, id: u64| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "stop_unit")?; cmd.set("entity_id", id)?;
        push_cmd(lua, cmd)
    })?)?;

    // ── Boj ───────────────────────────────────────────────────────────────────

    // Engine.attack_unit(attacker_id, target_id)
    e.set("attack_unit", lua.create_function(|lua, (attacker, target): (u64, u64)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",         "attack_unit")?;
        cmd.set("attacker_id", attacker)?;
        cmd.set("target_id",   target)?;
        push_cmd(lua, cmd)
    })?)?;

    // ── HP / smrt ─────────────────────────────────────────────────────────────

    // Engine.set_health(entity_id, hp)
    e.set("set_health", lua.create_function(|lua, (id, hp): (u64, i32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "set_health")?; cmd.set("entity_id", id)?; cmd.set("hp", hp)?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.kill_unit(entity_id)
    e.set("kill_unit", lua.create_function(|lua, id: u64| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "kill_unit")?; cmd.set("entity_id", id)?;
        push_cmd(lua, cmd)
    })?)?;

    // ── Suroviny ──────────────────────────────────────────────────────────────

    // Engine.add_resources({gold=n, lumber=n, oil=n})
    e.set("add_resources", lua.create_function(|lua, tbl: LuaTable| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",    "add_resources")?;
        cmd.set("gold",   tbl.get::<i32>("gold")  .unwrap_or(0))?;
        cmd.set("lumber", tbl.get::<i32>("lumber").unwrap_or(0))?;
        cmd.set("oil",    tbl.get::<i32>("oil")   .unwrap_or(0))?;
        push_cmd(lua, cmd)
    })?)?;

    // ── Spawn / výroba ────────────────────────────────────────────────────────

    // Engine.spawn_unit(kind_id, x, y, team?)
    e.set("spawn_unit", lua.create_function(|lua, (kind, x, y, team): (String, f32, f32, Option<u8>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",     "spawn_unit")?;
        cmd.set("kind_id", kind)?;
        cmd.set("x",       x)?; cmd.set("y", y)?;
        cmd.set("team",    team.unwrap_or(0))?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.train_unit(building_id, kind_id, build_time?)
    e.set("train_unit", lua.create_function(|lua, (bid, kind, time): (u64, String, Option<f32>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",         "train_unit")?;
        cmd.set("building_id", bid)?;
        cmd.set("kind_id",     kind)?;
        cmd.set("build_time",  time.unwrap_or(0.0))?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.set_rally(building_id, x, y)
    e.set("set_rally", lua.create_function(|lua, (bid, x, y): (u64, f32, f32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",         "set_rally")?;
        cmd.set("building_id", bid)?;
        cmd.set("x",           x)?; cmd.set("y", y)?;
        push_cmd(lua, cmd)
    })?)?;

    // ── AI ────────────────────────────────────────────────────────────────────

    // Engine.set_ai(entity_id, script_id, tick_interval?)
    e.set("set_ai", lua.create_function(|lua, (id, script, interval): (u64, String, Option<f32>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",           "set_ai")?;
        cmd.set("entity_id",     id)?;
        cmd.set("script_id",     script)?;
        cmd.set("tick_interval", interval.unwrap_or(1.0))?;
        push_cmd(lua, cmd)
    })?)?;

    // Engine.set_ai_state(entity_id, state_json_string)
    e.set("set_ai_state", lua.create_function(|lua, (id, json): (u64, String)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd",        "set_ai_state")?;
        cmd.set("entity_id",  id)?;
        cmd.set("state_json", json)?;
        push_cmd(lua, cmd)
    })?)?;

    // ── Query ─────────────────────────────────────────────────────────────────

    // Engine.query_units(filter?)
    // filter = { team=n, kind="x", x=n, y=n, radius=n }
    // Vrátí tabulku z __query_result (předem naplněnou Rustem přes push_query_results).
    // Jednodušší: Lua jen čte __query_result přímo. Tato funkce ho jen vrátí.
    e.set("query_units", lua.create_function(|lua, filter: Option<LuaTable>| {
        // Uloží filtr aby si ho Rust přečetl při drain_commands
        lua.globals().set("__query_filter", filter)?;
        // Vrátí aktuální výsledek (naplní Rust před voláním AI ticku)
        let result: LuaTable = lua.globals().get("__query_result")?;
        Ok(result)
    })?)?;

    // Engine.get_unit(entity_id) → unit table | nil
    // Rust naplní __unit_cache[entity_id] před AI tickem; zde jen lookup.
    e.set("get_unit", lua.create_function(|lua, id: u64| {
        let cache: Option<LuaTable> = lua.globals().get("__unit_cache")?;
        if let Some(c) = cache {
            let v: LuaValue = c.get(id)?;
            return Ok(v);
        }
        Ok(LuaValue::Nil)
    })?)?;

    // ── Log ───────────────────────────────────────────────────────────────────

    e.set("log", lua.create_function(|_lua, msg: String| {
        log::info!("[Lua] {}", msg);
        Ok(())
    })?)?;

    // ── Util: tile size ───────────────────────────────────────────────────────
    e.set("TILE_SIZE", 32.0f32)?;

    // ── Asset loading ─────────────────────────────────────────────────────────

    // Engine.assets_dir() → string – cesta ke složce assets/
    e.set("assets_dir", lua.create_function(|_, ()| {
        let dir = locate_assets_dir();
        Ok(dir.to_string_lossy().to_string())
    })?)?;

    // Engine.load_json(path) → table
    // Načte JSON soubor relativně k assets_dir a vrátí ho jako Lua tabulku.
    e.set("load_json", lua.create_function(|lua, path: String| {
        let full = locate_assets_dir().join(&path);
        let text = std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("load_json {:?}: {e}", full)))?;
        let val: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| LuaError::RuntimeError(format!("load_json parse: {e}")))?;
        json_to_lua(lua, &val)
    })?)?;

    // Engine.load_asset_text(path) → string
    e.set("load_asset_text", lua.create_function(|_, path: String| {
        let full = locate_assets_dir().join(&path);
        std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("load_asset_text {:?}: {e}", full)))
    })?)?;

    lua.globals().set("Engine", e)?;
    Ok(())
}

// ── Asset helpers ─────────────────────────────────────────────────────────────

/// Najde složku assets/ – vedle exe nebo od working directory.
fn locate_assets_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.parent().unwrap_or(std::path::Path::new(".")).join("assets");
        if c.exists() { return c; }
    }
    std::path::PathBuf::from("assets")
}

/// Převede serde_json::Value na Lua hodnotu.
fn json_to_lua(lua: &mlua::Lua, val: &serde_json::Value) -> LuaResult<LuaValue> {
    match val {
        serde_json::Value::Null       => Ok(LuaValue::Nil),
        serde_json::Value::Bool(b)    => Ok(LuaValue::Boolean(*b)),
        serde_json::Value::Number(n)  => {
            if let Some(i) = n.as_i64() { Ok(LuaValue::Integer(i)) }
            else { Ok(LuaValue::Number(n.as_f64().unwrap_or(0.0))) }
        }
        serde_json::Value::String(s)  => Ok(LuaValue::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.raw_set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
        serde_json::Value::Object(obj) => {
            let t = lua.create_table()?;
            for (k, v) in obj { t.raw_set(k.as_str(), json_to_lua(lua, v)?)?; }
            Ok(LuaValue::Table(t))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn push_cmd(lua: &Lua, cmd: LuaTable) -> LuaResult<()> {
    let queue: LuaTable = lua.globals().get("__cmd_queue")?;
    let len = queue.raw_len();
    queue.raw_set(len + 1, cmd)?;
    Ok(())
}

fn default_params_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("speed",        128.0f32)?;
    t.set("can_swim",     false)?;
    t.set("can_fly",      false)?;
    t.set("speed_water",  0.0f32)?;
    t.set("speed_forest", 0.75f32)?;
    t.set("speed_road",   1.0f32)?;
    Ok(t)
}
