-- units/init.lua  (StarCraft-inspirovaný obsah)

-- ── SCHOPNOSTI ────────────────────────────────────────────────────────────────

-- Patrol je standardní příkaz pohybu – nevyžaduje handler (řeší server)
RegisterAbility {
    id="patrol", name="Patrol", hotkey="P", target="point", cooldown=0,
    handler=function(caster, target_id, tx, ty)
        -- Logika patroly je řešena na serveru jako PatrolOrder
        -- Lua handler je volitelný hook pro logování/efekty
    end
}

-- Léčení – Mág obnoví 50 HP spojenci (cooldown 10s)
RegisterAbility {
    id="holy_light", name="Holy Light", hotkey="H", target="unit", cooldown=10,
    handler=function(caster, target_id, tx, ty)
        if not target_id then return end
        local target = Engine.get_unit(target_id)
        if not target then return end
        if target.team ~= caster.team then return end   -- pouze spojenci
        local healed = math.min(target.hp + 50, target.hp_max)
        Engine.set_health(target_id, healed)
        Engine.set_ability_cooldown(caster.entity_id, "holy_light", 10)
        Engine.log("Holy Light: " .. caster.kind .. " léčí " .. target.kind ..
                   " (" .. healed .. "/" .. target.hp_max .. " HP)")
    end
}

-- Death Coil – Rytíř smrti způsobí 50 poškození nepříteli (cooldown 8s)
RegisterAbility {
    id="death_coil", name="Death Coil", hotkey="D", target="unit", cooldown=8,
    handler=function(caster, target_id, tx, ty)
        if not target_id then return end
        local target = Engine.get_unit(target_id)
        if not target then return end
        if target.team == caster.team then return end   -- pouze nepřátelé
        Engine.set_health(target_id, math.max(target.hp - 50, 0))
        Engine.set_ability_cooldown(caster.entity_id, "death_coil", 8)
        Engine.log("Death Coil: " .. caster.kind .. " zasáhl " .. target.kind ..
                   " za 50 poškození")
    end
}

-- Blizzard – Drak způsobí 30 poškození všem nepřátelům v okruhu 128px (cooldown 15s)
RegisterAbility {
    id="blizzard", name="Blizzard", hotkey="B", target="point", cooldown=15,
    handler=function(caster, target_id, tx, ty)
        local radius = 128
        local units  = Engine.query_units()
        local count  = 0
        for i = 1, #units do
            local u = units[i]
            if u.team ~= caster.team and Dist(u.x, u.y, tx, ty) <= radius then
                Engine.set_health(u.entity_id, math.max(u.hp - 30, 0))
                count = count + 1
            end
        end
        Engine.set_ability_cooldown(caster.entity_id, "blizzard", 15)
        Engine.log("Blizzard: zasáhl " .. count .. " jednotek v okruhu " .. radius)
    end
}

-- ── LIDÉ ──────────────────────────────────────────────────────────────────────

RegisterUnit { kind="peasant",       name="Peasant",         hp_max=30,  damage=3,  pierce=0,  armor=0, speed=128, attack_range=0,   attack_cd=1.5, cost_gold=400,  cost_lumber=0,   build_time=15, ai_script="worker_ai" }
RegisterUnit { kind="footman",       name="Footman",         hp_max=60,  damage=6,  pierce=3,  armor=2, speed=128, attack_range=0,   attack_cd=1.0, cost_gold=600,  cost_lumber=0,   build_time=20, ai_script="melee_ai",  abilities={"patrol"} }
RegisterUnit { kind="archer",        name="Elven Archer",    hp_max=40,  damage=4,  pierce=6,  armor=0, speed=128, attack_range=160, attack_cd=1.2, cost_gold=500,  cost_lumber=50,  build_time=20, ai_script="ranged_ai", abilities={"patrol"} }
RegisterUnit { kind="knight",        name="Knight",          hp_max=100, damage=10, pierce=4,  armor=5, speed=192, attack_range=0,   attack_cd=1.0, cost_gold=800,  cost_lumber=100, build_time=30, ai_script="melee_ai",  abilities={"patrol"} }
RegisterUnit { kind="mage",          name="Mage",            hp_max=35,  damage=0,  pierce=12, armor=0, speed=96,  attack_range=192, attack_cd=2.0, cost_gold=1200, cost_lumber=0,   build_time=35, ai_script="ranged_ai", abilities={"patrol","holy_light"} }
RegisterUnit { kind="gryphon_rider", name="Gryphon Rider",   hp_max=100, damage=16, pierce=5,  armor=5, speed=384, attack_range=0,   attack_cd=1.0, cost_gold=2500, cost_lumber=0,   build_time=45, ai_script="melee_ai",  can_fly=true, abilities={"patrol"} }

-- ── ORKOVÉ ────────────────────────────────────────────────────────────────────

RegisterUnit { kind="peon",             name="Peon",            hp_max=30,  damage=3,  pierce=0,  armor=0, speed=128, attack_range=0,   attack_cd=1.5, cost_gold=400,  cost_lumber=0,   build_time=15, ai_script="worker_ai" }
RegisterUnit { kind="grunt",            name="Grunt",           hp_max=70,  damage=8,  pierce=2,  armor=2, speed=128, attack_range=0,   attack_cd=1.0, cost_gold=600,  cost_lumber=0,   build_time=20, ai_script="melee_ai",  abilities={"patrol"} }
RegisterUnit { kind="troll_axethrower", name="Troll Axethrower",hp_max=40,  damage=4,  pierce=6,  armor=0, speed=128, attack_range=160, attack_cd=1.2, cost_gold=500,  cost_lumber=50,  build_time=20, ai_script="ranged_ai", abilities={"patrol"} }
RegisterUnit { kind="ogre",             name="Ogre",            hp_max=100, damage=10, pierce=2,  armor=5, speed=96,  attack_range=0,   attack_cd=1.3, cost_gold=800,  cost_lumber=100, build_time=30, ai_script="melee_ai",  abilities={"patrol"} }
RegisterUnit { kind="death_knight",     name="Death Knight",    hp_max=60,  damage=0,  pierce=10, armor=2, speed=96,  attack_range=192, attack_cd=2.0, cost_gold=1200, cost_lumber=0,   build_time=35, ai_script="ranged_ai", abilities={"patrol","death_coil"} }
RegisterUnit { kind="dragon",           name="Dragon",          hp_max=100, damage=16, pierce=5,  armor=5, speed=384, attack_range=0,   attack_cd=1.0, cost_gold=2500, cost_lumber=0,   build_time=45, ai_script="melee_ai",  can_fly=true, abilities={"patrol","blizzard"} }

-- ── NÁMOŘNÍ ───────────────────────────────────────────────────────────────────

RegisterUnit { kind="elven_destroyer", name="Elven Destroyer", hp_max=100, damage=10, pierce=0, armor=0, speed=192, attack_range=192, attack_cd=1.5, cost_gold=700, cost_lumber=350, build_time=30, ai_script="ranged_ai", can_swim=true, speed_water=192, abilities={"patrol"} }

-- ── BUDOVY LIDÍ ───────────────────────────────────────────────────────────────

RegisterBuilding { kind="town_hall",      hp_max=1200, armor=2, size_tiles=4, cost_gold=1200, cost_lumber=800, build_time=60, produces={"peasant"}, food=0 }
RegisterBuilding { kind="barracks",       hp_max=800,  armor=0, size_tiles=3, cost_gold=700,  cost_lumber=450, build_time=40, produces={"footman","archer"}, food=0 }
RegisterBuilding { kind="stable",         hp_max=500,  armor=0, size_tiles=3, cost_gold=1000, cost_lumber=300, build_time=40, produces={"knight"}, food=0 }
RegisterBuilding { kind="mage_tower",     hp_max=400,  armor=0, size_tiles=2, cost_gold=1000, cost_lumber=200, build_time=50, produces={"mage"}, food=0 }
RegisterBuilding { kind="gryphon_aviary", hp_max=500,  armor=0, size_tiles=3, cost_gold=1500, cost_lumber=400, build_time=60, produces={"gryphon_rider"}, food=0 }
RegisterBuilding { kind="farm",           hp_max=400,  armor=0, size_tiles=2, cost_gold=500,  cost_lumber=250, build_time=20, produces={}, food=4 }

-- ── BUDOVY ORKŮ ───────────────────────────────────────────────────────────────

RegisterBuilding { kind="great_hall",   hp_max=1200, armor=2, size_tiles=4, cost_gold=1200, cost_lumber=800, build_time=60, produces={"peon"}, food=0 }
RegisterBuilding { kind="orc_barracks", hp_max=800,  armor=0, size_tiles=3, cost_gold=700,  cost_lumber=450, build_time=40, produces={"grunt","troll_axethrower"}, food=0 }
RegisterBuilding { kind="ogre_mound",   hp_max=600,  armor=0, size_tiles=3, cost_gold=1000, cost_lumber=300, build_time=40, produces={"ogre"}, food=0 }
RegisterBuilding { kind="altar",        hp_max=500,  armor=0, size_tiles=2, cost_gold=1000, cost_lumber=200, build_time=50, produces={"death_knight"}, food=0 }
RegisterBuilding { kind="dragon_roost", hp_max=600,  armor=0, size_tiles=3, cost_gold=1500, cost_lumber=400, build_time=60, produces={"dragon"}, food=0 }
RegisterBuilding { kind="pig_farm",     hp_max=400,  armor=0, size_tiles=2, cost_gold=500,  cost_lumber=250, build_time=20, produces={}, food=4 }

-- ── HOOKY ─────────────────────────────────────────────────────────────────────

function on_unit_died(unit)
    Engine.log("Padl: " .. unit.kind .. " #" .. unit.entity_id .. " (tym " .. unit.team .. ")")
end

function on_unit_trained(unit, building_id)
    Engine.log("Vycvicen: " .. unit.kind .. " z budovy #" .. tostring(building_id))
end
