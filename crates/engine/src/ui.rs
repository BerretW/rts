/// Jednoduchý immediate-mode UI systém.
///
/// Používá dva `SpriteBatch` s screen-space ortho kamerou –
/// souřadnice jsou vždy v pixelech obrazovky, (0,0) vlevo nahoře.
///
/// * `batch`      – bílá textura → solid-color panely, buttony, bary
/// * `text_batch` – font atlas textura → text přes `label()`
///
/// # Příklad
/// ```ignore
/// fn render_ui(&mut self, ui: &mut UiCtx) {
///     ui.panel(Rect::new(10., 10., 200., 40.), [0.1, 0.1, 0.1, 0.8]);
///     if ui.button(Rect::new(10., 60., 200., 40.), "Start") { ... }
///     ui.label(10., 110., "HP: 100", 1.0, colors::WHITE);
/// }
/// ```

use glam::Vec2;
use crate::{Rect, UvRect};
use crate::input::Input;
use crate::renderer::SpriteBatch;
use crate::font;

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
    pub const GREY:       [f32; 4] = [0.65, 0.65, 0.70, 1.00];
    pub const BLACK:      [f32; 4] = [0.00, 0.00, 0.00, 1.00];
    pub const TRANSPARENT:[f32; 4] = [0.00, 0.00, 0.00, 0.00];
}

// ── UiCtx ────────────────────────────────────────────────────────────────────

/// Kontext pro kreslení UI v jednom snímku.
pub struct UiCtx<'a> {
    /// Batch pro solid-color UI prvky (textura = bílý pixel).
    pub batch:      &'a mut SpriteBatch,
    /// Batch pro text (textura = font atlas).
    pub text_batch: &'a mut SpriteBatch,
    pub input:      &'a Input,
    pub screen:     Vec2,
}

impl<'a> UiCtx<'a> {
    pub fn new(
        batch:      &'a mut SpriteBatch,
        text_batch: &'a mut SpriteBatch,
        input:      &'a Input,
        screen:     Vec2,
    ) -> Self {
        Self { batch, text_batch, input, screen }
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

    // ── Text ─────────────────────────────────────────────────────────────

    /// Vykreslí text na pozici `(x, y)`.
    ///
    /// * `scale` – 1.0 = 8px vysoký font, 2.0 = 16px, atd.
    /// * `color` – RGBA tint (bílá = původní barva atlasu)
    ///
    /// Vrátí šířku vykresleného textu v pixelech.
    pub fn label(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 4]) -> f32 {
        let gw = font::GLYPH_W as f32 * scale;
        let gh = font::GLYPH_H as f32 * scale;
        let mut cx = x;
        for c in text.chars() {
            if c == ' ' {
                cx += gw;
                continue;
            }
            let uv  = font::glyph_uv(c);
            let dst = Rect::new(cx, y, gw, gh);
            self.text_batch.draw(dst, uv, color);
            cx += gw;
        }
        cx - x
    }

    /// Text vycentrovaný horizontálně uvnitř `rect`.
    pub fn label_centered(&mut self, rect: Rect, text: &str, scale: f32, color: [f32; 4]) {
        let gw  = font::GLYPH_W as f32 * scale;
        let tw  = text.chars().count() as f32 * gw;
        let x   = rect.x + (rect.w - tw) * 0.5;
        let y   = rect.y + (rect.h - font::GLYPH_H as f32 * scale) * 0.5;
        self.label(x, y, text, scale, color);
    }

    /// Stínovaný text (nejprve tmavá kopie o 1px posunutá).
    pub fn label_shadowed(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 4]) {
        self.label(x + scale, y + scale, text, scale, [0.0, 0.0, 0.0, color[3] * 0.6]);
        self.label(x, y, text, scale, color);
    }

    // ── Tlačítko ─────────────────────────────────────────────────────────

    /// Kreslí tlačítko a vrátí `true` pokud na něj bylo kliknuto.
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

    /// Tlačítko s textem uprostřed.
    pub fn button_text(&mut self, rect: Rect, label: &str, scale: f32) -> bool {
        let clicked = self.button(rect, colors::BTN_NORMAL);
        self.label_centered(rect, label, scale, colors::WHITE);
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

    /// Health bar nad jednotkou ve světových souřadnicích.
    pub fn health_bar_world(
        &mut self,
        world_pos:   glam::Vec2,
        entity_h:    f32,
        frac:        f32,
        camera:      &crate::camera::Camera,
    ) {
        let w  = 28.0;
        let h  =  4.0;
        let sp = camera.world_to_screen(world_pos - glam::Vec2::new(0.0, entity_h * 0.5 + 6.0));
        let rect = Rect::new(sp.x - w * 0.5, sp.y - h * 0.5, w, h);
        if rect.x + rect.w > 0.0 && rect.x < self.screen.x
        && rect.y + rect.h > 0.0 && rect.y < self.screen.y {
            self.health_bar(rect, frac);
        }
    }

    // ── Zdroje (resource bar) ────────────────────────────────────────────

    /// Panel se zdroji nahoře obrazovky.
    pub fn resource_bar(&mut self, gold: u32, lumber: u32, oil: u32) {
        let w = self.screen.x;
        self.panel(Rect::new(0.0, 0.0, w, 28.0), colors::BG_DARK);
        self.border(Rect::new(0.0, 0.0, w, 28.0), 1.0, colors::BORDER);

        // Gold ikonka + hodnota
        self.panel(Rect::new(8.0, 6.0, 16.0, 16.0), colors::GOLD);
        self.label_shadowed(30.0, 8.0, &format!("{}", gold),   1.0, colors::GOLD);

        // Lumber ikonka + hodnota
        self.panel(Rect::new(120.0, 6.0, 16.0, 16.0), colors::LUMBER);
        self.label_shadowed(142.0, 8.0, &format!("{}", lumber), 1.0, colors::LUMBER);

        // Oil (pokud > 0)
        if oil > 0 {
            self.panel(Rect::new(240.0, 6.0, 16.0, 16.0), [0.3, 0.3, 0.35, 1.0]);
            self.label_shadowed(262.0, 8.0, &format!("{}", oil), 1.0, [0.7, 0.7, 0.75, 1.0]);
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
        self.label(x + 4.0, y + 4.0, &format!("{}x{}", map_w, map_h), 1.0, colors::GREY);
    }

    // ── Info panel (spodek obrazovky) ────────────────────────────────────

    /// Panel informací o vybrané jednotce/budově.
    pub fn info_panel(&mut self, label_color: [f32; 4], hp_frac: f32, hp_max: i32) {
        let w = self.screen.x - 200.0;
        let h = 96.0;
        let y = self.screen.y - h;
        self.panel(Rect::new(0.0, y, w, h), colors::BG_DARK);
        self.border(Rect::new(0.0, y, w, h), 1.0, colors::BORDER);

        // Ikonka entity
        self.panel(Rect::new(8.0, y + 8.0, 64.0, 64.0), label_color);
        self.border(Rect::new(8.0, y + 8.0, 64.0, 64.0), 1.0, colors::BORDER);

        // Health bar + text
        self.health_bar(Rect::new(82.0, y + 12.0, 200.0, 14.0), hp_frac);
        let hp_cur = (hp_frac * hp_max as f32) as i32;
        self.label_shadowed(82.0, y + 30.0,
            &format!("HP {}/{}", hp_cur, hp_max), 1.0, colors::WHITE);
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

pub fn lighten(c: [f32; 4], factor: f32) -> [f32; 4] {
    [(c[0]*factor).min(1.0), (c[1]*factor).min(1.0), (c[2]*factor).min(1.0), c[3]]
}

pub fn darken(c: [f32; 4], factor: f32) -> [f32; 4] {
    [c[0]*factor, c[1]*factor, c[2]*factor, c[3]]
}
