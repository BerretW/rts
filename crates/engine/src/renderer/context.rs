use std::sync::Arc;
use winit::window::Window;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("Failed to create surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("No compatible GPU adapter found")]
    NoAdapter,
    #[error("Failed to acquire device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
}

/// Základní wgpu stav: zařízení, fronta, povrch, konfigurace.
pub struct RenderContext {
    pub device:         wgpu::Device,
    pub queue:          wgpu::Queue,
    pub surface:        wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
    pub size:           winit::dpi::PhysicalSize<u32>,
}

impl RenderContext {
    pub async fn new(window: Arc<Window>) -> Result<Self, ContextError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // Surface potřebuje 'static – Arc<Window> to splňuje.
        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(ContextError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label:             Some("RTS Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits:   wgpu::Limits::default(),
                    memory_hints:      Default::default(),
                },
                None,
            )
            .await?;

        let surface_caps   = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage:                        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format:                       surface_format,
            width:                        size.width.max(1),
            height:                       size.height.max(1),
            present_mode:                 wgpu::PresentMode::AutoVsync,
            alpha_mode:                   surface_caps.alpha_modes[0],
            view_formats:                 vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Ok(Self { device, queue, surface, surface_config, surface_format, size })
    }

    /// Překonfiguruje povrch po změně velikosti okna.
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.surface_config.width  = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Vrátí poměr stran viewportu.
    pub fn aspect_ratio(&self) -> f32 {
        self.size.width as f32 / self.size.height as f32
    }
}
