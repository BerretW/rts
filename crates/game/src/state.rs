/// GameRoot – implementuje `engine::app::Game` a deleguje vše na aktivní Screen.

use engine::{
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch},
    ui::UiCtx,
    app::Game,
};
use engine::wgpu;

use crate::screens::{Screen, Transition};
use crate::screens::loading::LoadingScreen;

pub struct GameRoot {
    screen:       Box<dyn Screen>,
    /// Příznak: nový screen potřebuje init před prvním snímkem.
    pending_init: bool,
}

impl GameRoot {
    pub fn new() -> Self {
        Self {
            screen:       Box::new(LoadingScreen::new()),
            pending_init: false, // LoadingScreen inicializuje Game::init()
        }
    }

    fn transition_to(&mut self, next: Box<dyn Screen>) {
        self.screen       = next;
        self.pending_init = true;
    }
}

impl Game for GameRoot {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch, _camera: &mut Camera) {
        // Inicializace prvního screenu (LoadingScreen).
        self.screen.init(ctx, batch);
    }

    fn needs_screen_init(&self) -> bool {
        self.pending_init
    }

    fn on_screen_init(&mut self, ctx: &RenderContext, batch: &SpriteBatch) {
        self.screen.init(ctx, batch);
        self.pending_init = false;
    }

    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera) {
        // Nevolej update pokud čekáme na init (engine to garantuje pořadím volání).
        match self.screen.update(dt, input, camera) {
            Transition::None    => {}
            Transition::To(next) => self.transition_to(next),
            Transition::Exit    => {
                // AppRunner nemá přímé API pro exit z Game traitu.
                // Workaround: přejdi na speciální "exit screen" nebo
                // nastav příznak a engine ho zachytí přes window close.
                // Pro teď: zavři okno posláním CloseRequested eventu není možné
                // přímo z Game traitu – použij std::process::exit jako krajní řešení.
                std::process::exit(0);
            }
        }
    }

    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera) {
        self.screen.render(batch, camera);
    }

    fn render_ui(&mut self, ui: &mut UiCtx) {
        self.screen.render_ui(ui);
    }

    fn texture(&self) -> &wgpu::BindGroup {
        self.screen.texture()
    }
}
