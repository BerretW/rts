/// Loading screen – simuluje načítání assetů.
///
/// V produkci nahraď `steps` skutečným async načítáním textur, zvuků atd.

use engine::{
    Rect,
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch, Texture},
    ui::{UiCtx, colors},
};

use super::{Screen, Transition};
use super::main_menu::MainMenuScreen;

// ── Krok načítání ─────────────────────────────────────────────────────────────

struct LoadStep {
    label:  &'static str,
    /// Funkce simulující práci (v reálu: volání std::fs::read / GPU upload…).
    work:   fn(&mut LoadingScreen, &RenderContext),
}

pub struct LoadingScreen {
    steps:       Vec<LoadStep>,
    current:     usize,
    /// Jak dlouho trvá jeden krok (simulace, sekundy).
    step_delay:  f32,
    elapsed:     f32,
    done:        bool,

    white_tex:   Option<Texture>,
    white_bg:    Option<engine::wgpu::BindGroup>,
}

impl LoadingScreen {
    pub fn new() -> Self {
        Self {
            steps: vec![
                LoadStep { label: "Initializing renderer...", work: |_, _| {} },
                LoadStep { label: "Loading terrain textures...", work: |_, _| {} },
                LoadStep { label: "Loading unit sprites...", work: |_, _| {} },
                LoadStep { label: "Loading building sprites...", work: |_, _| {} },
                LoadStep { label: "Loading audio assets...", work: |_, _| {} },
                LoadStep { label: "Generating map data...", work: |_, _| {} },
                LoadStep { label: "Initializing AI...", work: |_, _| {} },
                LoadStep { label: "Ready!", work: |_, _| {} },
            ],
            current:    0,
            step_delay: 0.25,
            elapsed:    0.0,
            done:       false,
            white_tex:  None,
            white_bg:   None,
        }
    }

    pub fn progress(&self) -> f32 {
        if self.steps.is_empty() { return 1.0; }
        self.current as f32 / self.steps.len() as f32
    }

    pub fn current_label(&self) -> &'static str {
        self.steps.get(self.current).map(|s| s.label).unwrap_or("Done")
    }
}

impl Screen for LoadingScreen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        let tex = Texture::white_pixel(ctx);
        let bg  = tex.create_bind_group(ctx, &batch.texture_bind_group_layout);
        self.white_tex = Some(tex);
        self.white_bg  = Some(bg);
    }

    fn update(&mut self, dt: f32, _input: &Input, _camera: &mut Camera) -> Transition {
        if self.done {
            return Transition::To(Box::new(MainMenuScreen::new()));
        }

        self.elapsed += dt;
        if self.elapsed >= self.step_delay {
            self.elapsed = 0.0;
            if self.current < self.steps.len() {
                // Proveď práci tohoto kroku
                let _work = self.steps[self.current].work;
                // work(self, ctx) – pro skutečné načítání potřebuješ RenderContext,
                // viz TODO níže
                self.current += 1;
            }
            if self.current >= self.steps.len() {
                self.done = true;
            }
        }
        Transition::None
    }

    fn render(&mut self, _batch: &mut SpriteBatch, _camera: &Camera) {
        // Loading screen nemá herní svět – vše se kreslí v render_ui.
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        let sw = ui.screen.x;
        let sh = ui.screen.y;

        // Tmavé pozadí
        ui.panel(Rect::new(0.0, 0.0, sw, sh), [0.05, 0.05, 0.07, 1.0]);

        // Logo placeholder (velký barevný blok uprostřed)
        let logo_w = 320.0;
        let logo_h = 80.0;
        let logo_x = (sw - logo_w) * 0.5;
        let logo_y = sh * 0.3;
        ui.panel(Rect::new(logo_x, logo_y, logo_w, logo_h), [0.15, 0.25, 0.45, 1.0]);
        ui.border(Rect::new(logo_x, logo_y, logo_w, logo_h), 2.0, colors::BORDER);

        // Vnitřní "nápis" – barevné proužky jako zástupný text "RTS"
        for (i, col) in [[0.4,0.6,1.0,1.0],[0.6,0.8,1.0,1.0],[0.3,0.5,0.9,1.0]].iter().enumerate() {
            ui.panel(Rect::new(logo_x + 20.0 + i as f32 * 100.0, logo_y + 20.0, 80.0, 40.0), *col);
        }

        // Progress bar
        let bar_w = 400.0;
        let bar_h = 20.0;
        let bar_x = (sw - bar_w) * 0.5;
        let bar_y = sh * 0.65;
        ui.progress_bar(
            Rect::new(bar_x, bar_y, bar_w, bar_h),
            self.progress(),
            [0.1, 0.1, 0.12, 1.0],
            [0.2, 0.45, 0.85, 1.0],
        );

        // Animovaný "spinner" – rotující čtverce (statická verze s pulsem)
        let spin_x = bar_x;
        let spin_y = bar_y + 30.0;
        let filled = (self.progress() * 8.0) as usize;
        for i in 0..8usize {
            let col = if i < filled { [0.2, 0.45, 0.85, 1.0] } else { [0.15, 0.15, 0.18, 1.0] };
            ui.panel(Rect::new(spin_x + i as f32 * 14.0, spin_y, 10.0, 10.0), col);
        }
    }

    fn texture(&self) -> &engine::wgpu::BindGroup {
        self.white_bg.as_ref().expect("LoadingScreen::init not called")
    }
}
