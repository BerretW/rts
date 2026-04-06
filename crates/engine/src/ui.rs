/// Jednoduchý immediate-mode UI systém.
///
/// Používá druhý `SpriteBatch` s screen-space ortho kamerou –
/// souřadnice jsou vždy v pixelech obrazovky, (0,0) vlevo nahoře.
///
/// # Příklad
/// ```ignore
/// fn render_ui(&mut self, ui: &mut UiCtx) {
///     ui.panel(Rect::new(10., 10., 200., 40.), [0.1, 0.1, 0.1, 0.8]);
///     if ui.button(Rect::new(10., 60., 200., 40.), "Start") {
///         self.start_game();
///     }
///     ui.progress_bar(Rect::new(10., 120., 300., 20.), 0.6, [0.2,0.2,0.2,1.], [0.,0.8,0.,1.]);
/// }
/// ```

use glam::Vec2;
use crate::{Rect, UvRect};
use crate::input::Input;
use crate::renderer::SpriteBatch;

// ── Paleta barev UI ──────────────────────────────────────────────────────────

pub mod colors {
    pub const BG_DARK:    [f32; 4] = [0.08, 0.08, 0.10, 0.92];
    pub const BG_MID:     [f32; 4] = [0.15, 0.15, 0.18, 0.95];
    pub const BG_LIGHT:   [f32; 4] = [0.22, 0.22, 0.26, 1.00];
    pub const BORDER:     [f32; 4] = [0.45, 0.45, 0.50, 1.00];
    pub const BTN_NORMAL: [f32; 4] = [0.20, 0.35, 0.55, 1.00];
    pub const BTN_HOVER:  [f32; 4] = [0.28, 0.48, 0.72, 1.00];
    pub const BTN_PRESS:  [f32; 4] = [0.12, 0.22, 0.38, 1.00];
    pub const BTN_DANGER: [f32; 4] = [0.55, 0.15, 0.15, 1.00];
    pub const HEALTH_HI:  [f32; 4] = [0.15, 0.80, 0.15, 1.00];
    pub const HEALTH_MID: [f32; 4] = [0.85, 0.75, 0.10, 1.00];
    pub const HEALTH_LO:  [f32; 4] = [0.85, 0.15, 0.10, 1.00];
    pub const GOLD:       [f32; 4] = [1.00, 0.85, 0.10, 1.00];
    pub const LUMBER:     [f32; 4] = [0.30, 0.75, 0.20, 1.00];
    pub const WHITE:      [f32; 4] = [1.00, 1.00, 1.00, 1.00];
    pub const BLACK:      [f32; 4] = [0.00, 0.00, 0.00, 1.00];
    pub const TRANSPARENT:[f32; 4] = [0.00, 0.00, 0.00, 0.00];
}

// ── UiCtx ────────────────────────────────────────────────────────────────────

/// Kontext pro kreslení UI v jednom snímku.
pub struct UiCtx<'a> {
    pub batch:  &'a mut SpriteBatch,
    pub input:  &'a Input,
    pub screen: Vec2,
}

impl<'a> UiCtx<'a> {
    pub fn new(batch: &'a mut SpriteBatch, input: &'a Input, screen: Vec2) -> Self {
        Self { batch, input, screen }
    }

    // ── Primitivy ────────────────────────────────────────────────────────

    /// Vyplněný obdélník.
    pub fn panel(&mut self, rect: Rect, color: [f32; 4]) {
        self.batch.draw(rect, UvRect::FULL, color);
    }

    /// Rámeček (4 strany).
    pub fn border(&mut self, rect: Rect, thickness: f32, color: [f32; 4]) {
        let t = thickness;
        let uv = UvRect::FULL;
        self.batch.draw(Rect::new(rect.x,               rect.y,               rect.w, t),     uv, color);
        self.batch.draw(Rect::new(rect.x,               rect.y + rect.h - t,  rect.w, t),     uv, color);
        self.batch.draw(Rect::new(rect.x,               rect.y,               t, rect.h),     uv, color);
        self.batch.draw(Rect::new(rect.x + rect.w - t,  rect.y,               t, rect.h),     uv, color);
    }

    /// Vyplněný panel s okrajem.
    pub fn panel_bordered(&mut self, rect: Rect, bg: [f32; 4], border: [f32; 4]) {
        self.panel(rect, bg);
        self.border(rect, 1.0, border);
    }

    // ── Tlačítko ─────────────────────────────────────────────────────────

    /// Kreslí tlačítko a vrátí `true` pokud na něj bylo kliknuto (LMB release uvnitř).
    ///
    /// Stav hover/press je určen polohou myši a stavem tlačítka.
    pub fn button(&mut self, rect: Rect, color: [f32; 4]) -> bool {
        let hover   = rect.contains(self.input.mouse_pos);
        let pressed = hover && self.input.mouse_held(engine_mouse_left());
        let clicked = hover && self.input.mouse_just_released(engine_mouse_left());

        let bg = if pressed {
            darken(color, 0.65)
        } else if hover {
            lighten(color, 1.25)
        } else {
            color
        };

        self.panel_bordered(rect, bg, colors::BORDER);
        clicked
    }

    /// Tlačítko s přednastaveným stylem.
    pub fn btn_primary(&mut self, rect: Rect) -> bool {
        self.button(rect, colors::BTN_NORMAL)
    }

    pub fn btn_danger(&mut self, rect: Rect) -> bool {
        self.button(rect, colors::BTN_DANGER)
    }

    // ── Progress / health bar ────────────────────────────────────────────

    /// Horizontální progress bar. `fill` ∈ [0.0, 1.0].
    pub fn progress_bar(&mut self, rect: Rect, fill: f32, bg: [f32; 4], fg: [f32; 4]) {
        self.panel(rect, bg);
        if fill > 0.0 {
            let fw = (rect.w * fill.clamp(0.0, 1.0)).max(0.0);
            self.panel(Rect::new(rect.x, rect.y, fw, rect.h), fg);
        }
        self.border(rect, 1.0, colors::BORDER);
    }

    /// Health bar – barva se mění dle procenta.
    pub fn health_bar(&mut self, rect: Rect, frac: f32) {
        let fg = health_color(frac);
        self.progress_bar(rect, frac, [0.1, 0.1, 0.1, 0.9], fg);
    }

    /// Health bar nad jednotkou ve světových souřadnicích (přemapován na screen).
    ///
    /// `world_pos` = střed entity, `camera` = aktuální kamera.
    pub fn health_bar_world(
        &mut self,
        world_pos:   glam::Vec2,
        entity_h:    f32,   // výška entity v herních pixelech
        frac:        f32,
        camera:      &crate::camera::Camera,
    ) {
        let w  = 28.0;
        let h  =  4.0;
        let sp = camera.world_to_screen(world_pos - glam::Vec2::new(0.0, entity_h * 0.5 + 6.0));
        let rect = Rect::new(sp.x - w * 0.5, sp.y - h * 0.5, w, h);
        // pouze pokud je na obrazovce
        if rect.x + rect.w > 0.0 && rect.x < self.screen.x
        && rect.y + rect.h > 0.0 && rect.y < self.screen.y {
            self.health_bar(rect, frac);
        }
    }

    // ── Zdroje (resource bar) ────────────────────────────────────────────

    /// Panel se zdroji nahoře obrazovky (gold + lumber).
    pub fn resource_bar(&mut self, gold: u32, lumber: u32, oil: u32) {
        let w = self.screen.x;
        self.panel(Rect::new(0.0, 0.0, w, 28.0), colors::BG_DARK);
        self.border(Rect::new(0.0, 0.0, w, 28.0), 1.0, colors::BORDER);

        // Barevné ikonky (malé obdélníky) + „počítadla" jako tlustší pásky
        // Gold
        self.panel(Rect::new(8.0, 6.0, 16.0, 16.0), colors::GOLD);
        self.panel(Rect::new(28.0, 8.0, (gold.min(9999) as f32 * 0.02).clamp(4.0, 120.0), 12.0),
                   colors::GOLD);

        // Lumber
        self.panel(Rect::new(170.0, 6.0, 16.0, 16.0), colors::LUMBER);
        self.panel(Rect::new(190.0, 8.0, (lumber.min(9999) as f32 * 0.02).clamp(4.0, 120.0), 12.0),
                   colors::LUMBER);

        // Oil (pokud > 0)
        if oil > 0 {
            self.panel(Rect::new(330.0, 6.0, 16.0, 16.0), [0.3, 0.3, 0.35, 1.0]);
            self.panel(Rect::new(350.0, 8.0, (oil.min(9999) as f32 * 0.02).clamp(4.0, 80.0), 12.0),
                       [0.5, 0.5, 0.55, 1.0]);
        }
    }

    // ── Minimap ──────────────────────────────────────────────────────────

    /// Placeholder minimapy v pravém dolním rohu.
    pub fn minimap_placeholder(&mut self, map_w: u32, map_h: u32) {
        let size = 180.0_f32;
        let x = self.screen.x - size - 8.0;
        let y = self.screen.y - size - 8.0;
        self.panel(Rect::new(x, y, size, size), [0.05, 0.08, 0.05, 0.95]);
        self.border(Rect::new(x, y, size, size), 2.0, colors::BORDER);
        // Rasterizovaná mřížka (velmi hrubá reprezentace)
        let tile_px_x = size / map_w as f32;
        let tile_px_y = size / map_h as f32;
        let _ = (tile_px_x, tile_px_y); // bude použito v skutečné implementaci
    }

    // ── Info panel (spodek obrazovky) ────────────────────────────────────

    /// Panel informací o vybrané jednotce/budově.
    pub fn info_panel(&mut self, label_color: [f32; 4], hp_frac: f32, hp_max: i32) {
        let w = self.screen.x - 200.0; // minus minimap
        let h = 96.0;
        let y = self.screen.y - h;
        self.panel(Rect::new(0.0, y, w, h), colors::BG_DARK);
        self.border(Rect::new(0.0, y, w, h), 1.0, colors::BORDER);

        // Ikonka entity (barevný čtverec)
        self.panel(Rect::new(8.0, y + 8.0, 64.0, 64.0), label_color);
        self.border(Rect::new(8.0, y + 8.0, 64.0, 64.0), 1.0, colors::BORDER);

        // Health bar
        self.health_bar(Rect::new(82.0, y + 12.0, 200.0, 14.0), hp_frac);

        // HP čísla jako malé tečky (bez textu)
        let dot_w = (hp_max as f32 / 10.0).clamp(1.0, 8.0);
        for i in 0..(hp_max.min(20)) {
            let filled = (i as f32 / hp_max as f32) < hp_frac;
            let col = if filled { colors::HEALTH_HI } else { [0.2, 0.2, 0.2, 1.0] };
            self.panel(Rect::new(82.0 + i as f32 * (dot_w + 2.0), y + 32.0, dot_w, 8.0), col);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn engine_mouse_left() -> winit::event::MouseButton {
    winit::event::MouseButton::Left
}

fn health_color(frac: f32) -> [f32; 4] {
    if frac > 0.5 { colors::HEALTH_HI }
    else if frac > 0.25 { colors::HEALTH_MID }
    else { colors::HEALTH_LO }
}

fn lighten(c: [f32; 4], factor: f32) -> [f32; 4] {
    [c[0] * factor, c[1] * factor, c[2] * factor, c[3]]
}

fn darken(c: [f32; 4], factor: f32) -> [f32; 4] {
    lighten(c, factor)
}
