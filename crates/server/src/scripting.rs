//! Server-side Lua resource runtime – FiveM style.
//!
//! ## Adresářová struktura resources/
//! ```
//! resources/
//!   [base]/
//!     fxmanifest.lua       -- popis resource
//!     server/              -- skripty jen pro server
//!       init.lua
//!     shared/              -- skripty pro server i klient
//!       defs.lua
//!     client/              -- skripty jen pro klienta (server nenačítá)
//! ```
//!
//! ## Formát fxmanifest.lua
//! ```lua
//! fx_version 'rts1'
//! name       'base'
//! load_order = 0
//!
//! shared_scripts { 'shared/*.lua' }
//! server_scripts { 'server/*.lua' }
//! client_scripts { 'client/*.lua' }   -- deklarace pro klienta, server ignoruje
//! ```
//!
//! ## Lua API (server context)
//! ```lua
//! AddEventHandler(name, cb)                   -- registruje handler
//! TriggerEvent(name, ...)                     -- lokální event (stejný runtime)
//! TriggerClientEvent(name, target, ...)       -- pošle event klientovi (-1 = všem)
//! Engine.*                                    -- herní příkazy (beze změny)
//! ```

use std::path::{Path, PathBuf};
use mlua::prelude::*;

// ── Outgoing cross-context event ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OutgoingClientEvent {
    pub name:      String,
    pub target:    i64,    // -1 = broadcast, ≥0 = player_id
    pub args_json: String,
}

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
    MoveUnit           { entity_id: u64, target_x: f32, target_y: f32, speed: f32 },
    AttackUnit         { attacker_id: u64, target_id: u64 },
    StopUnit           { entity_id: u64 },
    SetHealth          { entity_id: u64, hp: i32 },
    KillUnit           { entity_id: u64 },
    SpawnUnit          { kind_id: String, x: f32, y: f32, team: u8 },
    TrainUnit          { building_id: u64, kind_id: String, build_time: f32 },
    SetRally           { building_id: u64, x: f32, y: f32 },
    SetAi              { entity_id: u64, script_id: String, tick_interval: f32 },
    SetAbilityCooldown { entity_id: u64, ability_id: String, cooldown: f32 },
}

// ── ResourceManifest ─────────────────────────────────────────────────────────

struct ResourceManifest {
    name:           String,
    load_order:     i64,
    shared_scripts: Vec<String>,
    server_scripts: Vec<String>,
}

// ── LuaRuntime ────────────────────────────────────────────────────────────────

pub struct LuaRuntime {
    lua:        Lua,
    assets_dir: PathBuf,
}

impl LuaRuntime {
    pub fn new(resources_dir: &Path, assets_dir: PathBuf) -> LuaResult<Self> {
        let lua = Lua::new();
        register_api(&lua, assets_dir.clone())?;
        register_event_api(&lua)?;
        let rt = Self { lua, assets_dir };
        rt.load_resources(resources_dir)?;
        Ok(rt)
    }

    fn load_resources(&self, dir: &Path) -> LuaResult<()> {
        if !dir.exists() {
            log::warn!("resources: složka {:?} neexistuje", dir);
            return Ok(());
        }

        let mut resources: Vec<(PathBuf, i64)> = std::fs::read_dir(dir)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("fxmanifest.lua").exists())
            .map(|e| {
                let path  = e.path();
                let order = manifest_load_order(&path.join("fxmanifest.lua"));
                (path, order)
            })
            .collect();

        resources.sort_by(|(a, oa), (b, ob)| oa.cmp(ob).then(a.cmp(b)));

        for (res, _) in resources {
            self.load_resource(&res)?;
        }
        Ok(())
    }

    fn load_resource(&self, dir: &Path) -> LuaResult<()> {
        let manifest = parse_manifest(&dir.join("fxmanifest.lua"))?;

        let mut loaded = 0usize;
        for pattern in manifest.shared_scripts.iter().chain(manifest.server_scripts.iter()) {
            for path in resolve_glob(dir, pattern) {
                self.exec_file(&path)?;
                loaded += 1;
            }
        }

        log::info!("resource '{}': načteno {} skriptů (server+shared)", manifest.name, loaded);
        Ok(())
    }

    fn exec_file(&self, path: &Path) -> LuaResult<()> {
        let src = std::fs::read_to_string(path)
            .map_err(|e| LuaError::RuntimeError(format!("{}: {e}", path.display())))?;
        self.lua.load(&src)
            .set_name(path.to_string_lossy().as_ref())
            .exec()
    }

    // ── Event: cross-context ─────────────────────────────────────────────────

    /// Vrátí a vyprázdní frontu TriggerClientEvent volání.
    pub fn drain_client_events(&self) -> LuaResult<Vec<OutgoingClientEvent>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__client_event_queue")?;
        let len = queue.raw_len();
        let mut events = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let t: LuaTable = queue.raw_get(i)?;
            events.push(OutgoingClientEvent {
                name:      t.get("name")?,
                target:    t.get("target")?,
                args_json: t.get("args_json")?,
            });
        }
        globals.set("__client_event_queue", self.lua.create_table()?)?;
        Ok(events)
    }

    /// Spustí event přijatý od klienta přes síť.
    /// Zavolá TriggerEvent(name, source, ...args) v serverovém Lua prostoru.
    pub fn trigger_network_event(&self, name: &str, source: u64, args_json: &str) -> LuaResult<()> {
        let json: serde_json::Value = serde_json::from_str(args_json)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

        let trigger: LuaFunction = self.lua.globals().get("TriggerEvent")?;

        let mut args_vec: Vec<LuaValue> = Vec::new();
        args_vec.push(LuaValue::String(self.lua.create_string(name)?));
        args_vec.push(LuaValue::Integer(source as i64));
        if let serde_json::Value::Array(arr) = json {
            for v in arr {
                args_vec.push(json_to_lua(&self.lua, &v)?);
            }
        }

        trigger.call::<()>(LuaMultiValue::from_vec(args_vec))?;
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

    // ── Hooky Rust → Lua ─────────────────────────────────────────────────────

    pub fn hook_game_tick(&self, dt: f32) -> LuaResult<()> {
        call_opt_fn(&self.lua, "on_game_tick", dt)
    }

    pub fn hook_unit_died(&self, unit: &UnitInfo) -> LuaResult<()> {
        call_unit_hook(&self.lua, "on_unit_died", unit)
    }

    pub fn hook_unit_spawned(&self, unit: &UnitInfo) -> LuaResult<()> {
        call_unit_hook(&self.lua, "on_unit_spawned", unit)
    }

    pub fn hook_ai_tick(&self, unit: &UnitInfo, script_id: &str, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        // Specifický handler přes AiDefs
        if let Ok(defs) = globals.get::<LuaTable>("AiDefs") {
            if let Ok(def) = defs.get::<LuaTable>(script_id) {
                if let Ok(f) = def.get::<LuaFunction>("on_tick") {
                    let utbl = unit_to_table(&self.lua, unit)?;
                    f.call::<()>((utbl, dt))?;
                    return Ok(());
                }
            }
        }
        // Fallback na globální on_ai_tick
        if let Ok(f) = globals.get::<LuaFunction>("on_ai_tick") {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, dt))?;
        }
        Ok(())
    }

    /// on_ability_used(caster, ability_id, target_id_or_nil, tx, ty)
    pub fn hook_ability_used(
        &self,
        caster:     &UnitInfo,
        ability_id: &str,
        target_id:  Option<u64>,
        target_x:   f32,
        target_y:   f32,
    ) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(f) = globals.get::<LuaFunction>("on_ability_used") {
            let ctbl = unit_to_table(&self.lua, caster)?;
            let tid: LuaValue = match target_id {
                Some(id) => LuaValue::Integer(id as i64),
                None     => LuaValue::Nil,
            };
            f.call::<()>((ctbl, ability_id.to_string(), tid, target_x, target_y))?;
        }
        Ok(())
    }

    // ── Drain commands ────────────────────────────────────────────────────────

    pub fn drain_commands(&self) -> LuaResult<Vec<ScriptCmd>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__cmd_queue")?;
        let len   = queue.raw_len();
        let mut cmds = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let t: LuaTable = queue.raw_get(i)?;
            match table_to_cmd(&t) {
                Ok(c)  => cmds.push(c),
                Err(e) => log::warn!("server: neplatný cmd: {e}"),
            }
        }
        globals.set("__cmd_queue", self.lua.create_table()?)?;
        Ok(cmds)
    }
}

// ── Manifest parser ───────────────────────────────────────────────────────────

fn parse_manifest(path: &Path) -> LuaResult<ResourceManifest> {
    let lua = Lua::new();
    let g   = lua.globals();

    // Inicializace
    g.set("load_order", 100i64)?;
    g.set("__name",   lua.create_string("")?)?;
    g.set("__shared", lua.create_table()?)?;
    g.set("__server", lua.create_table()?)?;
    g.set("__client", lua.create_table()?)?; // deklarativní – server nenačítá

    // No-op metadata funkce
    for key in &["fx_version", "game", "author", "description", "version"] {
        g.set(*key, lua.create_function(|_, _: LuaValue| Ok(()))?)?;
    }
    g.set("name", lua.create_function(|lua, n: String| {
        lua.globals().set("__name", lua.create_string(&n)?)
    })?)?;

    // Sběrné funkce
    g.set("shared_scripts", lua.create_function(|lua, list: LuaTable| {
        append_to_global(lua, "__shared", list)
    })?)?;
    g.set("server_scripts", lua.create_function(|lua, list: LuaTable| {
        append_to_global(lua, "__server", list)
    })?)?;
    g.set("client_scripts", lua.create_function(|lua, list: LuaTable| {
        append_to_global(lua, "__client", list)
    })?)?;

    let src = std::fs::read_to_string(path)
        .map_err(|e| LuaError::RuntimeError(format!("{}: {e}", path.display())))?;
    lua.load(&src).set_name(path.to_string_lossy().as_ref()).exec()?;

    let name_raw: String = g.get::<String>("__name").unwrap_or_default();
    let name = if name_raw.is_empty() {
        path.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        name_raw
    };
    let load_order: i64 = g.get("load_order").unwrap_or(100);

    Ok(ResourceManifest {
        name,
        load_order,
        shared_scripts: table_to_string_vec(g.get::<LuaTable>("__shared")?),
        server_scripts: table_to_string_vec(g.get::<LuaTable>("__server")?),
    })
}

fn append_to_global(lua: &Lua, key: &str, list: LuaTable) -> LuaResult<()> {
    let t: LuaTable = lua.globals().get(key)?;
    for v in list.sequence_values::<String>() {
        let s   = v?;
        let len = t.raw_len();
        t.raw_set(len + 1, s)?;
    }
    Ok(())
}

fn table_to_string_vec(t: LuaTable) -> Vec<String> {
    (1..=t.raw_len())
        .filter_map(|i| t.raw_get::<String>(i).ok())
        .collect()
}

/// Čte load_order z fxmanifest.lua bez plné syntaktické analýzy.
fn manifest_load_order(path: &Path) -> i64 {
    let Ok(text) = std::fs::read_to_string(path) else { return 100 };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("load_order") {
            let rest = rest.trim_start().strip_prefix('=').unwrap_or("").trim();
            let rest = rest.trim_end_matches(',').trim();
            if let Ok(n) = rest.parse::<i64>() { return n; }
        }
    }
    100
}

// ── Glob resolver ─────────────────────────────────────────────────────────────

fn resolve_glob(base: &Path, pattern: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = pattern.splitn(2, '/').collect();
    match parts.as_slice() {
        [dir, "*.lua"] => {
            let full_dir = base.join(dir);
            let mut files: Vec<PathBuf> = std::fs::read_dir(&full_dir)
                .into_iter().flatten().flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |x| x == "lua"))
                .collect();
            files.sort();
            files
        }
        _ => {
            let p = base.join(pattern);
            if p.exists() { vec![p] } else { vec![] }
        }
    }
}

// ── Event API ─────────────────────────────────────────────────────────────────

fn register_event_api(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    g.set("__handlers",            lua.create_table()?)?;
    g.set("__client_event_queue",  lua.create_table()?)?;

    // AddEventHandler a TriggerEvent jako čisté Lua
    lua.load(r#"
function AddEventHandler(name, cb)
    if not __handlers[name] then __handlers[name] = {} end
    table.insert(__handlers[name], cb)
end

function TriggerEvent(name, ...)
    local hs = __handlers[name]
    if hs then
        local args = {...}
        for _, h in ipairs(hs) do h(table.unpack(args)) end
    end
end
"#).exec()?;

    // TriggerClientEvent(name, target, ...) – Rust: serializuje args do JSON
    g.set("TriggerClientEvent", lua.create_function(|lua, args: LuaMultiValue| {
        let mut iter = args.into_iter();

        let name = match iter.next() {
            Some(LuaValue::String(s)) => s.to_str()?.to_owned(),
            other => return Err(LuaError::RuntimeError(
                format!("TriggerClientEvent: arg1 musí být string, dostáno {:?}", other)
            )),
        };
        let target: i64 = match iter.next() {
            Some(LuaValue::Integer(n)) => n,
            Some(LuaValue::Number(n))  => n as i64,
            _                          => -1,
        };
        let rest: Vec<LuaValue> = iter.collect();
        let args_json = lua_multi_to_json(&rest)?;

        let queue: LuaTable = lua.globals().get("__client_event_queue")?;
        let entry = lua.create_table()?;
        entry.set("name",      name)?;
        entry.set("target",    target)?;
        entry.set("args_json", args_json)?;
        queue.raw_set(queue.raw_len() + 1, entry)?;
        Ok(())
    })?)?;

    Ok(())
}

// ── Engine API registrace ─────────────────────────────────────────────────────

fn register_api(lua: &Lua, assets_dir: PathBuf) -> LuaResult<()> {
    lua.globals().set("__cmd_queue",    lua.create_table()?)?;
    lua.globals().set("__query_result", lua.create_table()?)?;
    lua.globals().set("__unit_cache",   lua.create_table()?)?;

    let e = lua.create_table()?;

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

    e.set("set_ability_cooldown", lua.create_function(|lua, (id, ability, cd): (u64, String, f32)| {
        let cmd = lua.create_table()?;
        cmd.set("cmd", "set_ability_cooldown")?;
        cmd.set("entity_id",  id)?; cmd.set("ability_id", ability)?; cmd.set("cooldown", cd)?;
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

    e.set("log", lua.create_function(|_, msg: String| {
        log::info!("[Lua] {}", msg); Ok(())
    })?)?;

    e.set("TILE_SIZE", 32.0f32)?;

    // Asset loading
    let ad1 = assets_dir.clone();
    e.set("assets_dir", lua.create_function(move |_, ()| {
        Ok(ad1.to_string_lossy().to_string())
    })?)?;

    let ad2 = assets_dir.clone();
    e.set("load_json", lua.create_function(move |lua, path: String| {
        let full = ad2.join(&path);
        let text = std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("{}: {e}", full.display())))?;
        let val: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| LuaError::RuntimeError(format!("load_json parse: {e}")))?;
        json_to_lua(lua, &val)
    })?)?;

    let ad3 = assets_dir.clone();
    e.set("load_asset_text", lua.create_function(move |_, path: String| {
        let full = ad3.join(&path);
        std::fs::read_to_string(&full)
            .map_err(|e| LuaError::RuntimeError(format!("{}: {e}", full.display())))
    })?)?;

    lua.globals().set("Engine", e)?;
    Ok(())
}

fn push_cmd(lua: &Lua, cmd: LuaTable) -> LuaResult<()> {
    let queue: LuaTable = lua.globals().get("__cmd_queue")?;
    queue.raw_set(queue.raw_len() + 1, cmd)?;
    Ok(())
}

// ── table_to_cmd ──────────────────────────────────────────────────────────────

fn table_to_cmd(t: &LuaTable) -> LuaResult<ScriptCmd> {
    let cmd: String = t.get("cmd")?;
    match cmd.as_str() {
        "move_unit"  => Ok(ScriptCmd::MoveUnit {
            entity_id: t.get("entity_id")?,
            target_x:  t.get("target_x")?,
            target_y:  t.get("target_y")?,
            speed:     t.get::<f32>("speed").unwrap_or(128.0),
        }),
        "attack_unit" => Ok(ScriptCmd::AttackUnit {
            attacker_id: t.get("attacker_id")?,
            target_id:   t.get("target_id")?,
        }),
        "stop_unit"   => Ok(ScriptCmd::StopUnit    { entity_id: t.get("entity_id")? }),
        "set_health"  => Ok(ScriptCmd::SetHealth   { entity_id: t.get("entity_id")?, hp: t.get("hp")? }),
        "kill_unit"   => Ok(ScriptCmd::KillUnit    { entity_id: t.get("entity_id")? }),
        "spawn_unit"  => Ok(ScriptCmd::SpawnUnit   {
            kind_id: t.get("kind_id")?,
            x: t.get("x")?, y: t.get("y")?,
            team: t.get::<u8>("team").unwrap_or(0),
        }),
        "train_unit"  => Ok(ScriptCmd::TrainUnit   {
            building_id: t.get("building_id")?,
            kind_id:     t.get("kind_id")?,
            build_time:  t.get::<f32>("build_time").unwrap_or(0.0),
        }),
        "set_rally"   => Ok(ScriptCmd::SetRally    {
            building_id: t.get("building_id")?,
            x: t.get("x")?, y: t.get("y")?,
        }),
        "set_ai"      => Ok(ScriptCmd::SetAi {
            entity_id:     t.get("entity_id")?,
            script_id:     t.get("script_id")?,
            tick_interval: t.get::<f32>("tick_interval").unwrap_or(1.0),
        }),
        "set_ability_cooldown" => Ok(ScriptCmd::SetAbilityCooldown {
            entity_id:  t.get("entity_id")?,
            ability_id: t.get("ability_id")?,
            cooldown:   t.get::<f32>("cooldown").unwrap_or(0.0),
        }),
        other => Err(LuaError::RuntimeError(format!("neznámý cmd: {other}"))),
    }
}

// ── Pomocné funkce ────────────────────────────────────────────────────────────

fn call_opt_fn<A: IntoLuaMulti>(lua: &Lua, name: &str, args: A) -> LuaResult<()> {
    if let Ok(f) = lua.globals().get::<LuaFunction>(name) {
        f.call::<()>(args)?;
    }
    Ok(())
}

fn call_unit_hook(lua: &Lua, name: &str, unit: &UnitInfo) -> LuaResult<()> {
    if let Ok(f) = lua.globals().get::<LuaFunction>(name) {
        f.call::<()>(unit_to_table(lua, unit)?)?;
    }
    Ok(())
}

// ── JSON ↔ Lua konverze ───────────────────────────────────────────────────────

fn json_to_lua(lua: &Lua, val: &serde_json::Value) -> LuaResult<LuaValue> {
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

fn lua_value_to_json(val: &LuaValue) -> serde_json::Value {
    match val {
        LuaValue::Nil        => serde_json::Value::Null,
        LuaValue::Boolean(b) => serde_json::Value::Bool(*b),
        LuaValue::Integer(n) => serde_json::Value::Number((*n).into()),
        LuaValue::Number(n)  => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        LuaValue::String(s)  => serde_json::Value::String(
            String::from_utf8_lossy(&s.as_bytes()).into_owned()
        ),
        LuaValue::Table(t)   => {
            let len = t.raw_len();
            if len > 0 {
                let arr: Vec<serde_json::Value> = (1..=len)
                    .filter_map(|i| t.raw_get::<LuaValue>(i).ok())
                    .map(|v| lua_value_to_json(&v))
                    .collect();
                serde_json::Value::Array(arr)
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.clone().pairs::<String, LuaValue>() {
                    if let Ok((k, v)) = pair {
                        map.insert(k, lua_value_to_json(&v));
                    }
                }
                serde_json::Value::Object(map)
            }
        }
        _ => serde_json::Value::Null,
    }
}

fn lua_multi_to_json(args: &[LuaValue]) -> LuaResult<String> {
    let arr: Vec<serde_json::Value> = args.iter().map(lua_value_to_json).collect();
    serde_json::to_string(&serde_json::Value::Array(arr))
        .map_err(|e| LuaError::RuntimeError(e.to_string()))
}
