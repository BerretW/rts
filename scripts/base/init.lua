-- base/init.lua
-- Globální registry, utility a výchozí hooky.

-- ── Registry ────────────────────────────────────────────────────────────────
UnitDefs     = {}   -- kind_id → def table
BuildingDefs = {}   -- kind_id → def table
AiDefs       = {}   -- script_id → { on_tick = fn, ... }
AbilityDefs  = {}   -- ability_id → def table

-- ── Registrace ───────────────────────────────────────────────────────────────

function RegisterUnit(def)
    assert(def.kind,   "RegisterUnit: chybí 'kind'")
    assert(def.hp_max, "RegisterUnit: chybí 'hp_max'")
    def.speed        = def.speed        or 128.0
    def.can_swim     = def.can_swim     or false
    def.can_fly      = def.can_fly      or false
    def.speed_water  = def.speed_water  or 0.0
    def.speed_forest = def.speed_forest or 0.75
    def.speed_road   = def.speed_road   or 1.0
    def.damage       = def.damage       or 3
    def.pierce       = def.pierce       or 0
    def.armor        = def.armor        or 0
    def.attack_range = def.attack_range or 0.0   -- 0 = melee
    def.attack_cd    = def.attack_cd    or 1.0
    def.sight        = def.sight        or 4
    def.cost_gold    = def.cost_gold    or 0
    def.cost_lumber  = def.cost_lumber  or 0
    def.build_time   = def.build_time   or 30.0
    def.ai_script    = def.ai_script    or nil
    def.abilities    = def.abilities    or {}     -- seznam ability_id
    UnitDefs[def.kind] = def
end

function RegisterBuilding(def)
    assert(def.kind, "RegisterBuilding: chybí 'kind'")
    def.hp_max      = def.hp_max      or 500
    def.armor       = def.armor       or 0
    def.size_tiles  = def.size_tiles  or 2
    def.cost_gold   = def.cost_gold   or 0
    def.cost_lumber = def.cost_lumber or 0
    def.build_time  = def.build_time  or 60.0
    def.produces    = def.produces    or {}
    def.food        = def.food        or 0
    BuildingDefs[def.kind] = def
end

function RegisterAi(script_id, def)
    assert(def.on_tick, "RegisterAi: chybí 'on_tick'")
    AiDefs[script_id] = def
end

--- Zaregistruje aktivní schopnost.
--- def = { id, name, hotkey, target, cooldown, handler }
---   target: "none" | "unit" | "point"
function RegisterAbility(def)
    assert(def.id,   "RegisterAbility: chybí 'id'")
    assert(def.name, "RegisterAbility: chybí 'name'")
    def.hotkey   = def.hotkey   or ""
    def.target   = def.target   or "none"
    def.cooldown = def.cooldown or 0.0
    def.handler  = def.handler  or function() end
    AbilityDefs[def.id] = def
end

-- ── Pohybové parametry z definice ────────────────────────────────────────────

function MoveParamsForUnit(kind)
    local def = UnitDefs[kind]
    if not def then return nil end
    return {
        speed        = def.speed,
        can_swim     = def.can_swim,
        can_fly      = def.can_fly,
        speed_water  = def.speed_water,
        speed_forest = def.speed_forest,
        speed_road   = def.speed_road,
    }
end

-- ── Výchozí hooky ────────────────────────────────────────────────────────────

function on_move_order(unit, tx, ty, params)
    local p = MoveParamsForUnit(unit.kind)
    if p then
        for k, v in pairs(p) do params[k] = v end
    end
    return params
end

function on_unit_spawned(unit)
    local def = UnitDefs[unit.kind]
    if def and def.ai_script then
        Engine.set_ai(unit.entity_id, def.ai_script, 0.5)
    end
end

function on_unit_arrived(unit) end
function on_unit_died(unit)    end
function on_unit_attack(attacker, target, damage) end
function on_unit_hit(unit, damage, attacker_id)   end
function on_unit_trained(unit, building_id)       end
function on_game_tick(dt)                         end
function on_resource_changed(gold, lumber, oil)   end

--- Voláno serverem při použití schopnosti hráčem.
--- caster    = unit info table
--- ability_id = string
--- target_id  = number | nil
--- tx, ty    = cílová pozice (pro "point" schopnosti)
function on_ability_used(caster, ability_id, target_id, tx, ty)
    local def = AbilityDefs[ability_id]
    if def and def.handler then
        def.handler(caster, target_id, tx, ty)
    end
end

-- ── Utility ──────────────────────────────────────────────────────────────────

--- Vzdálenost dvou pozic
function Dist(ax, ay, bx, by)
    local dx = ax - bx; local dy = ay - by
    return math.sqrt(dx*dx + dy*dy)
end

--- Vrátí nejbližšího nepřítele z __query_result (nebo nil)
function NearestEnemy(unit)
    local best, best_dist = nil, math.huge
    local result = Engine.query_units()
    for i = 1, #result do
        local u = result[i]
        if u.team ~= unit.team and u.hp > 0 then
            local d = Dist(unit.x, unit.y, u.x, u.y)
            if d < best_dist then best_dist = d; best = u end
        end
    end
    return best, best_dist
end

--- Vrátí přátelský town_hall (pro worker rally point)
function FriendlyBase(unit)
    local result = Engine.query_units()
    for i = 1, #result do
        local u = result[i]
        if u.team == unit.team and (u.kind == "town_hall" or u.kind == "great_hall") then
            return u
        end
    end
    return nil
end
