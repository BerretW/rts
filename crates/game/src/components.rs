use glam::Vec2;

// ── Základní komponenty ECS ──────────────────────────────────────────────────

/// Pozice ve světě (střed entity).
pub struct Position(pub Vec2);

/// Rychlost v pixelech/s.
pub struct Velocity(pub Vec2);

/// Vizuální reprezentace – výřez v spritesheet.
pub struct Sprite {
    /// Sloupec a řádek v terrain/units sheetu (8×8 grid = 32px tiles).
    pub col: u32,
    pub row: u32,
    /// Velikost v herních pixelech.
    pub size: Vec2,
    /// Barevný tint [R,G,B,A].
    pub color: [f32; 4],
}

impl Sprite {
    pub fn new(col: u32, row: u32, size: f32) -> Self {
        Self { col, row, size: Vec2::splat(size), color: [1.0; 4] }
    }
}

/// Tým: 0 = hráč, 1–7 = AI.
pub struct Team(pub u8);

/// Zdraví jednotky.
pub struct Health {
    pub current: i32,
    pub max:     i32,
}

impl Health {
    pub fn new(max: i32) -> Self { Self { current: max, max } }
    pub fn fraction(&self) -> f32 { self.current as f32 / self.max as f32 }
    pub fn is_alive(&self) -> bool { self.current > 0 }
}

/// Označení výběrem hráče.
pub struct Selected;

/// Parametry pohybu – jak jednotka zvládá terén.
/// Nastavují je Lua skripty; Rust systém je čte při výpočtu efektivní rychlosti.
#[derive(Clone, Debug)]
pub struct MoveFlags {
    /// Může pohybovat po vodě (plavba / lodě).
    pub can_swim:     bool,
    /// Létající jednotka – ignoruje terén úplně.
    pub can_fly:      bool,
    /// Násobitel rychlosti na vodní dlaždicích (0.0 = neprojde).
    pub speed_water:  f32,
    /// Násobitel rychlosti v lese.
    pub speed_forest: f32,
    /// Násobitel rychlosti na cestě / mostu / písku.
    pub speed_road:   f32,
}

impl Default for MoveFlags {
    fn default() -> Self {
        Self {
            can_swim:     false,
            can_fly:      false,
            speed_water:  0.0,   // pěchota neprojde přes vodu bez mostu
            speed_forest: 0.75,  // les zpomaluje
            speed_road:   1.0,
        }
    }
}

/// Pohybový rozkaz – cílová pozice + parametry pohybu.
pub struct MoveOrder {
    pub target: Vec2,
    pub speed:  f32,   // px/s (základní – terén může násobit)
    pub flags:  MoveFlags,
}

/// Druh jednotky (pro AI a statistiky).
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
