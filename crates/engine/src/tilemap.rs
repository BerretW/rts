use glam::Vec2;
use crate::{Rect, UvRect};

pub const TILE_SIZE: f32 = 32.0; // pixely na herní dlaždici

/// Druh dlaždice – rozšiřuj dle potřeby.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TileKind {
    Grass      = 0,
    Dirt       = 1,
    Water      = 2,
    DeepWater  = 3,
    Forest     = 4,
    Rock       = 5,
    Sand       = 6,
    Bridge     = 7,
}

impl TileKind {
    /// Mapování druhu dlaždice na pozici v terrain spritesheet (sloupec, řádek).
    ///
    /// Sheet má 8 sloupců × 8 řádků (64 dlaždic 32×32 px).
    pub fn sheet_pos(self) -> (u32, u32) {
        match self {
            TileKind::Grass     => (0, 0),
            TileKind::Dirt      => (1, 0),
            TileKind::Water     => (2, 0),
            TileKind::DeepWater => (3, 0),
            TileKind::Forest    => (4, 0),
            TileKind::Rock      => (5, 0),
            TileKind::Sand      => (6, 0),
            TileKind::Bridge    => (7, 0),
        }
    }

    /// True pokud je dlaždice průchodná pro pozemní jednotky.
    pub fn is_passable(self) -> bool {
        !matches!(self, TileKind::Water | TileKind::DeepWater | TileKind::Rock)
    }
}

/// Jedna dlaždice v mapě.
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub kind: TileKind,
    /// True pokud je dlaždice viditelná (fog of war zrušen).
    pub visible: bool,
    /// True pokud ji hráč již prozkoumal (pamatuje se i když není visible).
    pub explored: bool,
}

impl Tile {
    pub fn new(kind: TileKind) -> Self {
        Self { kind, visible: false, explored: false }
    }
}

/// 2D mapa dlaždic.
pub struct TileMap {
    pub width:  u32,
    pub height: u32,
    tiles: Vec<Tile>,
}

impl TileMap {
    /// Vytvoří mapu vyplněnou daným druhem dlaždice.
    pub fn new_filled(width: u32, height: u32, kind: TileKind) -> Self {
        let tiles = vec![Tile::new(kind); (width * height) as usize];
        Self { width, height, tiles }
    }

    pub fn get(&self, x: u32, y: u32) -> Option<&Tile> {
        if x < self.width && y < self.height {
            Some(&self.tiles[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut Tile> {
        if x < self.width && y < self.height {
            Some(&mut self.tiles[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: u32, y: u32, kind: TileKind) {
        if let Some(t) = self.get_mut(x, y) {
            t.kind = kind;
        }
    }

    /// Převede světové souřadnice na tile index. Vrátí None pokud mimo mapu.
    pub fn world_to_tile(&self, world: Vec2) -> Option<(u32, u32)> {
        if world.x < 0.0 || world.y < 0.0 {
            return None;
        }
        let tx = (world.x / TILE_SIZE) as u32;
        let ty = (world.y / TILE_SIZE) as u32;
        if tx < self.width && ty < self.height {
            Some((tx, ty))
        } else {
            None
        }
    }

    /// Vrátí levý horní roh dlaždice ve světových souřadnicích.
    pub fn tile_to_world(&self, tx: u32, ty: u32) -> Vec2 {
        Vec2::new(tx as f32 * TILE_SIZE, ty as f32 * TILE_SIZE)
    }

    /// Vrátí Rect dlaždice ve světových souřadnicích.
    pub fn tile_rect(&self, tx: u32, ty: u32) -> Rect {
        let pos = self.tile_to_world(tx, ty);
        Rect::new(pos.x, pos.y, TILE_SIZE, TILE_SIZE)
    }

    /// Vrátí iterátor přes indexy dlaždic viditelných kamerou (AABB culling).
    pub fn visible_tiles(&self, view_rect: Rect) -> impl Iterator<Item = (u32, u32)> + '_ {
        let x0 = ((view_rect.x / TILE_SIZE).floor() as i32).max(0) as u32;
        let y0 = ((view_rect.y / TILE_SIZE).floor() as i32).max(0) as u32;
        let x1 = (((view_rect.x + view_rect.w) / TILE_SIZE).ceil() as u32).min(self.width);
        let y1 = (((view_rect.y + view_rect.h) / TILE_SIZE).ceil() as u32).min(self.height);

        (y0..y1).flat_map(move |ty| (x0..x1).map(move |tx| (tx, ty)))
    }

    /// Celkový Rect mapy ve světových souřadnicích.
    pub fn world_bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32 * TILE_SIZE, self.height as f32 * TILE_SIZE)
    }

    /// Odhalí kružnici kolem bodu (jednotka / budova – fog of war).
    pub fn reveal_circle(&mut self, center: Vec2, radius_tiles: u32) {
        let (cx, cy) = match self.world_to_tile(center) {
            Some(v) => v,
            None => return,
        };
        let r = radius_tiles as i32;
        let r2 = r * r;

        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r2 {
                    let tx = cx as i32 + dx;
                    let ty = cy as i32 + dy;
                    if tx >= 0 && ty >= 0 {
                        if let Some(tile) = self.get_mut(tx as u32, ty as u32) {
                            tile.visible  = true;
                            tile.explored = true;
                        }
                    }
                }
            }
        }
    }
}

/// Pomocník: vrátí UvRect pro danou dlaždici v terrain sheetu
/// (SHEET_COLS × SHEET_ROWS dlaždic).
pub fn tile_uv(kind: TileKind, sheet_cols: u32, sheet_rows: u32) -> UvRect {
    let (col, row) = kind.sheet_pos();
    UvRect::from_tile(col, row, sheet_cols, sheet_rows)
}
