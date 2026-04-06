//! Lua scripting runtime – FiveM-inspirovaný modulový systém.
//!
//! ## Struktura skriptů
//! ```
//! scripts/
//!   base/
//!     manifest.lua      ← název modulu, závislosti
//!     init.lua          ← hlavní vstupní bod modulu
//!   units/
//!     manifest.lua
//!     peasant.lua
//!     grunt.lua
//!     ...
//! ```
//!
//! ## Tok dat Rust ↔ Lua
//! Rust volá Lua hooky (on_move_order, on_unit_arrived, on_unit_died, ...).
//! Lua volá Rust callbacky přes globální tabulku `Engine` (engine.move_unit, ...).
//! Příkazy z Lua jsou fronty ve `Vec<ScriptCmd>` – Rust je zpracuje na konci snímku.

pub mod api;

use std::path::Path;
use mlua::prelude::*;

use crate::components::MoveFlags;

// ── Příkazy z Lua → Rust ──────────────────────────────────────────────────────

/// Příkaz, který Lua skript požaduje od enginu.
/// Rust zpracuje celou frontu po Lua ticku – žádné borrow konflikty.
#[derive(Debug)]
pub enum ScriptCmd {
    /// Pohni s entitou `entity_id` na cíl `(x, y)` s danými parametry.
    MoveUnit {
        entity_id: u64,
        target_x:  f32,
        target_y:  f32,
        params:    MoveParams,
    },
    /// Nastav HP entity.
    SetHealth {
        entity_id: u64,
        hp:        i32,
    },
    /// Okamžitě zničí entitu (despawn na konci snímku).
    KillUnit {
        entity_id: u64,
    },
    /// Přidej suroviny hráče (záporné = odeber).
    AddResources {
        gold:   i32,
        lumber: i32,
        oil:    i32,
    },
    /// Spawnuj novou jednotku (z Lua definice).
    SpawnUnit {
        kind_id:  String,  // např. "peasant", "grunt"
        x: f32, y: f32,
        team: u8,
    },
}

/// Pohybové parametry předané Lua skriptem – mapuje se na `MoveFlags` + speed.
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
        Self {
            speed:        128.0,
            can_swim:     false,
            can_fly:      false,
            speed_water:  0.0,
            speed_forest: 0.75,
            speed_road:   1.0,
        }
    }
}

impl From<MoveParams> for MoveFlags {
    fn from(p: MoveParams) -> Self {
        MoveFlags {
            can_swim:     p.can_swim,
            can_fly:      p.can_fly,
            speed_water:  p.speed_water,
            speed_forest: p.speed_forest,
            speed_road:   p.speed_road,
        }
    }
}

// ── Informace o entitě předávané Lua hookům ───────────────────────────────────

/// Snapshot entity posílaný do Lua hooků (kopie – nevyžaduje zapůjčení World).
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub entity_id: u64,
    pub x:         f32,
    pub y:         f32,
    pub hp:        i32,
    pub hp_max:    i32,
    pub team:      u8,
    pub kind_id:   String,  // "peasant", "grunt", …
}

// ── Lua runtime ───────────────────────────────────────────────────────────────

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    /// Vytvoří nový runtime a zaregistruje Engine API.
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();
        api::register(&lua)?;
        Ok(Self { lua })
    }

    /// Načte všechny moduly ze složky `scripts/`.
    /// Prochází podsložky s `manifest.lua` a spouští `init.lua`.
    pub fn load_scripts(&self, scripts_dir: &Path) -> LuaResult<()> {
        if !scripts_dir.exists() {
            log::warn!("scripting: složka {:?} neexistuje, přeskakuji", scripts_dir);
            return Ok(());
        }

        let entries = std::fs::read_dir(scripts_dir)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

        let mut modules: Vec<std::path::PathBuf> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter(|e| e.path().join("manifest.lua").exists())
            .map(|e| e.path())
            .collect();

        // Seřadit abecedně – "base" se načte před "units" atd.
        modules.sort();

        for module_path in modules {
            self.load_module(&module_path)?;
        }

        Ok(())
    }

    fn load_module(&self, module_path: &Path) -> LuaResult<()> {
        let manifest_path = module_path.join("manifest.lua");
        let init_path     = module_path.join("init.lua");

        // Načti manifest (nepovinný kód – jen metadata)
        let manifest_src = std::fs::read_to_string(&manifest_path)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        self.lua.load(&manifest_src)
            .set_name(manifest_path.to_string_lossy().as_ref())
            .exec()?;

        // Načti init.lua pokud existuje
        if init_path.exists() {
            let init_src = std::fs::read_to_string(&init_path)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            self.lua.load(&init_src)
                .set_name(init_path.to_string_lossy().as_ref())
                .exec()?;
        }

        log::info!("scripting: načten modul {:?}", module_path.file_name().unwrap_or_default());
        Ok(())
    }

    /// Načte a spustí libovolný Lua soubor (pro hotreload v budoucnu).
    pub fn exec_file(&self, path: &Path) -> LuaResult<()> {
        let src = std::fs::read_to_string(path)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        self.lua.load(&src)
            .set_name(path.to_string_lossy().as_ref())
            .exec()
    }

    // ── Hooky Rust → Lua ─────────────────────────────────────────────────────

    /// Volá `on_move_order(unit, target_x, target_y, params_table)` pokud existuje.
    /// Vrátí `Some(ScriptCmd::MoveUnit)` pokud Lua potvrdí pohyb,
    /// nebo `None` pokud hook neexistuje / pohyb zablokuje.
    pub fn hook_move_order(
        &self,
        unit: &UnitInfo,
        target_x: f32,
        target_y: f32,
        default_params: MoveParams,
    ) -> LuaResult<Option<ScriptCmd>> {
        let globals = self.lua.globals();
        let hook: Option<LuaFunction> = globals.get("on_move_order")?;
        let Some(f) = hook else {
            // Žádný hook – provést pohyb s výchozími parametry
            return Ok(Some(ScriptCmd::MoveUnit {
                entity_id: unit.entity_id,
                target_x,
                target_y,
                params: default_params,
            }));
        };

        let unit_tbl  = unit_to_table(&self.lua, unit)?;
        let params_tbl = params_to_table(&self.lua, &default_params)?;

        // Lua hook může vrátit:
        //   false / nil  → pohyb zablokovat
        //   true / nic   → pohyb s původními parametry
        //   tabulku      → přepsat parametry pohybu
        let result: LuaValue = f.call((unit_tbl, target_x, target_y, params_tbl))?;

        match result {
            LuaValue::Boolean(false) | LuaValue::Nil => Ok(None),
            LuaValue::Table(t) => {
                let params = table_to_params(&t, default_params)?;
                Ok(Some(ScriptCmd::MoveUnit {
                    entity_id: unit.entity_id,
                    target_x,
                    target_y,
                    params,
                }))
            }
            _ => Ok(Some(ScriptCmd::MoveUnit {
                entity_id: unit.entity_id,
                target_x,
                target_y,
                params: default_params,
            })),
        }
    }

    /// Volá `on_unit_arrived(unit)` – jednotka dosáhla cíle.
    pub fn hook_unit_arrived(&self, unit: &UnitInfo) -> LuaResult<()> {
        call_optional_hook(&self.lua, "on_unit_arrived", unit)
    }

    /// Volá `on_unit_died(unit)` – jednotka zemřela.
    pub fn hook_unit_died(&self, unit: &UnitInfo) -> LuaResult<()> {
        call_optional_hook(&self.lua, "on_unit_died", unit)
    }

    /// Vrátí frontu příkazů nagenerovaných Lua skripty od posledního volání.
    /// Fronta se po vrácení vyprázdní.
    pub fn drain_commands(&self) -> LuaResult<Vec<ScriptCmd>> {
        let globals = self.lua.globals();
        let queue: LuaTable = globals.get("__cmd_queue")?;
        let len = queue.raw_len();
        let mut cmds = Vec::with_capacity(len as usize);

        for i in 1..=len {
            let tbl: LuaTable = queue.raw_get(i)?;
            if let Ok(cmd) = table_to_cmd(&tbl) {
                cmds.push(cmd);
            }
        }

        // Vyprázdni frontu
        globals.set("__cmd_queue", self.lua.create_table()?)?;

        Ok(cmds)
    }
}

// ── Pomocné konverze Rust ↔ Lua tabulky ──────────────────────────────────────

pub fn unit_to_table(lua: &Lua, u: &UnitInfo) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("entity_id", u.entity_id)?;
    t.set("x",         u.x)?;
    t.set("y",         u.y)?;
    t.set("hp",        u.hp)?;
    t.set("hp_max",    u.hp_max)?;
    t.set("team",      u.team)?;
    t.set("kind",      u.kind_id.clone())?;
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

fn table_to_params(t: &LuaTable, defaults: MoveParams) -> LuaResult<MoveParams> {
    Ok(MoveParams {
        speed:        t.get("speed")        .unwrap_or(defaults.speed),
        can_swim:     t.get("can_swim")     .unwrap_or(defaults.can_swim),
        can_fly:      t.get("can_fly")      .unwrap_or(defaults.can_fly),
        speed_water:  t.get("speed_water")  .unwrap_or(defaults.speed_water),
        speed_forest: t.get("speed_forest") .unwrap_or(defaults.speed_forest),
        speed_road:   t.get("speed_road")   .unwrap_or(defaults.speed_road),
    })
}

fn table_to_cmd(t: &LuaTable) -> LuaResult<ScriptCmd> {
    let cmd_type: String = t.get("cmd")?;
    match cmd_type.as_str() {
        "move_unit" => Ok(ScriptCmd::MoveUnit {
            entity_id: t.get("entity_id")?,
            target_x:  t.get("target_x")?,
            target_y:  t.get("target_y")?,
            params: {
                let pt: LuaTable = t.get("params")?;
                table_to_params(&pt, MoveParams::default())?
            },
        }),
        "set_health" => Ok(ScriptCmd::SetHealth {
            entity_id: t.get("entity_id")?,
            hp:        t.get("hp")?,
        }),
        "kill_unit" => Ok(ScriptCmd::KillUnit {
            entity_id: t.get("entity_id")?,
        }),
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
        other => Err(LuaError::RuntimeError(format!("neznámý ScriptCmd: {other}"))),
    }
}

fn call_optional_hook(lua: &Lua, name: &str, unit: &UnitInfo) -> LuaResult<()> {
    let globals = lua.globals();
    let hook: Option<LuaFunction> = globals.get(name)?;
    if let Some(f) = hook {
        let tbl = unit_to_table(lua, unit)?;
        f.call::<()>(tbl)?;
    }
    Ok(())
}
