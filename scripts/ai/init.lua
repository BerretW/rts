-- ai/init.lua
-- Implementace AI skriptů: worker_ai, melee_ai, ranged_ai.

-- ════════════════════════════════════════════════
-- WORKER AI
-- Chování: pohybuj se u základny; pokud je
--          jednotka daleko, vrať se zpět.
-- ════════════════════════════════════════════════

RegisterAi("worker_ai", {
    on_spawned = function(unit)
        Engine.log("Worker #" .. unit.entity_id .. " nasazen")
    end,

    on_tick = function(unit, dt)
        local base = FriendlyBase(unit)
        if base then
            local d = Dist(unit.x, unit.y, base.x, base.y)
            if d > 96 then
                -- Vrať se k základně s malým rozptylem
                local ox = (unit.entity_id % 5) * 24 - 48
                local oy = math.floor(unit.entity_id / 5) % 5 * 24 - 48
                Engine.move_unit(unit.entity_id, base.x + ox, base.y + 80 + oy)
            end
        end
    end,
})

-- ════════════════════════════════════════════════
-- MELEE AI
-- Chování: najdi nejbližšího nepřítele, zaútoč.
--          Pokud žádný nepřítel není, stůj.
-- ════════════════════════════════════════════════

RegisterAi("melee_ai", {
    on_tick = function(unit, dt)
        local enemy, dist = NearestEnemy(unit)
        if not enemy then return end

        -- Melee dosah = 1.5 tile = 48 px (plus malý buffer)
        local melee_range = 56
        if dist <= melee_range then
            Engine.attack_unit(unit.entity_id, enemy.entity_id)
        else
            Engine.move_unit(unit.entity_id, enemy.x, enemy.y)
        end
    end,
})

-- ════════════════════════════════════════════════
-- RANGED AI
-- Chování: drž optimální vzdálenost, útočí
--          projektily. Ustupuje pokud je příliš
--          blízko.
-- ════════════════════════════════════════════════

RegisterAi("ranged_ai", {
    on_tick = function(unit, dt)
        local enemy, dist = NearestEnemy(unit)
        if not enemy then return end

        local range     = unit.attack_range
        -- Snaž se stát na 75 % max dosahu
        local optimal   = range * 0.75
        -- Pokud nepřítel je blíž než 30 % dosahu – ustupuj
        local too_close = range * 0.30

        if dist < too_close then
            -- Vektor ústupu
            local dx = unit.x - enemy.x
            local dy = unit.y - enemy.y
            local len = math.sqrt(dx*dx + dy*dy)
            if len > 0 then dx = dx/len; dy = dy/len end
            Engine.move_unit(unit.entity_id,
                unit.x + dx * optimal,
                unit.y + dy * optimal)
        elseif dist <= range + 8 then
            -- V dosahu – zaútoč
            Engine.attack_unit(unit.entity_id, enemy.entity_id)
        else
            -- Mimo dosah – přibliž se na optimální vzdálenost
            local dx = enemy.x - unit.x
            local dy = enemy.y - unit.y
            local len = math.sqrt(dx*dx + dy*dy)
            if len > 0 then dx = dx/len; dy = dy/len end
            Engine.move_unit(unit.entity_id,
                enemy.x - dx * optimal,
                enemy.y - dy * optimal)
        end
    end,
})
