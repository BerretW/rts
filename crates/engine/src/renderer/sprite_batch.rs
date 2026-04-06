use wgpu::util::DeviceExt;
use crate::{Rect, UvRect};
use crate::camera::CameraUniform;
use super::context::RenderContext;

/// Jeden vertex spritů – musí odpovídat layoutu v sprite.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub uv:       [f32; 2],
    pub color:    [f32; 4],
}

impl SpriteVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &Self::ATTRIBS,
        }
    }
}

/// Dávkový renderer pro 2D sprity a dlaždice.
///
/// Typické použití v každém snímku:
/// ```ignore
/// batch.update_camera(ctx, &camera);
/// // ... batch.draw(...) volání ...
/// batch.flush(ctx, encoder, view, texture_bind_group);
/// ```
pub struct SpriteBatch {
    pipeline:                 wgpu::RenderPipeline,
    vertex_buffer:            wgpu::Buffer,
    index_buffer:             wgpu::Buffer,
    camera_buffer:            wgpu::Buffer,
    camera_bind_group:        wgpu::BindGroup,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,

    vertices:  Vec<SpriteVertex>,
    max_quads: usize,
}

impl SpriteBatch {
    /// Maximální počet quads v jednom flushu.
    const DEFAULT_MAX_QUADS: usize = 16_384;

    pub fn new(ctx: &RenderContext) -> Self {
        let device = &ctx.device;

        // ── Camera uniform ──────────────────────────────────────────────
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("camera_uniform"),
            size:               std::mem::size_of::<CameraUniform>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("camera_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty:         wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("camera_bg"),
            layout:  &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // ── Texture bind group layout ───────────────────────────────────
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("texture_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty:         wgpu::BindingType::Texture {
                        multisampled:   false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count:      None,
                },
            ],
        });

        // ── Shader & pipeline ───────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/sprite.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("sprite_pipeline_layout"),
            bind_group_layouts:   &[&camera_bgl, &texture_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("sprite_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         "vs_main",
                buffers:             &[SpriteVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         "fs_main",
                targets:             &[Some(wgpu::ColorTargetState {
                    format:     ctx.surface_format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology:           wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face:         wgpu::FrontFace::Ccw,
                cull_mode:          None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // ── Vertex buffer (dynamický) ───────────────────────────────────
        let max_quads   = Self::DEFAULT_MAX_QUADS;
        let max_verts   = max_quads * 4;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("sprite_vb"),
            size:               (max_verts * std::mem::size_of::<SpriteVertex>()) as u64,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Index buffer (statický – vždy stejný vzor pro quady) ────────
        let indices: Vec<u32> = (0..max_quads as u32)
            .flat_map(|i| {
                let b = i * 4;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect();

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("sprite_ib"),
            contents: bytemuck::cast_slice(&indices),
            usage:    wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            camera_buffer,
            camera_bind_group,
            texture_bind_group_layout: texture_bgl,
            vertices: Vec::with_capacity(max_quads * 4),
            max_quads,
        }
    }

    // ── Kamera ──────────────────────────────────────────────────────────

    /// Nahraje matici kamery do GPU – volat jednou za snímek před flush().
    pub fn update_camera(&self, ctx: &RenderContext, uniform: &CameraUniform) {
        ctx.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(uniform));
    }

    // ── Kreslení ────────────────────────────────────────────────────────

    /// Přidá sprite/tile do dávky.
    ///
    /// * `dst` – cílový obdélník ve světových souřadnicích
    /// * `src` – UV výřez v textuře
    /// * `color` – barevný tint (RGBA 0.0–1.0); [1,1,1,1] = bez tintování
    pub fn draw(&mut self, dst: Rect, src: UvRect, color: [f32; 4]) {
        if self.vertices.len() / 4 >= self.max_quads {
            log::warn!("SpriteBatch: přesáhnut limit {} quads, přebytečné sprity přeskočeny", self.max_quads);
            return;
        }

        let (x0, y0) = (dst.x, dst.y);
        let (x1, y1) = (dst.x + dst.w, dst.y + dst.h);
        let (u0, v0) = (src.u, src.v);
        let (u1, v1) = (src.u + src.uw, src.v + src.vh);

        self.vertices.extend_from_slice(&[
            SpriteVertex { position: [x0, y0], uv: [u0, v0], color },
            SpriteVertex { position: [x1, y0], uv: [u1, v0], color },
            SpriteVertex { position: [x1, y1], uv: [u1, v1], color },
            SpriteVertex { position: [x0, y1], uv: [u0, v1], color },
        ]);
    }

    /// Zkrácený zápis: bílá barva (bez tintování).
    pub fn draw_plain(&mut self, dst: Rect, src: UvRect) {
        self.draw(dst, src, [1.0, 1.0, 1.0, 1.0]);
    }

    /// Odeslat nahromaděné sprity na GPU a vykreslit.
    ///
    /// * `texture_bind_group` – bind group textury vytvořená přes `Texture::create_bind_group()`
    ///
    /// Metoda sama resetuje vnitřní buffer – volat na konci každého snímku.
    pub fn flush(
        &mut self,
        ctx:                 &RenderContext,
        encoder:             &mut wgpu::CommandEncoder,
        view:                &wgpu::TextureView,
        texture_bind_group:  &wgpu::BindGroup,
    ) {
        if self.vertices.is_empty() {
            return;
        }

        let quad_count = self.vertices.len() / 4;

        ctx.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.vertices),
        );

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:                    Some("sprite_pass"),
            color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load:  wgpu::LoadOp::Load, // předchozí clear nebo jiný pass
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes:         None,
            occlusion_query_set:      None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, texture_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..(quad_count as u32 * 6), 0, 0..1);

        drop(pass);
        self.vertices.clear();
    }
}
