/// Hlavní menu.

use engine::{
    Rect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    ui::{UiCtx, colors},
};
use engine::winit::event::MouseButton;

use super::{Screen, Transition};
use super::in_game::InGameScreen;

pub struct MainMenuScreen {
    white_bg: Option<engine::wgpu::BindGroup>,
    // Animace – pomalu se pohybující pozadí
    bg_offset: f32,
    // Která položka je hovered (pro vizuální efekt)
    hovered: Option<usize>,
}

impl MainMenuScreen {
    pub fn new() -> Self {
        Self {
            white_bg:  None,
            bg_offset: 0.0,
            hovered:   None,
        }
    }

    /// Vrátí Rect i-tého tlačítka menu.
    fn btn_rect(sw: f32, sh: f32, i: usize) -> Rect {
        let w = 260.0;
        let h = 48.0;
        let gap = 12.0;
        let x = (sw - w) * 0.5;
        let y = sh * 0.45 + i as f32 * (h + gap);
        Rect::new(x, y, w, h)
    }
}

impl Screen for MainMenuScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        let bg  = tex.create_bind_group(ctx, &batch.texture_bind_group_layout);
        self.white_bg = Some(bg);
    }

    fn update(&mut self, dt: f32, input: &Input, _camera: &mut Camera) -> Transition {
        self.bg_offset = (self.bg_offset + dt * 8.0) % 360.0;

        // Detekce kliknutí se řeší v render_ui přes UiCtx::button.
        // Ale protože render_ui nemůže vracet Transition, použijeme flag.
        // Proto detekujeme kliknutí zde z Inputu.

        let sw = 1280.0_f32; // Fallback rozlišení – skutečné se předá v render_ui
        let sh = 720.0_f32;

        // "New Game"
        let r0 = Self::btn_rect(sw, sh, 0);
        if r0.contains(input.mouse_pos) && input.mouse_just_released(MouseButton::Left) {
            return Transition::To(Box::new(InGameScreen::new()));
        }

        // "Exit"
        let r2 = Self::btn_rect(sw, sh, 2);
        if r2.contains(input.mouse_pos) && input.mouse_just_released(MouseButton::Left) {
            return Transition::Exit;
        }

        Transition::None
    }

    fn render(&mut self, _batch: &mut SpriteBatch, _camera: &Camera) {}

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        // ── Pozadí ──────────────────────────────────────────────────────
        ui.panel(Rect::new(0.0, 0.0, sw, sh), [0.04, 0.04, 0.06, 1.0]);

        // Dekorativní pruhy (animované)
        for i in 0..6i32 {
            let x = (i as f32 * 220.0 + self.bg_offset * 3.0) % (sw + 200.0) - 100.0;
            ui.panel(Rect::new(x, 0.0, 4.0, sh), [0.08, 0.12, 0.20, 0.4]);
        }

        // ── Logo ─────────────────────────────────────────────────────────
        let logo_w = 400.0;
        let logo_h = 100.0;
        let logo_x = (sw - logo_w) * 0.5;
        let logo_y = sh * 0.12;
        ui.panel(Rect::new(logo_x, logo_y, logo_w, logo_h), [0.10, 0.18, 0.35, 0.95]);
        ui.border(Rect::new(logo_x, logo_y, logo_w, logo_h), 2.0, [0.30, 0.50, 0.80, 1.0]);

        // Stylizované "logo bloky"
        let colors_logo = [
            [0.30, 0.55, 1.00, 1.0],
            [0.20, 0.40, 0.80, 1.0],
            [0.40, 0.65, 1.00, 1.0],
            [0.15, 0.30, 0.65, 1.0],
        ];
        for (i, col) in colors_logo.iter().enumerate() {
            ui.panel(Rect::new(logo_x + 20.0 + i as f32 * 90.0, logo_y + 15.0, 70.0, 70.0), *col);
        }

        // Podtitulek – řada malých čtverečků
        let sub_y = logo_y + logo_h + 15.0;
        for i in 0..20i32 {
            let col = if i % 3 == 0 { [0.4,0.6,0.9,0.7] } else { [0.2,0.3,0.5,0.5] };
            ui.panel(Rect::new(logo_x + i as f32 * 20.0, sub_y, 16.0, 4.0), col);
        }

        // ── Tlačítka ──────────────────────────────────────────────────────
        let labels_colors: [([f32;4], [f32;4]); 3] = [
            (colors::BTN_NORMAL,  [0.15, 0.55, 0.20, 1.0]),   // New Game – zelenkavý akcent
            ([0.25, 0.20, 0.40, 1.0], [0.35, 0.28, 0.55, 1.0]), // Options
            (colors::BTN_DANGER,  [0.70, 0.20, 0.15, 1.0]),   // Exit – červená
        ];

        for (i, (base, accent)) in labels_colors.iter().enumerate() {
            let rect = Self::btn_rect(sw, sh, i);
            let clicked = ui.button(rect, *base);

            // Barevný akcentový proužek vlevo
            ui.panel(Rect::new(rect.x, rect.y, 5.0, rect.h), *accent);

            // Tečky jako placeholder textu (3 tečky = "nápis")
            let dot_x = rect.x + 20.0;
            let dot_y = rect.y + rect.h * 0.5 - 4.0;
            let dot_colors = [*base, *accent, *base];
            for (j, dc) in dot_colors.iter().enumerate() {
                ui.panel(Rect::new(dot_x + j as f32 * 16.0, dot_y, 10.0, 8.0),
                         lighten(*dc, 1.4));
            }

            // Kód akce – klíč pro debug výpis (v produkci text renderer)
            let _ = clicked;
        }

        // ── Verze (spodní lišta) ──────────────────────────────────────────
        ui.panel(Rect::new(0.0, sh - 24.0, sw, 24.0), [0.06, 0.06, 0.08, 0.9]);
        ui.border(Rect::new(0.0, sh - 24.0, sw, 24.0), 1.0, colors::BORDER);
        // Verze jako malé obdélníčky
        for i in 0..5i32 {
            ui.panel(Rect::new(10.0 + i as f32 * 8.0, sh - 18.0, 5.0, 12.0),
                     [0.3, 0.4, 0.5, 0.8]);
        }
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.white_bg.as_ref().expect("MainMenuScreen::init not called")
    }
}

fn lighten(c: [f32; 4], f: f32) -> [f32; 4] {
    [c[0]*f, c[1]*f, c[2]*f, c[3]]
}
