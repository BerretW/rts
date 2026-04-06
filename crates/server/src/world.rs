//! Server-side ECS komponenty (bez renderovacích dat).

use glam::Vec2;

pub struct Position(pub Vec2);
pub struct Velocity(pub Vec2);
pub struct Team(pub u8);

pub struct Health {
    pub current: i32,
    pub max:     i32,
}
impl Health {
    pub fn new(max: i32) -> Self { Self { current: max, max } }
    pub fn is_alive(&self) -> bool { self.current > 0 }
}

#[derive(Clone, Default)]
pub struct MoveFlags {
    pub can_swim:     bool,
    pub can_fly:      bool,
    pub speed_water:  f32,
    pub speed_forest: f32,
    pub speed_road:   f32,
}

pub struct MoveOrder {
    pub target: Vec2,
    pub speed:  f32,
    pub flags:  MoveFlags,
}

#[derive(Clone)]
pub struct AttackStats {
    pub damage:        i32,
    pub pierce:        i32,
    pub armor:         i32,
    pub range:         f32,
    pub cooldown:      f32,
    pub cooldown_left: f32,
}

pub struct AttackOrder {
    pub target: hecs::Entity,
}

pub struct ProductionQueue {
    pub current:  Option<(String, f32)>,
    pub capacity: usize,
    pub queue:    Vec<String>,
    pub rally:    Vec2,
}
impl ProductionQueue {
    pub fn new(cap: usize) -> Self {
        Self { current: None, capacity: cap, queue: Vec::new(), rally: Vec2::ZERO }
    }
    pub fn enqueue(&mut self, kind: String) -> bool {
        if self.queue.len() < self.capacity { self.queue.push(kind); true } else { false }
    }
}

pub struct AiController {
    pub script_id:     String,
    pub tick_timer:    f32,
    pub tick_interval: f32,
    pub state_json:    String,
}
impl AiController {
    pub fn new(script_id: impl Into<String>, interval: f32) -> Self {
        Self { script_id: script_id.into(), tick_timer: 0.0, tick_interval: interval, state_json: "{}".into() }
    }
}

pub struct UnitKindId(pub String);

/// Tilemap – zjednodušená verze bez renderování.
pub struct TileMap {
    pub width:  u32,
    pub height: u32,
    tiles:      Vec<TileKind>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Grass, Dirt, Water, DeepWater, Forest, Rock, Sand, Bridge,
}

pub const TILE_SIZE: f32 = 32.0;

impl TileKind {
    pub fn to_byte(self) -> u8 {
        match self {
            TileKind::Grass     => 0,
            TileKind::Dirt      => 1,
            TileKind::Water     => 2,
            TileKind::DeepWater => 3,
            TileKind::Forest    => 4,
            TileKind::Rock      => 5,
            TileKind::Sand      => 6,
            TileKind::Bridge    => 7,
        }
    }
}

impl TileMap {
    pub fn new_filled(w: u32, h: u32, kind: TileKind) -> Self {
        Self { width: w, height: h, tiles: vec![kind; (w*h) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> Option<TileKind> {
        if x < self.width && y < self.height {
            Some(self.tiles[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: u32, y: u32, kind: TileKind) {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize] = kind;
        }
    }
}
