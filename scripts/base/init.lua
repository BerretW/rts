-- base/init.lua
-- Globální utility a výchozí hooky enginu.
-- Ostatní moduly je mohou přepsat nebo rozšířit.

-- ── Registr definic jednotek ────────────────────────────────────────────────
-- Klíč: kind_id (string), hodnota: tabulka s parametry jednotky.
UnitDefs = {}

--- Zaregistruje definici jednotky.
function RegisterUnit(def)
    assert(def.kind,    "RegisterUnit: chybí 'kind'")
    assert(def.hp_max,  "RegisterUnit: chybí 'hp_max'")
    UnitDefs[def.kind] = def
    Engine.log("registrována jednotka: " .. def.kind)
end

--- Vrátí výchozí pohybové parametry pro daný kind.
function DefaultMoveParams(kind)
    local def = UnitDefs[kind]
    if not def then
        return { speed = 128.0, can_swim = false, can_fly = false,
                 speed_water = 0.0, speed_forest = 0.75, speed_road = 1.0 }
    end
    return {
        speed        = def.speed        or 128.0,
        can_swim     = def.can_swim     or false,
        can_fly      = def.can_fly      or false,
        speed_water  = def.speed_water  or 0.0,
        speed_forest = def.speed_forest or 0.75,
        speed_road   = def.speed_road   or 1.0,
    }
end

-- ── Výchozí hooky (lze přepsat jinými moduly) ───────────────────────────────

--- Voláno Rustem před každým pohybovým rozkazem.
--- Vrátí tabulku parametrů pohybu (může přepsat), nebo false pro zablokování.
function on_move_order(unit, target_x, target_y, params)
    -- Výchozí: přenes parametry z definice jednotky, pokud existuje.
    local def = UnitDefs[unit.kind]
    if def then
        params.speed        = def.speed        or params.speed
        params.can_swim     = def.can_swim     or params.can_swim
        params.can_fly      = def.can_fly      or params.can_fly
        params.speed_water  = def.speed_water  or params.speed_water
        params.speed_forest = def.speed_forest or params.speed_forest
        params.speed_road   = def.speed_road   or params.speed_road
    end
    return params  -- vrátit tabulku = potvrdit pohyb s (případně upravenými) parametry
end

--- Voláno když jednotka dorazí na cíl.
function on_unit_arrived(unit)
    -- výchozí: nic
end

--- Voláno když jednotka zemře.
function on_unit_died(unit)
    -- výchozí: nic
end
