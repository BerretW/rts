use wgpu::util::DeviceExt;
use super::context::RenderContext;

/// GPU textura s pohledem a samplerem.
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view:    wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width:   u32,
    pub height:  u32,
}

impl Texture {
    /// Načte PNG ze surových bytů (např. `include_bytes!(...)`).
    pub fn from_bytes(
        ctx:   &RenderContext,
        bytes: &[u8],
        label: &str,
    ) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory(bytes)?.into_rgba8();
        Ok(Self::from_rgba8(ctx, &img, label))
    }

    /// Vytvoří texturu ze `image::RgbaImage`.
    pub fn from_rgba8(
        ctx:   &RenderContext,
        img:   &image::RgbaImage,
        label: &str,
    ) -> Self {
        let width  = img.width();
        let height = img.height();

        let texture = ctx.device.create_texture_with_data(
            &ctx.queue,
            &wgpu::TextureDescriptor {
                label:           Some(label),
                size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          wgpu::TextureFormat::Rgba8UnormSrgb,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats:    &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            img.as_raw(),
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label:            Some(&format!("{label}_sampler")),
            // Pixelart: nejbližší soused – zachová ostré hrany.
            mag_filter:       wgpu::FilterMode::Nearest,
            min_filter:       wgpu::FilterMode::Nearest,
            mipmap_filter:    wgpu::FilterMode::Nearest,
            address_mode_u:   wgpu::AddressMode::ClampToEdge,
            address_mode_v:   wgpu::AddressMode::ClampToEdge,
            address_mode_w:   wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        Self { texture, view, sampler, width, height }
    }

    /// Jednobarevná 1×1 bílá textura – záchytná síť nebo barevné přebarvení.
    pub fn white_pixel(ctx: &RenderContext) -> Self {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        Self::from_rgba8(ctx, &img, "white_pixel")
    }

    /// Vytvoří bind group pro tuto texturu.
    ///
    /// `layout` musí odpovídat `TextureBindGroupLayout` ze `SpriteBatch`.
    pub fn create_bind_group(
        &self,
        ctx:    &RenderContext,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("texture_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}
