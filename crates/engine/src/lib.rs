pub mod app;
pub mod camera;
pub mod input;
pub mod tilemap;
pub mod renderer;
pub mod ui;

// Re-export klíčových závislostí – game crate je používá přes engine::wgpu / engine::winit
// aby se zamezilo version mismatch.
pub use wgpu;
pub use winit;

pub use glam::{Vec2, Vec4};
pub use hecs;

/// Obdélník v herních souřadnicích (pixely, y dolů).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[inline]
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Vrátí true pokud bod leží uvnitř obdélníku.
    #[inline]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }

    /// Vrátí střed obdélníku.
    #[inline]
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

/// UV souřadnice výřezu v textuře (0.0–1.0).
#[derive(Clone, Copy, Debug)]
pub struct UvRect {
    pub u: f32,
    pub v: f32,
    pub uw: f32,
    pub vh: f32,
}

impl UvRect {
    pub const FULL: Self = Self { u: 0.0, v: 0.0, uw: 1.0, vh: 1.0 };

    pub fn new(u: f32, v: f32, uw: f32, vh: f32) -> Self {
        Self { u, v, uw, vh }
    }

    /// Vytvoří UvRect z tile indexu v spritesheet mřížce.
    ///
    /// * `col`, `row`  – index sloupce/řádku
    /// * `cols`, `rows` – celkový počet sloupců/řádků v sheetu
    pub fn from_tile(col: u32, row: u32, cols: u32, rows: u32) -> Self {
        let uw = 1.0 / cols as f32;
        let vh = 1.0 / rows as f32;
        Self {
            u: col as f32 * uw,
            v: row as f32 * vh,
            uw,
            vh,
        }
    }
}
