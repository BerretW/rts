//! Server-side Lua runtime.
//!
//! Stejný design jako game/src/scripting, ale bez renderovacích importů.
//! Navíc: Engine.load_json / Engine.load_asset_text / Engine.assets_dir.

use std::path::{Path, PathBuf};
use mlua::prelude::*;

use crate::world::UnitKindId;

// ── UnitInfo snapshot ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub entity_id:    u64,
    pub x:            f32,
    pub y:            f32,
    pub hp:           i32,
    pub hp_max:       i32,
    pub damage:       i32,
    pub pierce:       i32,
    pub armor:        i32,
    pub attack_range: f32,
    pub team:         u8,
    pub kind_id:      String,
}

pub fn unit_to_table(lua: &Lua, u: &UnitInfo) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("entity_id",    u.entity_id)?;
    t.set("x",            u.x)?;
    t.set("y",            u.y)?;
    t.set("hp",           u.hp)?;
    t.set("hp_max",       u.hp_max)?;
    t.set("damage",       u.damage)?;
    t.set("pierce",       u.pierce)?;
    t.set("armor",        u.armor)?;
    t.set("attack_range", u.attack_range)?;
    t.set("team",         u.team)?;
    t.set("kind",         u.kind_id.clone())?;
    Ok(t)
}

// ── ScriptCmd ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ScriptCmd {
    MoveUnit    { entity_id: u64, target_x: f32, target_y: f32, speed: f32 },
    AttackUnit  { attacker_id: u64, target_id: u64 },
    StopUnit    { entity_id: u64 },
    SetHealth   { entity_id: u64, hp: i32 },
    KillUnit    { entity_id: u64 },
    SpawnUnit   { kind_id: String, x: f32, y: f32, team: u8 },
    TrainUnit   { building_id: u64, kind_id: String, build_time: f32 },
    SetRally    { building_id: u64, x: f32, y: f32 },
    SetAi       { entity_id: u64, script_id: String, tick_interval: f32 },
}

// ── LuaRuntime ────────────────────────────────────────────────────────────────

pub struct LuaRuntime {
    lua:        Lua,
    assets_dir: PathBuf,
}

impl LuaRuntime {
    pub fn new(scripts_dir: &Path, assets_dir: PathBuf) -> LuaResult<Self> {
        let lua = Lua::new();
        register_api(&lua, assets_dir.clone())?;
        let rt = Self { lua, assets_dir };
        rt.load_scripts(scripts_dir)?;
        Ok(rt)
    }

    fn load_scripts(&self, scripts_dir: &Path) -> LuaResult<()> {
        if !scripts_dir.exists() {
            log::warn!("server scripting: složka {:?} neexistuje", scripts_dir);
            return Ok(());
        }
        let mut modules: Vec<(PathBuf, i64)> = std::fs::read_dir(scripts_dir)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("manifest.lua").exists())
            .map(|e| {
                let path  = e.path();
                let order = manifest_load_order(&path);
                (path, order)
            })
            .collect();
        // Seřaď podle load_order (pak abecedně jako tie-break)
        modules.sort_by(|(a, oa), (b, ob)| oa.cmp(ob).then(a.cmp(b)));
        for (m, _) in modules { self.load_module(&m)?; }
        Ok(())
    }

    fn load_module(&self, path: &Path) -> LuaResult<()> {
        let manifest = std::fs::read_to_string(path.join("manifest.lua"))
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        self.lua.load(&manifest)
            .set_name(path.join("manifest.lua").to_string_lossy().as_ref())
            .exec()?;
        let init = path.join("init.lua");
        if init.exists() {
            let src = std::fs::read_to_string(&init)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            self.lua.load(&src)
                .set_name(init.to_string_lossy().as_ref())
                .exec()?;
        }
        log::info!("server: načten modul {:?}", path.file_name().unwrap_or_default());
        Ok(())
    }

    // ── Query cache ───────────────────────────────────────────────────────────

    pub fn push_query_results(&self, units: &[UnitInfo]) -> LuaResult<()> {
        let tbl = self.lua.create_table()?;
        for (i, u) in units.iter().enumerate() {
            tbl.raw_set(i + 1, unit_to_table(&self.lua, u)?)?;
        }
        self.lua.globals().set("__query_result", tbl)?;
        Ok(())
    }

    pub fn push_unit_cache(&self, units: &[UnitInfo]) -> LuaResult<()> {
        let cache = self.lua.create_table()?;
        for u in units {
            cache.raw_set(u.entity_id, unit_to_table(&self.lua, u)?)?;
        }
        self.lua.globals().set("__unit_cache", cache)?;
        Ok(())
    }

    // ── Hooky ─────────────────────────────────────────────────────────────────

    pub fn hook_game_tick(&self, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        let f: Option<LuaFunction> = globals.get("on_game_tick")?;
        if let Some(f) = f { f.call::<()>(dt)?; }
        Ok(())
    }

    pub fn hook_unit_died(&self, unit: &UnitInfo) -> LuaResult<()> {
        let globals = self.lua.globals();
        let f: Option<LuaFunction> = globals.get("on_unit_died")?;
        if let Some(f) = f { f.call::<()>(unit_to_table(&self.lua, unit)?)?; }
        Ok(())
    }

    pub fn hook_unit_spawned(&self, unit: &UnitInfo) -> LuaResult<()> {
        let globals = self.lua.globals();
        let f: Option<LuaFunction> = globals.get("on_unit_spawned")?;
        if let Some(f) = f { f.call::<()>(unit_to_table(&self.lua, unit)?)?; }
        Ok(())
    }

    pub fn hook_ai_tick(&self, unit: &UnitInfo, script_id: &str, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        let ai_defs: Option<LuaTable> = globals.get("AiDefs")?;
        if let Some(defs) = ai_defs {
            let def: Option<LuaTable> = defs.get(script_id)?;
            if let Some(def) = def {
                let handler: Option<LuaFunction> = def.get("on_tick")?;
                if let Some(f) = handler {
                    f.call::<()>((unit_to_table(&self.lua, unit)?, dt))?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Vrátí a vymaže frontu ScriptCmd.
    pub fn drain_commands(&self) -> LuaResult<Vec<ScriptCmd>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__cmd_queue")?;
        let len   = queue.raw_len();
        let mut cmds = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let t: LuaTable = queue.raw_get(i)?;
            match table_to_cmd(&t) {
                Ok(c) => cmds.push(c),
                Err(e) => log::warn!("server: neplatný cmd: {e}"),
            }
        }
        globals.set("__cmd_queue", self.lua.create_table()?)?;
        Ok(cmds)
    }
}

// ── Registrace Engine API ─────────────────────────────────────────────────────

fn register_api(lua: &Lua, assets_dir: PathBuf) -> LuaResult<()> {
    lua.globals().set("__cmd_queue",    lua.create_table()?)?;
    lua.globals().set("__query_result", lua.create_table()?)?;
    lua.globals().set("__unit_cache",   lua.create_table()?)?;

    let e = lua.create_table()?;

    // Engine.move_unit(id, tx, ty, speed?)
    e.set("move_unit", lua.create_function(|lua, (id, tx, ty, speed): (u64, f32, f32, Option<f32>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "move_unit")?; cmd.set("entity_id", id)?;
        cmd.set("target_x", tx)?; cmd.set("target_y", ty)?;
        cmd.set("speed", speed.unwrap_or(128.0))?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("stop_unit", lua.create_function(|lua, id: u64| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "stop_unit")?; cmd.set("entity_id", id)?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("attack_unit", lua.create_function(|lua, (a, t): (u64, u64)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "attack_unit")?; cmd.set("attacker_id", a)?; cmd.set("target_id", t)?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("kill_unit", lua.create_function(|lua, id: u64| {
        let cmd = lua.create_table()?; cmd.set("cmd", "kill_unit")?; cmd.set("entity_id", id)?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("set_health", lua.create_function(|lua, (id, hp): (u64, i32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "set_health")?; cmd.set("entity_id", id)?; cmd.set("hp", hp)?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("spawn_unit", lua.create_function(|lua, (kind, x, y, team): (String, f32, f32, Option<u8>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "spawn_unit")?; cmd.set("kind_id", kind)?;
        cmd.set("x", x)?; cmd.set("y", y)?; cmd.set("team", team.unwrap_or(0))?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("train_unit", lua.create_function(|lua, (bid, kind, time): (u64, String, Option<f32>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "train_unit")?; cmd.set("building_id", bid)?;
        cmd.set("kind_id", kind)?; cmd.set("build_time", time.unwrap_or(0.0))?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("set_rally", lua.create_function(|lua, (bid, x, y): (u64, f32, f32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "set_rally")?; cmd.set("building_id", bid)?;
        cmd.set("x", x)?; cmd.set("y", y)?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("set_ai", lua.create_function(|lua, (id, script, interval): (u64, String, Option<f32>)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "set_ai")?; cmd.set("entity_id", id)?;
        cmd.set("script_id", script)?; cmd.set("tick_interval", interval.unwrap_or(1.0))?;
        push_cmd(lua, cmd)
    })?)?;

    e.set("query_units", lua.create_function(|lua, filter: Option<LuaTable>| {
        lua.globals().set("__query_filter", filter)?;
        let result: LuaTable = lua.globals().get("__query_result")?;
        Ok(result)
    })?)?;

    e.set("get_unit", lua.create_function(|lua, id: u64| {
        let cache: Option<LuaTable> = lua.globals().get("__unit_cache")?;
        if let Some(c) = cache { return Ok(c.get::<LuaValue>(id)?); }
        Ok(LuaValue::Nil)
    })?)?;

    e.set("log", lua.create_function(|_lua, msg: String| {
        log::info!("[Lua] {}", msg);
        Ok(())
    })?)?;

    e.set("TILE_SIZE", 32.0f32)?;

    // ── Asset loading ─────────────────────────────────────────────────────────

    let ad1 = assets_dir.clone();
    e.set("assets_dir", lua.create_function(move |_, ()| {
        Ok(ad1.to_string_lossy().to_string())
    })?)?;

    let ad2 = assets_dir.clone();
    e.set("load_json", lua.create_function(move |lua, path: String| {
        let full = ad2.join(&path);
        let text = std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("load_json {:?}: {e}", full)))?;
        let val: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| LuaError::RuntimeError(format!("load_json parse: {e}")))?;
        json_to_lua(lua, &val)
    })?)?;

    let ad3 = assets_dir.clone();
    e.set("load_asset_text", lua.create_function(move |_, path: String| {
        let full = ad3.join(&path);
        std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("load_asset_text {:?}: {e}", full)))
    })?)?;

    lua.globals().set("Engine", e)?;
    Ok(())
}

fn push_cmd(lua: &Lua, cmd: LuaTable) -> LuaResult<()> {
    let queue: LuaTable = lua.globals().get("__cmd_queue")?;
    let len = queue.raw_len();
    queue.raw_set(len + 1, cmd)?;
    Ok(())
}

fn table_to_cmd(t: &LuaTable) -> LuaResult<ScriptCmd> {
    let cmd: String = t.get("cmd")?;
    match cmd.as_str() {
        "move_unit"  => Ok(ScriptCmd::MoveUnit  {
            entity_id: t.get("entity_id")?,
            target_x:  t.get("target_x")?,
            target_y:  t.get("target_y")?,
            speed:     t.get::<f32>("speed").unwrap_or(128.0),
        }),
        "attack_unit" => Ok(ScriptCmd::AttackUnit  { attacker_id: t.get("attacker_id")?, target_id: t.get("target_id")? }),
        "stop_unit"   => Ok(ScriptCmd::StopUnit    { entity_id: t.get("entity_id")? }),
        "set_health"  => Ok(ScriptCmd::SetHealth   { entity_id: t.get("entity_id")?, hp: t.get("hp")? }),
        "kill_unit"   => Ok(ScriptCmd::KillUnit    { entity_id: t.get("entity_id")? }),
        "spawn_unit"  => Ok(ScriptCmd::SpawnUnit   { kind_id: t.get("kind_id")?, x: t.get("x")?, y: t.get("y")?, team: t.get::<u8>("team").unwrap_or(0) }),
        "train_unit"  => Ok(ScriptCmd::TrainUnit   { building_id: t.get("building_id")?, kind_id: t.get("kind_id")?, build_time: t.get::<f32>("build_time").unwrap_or(0.0) }),
        "set_rally"   => Ok(ScriptCmd::SetRally    { building_id: t.get("building_id")?, x: t.get("x")?, y: t.get("y")? }),
        "set_ai"      => Ok(ScriptCmd::SetAi       { entity_id: t.get("entity_id")?, script_id: t.get("script_id")?, tick_interval: t.get::<f32>("tick_interval").unwrap_or(1.0) }),
        other => Err(LuaError::RuntimeError(format!("neznámý cmd: {other}"))),
    }
}

/// Přečte `load_order` z manifest.lua bez spuštění Lua (jednoduchý textový parse).
/// Hledá řádek tvaru `load_order = <číslo>` uvnitř tabulky Module.
/// Výchozí hodnota je 100, pokud pole chybí.
fn manifest_load_order(module_path: &Path) -> i64 {
    let Ok(text) = std::fs::read_to_string(module_path.join("manifest.lua")) else {
        return 100;
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("load_order") {
            let rest = rest.trim_start().strip_prefix('=').unwrap_or("").trim();
            // odstraň případnou čárku na konci
            let rest = rest.trim_end_matches(',').trim();
            if let Ok(n) = rest.parse::<i64>() {
                return n;
            }
        }
    }
    100
}

/// Převod serde_json::Value → Lua hodnota.
fn json_to_lua(lua: &Lua, val: &serde_json::Value) -> LuaResult<LuaValue> {
    match val {
        serde_json::Value::Null           => Ok(LuaValue::Nil),
        serde_json::Value::Bool(b)        => Ok(LuaValue::Boolean(*b)),
        serde_json::Value::Number(n)      => {
            if let Some(i) = n.as_i64() { Ok(LuaValue::Integer(i)) }
            else { Ok(LuaValue::Number(n.as_f64().unwrap_or(0.0))) }
        }
        serde_json::Value::String(s)      => Ok(LuaValue::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr)     => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.raw_set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
        serde_json::Value::Object(obj)    => {
            let t = lua.create_table()?;
            for (k, v) in obj {
                t.raw_set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
    }
}
