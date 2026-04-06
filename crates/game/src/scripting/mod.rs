//! Lua scripting runtime – FiveM-inspirovaný modulový systém.
//!
//! ## Tok dat Rust ↔ Lua
//!
//! **Rust → Lua (hooky)**
//! ```
//! on_move_order(unit, tx, ty, params)  → params | false
//! on_unit_arrived(unit)
//! on_unit_spawned(unit)
//! on_unit_died(unit)
//! on_unit_attack(attacker, target, damage)
//! on_unit_hit(unit, damage, attacker_id)
//! on_unit_trained(unit, building_id)
//! on_ai_tick(unit, dt)                 → commands via Engine.*
//! on_game_tick(dt)                     → global periodic
//! on_resource_changed(gold, lumber, oil)
//! ```
//!
//! **Lua → Rust (Engine API)**
//! ```lua
//! Engine.move_unit(id, x, y, params?)
//! Engine.attack_unit(attacker_id, target_id)
//! Engine.stop_unit(id)
//! Engine.set_health(id, hp)
//! Engine.kill_unit(id)
//! Engine.add_resources({gold, lumber, oil})
//! Engine.spawn_unit(kind_id, x, y, team)
//! Engine.train_unit(building_id, kind_id, build_time?)
//! Engine.set_rally(building_id, x, y)
//! Engine.set_ai(entity_id, script_id, tick_interval?)
//! Engine.query_units(filter_table)     → list of unit tables
//! Engine.get_unit(entity_id)           → unit table | nil
//! Engine.log(msg)
//! ```

pub mod api;

use std::path::Path;
use mlua::prelude::*;

use crate::components::MoveFlags;

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
        build_time:  f32,   // 0 = use default from UnitDefs
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

// ── UnitInfo – snapshot entity pro Lua hooky ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub entity_id:   u64,
    pub x:           f32,
    pub y:           f32,
    pub hp:          i32,
    pub hp_max:      i32,
    pub damage:      i32,
    pub pierce:      i32,
    pub armor:       i32,
    pub attack_range:f32,
    pub team:        u8,
    pub kind_id:     String,
}

// ── LuaRuntime ────────────────────────────────────────────────────────────────

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();
        api::register(&lua)?;
        Ok(Self { lua })
    }

    pub fn load_scripts(&self, scripts_dir: &Path) -> LuaResult<()> {
        if !scripts_dir.exists() {
            log::warn!("scripting: složka {:?} neexistuje", scripts_dir);
            return Ok(());
        }
        let mut modules: Vec<(std::path::PathBuf, i64)> = std::fs::read_dir(scripts_dir)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("manifest.lua").exists())
            .map(|e| {
                let path  = e.path();
                let order = manifest_load_order(&path);
                (path, order)
            })
            .collect();
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
        let utbl  = unit_to_table(&self.lua, unit)?;
        let ptbl  = params_to_table(&self.lua, &default_params)?;
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

    pub fn hook_unit_arrived(&self, unit: &UnitInfo)    -> LuaResult<()> { call_hook1(&self.lua, "on_unit_arrived",     unit) }
    pub fn hook_unit_spawned(&self, unit: &UnitInfo)    -> LuaResult<()> { call_hook1(&self.lua, "on_unit_spawned",      unit) }
    pub fn hook_unit_died   (&self, unit: &UnitInfo)    -> LuaResult<()> { call_hook1(&self.lua, "on_unit_died",         unit) }

    /// on_unit_attack(attacker, target, damage)
    pub fn hook_unit_attack(&self, attacker: &UnitInfo, target: &UnitInfo, damage: i32) -> LuaResult<()> {
        call_optional_fn2d(&self.lua, "on_unit_attack", attacker, target, damage)
    }

    /// on_unit_hit(unit, damage, attacker_id)
    pub fn hook_unit_hit(&self, unit: &UnitInfo, damage: i32, attacker_id: u64) -> LuaResult<()> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_unit_hit")?;
        if let Some(f) = hook {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, damage, attacker_id))?;
        }
        Ok(())
    }

    /// on_unit_trained(unit_info, building_id)
    pub fn hook_unit_trained(&self, unit: &UnitInfo, building_id: u64) -> LuaResult<()> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_unit_trained")?;
        if let Some(f) = hook {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, building_id))?;
        }
        Ok(())
    }

    /// on_ai_tick(unit, dt) – voláno pro entity s AiController.
    /// `script_id` je klíč do Lua `AiDefs` tabulky.
    pub fn hook_ai_tick(&self, unit: &UnitInfo, script_id: &str, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        // Zkus specifický handler registrovaný přes RegisterAi
        let ai_defs: Option<LuaTable> = globals.get("AiDefs")?;
        if let Some(defs) = ai_defs {
            let specific: Option<LuaTable> = defs.get(script_id)?;
            if let Some(def) = specific {
                let handler: Option<LuaFunction> = def.get("on_tick")?;
                if let Some(f) = handler {
                    let utbl = unit_to_table(&self.lua, unit)?;
                    f.call::<()>((utbl, dt))?;
                    return Ok(());
                }
            }
        }
        // Fallback na globální on_ai_tick
        let hook: Option<LuaFunction> = globals.get("on_ai_tick")?;
        if let Some(f) = hook {
            let utbl = unit_to_table(&self.lua, unit)?;
            f.call::<()>((utbl, dt))?;
        }
        Ok(())
    }

    /// Naplní `__unit_cache` – tabulka indexovaná entity_id pro Engine.get_unit().
    pub fn push_unit_cache(&self, units: &[UnitInfo]) -> LuaResult<()> {
        let cache = self.lua.create_table()?;
        for u in units {
            cache.raw_set(u.entity_id, unit_to_table(&self.lua, u)?)?;
        }
        self.lua.globals().set("__unit_cache", cache)?;
        Ok(())
    }

    /// on_game_tick(dt) – globální periodický tick pro skriptové systémy
    pub fn hook_game_tick(&self, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_game_tick")?;
        if let Some(f) = hook { f.call::<()>(dt)?; }
        Ok(())
    }

    /// on_resource_changed(gold, lumber, oil)
    pub fn hook_resource_changed(&self, gold: u32, lumber: u32, oil: u32) -> LuaResult<()> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_resource_changed")?;
        if let Some(f) = hook { f.call::<()>((gold, lumber, oil))?; }
        Ok(())
    }

    // ── Query: Lua může zavolat, Rust naplní tabulku ──────────────────────────

    /// Naplní globální `__query_result` snapshot entit vyhovujících filtru.
    /// Voláno z Engine.query_units() v api.rs před spuštěním Lua callbacku.
    pub fn push_query_results(&self, units: Vec<UnitInfo>) -> LuaResult<()> {
        let tbl = self.lua.create_table()?;
        for (i, u) in units.iter().enumerate() {
            tbl.raw_set(i + 1, unit_to_table(&self.lua, u)?)?;
        }
        self.lua.globals().set("__query_result", tbl)?;
        Ok(())
    }

    /// Vrátí a smaže frontu ScriptCmd
    pub fn drain_commands(&self) -> LuaResult<Vec<ScriptCmd>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__cmd_queue")?;
        let len  = queue.raw_len();
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

    /// Přímý přístup k Lua globals (pro unit info queries z in_game.rs)
    pub fn lua(&self) -> &Lua { &self.lua }
}

fn manifest_load_order(module_path: &std::path::Path) -> i64 {
    let Ok(text) = std::fs::read_to_string(module_path.join("manifest.lua")) else {
        return 100;
    };
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

// ── Konverzní funkce ──────────────────────────────────────────────────────────

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

pub fn params_to_table(lua: &Lua, p: &MoveParams) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("speed",        p.speed)?;
    t.set("can_swim",     p.can_swim)?;
    t.set("can_fly",      p.can_fly)?;
    t.set("speed_water",  p.speed_water)?;
    t.set("speed_forest", p.speed_forest)?;
    t.set("speed_road",   p.speed_road)?;
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
        "attack_unit" => Ok(ScriptCmd::AttackUnit {
            attacker_id: t.get("attacker_id")?,
            target_id:   t.get("target_id")?,
        }),
        "stop_unit" => Ok(ScriptCmd::StopUnit {
            entity_id: t.get("entity_id")?,
        }),
        "set_health" => Ok(ScriptCmd::SetHealth {
            entity_id: t.get("entity_id")?,
            hp:        t.get("hp")?,
        }),
        "kill_unit"  => Ok(ScriptCmd::KillUnit  { entity_id: t.get("entity_id")? }),
        "add_resources" => Ok(ScriptCmd::AddResources {
            gold:   t.get("gold")  .unwrap_or(0),
            lumber: t.get("lumber").unwrap_or(0),
            oil:    t.get("oil")   .unwrap_or(0),
        }),
        "spawn_unit" => Ok(ScriptCmd::SpawnUnit {
            kind_id: t.get("kind_id")?,
            x:       t.get("x")?,
            y:       t.get("y")?,
            team:    t.get("team").unwrap_or(0),
        }),
        "train_unit" => Ok(ScriptCmd::TrainUnit {
            building_id: t.get("building_id")?,
            kind_id:     t.get("kind_id")?,
            build_time:  t.get("build_time").unwrap_or(0.0),
        }),
        "set_rally" => Ok(ScriptCmd::SetRally {
            building_id: t.get("building_id")?,
            x:           t.get("x")?,
            y:           t.get("y")?,
        }),
        "set_ai" => Ok(ScriptCmd::SetAi {
            entity_id:     t.get("entity_id")?,
            script_id:     t.get("script_id")?,
            tick_interval: t.get("tick_interval").unwrap_or(1.0),
        }),
        "set_ai_state" => Ok(ScriptCmd::SetAiState {
            entity_id:  t.get("entity_id")?,
            state_json: t.get("state_json")?,
        }),
        other => Err(LuaError::RuntimeError(format!("neznámý cmd: {other}"))),
    }
}

fn call_hook1(lua: &Lua, name: &str, unit: &UnitInfo) -> LuaResult<()> {
    let globals = lua.globals();
    let hook: Option<LuaFunction> = globals.get(name)?;
    if let Some(f) = hook {
        let tbl = unit_to_table(lua, unit)?;
        f.call::<()>(tbl)?;
    }
    Ok(())
}

fn call_optional_fn2d(lua: &Lua, name: &str, a: &UnitInfo, b: &UnitInfo, d: i32) -> LuaResult<()> {
    let globals = lua.globals();
    let hook: Option<LuaFunction> = globals.get(name)?;
    if let Some(f) = hook {
        let at = unit_to_table(lua, a)?;
        let bt = unit_to_table(lua, b)?;
        f.call::<()>((at, bt, d))?;
    }
    Ok(())
}
