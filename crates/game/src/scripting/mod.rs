//! Client-side Lua resource runtime – FiveM style.
//!
//! Načítá `shared/` + `client/` skripty z každého resource.
//! Server skripty jsou ignorovány (patří na server).
//!
//! ## Lua API (client context)
//! ```lua
//! AddEventHandler(name, cb)          -- registruje handler
//! TriggerEvent(name, ...)            -- lokální event
//! TriggerServerEvent(name, ...)      -- pošle event na server
//! Engine.*                           -- herní příkazy
//! ```

pub mod api;

use std::path::Path;
use mlua::prelude::*;

use crate::components::MoveFlags;

// ── Outgoing server event ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct OutgoingServerEvent {
    pub name:      String,
    pub args_json: String,
}

// ── ScriptCmd – příkazy z Lua → Rust ─────────────────────────────────────────

#[derive(Debug)]
pub enum ScriptCmd {
    MoveUnit {
        entity_id: u64,
        target_x:  f32,
        target_y:  f32,
        params:    MoveParams,
    },
    AttackUnit {
        attacker_id: u64,
        target_id:   u64,
    },
    StopUnit {
        entity_id: u64,
    },
    SetHealth {
        entity_id: u64,
        hp:        i32,
    },
    KillUnit {
        entity_id: u64,
    },
    AddResources {
        gold:   i32,
        lumber: i32,
        oil:    i32,
    },
    SpawnUnit {
        kind_id: String,
        x:       f32,
        y:       f32,
        team:    u8,
    },
    TrainUnit {
        building_id: u64,
        kind_id:     String,
        build_time:  f32,
    },
    SetRally {
        building_id: u64,
        x:           f32,
        y:           f32,
    },
    SetAi {
        entity_id:     u64,
        script_id:     String,
        tick_interval: f32,
    },
    SetAiState {
        entity_id:  u64,
        state_json: String,
    },
}

// ── MoveParams ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MoveParams {
    pub speed:        f32,
    pub can_swim:     bool,
    pub can_fly:      bool,
    pub speed_water:  f32,
    pub speed_forest: f32,
    pub speed_road:   f32,
}

impl Default for MoveParams {
    fn default() -> Self {
        Self { speed: 128.0, can_swim: false, can_fly: false,
               speed_water: 0.0, speed_forest: 0.75, speed_road: 1.0 }
    }
}

impl From<MoveParams> for MoveFlags {
    fn from(p: MoveParams) -> Self {
        MoveFlags {
            can_swim: p.can_swim, can_fly: p.can_fly,
            speed_water: p.speed_water, speed_forest: p.speed_forest,
            speed_road: p.speed_road,
        }
    }
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

// ── LuaRuntime ────────────────────────────────────────────────────────────────

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();
        api::register(&lua)?;
        register_event_api(&lua)?;
        Ok(Self { lua })
    }

    /// Načte resources z adresáře – klient načítá shared + client skripty.
    pub fn load_resources(&self, resources_dir: &Path) -> LuaResult<()> {
        if !resources_dir.exists() {
            log::warn!("client scripting: složka {:?} neexistuje", resources_dir);
            return Ok(());
        }

        let mut resources: Vec<(std::path::PathBuf, i64)> = std::fs::read_dir(resources_dir)
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
        for (res, _) in resources { self.load_resource(&res)?; }
        Ok(())
    }

    /// Zpětná kompatibilita – singleplayer načítá z scripts/ složky (starý formát).
    pub fn load_scripts(&self, scripts_dir: &Path) -> LuaResult<()> {
        if !scripts_dir.exists() {
            log::warn!("scripting: složka {:?} neexistuje", scripts_dir);
            return Ok(());
        }
        // Zkus nejdřív nový formát (fxmanifest.lua)
        if scripts_dir.join("fxmanifest.lua").exists() {
            return self.load_resource(scripts_dir);
        }
        // Starý formát: pod-složky s manifest.lua
        let mut modules: Vec<(std::path::PathBuf, i64)> = std::fs::read_dir(scripts_dir)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("manifest.lua").exists())
            .map(|e| {
                let path  = e.path();
                let order = manifest_load_order_legacy(&path);
                (path, order)
            })
            .collect();
        modules.sort_by(|(a, oa), (b, ob)| oa.cmp(ob).then(a.cmp(b)));
        for (m, _) in modules { self.load_legacy_module(&m)?; }
        Ok(())
    }

    fn load_resource(&self, dir: &Path) -> LuaResult<()> {
        let manifest = parse_manifest(&dir.join("fxmanifest.lua"))?;
        let mut loaded = 0usize;
        // shared + client skripty
        for pattern in manifest.shared_scripts.iter().chain(manifest.client_scripts.iter()) {
            for path in resolve_glob(dir, pattern) {
                self.exec_file(&path)?;
                loaded += 1;
            }
        }
        log::info!("resource '{}' (client): načteno {} skriptů (shared+client)", manifest.name, loaded);
        Ok(())
    }

    fn load_legacy_module(&self, path: &Path) -> LuaResult<()> {
        let manifest = std::fs::read_to_string(path.join("manifest.lua"))
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        self.lua.load(&manifest)
            .set_name(path.join("manifest.lua").to_string_lossy().as_ref())
            .exec()?;
        let init = path.join("init.lua");
        if init.exists() {
            self.exec_file(&init)?;
        }
        log::info!("scripting: načten modul {:?}", path.file_name().unwrap_or_default());
        Ok(())
    }

    pub fn exec_file(&self, path: &Path) -> LuaResult<()> {
        let src = std::fs::read_to_string(path)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        self.lua.load(&src)
            .set_name(path.to_string_lossy().as_ref())
            .exec()
    }

    // ── Network events ────────────────────────────────────────────────────────

    /// Vrátí a vyprázdní frontu TriggerServerEvent volání.
    pub fn drain_server_events(&self) -> LuaResult<Vec<OutgoingServerEvent>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__server_event_queue")?;
        let len = queue.raw_len();
        let mut events = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let t: LuaTable = queue.raw_get(i)?;
            events.push(OutgoingServerEvent {
                name:      t.get("name")?,
                args_json: t.get("args_json")?,
            });
        }
        globals.set("__server_event_queue", self.lua.create_table()?)?;
        Ok(events)
    }

    /// Spustí event přijatý ze serveru přes síť.
    /// Zavolá TriggerEvent(name, ...args) v klientském Lua prostoru.
    pub fn trigger_network_event(&self, name: &str, args_json: &str) -> LuaResult<()> {
        let json: serde_json::Value = serde_json::from_str(args_json)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

        let trigger: LuaFunction = self.lua.globals().get("TriggerEvent")?;

        let mut args_vec: Vec<LuaValue> = Vec::new();
        args_vec.push(LuaValue::String(self.lua.create_string(name)?));
        if let serde_json::Value::Array(arr) = json {
            for v in arr {
                args_vec.push(json_to_lua(&self.lua, &v)?);
            }
        }

        trigger.call::<()>(LuaMultiValue::from_vec(args_vec))?;
        Ok(())
    }

    // ── Hooky Rust → Lua ─────────────────────────────────────────────────────

    /// on_move_order(unit, tx, ty, params) → params_table | false | nil
    pub fn hook_move_order(
        &self, unit: &UnitInfo, tx: f32, ty: f32, default_params: MoveParams,
    ) -> LuaResult<Option<ScriptCmd>> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_move_order")?;
        let Some(f) = hook else {
            return Ok(Some(ScriptCmd::MoveUnit {
                entity_id: unit.entity_id, target_x: tx, target_y: ty,
                params: default_params,
            }));
        };
        let utbl = unit_to_table(&self.lua, unit)?;
        let ptbl = params_to_table(&self.lua, &default_params)?;
        match f.call::<LuaValue>((utbl, tx, ty, ptbl))? {
            LuaValue::Boolean(false) | LuaValue::Nil => Ok(None),
            LuaValue::Table(t) => Ok(Some(ScriptCmd::MoveUnit {
                entity_id: unit.entity_id, target_x: tx, target_y: ty,
                params: table_to_params(&t, default_params)?,
            })),
            _ => Ok(Some(ScriptCmd::MoveUnit {
                entity_id: unit.entity_id, target_x: tx, target_y: ty,
                params: default_params,
            })),
        }
    }

    pub fn hook_unit_arrived(&self, unit: &UnitInfo) -> LuaResult<()> { call_hook1(&self.lua, "on_unit_arrived", unit) }
    pub fn hook_unit_spawned(&self, unit: &UnitInfo) -> LuaResult<()> { call_hook1(&self.lua, "on_unit_spawned",  unit) }
    pub fn hook_unit_died   (&self, unit: &UnitInfo) -> LuaResult<()> { call_hook1(&self.lua, "on_unit_died",     unit) }

    pub fn hook_unit_attack(&self, attacker: &UnitInfo, target: &UnitInfo, damage: i32) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(f) = globals.get::<LuaFunction>("on_unit_attack") {
            let at = unit_to_table(&self.lua, attacker)?;
            let bt = unit_to_table(&self.lua, target)?;
            f.call::<()>((at, bt, damage))?;
        }
        Ok(())
    }

    pub fn hook_unit_hit(&self, unit: &UnitInfo, damage: i32, attacker_id: u64) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(f) = globals.get::<LuaFunction>("on_unit_hit") {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, damage, attacker_id))?;
        }
        Ok(())
    }

    pub fn hook_unit_trained(&self, unit: &UnitInfo, building_id: u64) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(f) = globals.get::<LuaFunction>("on_unit_trained") {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, building_id))?;
        }
        Ok(())
    }

    pub fn hook_ai_tick(&self, unit: &UnitInfo, script_id: &str, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(defs) = globals.get::<LuaTable>("AiDefs") {
            if let Ok(def) = defs.get::<LuaTable>(script_id) {
                if let Ok(f) = def.get::<LuaFunction>("on_tick") {
                    let utbl = unit_to_table(&self.lua, unit)?;
                    f.call::<()>((utbl, dt))?;
                    return Ok(());
                }
            }
        }
        if let Ok(f) = globals.get::<LuaFunction>("on_ai_tick") {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, dt))?;
        }
        Ok(())
    }

    pub fn push_unit_cache(&self, units: &[UnitInfo]) -> LuaResult<()> {
        let cache = self.lua.create_table()?;
        for u in units { cache.raw_set(u.entity_id, unit_to_table(&self.lua, u)?)?; }
        self.lua.globals().set("__unit_cache", cache)?;
        Ok(())
    }

    pub fn hook_game_tick(&self, dt: f32) -> LuaResult<()> {
        if let Ok(f) = self.lua.globals().get::<LuaFunction>("on_game_tick") {
            f.call::<()>(dt)?;
        }
        Ok(())
    }

    pub fn hook_resource_changed(&self, gold: u32, lumber: u32, oil: u32) -> LuaResult<()> {
        if let Ok(f) = self.lua.globals().get::<LuaFunction>("on_resource_changed") {
            f.call::<()>((gold, lumber, oil))?;
        }
        Ok(())
    }

    pub fn push_query_results(&self, units: Vec<UnitInfo>) -> LuaResult<()> {
        let tbl = self.lua.create_table()?;
        for (i, u) in units.iter().enumerate() {
            tbl.raw_set(i + 1, unit_to_table(&self.lua, u)?)?;
        }
        self.lua.globals().set("__query_result", tbl)?;
        Ok(())
    }

    pub fn drain_commands(&self) -> LuaResult<Vec<ScriptCmd>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__cmd_queue")?;
        let len   = queue.raw_len();
        let mut cmds = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let tbl: LuaTable = queue.raw_get(i)?;
            match table_to_cmd(&tbl) {
                Ok(cmd) => cmds.push(cmd),
                Err(e)  => log::warn!("scripting: neplatný cmd: {e}"),
            }
        }
        globals.set("__cmd_queue", self.lua.create_table()?)?;
        Ok(cmds)
    }

    pub fn lua(&self) -> &Lua { &self.lua }
}

// ── Manifest parser ───────────────────────────────────────────────────────────

struct ResourceManifest {
    name:           String,
    shared_scripts: Vec<String>,
    client_scripts: Vec<String>,
}

fn parse_manifest(path: &Path) -> LuaResult<ResourceManifest> {
    let lua = Lua::new();
    let g   = lua.globals();

    g.set("load_order", 100i64)?;
    g.set("__name",   lua.create_string("")?)?;
    g.set("__shared", lua.create_table()?)?;
    g.set("__server", lua.create_table()?)?;
    g.set("__client", lua.create_table()?)?;

    for key in &["fx_version", "game", "author", "description", "version"] {
        g.set(*key, lua.create_function(|_, _: LuaValue| Ok(()))?)?;
    }
    g.set("name", lua.create_function(|lua, n: String| {
        lua.globals().set("__name", lua.create_string(&n)?)
    })?)?;
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

    Ok(ResourceManifest {
        name,
        shared_scripts: table_to_string_vec(g.get::<LuaTable>("__shared")?),
        client_scripts: table_to_string_vec(g.get::<LuaTable>("__client")?),
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
    (1..=t.raw_len()).filter_map(|i| t.raw_get::<String>(i).ok()).collect()
}

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

/// Stará verze: čte load_order z manifest.lua uvnitř složky modulu.
fn manifest_load_order_legacy(module_path: &Path) -> i64 {
    manifest_load_order(&module_path.join("manifest.lua"))
}

// ── Glob resolver ─────────────────────────────────────────────────────────────

fn resolve_glob(base: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
    let parts: Vec<&str> = pattern.splitn(2, '/').collect();
    match parts.as_slice() {
        [dir, "*.lua"] => {
            let full_dir = base.join(dir);
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&full_dir)
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
    g.set("__server_event_queue",  lua.create_table()?)?;

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

    // TriggerServerEvent(name, ...) – serializuje args a přidá do fronty
    g.set("TriggerServerEvent", lua.create_function(|lua, args: LuaMultiValue| {
        let mut iter = args.into_iter();
        let name = match iter.next() {
            Some(LuaValue::String(s)) => s.to_str()?.to_owned(),
            other => return Err(LuaError::RuntimeError(
                format!("TriggerServerEvent: arg1 musí být string, dostáno {:?}", other)
            )),
        };
        let rest: Vec<LuaValue> = iter.collect();
        let args_json = lua_multi_to_json(&rest)?;

        let queue: LuaTable = lua.globals().get("__server_event_queue")?;
        let entry = lua.create_table()?;
        entry.set("name",      name)?;
        entry.set("args_json", args_json)?;
        queue.raw_set(queue.raw_len() + 1, entry)?;
        Ok(())
    })?)?;

    Ok(())
}

// ── Konverzní funkce ──────────────────────────────────────────────────────────

pub fn unit_to_table(lua: &Lua, u: &UnitInfo) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("entity_id",    u.entity_id)?;
    t.set("x",            u.x)?; t.set("y", u.y)?;
    t.set("hp",           u.hp)?; t.set("hp_max", u.hp_max)?;
    t.set("damage",       u.damage)?; t.set("pierce", u.pierce)?;
    t.set("armor",        u.armor)?; t.set("attack_range", u.attack_range)?;
    t.set("team",         u.team)?; t.set("kind", u.kind_id.clone())?;
    Ok(t)
}

pub fn params_to_table(lua: &Lua, p: &MoveParams) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("speed",        p.speed)?; t.set("can_swim",     p.can_swim)?;
    t.set("can_fly",      p.can_fly)?; t.set("speed_water",  p.speed_water)?;
    t.set("speed_forest", p.speed_forest)?; t.set("speed_road", p.speed_road)?;
    Ok(t)
}

fn table_to_params(t: &LuaTable, d: MoveParams) -> LuaResult<MoveParams> {
    Ok(MoveParams {
        speed:        t.get("speed")        .unwrap_or(d.speed),
        can_swim:     t.get("can_swim")     .unwrap_or(d.can_swim),
        can_fly:      t.get("can_fly")      .unwrap_or(d.can_fly),
        speed_water:  t.get("speed_water")  .unwrap_or(d.speed_water),
        speed_forest: t.get("speed_forest") .unwrap_or(d.speed_forest),
        speed_road:   t.get("speed_road")   .unwrap_or(d.speed_road),
    })
}

fn table_to_cmd(t: &LuaTable) -> LuaResult<ScriptCmd> {
    let cmd: String = t.get("cmd")?;
    match cmd.as_str() {
        "move_unit" => Ok(ScriptCmd::MoveUnit {
            entity_id: t.get("entity_id")?,
            target_x:  t.get("target_x")?,
            target_y:  t.get("target_y")?,
            params: {
                let pt: LuaTable = t.get("params")?;
                table_to_params(&pt, MoveParams::default())?
            },
        }),
        "attack_unit"   => Ok(ScriptCmd::AttackUnit  { attacker_id: t.get("attacker_id")?, target_id: t.get("target_id")? }),
        "stop_unit"     => Ok(ScriptCmd::StopUnit     { entity_id: t.get("entity_id")? }),
        "set_health"    => Ok(ScriptCmd::SetHealth    { entity_id: t.get("entity_id")?, hp: t.get("hp")? }),
        "kill_unit"     => Ok(ScriptCmd::KillUnit     { entity_id: t.get("entity_id")? }),
        "add_resources" => Ok(ScriptCmd::AddResources {
            gold:   t.get("gold")  .unwrap_or(0),
            lumber: t.get("lumber").unwrap_or(0),
            oil:    t.get("oil")   .unwrap_or(0),
        }),
        "spawn_unit"    => Ok(ScriptCmd::SpawnUnit    {
            kind_id: t.get("kind_id")?,
            x: t.get("x")?, y: t.get("y")?,
            team: t.get("team").unwrap_or(0),
        }),
        "train_unit"    => Ok(ScriptCmd::TrainUnit    {
            building_id: t.get("building_id")?,
            kind_id:     t.get("kind_id")?,
            build_time:  t.get("build_time").unwrap_or(0.0),
        }),
        "set_rally"     => Ok(ScriptCmd::SetRally     {
            building_id: t.get("building_id")?,
            x: t.get("x")?, y: t.get("y")?,
        }),
        "set_ai"        => Ok(ScriptCmd::SetAi        {
            entity_id:     t.get("entity_id")?,
            script_id:     t.get("script_id")?,
            tick_interval: t.get("tick_interval").unwrap_or(1.0),
        }),
        "set_ai_state"  => Ok(ScriptCmd::SetAiState   {
            entity_id:  t.get("entity_id")?,
            state_json: t.get("state_json")?,
        }),
        other => Err(LuaError::RuntimeError(format!("neznámý cmd: {other}"))),
    }
}

fn call_hook1(lua: &Lua, name: &str, unit: &UnitInfo) -> LuaResult<()> {
    if let Ok(f) = lua.globals().get::<LuaFunction>(name) {
        f.call::<()>(unit_to_table(lua, unit)?)?;
    }
    Ok(())
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

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
            for (i, v) in arr.iter().enumerate() { t.raw_set(i + 1, json_to_lua(lua, v)?)?; }
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
            .map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
        LuaValue::String(s)  => serde_json::Value::String(String::from_utf8_lossy(&s.as_bytes()).into_owned()),
        LuaValue::Table(t)   => {
            let len = t.raw_len();
            if len > 0 {
                let arr: Vec<_> = (1..=len)
                    .filter_map(|i| t.raw_get::<LuaValue>(i).ok())
                    .map(|v| lua_value_to_json(&v)).collect();
                serde_json::Value::Array(arr)
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.clone().pairs::<String, LuaValue>() {
                    if let Ok((k, v)) = pair { map.insert(k, lua_value_to_json(&v)); }
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
