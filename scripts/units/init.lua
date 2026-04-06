-- units/init.lua
-- Registrace všech herních jednotek.
-- Každá jednotka definuje své pohybové vlastnosti – engine je použije
-- automaticky přes on_move_order hook v base/init.lua.

-- ── Humanité ────────────────────────────────────────────────────────────────

RegisterUnit {
    kind         = "peasant",
    name         = "Peasant",
    hp_max       = 30,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.75,
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "footman",
    name         = "Footman",
    hp_max       = 60,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.6,
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "archer",
    name         = "Elven Archer",
    hp_max       = 40,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.9,  -- elfové v lese rychlejší
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "knight",
    name         = "Knight",
    hp_max       = 90,
    speed        = 192.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.4,  -- těžká jízda v lese velmi pomalá
    speed_road   = 1.3,  -- ale na cestě rychlejší než pěchota
}

RegisterUnit {
    kind         = "gryphon_rider",
    name         = "Gryphon Rider",
    hp_max       = 100,
    speed        = 384.0,
    can_swim     = false,
    can_fly      = true,   -- létá – ignoruje terén
    speed_forest = 1.0,
    speed_road   = 1.0,
}

-- ── Orci ────────────────────────────────────────────────────────────────────

RegisterUnit {
    kind         = "peon",
    name         = "Peon",
    hp_max       = 30,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.75,
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "grunt",
    name         = "Grunt",
    hp_max       = 60,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.65,
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "troll_axethrower",
    name         = "Troll Axethrower",
    hp_max       = 40,
    speed        = 128.0,
    can_swim     = false,
    can_fly      = false,
    speed_forest = 0.85,
    speed_road   = 1.0,
}

RegisterUnit {
    kind         = "dragon",
    name         = "Dragon",
    hp_max       = 100,
    speed        = 384.0,
    can_swim     = false,
    can_fly      = true,
    speed_forest = 1.0,
    speed_road   = 1.0,
}

-- ── Námořní ─────────────────────────────────────────────────────────────────

RegisterUnit {
    kind         = "elven_destroyer",
    name         = "Elven Destroyer",
    hp_max       = 100,
    speed        = 192.0,
    can_swim     = true,
    can_fly      = false,
    speed_water  = 1.0,   -- pohybuje se výhradně na vodě
    speed_forest = 0.0,
    speed_road   = 0.0,
}

-- ── Hooky specifické pro units modul ────────────────────────────────────────

-- Přepiš on_unit_arrived – peasant po dosažení cíle zahlásí zprávu
local _base_arrived = on_unit_arrived
function on_unit_arrived(unit)
    _base_arrived(unit)
    if unit.kind == "peasant" then
        Engine.log("Peasant #" .. unit.entity_id .. " dorazil na cíl")
    end
end

-- Přepiš on_unit_died – loguj a případně spawn resource drop
local _base_died = on_unit_died
function on_unit_died(unit)
    _base_died(unit)
    Engine.log(unit.kind .. " #" .. unit.entity_id .. " zemřel (tým " .. unit.team .. ")")
end
