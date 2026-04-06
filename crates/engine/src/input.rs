use std::collections::HashSet;
use glam::Vec2;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::KeyCode;

/// Stav vstupů pro jeden snímek.
///
/// Volat `Input::end_frame()` na konci každého framu aby se vyčistily "just_*" sady.
#[derive(Default)]
pub struct Input {
    held:         HashSet<KeyCode>,
    just_pressed: HashSet<KeyCode>,
    just_released: HashSet<KeyCode>,

    mouse_held:         HashSet<MouseButton>,
    mouse_just_pressed: HashSet<MouseButton>,
    mouse_just_released: HashSet<MouseButton>,

    /// Aktuální pozice myši v obrazovkových pixelech.
    pub mouse_pos: Vec2,
    /// Delta pohybu myši v tomto framu.
    pub mouse_delta: Vec2,
    /// Pohyb kolečka myši (kladné = nahoru).
    pub scroll_delta: f32,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Klávesnice ──────────────────────────────────────────────────────

    pub fn on_key(&mut self, code: KeyCode, state: ElementState) {
        match state {
            ElementState::Pressed => {
                if !self.held.contains(&code) {
                    self.just_pressed.insert(code);
                }
                self.held.insert(code);
            }
            ElementState::Released => {
                self.held.remove(&code);
                self.just_released.insert(code);
            }
        }
    }

    /// True pokud je klávesa právě stisknuta (pouze první snímek).
    pub fn key_just_pressed(&self, code: KeyCode) -> bool {
        self.just_pressed.contains(&code)
    }

    /// True pokud je klávesa držena.
    pub fn key_held(&self, code: KeyCode) -> bool {
        self.held.contains(&code)
    }

    /// True pokud byla klávesa právě uvolněna.
    pub fn key_just_released(&self, code: KeyCode) -> bool {
        self.just_released.contains(&code)
    }

    // ── Myš ─────────────────────────────────────────────────────────────

    pub fn on_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        match state {
            ElementState::Pressed => {
                if !self.mouse_held.contains(&button) {
                    self.mouse_just_pressed.insert(button);
                }
                self.mouse_held.insert(button);
            }
            ElementState::Released => {
                self.mouse_held.remove(&button);
                self.mouse_just_released.insert(button);
            }
        }
    }

    pub fn on_mouse_moved(&mut self, x: f32, y: f32) {
        let new_pos = Vec2::new(x, y);
        self.mouse_delta = new_pos - self.mouse_pos;
        self.mouse_pos = new_pos;
    }

    pub fn on_scroll(&mut self, delta: f32) {
        self.scroll_delta += delta;
    }

    pub fn mouse_just_pressed(&self, button: MouseButton) -> bool {
        self.mouse_just_pressed.contains(&button)
    }

    pub fn mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_held.contains(&button)
    }

    pub fn mouse_just_released(&self, button: MouseButton) -> bool {
        self.mouse_just_released.contains(&button)
    }

    // ── Správa snímků ────────────────────────────────────────────────────

    /// Vyčistit "just_*" sady a resetnout delty – volat na konci každého snímku.
    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.mouse_just_pressed.clear();
        self.mouse_just_released.clear();
        self.mouse_delta = Vec2::ZERO;
        self.scroll_delta = 0.0;
    }
}
