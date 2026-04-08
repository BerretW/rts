## Lua API přehled

### Globální funkce (server i klient)

| Funkce                        | Popis                                                  |
| ----------------------------- | ------------------------------------------------------ |
| `AddEventHandler(name, cb)` | Registruje callback na event                           |
| `TriggerEvent(name, ...)`   | Spustí event lokálně (handlers ve stejném runtemu) |

### Jen server

| Funkce                                    | Popis                                                                       |
| ----------------------------------------- | --------------------------------------------------------------------------- |
| `TriggerClientEvent(name, target, ...)` | Pošle event klientovi.`target` = `player_id` nebo `-1` pro broadcast |

### Jen klient

| Funkce                            | Popis                  |
| --------------------------------- | ---------------------- |
| `TriggerServerEvent(name, ...)` | Pošle event na server |

---

### Engine API (server i klient)

#### Pohyb

| Funkce                                   | Popis           |
| ---------------------------------------- | --------------- |
| `Engine.move_unit(id, tx, ty, speed?)` | Rozkaz k pohybu |
| `Engine.stop_unit(id)`                 | Zastav jednotku |

#### Boj

| Funkce                                              | Popis                      |
| --------------------------------------------------- | -------------------------- |
| `Engine.attack_unit(attacker_id, target_id)`      | Rozkaz k útoku            |
| `Engine.kill_unit(id)`                            | Okamžitě zabij jednotku  |
| `Engine.set_health(id, hp)`                       | Nastav HP                  |
| `Engine.set_ability_cooldown(id, ability_id, cd)` | Nastav cooldown schopnosti |

#### Spawn / výroba

| Funkce                                                   | Popis                       |
| -------------------------------------------------------- | --------------------------- |
| `Engine.spawn_unit(kind_id, x, y, team?)`              | Spawni jednotku             |
| `Engine.train_unit(building_id, kind_id, build_time?)` | Přidej do výrobní fronty |
| `Engine.set_rally(building_id, x, y)`                  | Nastav rally point budovy   |

#### AI

| Funkce                                           | Popis                       |
| ------------------------------------------------ | --------------------------- |
| `Engine.set_ai(id, script_id, tick_interval?)` | Přiřaď AI skript entitě |

#### Query

| Funkce                          | Popis                                                  |
| ------------------------------- | ------------------------------------------------------ |
| `Engine.query_units(filter?)` | Vrátí seznam jednotek (filtr:`{team=n, kind="x"}`) |
| `Engine.get_unit(id)`         | Vrátí snapshot entity nebo `nil`                   |

#### Assets / debug

| Funkce                           | Popis                                  |
| -------------------------------- | -------------------------------------- |
| `Engine.log(msg)`              | Výpis do logu serveru                 |
| `Engine.load_json(path)`       | Načte JSON z assets/ jako Lua tabulku |
| `Engine.load_asset_text(path)` | Načte textový soubor z assets/       |
| `Engine.assets_dir()`          | Vrátí cestu k assets/ složce        |
| `Engine.TILE_SIZE`             | Konstanta =`32.0`                    |

---

### Hooky (globální funkce – server i klient)

Definuješ je v Lua a engine je volá automaticky:

| Hook                                                           | Argumenty                       | Popis                                                                |
| -------------------------------------------------------------- | ------------------------------- | -------------------------------------------------------------------- |
| `on_unit_died(unit)`                                         | UnitInfo                        | Jednotka zemřela                                                    |
| `on_unit_spawned(unit)`                                      | UnitInfo                        | Jednotka se zjevila                                                  |
| `on_unit_arrived(unit)`                                      | UnitInfo                        | Dorazila na cíl (jen klient)                                        |
| `on_unit_attack(attacker, target, damage)`                   | UnitInfo, UnitInfo, int         | Útok                                                                |
| `on_unit_hit(unit, damage, attacker_id)`                     | UnitInfo, int, id               | Zásah                                                               |
| `on_unit_trained(unit, building_id)`                         | UnitInfo, id                    | Výroba dokončena                                                   |
| `on_ai_tick(unit, dt)`                                       | UnitInfo, float                 | AI tick (fallback)                                                   |
| `on_game_tick(dt)`                                           | float                           | Globální tick každý frame                                        |
| `on_ability_used(caster, ability_id, target_id\|nil, tx, ty)` | –                              | Schopnost použita (jen server)                                      |
| `on_move_order(unit, tx, ty, params)`                        | UnitInfo, float, float, tabulka | Pohybový rozkaz; vrátí `params`, `false` = zruš (jen klient) |
| `on_resource_changed(gold, lumber, oil)`                     | –                              | Změna zdrojů (jen klient)                                          |

---

### UnitInfo tabulka

```lua
unit.entity_id    -- číslo (hecs ID)
unit.x, unit.y    -- pozice
unit.hp, unit.hp_max
unit.damage, unit.pierce, unit.armor
unit.attack_range
unit.team         -- 0, 1, 2 ...
unit.kind         -- "footman", "peasant" atd.
```

### AiDefs (registrace vlastního AI)

```lua
AiDefs = AiDefs or {}
AiDefs["moje_ai"] = {
    on_tick = function(unit, dt)
        -- vlastní AI logika
    end
}
-- pak zaregistrovat na entitu:
Engine.set_ai(unit.entity_id, "moje_ai", 0.5)
```
