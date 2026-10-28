//! Advanced iced artwork: a WGSL aurora backdrop, animated pulse rings, and
//! canvas charts (orbit, bars, and gauge).
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
use iced::{
    Alignment, Color, Font, Length, Pixels, Point, Radians, Rectangle, Size, border, wgpu,
    widget::canvas,
};

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

    /// Windows 11 Fluent: deep steel with a cold blue ribbon.
    pub const WINDOWS: Self = Self {
        abyss: Color::from_rgb(0.043, 0.071, 0.125),
        glow: Color::from_rgb(0.039, 0.231, 0.431),
        accent: Color::from_rgb(0.180, 0.380, 0.720),
        highlight: Color::from_rgb(0.376, 0.804, 1.000),
    };

    /// macOS: graphite with blue and violet light.
    pub const MACOS: Self = Self {
        abyss: Color::from_rgb(0.078, 0.078, 0.090),
        glow: Color::from_rgb(0.118, 0.227, 0.431),
        accent: Color::from_rgb(0.300, 0.165, 0.478),
        highlight: Color::from_rgb(0.392, 0.824, 1.000),
    };

    /// Ember: warm charcoal with copper and gold light.
    pub const EMBER: Self = Self {
        abyss: Color::from_rgb(0.102, 0.059, 0.043),
        glow: Color::from_rgb(0.353, 0.122, 0.051),
        accent: Color::from_rgb(0.604, 0.231, 0.102),
        highlight: Color::from_rgb(1.000, 0.706, 0.361),
    };
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
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
            time: finite_or(self.time, 0.0),
            palette: self.palette,
            intensity: finite_or(self.intensity, 0.0).clamp(0.0, 1.0),
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

/// One slice of an [`Orbit`] composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitSegment {
    /// Relative weight. Finite values at or below zero are skipped.
    pub value: f32,
    /// Slice color.
    pub color: Color,
}

impl OrbitSegment {
    /// Create a segment.
    pub fn new(value: f32, color: Color) -> Self {
        Self { value, color }
    }
}

/// An animated radial composition chart.
///
/// The orbit is a generic template for directory composition, storage usage,
/// playback sources, and dashboard statistics. It is a pure canvas program:
/// the caller supplies weights, colors, and a time value driven by a
/// subscription.
#[derive(Debug, Clone)]
pub struct Orbit {
    /// Weighted slices.
    pub segments: Vec<OrbitSegment>,
    /// Elapsed time in seconds, used for the rotating spark and slice phase.
    pub time: f32,
    /// Large text inside the ring.
    pub center: String,
    /// Small caption below the center text.
    pub label: String,
    /// Number of concentric guide rings.
    pub rings: u8,
    /// Gap between slices, in radians.
    pub gap: f32,
    /// Draw a rotating spark on the outer ring.
    pub spark: bool,
    /// Center value color.
    pub text_color: Color,
    /// Caption color.
    pub label_color: Color,
    /// Guide-ring color.
    pub guide_color: Color,
    /// Rotating spark color.
    pub spark_color: Color,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            time: 0.0,
            center: String::new(),
            label: String::new(),
            rings: 3,
            gap: 0.03,
            spark: true,
            text_color: Color::WHITE,
            label_color: Color::WHITE,
            guide_color: Color::WHITE,
            spark_color: Color::WHITE,
        }
    }
}

impl Orbit {
    /// Sum of all finite positive segment weights.
    pub fn total(&self) -> f32 {
        self.segments
            .iter()
            .map(|segment| finite_or(segment.value, 0.0).max(0.0))
            .sum()
    }
}

impl<Message> canvas::Program<Message> for Orbit {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = (bounds.width.min(bounds.height) * 0.5 - 12.0).max(20.0);
        let inner = (radius * 0.66).max(12.0);
        let total = self.total().max(0.000_001);
        let tau = std::f32::consts::TAU;

        for ring in 1..=self.rings.max(1) {
            let factor = f32::from(ring) / f32::from(self.rings.max(1));
            frame.stroke(
                &canvas::Path::circle(center, radius * factor),
                canvas::Stroke::<'_>::default()
                    .with_color(Color {
                        a: 0.08,
                        ..self.guide_color
                    })
                    .with_width(1.0),
            );
        }

        let time = finite_or(self.time, 0.0);
        let gap = finite_or(self.gap, 0.03).clamp(0.0, 0.4);
        let mut angle = -std::f32::consts::FRAC_PI_2 + time * 0.18;
        for segment in &self.segments {
            let ratio = finite_or(segment.value, 0.0).max(0.0) / total;
            if ratio > 0.0 {
                let sweep = (ratio * tau - gap).max(0.01);
                let path = ring_sector(center, inner, radius, angle, angle + sweep);
                frame.fill(&path, segment.color);
            }
            angle += ratio * tau;
        }

        if self.spark {
            let spark_angle = self.time * 0.9;
            let spark = polar(center, radius, spark_angle);
            frame.fill(
                &canvas::Path::circle(spark, 3.0),
                Color {
                    a: 0.9,
                    ..self.spark_color
                },
            );
        }

        if !self.center.is_empty() {
            frame.fill_text(canvas::Text {
                content: self.center.clone(),
                position: Point::new(center.x, center.y - 4.0),
                color: self.text_color,
                size: Pixels(28.0),
                font: Font::MONOSPACE,
                align_x: Alignment::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        if !self.label.is_empty() {
            frame.fill_text(canvas::Text {
                content: self.label.clone(),
                position: Point::new(center.x, center.y + 22.0),
                color: Color {
                    a: 0.68,
                    ..self.label_color
                },
                size: Pixels(10.0),
                align_x: Alignment::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

/// Build a full-size orbit canvas. Place it in a fixed-height container.
pub fn orbit<'a, Message: 'a>(orbit: Orbit) -> iced::Element<'a, Message> {
    canvas::Canvas::new(orbit)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn ring_sector(center: Point, inner: f32, outer: f32, start: f32, end: f32) -> canvas::Path {
    canvas::Path::new(|builder| {
        builder.move_to(polar(center, outer, start));
        builder.arc(canvas::path::Arc {
            center,
            radius: outer,
            start_angle: Radians(start),
            end_angle: Radians(end),
        });
        builder.line_to(polar(center, inner, end));
        builder.arc(canvas::path::Arc {
            center,
            radius: inner,
            start_angle: Radians(end),
            end_angle: Radians(start),
        });
        builder.close();
    })
}

fn polar(center: Point, radius: f32, angle: f32) -> Point {
    center + iced::Vector::new(angle.cos() * radius, angle.sin() * radius)
}

/// A compact bar chart for time-series or spectrum data.
///
/// Values are drawn left to right. Negative and non-finite values are treated
/// as zero; the caller owns sampling, windowing, and the update clock. This is
/// the small visual primitive shared by network monitors, audio meters, and
/// launcher activity strips; it is not a plotting library.
#[derive(Debug, Clone, PartialEq)]
pub struct Bars {
    /// Sample values in display order.
    pub values: Vec<f32>,
    /// Bar color.
    pub color: Color,
    /// Optional color for the newest (rightmost) sample.
    pub latest_color: Option<Color>,
    /// Fixed scale maximum. `None` scales to the largest positive sample.
    pub max: Option<f32>,
    /// Gap between bars in logical pixels.
    pub gap: f32,
    /// Bar corner radius in logical pixels.
    pub radius: f32,
    /// Baseline color. Use a transparent color to omit it.
    pub baseline: Color,
}

impl Default for Bars {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            color: Color::WHITE,
            latest_color: None,
            max: None,
            gap: 2.0,
            radius: 2.0,
            baseline: Color::TRANSPARENT,
        }
    }
}

impl Bars {
    /// Largest finite positive sample, or `0.0` for an empty/all-negative series.
    pub fn peak(&self) -> f32 {
        self.values.iter().copied().fold(0.0_f32, |peak, value| {
            if value.is_finite() {
                peak.max(value)
            } else {
                peak
            }
        })
    }

    fn scale(&self) -> f32 {
        self.max
            .filter(|max| max.is_finite() && *max > 0.0)
            .unwrap_or_else(|| self.peak().max(0.000_001))
    }
}

impl<Message> canvas::Program<Message> for Bars {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        if self.baseline.a > 0.0 && bounds.height > 0.0 {
            frame.fill_rectangle(
                Point::new(0.0, bounds.height - 1.0),
                Size::new(bounds.width, 1.0),
                self.baseline,
            );
        }

        if self.values.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![frame.into_geometry()];
        }

        let scale = self.scale();
        let slot = bounds.width / self.values.len() as f32;
        let gap = finite_or(self.gap, 2.0).clamp(0.0, slot * 0.6);
        let bar_width = (slot - gap).max(1.0).min(slot);

        for (index, value) in self.values.iter().copied().enumerate() {
            let value = finite_or(value, 0.0).max(0.0);
            let ratio = (value / scale).clamp(0.0, 1.0);
            let height = bounds.height * ratio;
            if height <= 0.0 {
                continue;
            }

            let x = index as f32 * slot + (slot - bar_width) * 0.5;
            let radius = finite_or(self.radius, 0.0)
                .clamp(0.0, bar_width * 0.5)
                .min(height * 0.5);
            let path = canvas::Path::rounded_rectangle(
                Point::new(x, bounds.height - height),
                Size::new(bar_width, height),
                border::Radius::from(radius),
            );
            let color = if index + 1 == self.values.len() {
                self.latest_color.unwrap_or(self.color)
            } else {
                self.color
            };
            frame.fill(&path, color);
        }

        vec![frame.into_geometry()]
    }
}

/// Build a full-size bar chart. Place it in a fixed-height container.
pub fn bars<'a, Message: 'a>(bars: Bars) -> iced::Element<'a, Message> {
    canvas::Canvas::new(bars)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A circular progress gauge with center text.
///
/// Use this for bounded progress states such as downloads, imports, and
/// installers. The value is clamped to `0.0..=1.0`; the application still owns
/// the state machine and the timer that drives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Gauge {
    /// Progress in the range `0.0..=1.0`.
    pub value: f32,
    /// Large center text, typically a percentage.
    pub center: String,
    /// Small caption below the center text.
    pub label: String,
    /// Unfilled track color.
    pub track: Color,
    /// Filled arc color.
    pub fill: Color,
    /// Center text color.
    pub text_color: Color,
    /// Caption color.
    pub label_color: Color,
    /// Arc thickness in logical pixels.
    pub thickness: f32,
}

impl Default for Gauge {
    fn default() -> Self {
        Self {
            value: 0.0,
            center: String::new(),
            label: String::new(),
            track: Color::WHITE,
            fill: Color::WHITE,
            text_color: Color::WHITE,
            label_color: Color::WHITE,
            thickness: 10.0,
        }
    }
}

impl Gauge {
    /// Clamped progress; NaN is treated as zero.
    pub fn progress(&self) -> f32 {
        if self.value.is_nan() {
            0.0
        } else {
            self.value.clamp(0.0, 1.0)
        }
    }
}

impl<Message> canvas::Program<Message> for Gauge {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let outer = (bounds.width.min(bounds.height) * 0.5 - 4.0).max(12.0);
        let thickness = finite_or(self.thickness, 10.0).clamp(2.0, outer * 0.35);
        let radius = (outer - thickness * 0.5).max(2.0);
        let progress = self.progress();
        let start = -std::f32::consts::FRAC_PI_2;

        frame.stroke(
            &canvas::Path::circle(center, radius),
            canvas::Stroke::<'_>::default()
                .with_color(self.track)
                .with_width(thickness),
        );

        if progress > 0.0 {
            let path = if progress >= 1.0 {
                canvas::Path::circle(center, radius)
            } else {
                arc_path(
                    center,
                    radius,
                    start,
                    start + std::f32::consts::TAU * progress,
                )
            };
            frame.stroke(
                &path,
                canvas::Stroke::<'_>::default()
                    .with_color(self.fill)
                    .with_width(thickness)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }

        if !self.center.is_empty() {
            frame.fill_text(canvas::Text {
                content: self.center.clone(),
                position: Point::new(center.x, center.y - 2.0),
                color: self.text_color,
                size: Pixels(22.0),
                font: Font::MONOSPACE,
                align_x: Alignment::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        if !self.label.is_empty() {
            frame.fill_text(canvas::Text {
                content: self.label.clone(),
                position: Point::new(center.x, center.y + 17.0),
                color: Color {
                    a: 0.68,
                    ..self.label_color
                },
                size: Pixels(9.0),
                align_x: Alignment::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

/// Build a full-size circular gauge. Place it in a square or near-square box.
pub fn gauge<'a, Message: 'a>(gauge: Gauge) -> iced::Element<'a, Message> {
    canvas::Canvas::new(gauge)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Animated concentric pulse rings for focus, activity, or a live beacon.
///
/// `progress` is the normalized phase; values outside `0.0..=1.0` and
/// non-finite values are bounded. Drive it from a renderer-local clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulse {
    /// Ring color.
    pub color: Color,
    /// Normalized animation phase.
    pub progress: f32,
    /// Number of visible rings, clamped to `1..=12`.
    pub rings: u8,
    /// Stroke width in logical pixels.
    pub width: f32,
}

impl Default for Pulse {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            progress: 0.0,
            rings: 3,
            width: 1.5,
        }
    }
}

impl Pulse {
    fn phase(&self) -> f32 {
        finite_or(self.progress, 0.0).clamp(0.0, 1.0)
    }
}

impl<Message> canvas::Program<Message> for Pulse {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let outer = (bounds.width.min(bounds.height) * 0.5 - 4.0).max(8.0);
        let progress = self.phase();
        let rings = self.rings.clamp(1, 12);
        let width = finite_or(self.width, 1.5).clamp(0.5, 6.0);

        for ring in 0..rings {
            let phase = (progress + f32::from(ring) / f32::from(rings)).fract();
            let radius = outer * (0.18 + 0.82 * phase);
            let alpha = self.color.a * (1.0 - phase).powi(2) * 0.65;
            frame.stroke(
                &canvas::Path::circle(center, radius),
                canvas::Stroke::<'_>::default()
                    .with_color(Color {
                        a: alpha,
                        ..self.color
                    })
                    .with_width(width),
            );
        }

        frame.fill(
            &canvas::Path::circle(center, (outer * 0.05).max(1.0)),
            Color {
                a: self.color.a * 0.8,
                ..self.color
            },
        );

        vec![frame.into_geometry()]
    }
}

/// Build a full-size pulse layer.
pub fn pulse<'a, Message: 'a>(pulse: Pulse) -> iced::Element<'a, Message> {
    canvas::Canvas::new(pulse)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn arc_path(center: Point, radius: f32, start: f32, end: f32) -> canvas::Path {
    canvas::Path::new(|builder| {
        builder.arc(canvas::path::Arc {
            center,
            radius,
            start_angle: Radians(start),
            end_angle: Radians(end),
        });
    })
}

#[cfg(test)]
mod art_tests {
    use super::*;

    #[test]
    fn orbit_total_ignores_negative_weights() {
        let orbit = Orbit {
            segments: vec![
                OrbitSegment::new(2.0, Color::WHITE),
                OrbitSegment::new(-1.0, Color::WHITE),
                OrbitSegment::new(3.5, Color::WHITE),
            ],
            ..Orbit::default()
        };
        assert!((orbit.total() - 5.5).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_orbit_has_zero_total() {
        assert_eq!(Orbit::default().total(), 0.0);
    }

    #[test]
    fn bars_peak_ignores_negative_samples() {
        let bars = Bars {
            values: vec![0.2, -4.0, 3.5, 0.0],
            ..Bars::default()
        };
        assert!((bars.peak() - 3.5).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_bars_have_zero_peak() {
        assert_eq!(Bars::default().peak(), 0.0);
    }

    #[test]
    fn gauge_clamps_progress_to_bounds() {
        let gauge = Gauge {
            value: -0.2,
            ..Gauge::default()
        };
        assert_eq!(gauge.progress(), 0.0);

        let gauge = Gauge {
            value: 1.4,
            ..Gauge::default()
        };
        assert_eq!(gauge.progress(), 1.0);
    }

    #[test]
    fn orbit_total_ignores_non_finite_weights() {
        let orbit = Orbit {
            segments: vec![
                OrbitSegment::new(f32::NAN, Color::WHITE),
                OrbitSegment::new(f32::INFINITY, Color::WHITE),
                OrbitSegment::new(-1.0, Color::WHITE),
                OrbitSegment::new(2.0, Color::WHITE),
            ],
            ..Orbit::default()
        };
        assert_eq!(orbit.total(), 2.0);
    }

    #[test]
    fn bars_peak_ignores_non_finite_samples() {
        let bars = Bars {
            values: vec![f32::NAN, f32::INFINITY, -4.0, 3.5],
            ..Bars::default()
        };
        assert_eq!(bars.peak(), 3.5);
    }

    #[test]
    fn bars_scale_ignores_a_non_finite_maximum() {
        let bars = Bars {
            values: vec![2.0],
            max: Some(f32::INFINITY),
            ..Bars::default()
        };
        assert_eq!(bars.scale(), 2.0);
    }

    #[test]
    fn gauge_nan_progress_is_zero() {
        let gauge = Gauge {
            value: f32::NAN,
            ..Gauge::default()
        };
        assert_eq!(gauge.progress(), 0.0);
    }

    #[test]
    fn aurora_sanitizes_non_finite_inputs() {
        let aurora = Aurora {
            time: f32::INFINITY,
            palette: Palette::FERRISO,
            intensity: f32::NAN,
        };
        let primitive = <Aurora as shader::Program<()>>::draw(
            &aurora,
            &(),
            iced::mouse::Cursor::Unavailable,
            Rectangle::default(),
        );
        assert!(primitive.time.is_finite());
        assert!(primitive.intensity.is_finite());
    }

    #[test]
    fn pulse_bounds_progress_and_non_finite_input() {
        assert_eq!(Pulse::default().phase(), 0.0);
        assert_eq!(
            Pulse {
                progress: f32::NAN,
                ..Pulse::default()
            }
            .phase(),
            0.0
        );
        assert_eq!(
            Pulse {
                progress: 1.4,
                ..Pulse::default()
            }
            .phase(),
            1.0
        );
    }
}
