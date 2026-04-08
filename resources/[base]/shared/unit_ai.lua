-- Definice AI pro jednotlivé typy jednotek.
-- Načítají se jako shared script (klient i server).

AiDefs = AiDefs or {}

-- ── Pomocné funkce ────────────────────────────────────────────────────────────

local function find_nearest_enemy(unit)
    local units   = Engine.query_units()
    local best    = nil
    local bestDist = math.huge
    for i = 1, #units do
        local u = units[i]
        if u.team ~= unit.team then
            local dx   = u.x - unit.x
            local dy   = u.y - unit.y
            local dist = dx*dx + dy*dy
            if dist < bestDist then
                bestDist = dist
                best     = u
            end
        end
    end
    return best, math.sqrt(bestDist)
end

-- ── Worker AI ─────────────────────────────────────────────────────────────────
-- Dělník nemá ofenzivní AI – sklizeň řídí Rust HarvestOrder.
-- Pokud je napaden, pokusí se utéct k nejbližší vlastní budově.

AiDefs["worker_ai"] = {
    on_tick = function(unit, dt)
        -- Nic – klid. Sklizeň a kontext-kliknutí řídí Rust.
    end
}

-- ── Melee AI ──────────────────────────────────────────────────────────────────
-- Útočná jednotka na blízko: najde nejbližšího nepřítele a zaútočí.
-- Dosah detekce: 12 dlaždic.

AiDefs["melee_ai"] = {
    on_tick = function(unit, dt)
        local target, dist = find_nearest_enemy(unit)
        if target and dist < 12 * TILE_SIZE then
            Engine.attack_unit(unit.entity_id, target.entity_id)
        end
    end
}

-- ── Ranged AI ─────────────────────────────────────────────────────────────────
-- Útočná jednotka na dálku: detekuje na 14 dlaždic, útočí z dosahu.

AiDefs["ranged_ai"] = {
    on_tick = function(unit, dt)
        local target, dist = find_nearest_enemy(unit)
        if target and dist < 14 * TILE_SIZE then
            Engine.attack_unit(unit.entity_id, target.entity_id)
        end
    end
}
