use glam::Vec2;

// ── Základní komponenty ECS ──────────────────────────────────────────────────

pub struct Position(pub Vec2);
pub struct Velocity(pub Vec2);

pub struct Sprite {
    pub col:   u32,
    pub row:   u32,
    pub size:  Vec2,
    pub color: [f32; 4],
}

impl Sprite {
    pub fn new(col: u32, row: u32, size: f32) -> Self {
        Self { col, row, size: Vec2::splat(size), color: [1.0; 4] }
    }
}

/// Tým: 0 = hráč, 1–7 = AI.
pub struct Team(pub u8);

pub struct Health {
    pub current: i32,
    pub max:     i32,
}

impl Health {
    pub fn new(max: i32) -> Self { Self { current: max, max } }
    pub fn fraction(&self) -> f32 { self.current as f32 / self.max as f32 }
    pub fn is_alive(&self) -> bool { self.current > 0 }
}

pub struct Selected;

// ── Pohyb ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct MoveFlags {
    pub can_swim:     bool,
    pub can_fly:      bool,
    pub speed_water:  f32,
    pub speed_forest: f32,
    pub speed_road:   f32,
}

impl Default for MoveFlags {
    fn default() -> Self {
        Self {
            can_swim:     false,
            can_fly:      false,
            speed_water:  0.0,
            speed_forest: 0.75,
            speed_road:   1.0,
        }
    }
}

pub struct MoveOrder {
    pub target: Vec2,
    pub speed:  f32,
    pub flags:  MoveFlags,
}

// ── Boj ──────────────────────────────────────────────────────────────────────

/// Bojové statistiky – nastavují se při spawnu z Lua definice.
#[derive(Clone, Debug)]
pub struct AttackStats {
    /// Základní poškození (snižuje armor).
    pub damage:        i32,
    /// Piercing poškození (ignoruje armor).
    pub pierce:        i32,
    /// Dosah útoku v pixelech (0 = melee – 1.5×tile).
    pub range:         f32,
    /// Minimální cooldown mezi útoky (sekundy).
    pub cooldown:      f32,
    /// Zbývající čas do dalšího útoku.
    pub cooldown_left: f32,
    /// Brnění (absorbuje damage, ne pierce).
    pub armor:         i32,
}

impl AttackStats {
    pub fn melee(damage: i32, armor: i32, cooldown: f32) -> Self {
        Self { damage, pierce: 0, range: 0.0, cooldown, cooldown_left: 0.0, armor }
    }
    pub fn ranged(damage: i32, pierce: i32, range: f32, armor: i32, cooldown: f32) -> Self {
        Self { damage, pierce, range, cooldown, cooldown_left: 0.0, armor }
    }
    /// Spočítá reálné HP poškození při útoku.
    pub fn calc_damage(&self, target_armor: i32) -> i32 {
        let basic = (self.damage - target_armor).max(1);
        basic + self.pierce
    }
}

/// Rozkaz útoku – útočník se přesune do dosahu a opakovaně útočí.
pub struct AttackOrder {
    pub target: hecs::Entity,
}

// ── Výroba (budovy) ───────────────────────────────────────────────────────────

/// Fronta výroby budovy.
/// Každý slot = (kind_id, zbývající čas výroby).
pub struct ProductionQueue {
    /// Aktuálně vyráběné: (kind_id, zbývající sekundy).
    pub current:  Option<(String, f32)>,
    /// Max. délka fronty.
    pub capacity: usize,
    /// Čekající položky.
    pub queue:    Vec<String>,
    /// Odkud se vyrobená jednotka spawní (offset od středu budovy).
    pub rally:    Vec2,
}

impl ProductionQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            current:  None,
            capacity,
            queue:    Vec::new(),
            rally:    Vec2::ZERO,
        }
    }

    /// Přidá výrobu do fronty. Vrátí false pokud je plná.
    pub fn enqueue(&mut self, kind_id: String) -> bool {
        if self.queue.len() < self.capacity {
            self.queue.push(kind_id);
            true
        } else {
            false
        }
    }
}

// ── AI ────────────────────────────────────────────────────────────────────────

/// AI stav entity – řízen Lua skriptem.
pub struct AiController {
    /// Název AI skriptu (klíč do `AiDefs` tabulky v Lua).
    pub script_id:     String,
    /// Čas do dalšího AI ticku (sekundy).
    pub tick_timer:    f32,
    /// Interval AI ticků (sekundy).
    pub tick_interval: f32,
    /// Libovolná Lua data pro tento AI (serializovaná jako JSON string).
    pub state_json:    String,
}

impl AiController {
    pub fn new(script_id: impl Into<String>, tick_interval: f32) -> Self {
        Self {
            script_id:     script_id.into(),
            tick_timer:    0.0,
            tick_interval,
            state_json:    "{}".into(),
        }
    }
}

// ── Identita jednotky ─────────────────────────────────────────────────────────

/// Druh entity – string klíč do Lua UnitDefs (např. "peasant", "town_hall").
pub struct UnitKindId(pub String);

/// Starý enum – zachován pro editor paletu.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Peon,
    Grunt,
    Archer,
    Catapult,
    TownHall,
    Barracks,
}

pub struct Unit(pub UnitKind);

// ── Sight / fog of war ────────────────────────────────────────────────────────

pub struct Sight(pub u32);
