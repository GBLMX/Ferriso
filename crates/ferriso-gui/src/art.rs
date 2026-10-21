//! Advanced iced artwork: a WGSL aurora backdrop shared by Ferriso GUIs.
//!
//! The shader is a full-window animated atmosphere. It is deliberately a
//! [`shader::Program`] rather than a static gradient: applications can drive
//! `time` from a subscription and get flowing light without touching the core
//! actor or rebuilding the widget tree.
//!
//! ```no_run
//! # use ferriso_gui::iced::Element;
//! # fn view<'a, Message: 'a>(time: f32) -> Element<'a, Message> {
//! ferriso_gui::art::aurora(ferriso_gui::art::Aurora {
//!     time,
//!     palette: ferriso_gui::art::Palette::YAZI,
//!     intensity: 1.0,
//! })
//! # }
//! ```

use iced::widget::shader;
use iced::{Color, Length, Rectangle, wgpu};

/// Colors used by the aurora shader, in sRGB.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Deep background color.
    pub abyss: Color,
    /// Broad flowing ribbon color.
    pub glow: Color,
    /// Secondary ribbon color.
    pub accent: Color,
    /// Local halo highlight.
    pub highlight: Color,
}

impl Palette {
    /// The player palette: deep blue with a cold highlight.
    pub const FERRISO: Self = Self {
        abyss: Color::from_rgb(0.032, 0.042, 0.070),
        glow: Color::from_rgb(0.090, 0.290, 0.450),
        accent: Color::from_rgb(0.240, 0.300, 0.640),
        highlight: Color::from_rgb(0.400, 0.640, 0.980),
    };

    /// The file-manager palette: ink, teal, and warm coral.
    pub const YAZI: Self = Self {
        abyss: Color::from_rgb(0.016, 0.030, 0.055),
        glow: Color::from_rgb(0.030, 0.560, 0.500),
        accent: Color::from_rgb(0.520, 0.150, 0.460),
        highlight: Color::from_rgb(0.450, 0.980, 0.850),
    };
}

/// An animated aurora backdrop.
#[derive(Debug, Clone, Copy)]
pub struct Aurora {
    /// Elapsed time in seconds. Drive this from a subscription for motion.
    pub time: f32,
    /// Colors of the atmosphere.
    pub palette: Palette,
    /// Overall strength of the ribbons, `0.0..=1.0`.
    pub intensity: f32,
}

impl<Message> shader::Program<Message> for Aurora {
    type State = ();
    type Primitive = AuroraPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        AuroraPrimitive {
            time: self.time,
            palette: self.palette,
            intensity: self.intensity.clamp(0.0, 1.0),
        }
    }
}

/// Build a full-size aurora layer for a [`iced::widget::Stack`].
pub fn aurora<'a, Message: 'a>(aurora: Aurora) -> iced::Element<'a, Message> {
    shader::Shader::new(aurora)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[derive(Debug, Clone, Copy)]
pub struct AuroraPrimitive {
    time: f32,
    palette: Palette,
    intensity: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    intensity: f32,
    abyss: [f32; 4],
    glow: [f32; 4],
    accent: [f32; 4],
    highlight: [f32; 4],
}

impl shader::Primitive for AuroraPrimitive {
    type Pipeline = AuroraPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let uniforms = Uniforms {
            resolution: [
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ],
            time: self.time,
            intensity: self.intensity,
            abyss: linear(self.palette.abyss, 1.0),
            glow: linear(self.palette.glow, 1.0),
            accent: linear(self.palette.accent, 1.0),
            highlight: linear(self.palette.highlight, 1.0),
        };

        queue.write_buffer(&pipeline.uniforms, 0, bytemuck::bytes_of(&uniforms));
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        true
    }
}

#[derive(Debug)]
pub struct AuroraPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for AuroraPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        const SHADER: &str = include_str!("aurora.wgsl");

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ferriso.aurora.shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ferriso.aurora.uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ferriso.aurora.bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ferriso.aurora.bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ferriso.aurora.pipeline_layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ferriso.aurora.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniforms,
            bind_group,
        }
    }
}

/// Convert an sRGB [`Color`] to linear RGBA for the shader target.
fn linear(color: Color, alpha: f32) -> [f32; 4] {
    fn channel(value: f32) -> f32 {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    [channel(color.r), channel(color.g), channel(color.b), alpha]
}
