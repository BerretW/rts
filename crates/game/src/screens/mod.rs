pub mod loading;
pub mod main_menu;
pub mod in_game;

use engine::{
    camera::Camera,
    input::Input,
    renderer::{RenderContext, SpriteBatch},
    ui::UiCtx,
};
use engine::wgpu;

/// Co má engine udělat po `update()` daného screenu.
pub enum Transition {
    /// Zůstaň na aktuálním screenu.
    None,
    /// Přejdi na jiný screen.
    To(Box<dyn Screen>),
    /// Ukonči aplikaci.
    Exit,
}

/// Trait pro jednotlivé obrazovky hry.
pub trait Screen {
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch);
    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera) -> Transition;
    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera);
    fn render_ui(&mut self, ui: &mut UiCtx);
    fn texture(&self) -> &wgpu::BindGroup;
}
