-- Strategická AI nepřítele (team 1) pro singleplayer.
-- Spouštěna přes on_game_tick každý snímek.
--
-- Strategie:
--   · každých 20 s vytrénuje jednotku z dostupné budovy nepřítele
--   · každých 50 s pošle útočnou vlnu na hráče

local TRAIN_CD  = 20.0   -- interval výroby (sekundy)
local ATTACK_CD = 50.0   -- interval útočné vlny (sekundy)

local state = {
    train_timer  = TRAIN_CD * 0.5,  -- první výroba po 10 s
    attack_timer = ATTACK_CD,
}

-- Budovy schopné výroby a co vyrábí
local PRODUCE_MAP = {
    great_hall   = "peon",
    stronghold   = "peon",
    fortress     = "peon",
    orc_barracks = "grunt",
    barracks     = "footman",
}

-- Druhy budov (nefightují)
local BUILDING_KINDS = {
    great_hall=true, stronghold=true, fortress=true,
    town_hall=true,  keep=true,       castle=true,
    orc_barracks=true, barracks=true,
    farm=true, pig_farm=true,
    lumbermill=true, blacksmith=true, tower=true,
    gold_mine=true,
}

local function is_building(kind)
    return BUILDING_KINDS[kind] == true
end

-- ── Výroba jednotek ───────────────────────────────────────────────────────────

local function ai_train()
    local units = Engine.query_units()
    for i = 1, #units do
        local u = units[i]
        if u.team == 1 then
            local what = PRODUCE_MAP[u.kind_id]
            if what then
                -- build_time 0 = použije výchozí hodnotu v Rustu (30 s)
                Engine.train_unit(u.entity_id, what, 0)
            end
        end
    end
end

-- ── Útočná vlna ───────────────────────────────────────────────────────────────

local function ai_attack()
    local units     = Engine.query_units()
    local fighters  = {}
    local targets   = {}

    for i = 1, #units do
        local u = units[i]
        if u.team == 1 and not is_building(u.kind_id) then
            table.insert(fighters, u)
        elseif u.team == 0 then
            table.insert(targets, u)
        end
    end

    if #fighters == 0 or #targets == 0 then return end

    -- Vyber nejsnadnější cíl (nejblíže středu mapy, nebo prostě první)
    local target = targets[1]
    -- Najdi nejbližší cíl ke středu nepřátelské základny
    local base_x, base_y = 50 * TILE_SIZE, 5 * TILE_SIZE
    local best_dist = math.huge
    for _, t in ipairs(targets) do
        local dx = t.x - base_x
        local dy = t.y - base_y
        local d  = dx*dx + dy*dy
        if d < best_dist then
            best_dist = d
            target    = t
        end
    end

    -- Pošli všechny bojovníky útočit
    for _, f in ipairs(fighters) do
        Engine.attack_unit(f.entity_id, target.entity_id)
    end

    Engine.log(string.format("[AI] Utocna vlna: %d bojovniku -> cil %s",
        #fighters, target.kind_id))
end

-- ── Hlavní game tick ──────────────────────────────────────────────────────────

on_game_tick = function(dt)
    state.train_timer  = state.train_timer  + dt
    state.attack_timer = state.attack_timer + dt

    if state.train_timer >= TRAIN_CD then
        state.train_timer = 0
        ai_train()
    end

    if state.attack_timer >= ATTACK_CD then
        state.attack_timer = 0
        ai_attack()
    end
end
