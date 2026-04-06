-- units/init.lua
-- Definice všech jednotek a budov.

-- ════════════════════════════════════════════════
-- HUMANITÉ
-- ════════════════════════════════════════════════

RegisterUnit {
    kind         = "peasant",
    name         = "Peasant",
    hp_max       = 30,
    speed        = 128.0,
    damage       = 3,  pierce = 0,  armor = 0,
    attack_range = 0.0, attack_cd = 1.5,
    sight        = 4,
    cost_gold    = 400, cost_lumber = 0, build_time = 45.0,
    speed_forest = 0.85,
    ai_script    = "worker_ai",
}

RegisterUnit {
    kind         = "footman",
    name         = "Footman",
    hp_max       = 60,
    speed        = 128.0,
    damage       = 6,  pierce = 3,  armor = 2,
    attack_range = 0.0, attack_cd = 1.0,
    sight        = 4,
    cost_gold    = 600, cost_lumber = 0, build_time = 60.0,
    speed_forest = 0.6,
    ai_script    = "melee_ai",
}

RegisterUnit {
    kind         = "archer",
    name         = "Elven Archer",
    hp_max       = 40,
    speed        = 128.0,
    damage       = 3,  pierce = 6, armor = 0,
    attack_range = 128.0, attack_cd = 1.0,
    sight        = 5,
    cost_gold    = 500, cost_lumber = 50, build_time = 70.0,
    speed_forest = 0.9,
    ai_script    = "ranged_ai",
}

RegisterUnit {
    kind         = "knight",
    name         = "Knight",
    hp_max       = 90,
    speed        = 192.0,
    damage       = 8,  pierce = 4, armor = 4,
    attack_range = 0.0, attack_cd = 1.0,
    sight        = 4,
    cost_gold    = 800, cost_lumber = 100, build_time = 90.0,
    speed_forest = 0.4, speed_road = 1.3,
    ai_script    = "melee_ai",
}

RegisterUnit {
    kind         = "mage",
    name         = "Mage",
    hp_max       = 35,
    speed        = 128.0,
    damage       = 0,  pierce = 9, armor = 0,
    attack_range = 160.0, attack_cd = 1.5,
    sight        = 9,
    cost_gold    = 1200, cost_lumber = 0, build_time = 120.0,
    ai_script    = "ranged_ai",
}

RegisterUnit {
    kind         = "gryphon_rider",
    name         = "Gryphon Rider",
    hp_max       = 100,
    speed        = 384.0,
    damage       = 16, pierce = 0, armor = 5,
    attack_range = 0.0, attack_cd = 1.5,
    sight        = 6,
    cost_gold    = 2500, cost_lumber = 0, build_time = 250.0,
    can_fly      = true,
    ai_script    = "melee_ai",
}

-- ════════════════════════════════════════════════
-- ORCI
-- ════════════════════════════════════════════════

RegisterUnit {
    kind         = "peon",
    name         = "Peon",
    hp_max       = 30,
    speed        = 128.0,
    damage       = 3,  pierce = 0, armor = 0,
    attack_range = 0.0, attack_cd = 1.5,
    sight        = 4,
    cost_gold    = 400, build_time = 45.0,
    speed_forest = 0.85,
    ai_script    = "worker_ai",
}

RegisterUnit {
    kind         = "grunt",
    name         = "Grunt",
    hp_max       = 60,
    speed        = 128.0,
    damage       = 8,  pierce = 2, armor = 2,
    attack_range = 0.0, attack_cd = 1.0,
    sight        = 4,
    cost_gold    = 600, build_time = 60.0,
    speed_forest = 0.65,
    ai_script    = "melee_ai",
}

RegisterUnit {
    kind         = "troll_axethrower",
    name         = "Troll Axethrower",
    hp_max       = 40,
    speed        = 128.0,
    damage       = 3,  pierce = 6, armor = 0,
    attack_range = 128.0, attack_cd = 1.0,
    sight        = 5,
    cost_gold    = 500, cost_lumber = 50, build_time = 70.0,
    speed_forest = 0.85,
    ai_script    = "ranged_ai",
}

RegisterUnit {
    kind         = "ogre",
    name         = "Ogre",
    hp_max       = 90,
    speed        = 128.0,
    damage       = 10, pierce = 2, armor = 4,
    attack_range = 0.0, attack_cd = 1.3,
    sight        = 4,
    cost_gold    = 800, cost_lumber = 100, build_time = 90.0,
    ai_script    = "melee_ai",
}

RegisterUnit {
    kind         = "death_knight",
    name         = "Death Knight",
    hp_max       = 60,
    speed        = 192.0,
    damage       = 0,  pierce = 9, armor = 0,
    attack_range = 160.0, attack_cd = 1.5,
    sight        = 9,
    cost_gold    = 1200, build_time = 120.0,
    ai_script    = "ranged_ai",
}

RegisterUnit {
    kind         = "dragon",
    name         = "Dragon",
    hp_max       = 100,
    speed        = 384.0,
    damage       = 16, pierce = 0, armor = 5,
    attack_range = 0.0, attack_cd = 1.5,
    sight        = 6,
    cost_gold    = 2500, build_time = 250.0,
    can_fly      = true,
    ai_script    = "melee_ai",
}

-- ════════════════════════════════════════════════
-- NÁMOŘNÍ
-- ════════════════════════════════════════════════

RegisterUnit {
    kind         = "elven_destroyer",
    name         = "Elven Destroyer",
    hp_max       = 100,
    speed        = 192.0,
    damage       = 10, pierce = 0, armor = 2,
    attack_range = 192.0, attack_cd = 1.5,
    sight        = 6,
    cost_gold    = 700, cost_lumber = 350, build_time = 90.0,
    can_swim     = true, speed_water = 1.0,
    ai_script    = "ranged_ai",
}

-- ════════════════════════════════════════════════
-- BUDOVY
-- ════════════════════════════════════════════════

RegisterBuilding {
    kind        = "town_hall",
    name        = "Town Hall",
    hp_max      = 1200, armor = 0, size_tiles = 4,
    cost_gold   = 1200, cost_lumber = 800, build_time = 255.0,
    produces    = { "peasant" },
}

RegisterBuilding {
    kind        = "barracks",
    name        = "Barracks",
    hp_max      = 800, armor = 0, size_tiles = 3,
    cost_gold   = 700, cost_lumber = 450, build_time = 200.0,
    produces    = { "footman", "archer" },
}

RegisterBuilding {
    kind        = "farm",
    name        = "Farm",
    hp_max      = 400, size_tiles = 2,
    cost_gold   = 500, cost_lumber = 250, build_time = 100.0,
    produces    = {},  food = 4,
}

RegisterBuilding {
    kind        = "great_hall",
    name        = "Great Hall",
    hp_max      = 1200, size_tiles = 4,
    cost_gold   = 1200, cost_lumber = 800, build_time = 255.0,
    produces    = { "peon" },
}

RegisterBuilding {
    kind        = "orc_barracks",
    name        = "Orc Barracks",
    hp_max      = 800, size_tiles = 3,
    cost_gold   = 700, cost_lumber = 450, build_time = 200.0,
    produces    = { "grunt", "troll_axethrower" },
}

-- ════════════════════════════════════════════════
-- HOOKY jednotek
-- ════════════════════════════════════════════════

local _base_died = on_unit_died
function on_unit_died(unit)
    _base_died(unit)
    Engine.log(unit.kind .. " #" .. unit.entity_id .. " (team " .. unit.team .. ") zemřel")
    -- Odměna surovin zabíjejícímu týmu (TODO: přidat attacker_team do UnitInfo)
end

local _base_trained = on_unit_trained
function on_unit_trained(unit, building_id)
    _base_trained(unit, building_id)
    Engine.log("Vytrénována jednotka " .. unit.kind .. " z budovy #" .. building_id)
end
