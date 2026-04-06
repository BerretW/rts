use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::camera::{Camera, CameraUniform};
use crate::font;
use crate::input::Input;
use crate::renderer::{RenderContext, SpriteBatch, Texture};
use crate::ui::UiCtx;


// ── Game trait ───────────────────────────────────────────────────────────────

/// Trait který implementuje herní kód.
pub trait Game: 'static {
    /// Inicializace po vytvoření GPU kontextu (voláno jednou při startu).
    fn init(&mut self, ctx: &RenderContext, batch: &SpriteBatch, camera: &mut Camera);

    /// Volá engine před `update()` kdykoliv `needs_screen_init()` vrátí true.
    fn on_screen_init(&mut self, _ctx: &RenderContext, _batch: &SpriteBatch) {}

    /// Vrátí true pokud je potřeba zavolat `on_screen_init()` před dalším snímkem.
    fn needs_screen_init(&self) -> bool { false }

    /// Herní logika. `dt` je v sekundách.
    fn update(&mut self, dt: f32, input: &Input, camera: &mut Camera);

    /// Kreslení herního světa (world-space kamera).
    fn render(&mut self, batch: &mut SpriteBatch, camera: &Camera);

    /// Kreslení UI (screen-space, pixely obrazovky).
    fn render_ui(&mut self, _ui: &mut UiCtx) {}

    /// Bind group pro world texturu.
    fn texture(&self) -> &wgpu::BindGroup;
}

// ── Interní stav ─────────────────────────────────────────────────────────────

struct Running<G: Game> {
    window:      Arc<Window>,
    ctx:         RenderContext,
    world_batch: SpriteBatch,
    ui_batch:    SpriteBatch,
    text_batch:  SpriteBatch,       // druhý batch pro text
    white_bg:    wgpu::BindGroup,   // UI textura – bílý pixel
    font_bg:     wgpu::BindGroup,   // textura font atlasu
    camera:      Camera,
    input:       Input,
    game:        G,
    last_t:      Instant,
}

pub struct AppRunner<G: Game> {
    title:  String,
    width:  u32,
    height: u32,
    game:   Option<G>,
    state:  Option<Running<G>>,
}

impl<G: Game> AppRunner<G> {
    pub fn new(title: impl Into<String>, width: u32, height: u32, game: G) -> Self {
        Self { title: title.into(), width, height, game: Some(game), state: None }
    }
}

impl<G: Game> ApplicationHandler for AppRunner<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title(&self.title)
                        .with_inner_size(winit::dpi::PhysicalSize::new(self.width, self.height)),
                )
                .expect("Failed to create window"),
        );

        let ctx    = pollster::block_on(RenderContext::new(window.clone()))
            .expect("Failed to create render context");
        let size   = ctx.size;

        let world_batch = SpriteBatch::new(&ctx);
        let ui_batch    = SpriteBatch::new(&ctx);
        let text_batch  = SpriteBatch::new(&ctx);

        // Bílý pixel pro UI solid-color prvky
        let white_tex = Texture::white_pixel(&ctx);
        let white_bg  = white_tex.create_bind_group(&ctx, &ui_batch.texture_bind_group_layout);

        // Font atlas textura
        let font_atlas = font::build_atlas();
        let font_tex   = Texture::from_rgba8(&ctx, &font_atlas, "font_atlas");
        let font_bg    = font_tex.create_bind_group(&ctx, &text_batch.texture_bind_group_layout);

        let mut camera = Camera::new(size.width as f32, size.height as f32);
        let input      = Input::new();

        let mut game = self.game.take().expect("game already consumed");
        game.init(&ctx, &world_batch, &mut camera);

        self.state = Some(Running {
            window, ctx, world_batch, ui_batch, text_batch, white_bg, font_bg,
            camera, input, game,
            last_t: Instant::now(),
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let s = match self.state.as_mut() { Some(s) => s, None => return };

        match event {
            WindowEvent::Resized(size) => {
                s.ctx.resize(size);
                s.camera.set_viewport(size.width as f32, size.height as f32);
            }
            WindowEvent::KeyboardInput { event: ke, .. } => {
                if let PhysicalKey::Code(code) = ke.physical_key {
                    s.input.on_key(code, ke.state);
                }
                // Předej zadané znaky textovým polím UI
                if ke.state == winit::event::ElementState::Pressed {
                    if let Some(text) = &ke.text {
                        let s_text = text.as_str();
                        // Filtruj řídicí znaky (backspace, enter atd. se zpracují přes KeyCode)
                        if s_text.chars().all(|c| !c.is_control()) {
                            s.input.on_text_input(s_text);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                s.input.on_mouse_button(button, state);
            }
            WindowEvent::CursorMoved { position, .. } => {
                s.input.on_mouse_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p)   => p.y as f32 * 0.1,
                };
                s.input.on_scroll(scroll);
            }
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt  = now.duration_since(s.last_t).as_secs_f32().min(0.1);
                s.last_t = now;

                // ── Update ───────────────────────────────────────────────
                s.game.update(dt, &s.input, &mut s.camera);

                // ── Screen re-init ───────────────────────────────────────
                if s.game.needs_screen_init() {
                    s.game.on_screen_init(&s.ctx, &s.world_batch);
                }

                // ── World camera → GPU ───────────────────────────────────
                let cam_u = CameraUniform::from_camera(&s.camera);
                s.world_batch.update_camera(&s.ctx, &cam_u);

                // ── UI camera → GPU (screen-space) ───────────────────────
                let vp   = s.camera.viewport();
                let ui_u = CameraUniform::screen_space(vp.x, vp.y);
                s.ui_batch.update_camera(&s.ctx, &ui_u);
                s.text_batch.update_camera(&s.ctx, &ui_u);

                // ── Render ───────────────────────────────────────────────
                match s.ctx.surface.get_current_texture() {
                    Ok(surface_tex) => {
                        let view = surface_tex.texture.create_view(
                            &wgpu::TextureViewDescriptor::default(),
                        );
                        let mut enc = s.ctx.device.create_command_encoder(
                            &wgpu::CommandEncoderDescriptor { label: Some("frame") },
                        );

                        // Clear
                        {
                            let _p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("clear"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view, resolve_target: None,
                                    ops: wgpu::Operations {
                                        load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes:         None,
                                occlusion_query_set:      None,
                            });
                        }

                        // World render
                        s.game.render(&mut s.world_batch, &s.camera);
                        let world_tex = s.game.texture() as *const wgpu::BindGroup;
                        s.world_batch.flush(&s.ctx, &mut enc, &view, unsafe { &*world_tex });

                        // UI render (solid-color prvky)
                        {
                            let mut ui = UiCtx::new(
                                &mut s.ui_batch,
                                &mut s.text_batch,
                                &s.input,
                                vp,
                            );
                            s.game.render_ui(&mut ui);
                        }
                        let white = &s.white_bg as *const wgpu::BindGroup;
                        s.ui_batch.flush(&s.ctx, &mut enc, &view, unsafe { &*white });

                        // Text render (font atlas)
                        let font = &s.font_bg as *const wgpu::BindGroup;
                        s.text_batch.flush(&s.ctx, &mut enc, &view, unsafe { &*font });

                        s.ctx.queue.submit(std::iter::once(enc.finish()));
                        surface_tex.present();
                    }
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        s.ctx.resize(s.ctx.size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU OOM – exiting");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("Surface error: {e:?}"),
                }

                s.input.end_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = self.state.as_ref() {
            s.window.request_redraw();
        }
    }
}

/// Spustí herní smyčku. Blokuje do zavření okna.
pub fn run<G: Game>(title: impl Into<String>, width: u32, height: u32, game: G) {
    env_logger::init();
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut runner = AppRunner::new(title, width, height, game);
    event_loop.run_app(&mut runner).expect("Event loop error");
}
