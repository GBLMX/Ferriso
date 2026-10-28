//! Terminal-native artwork: gradients, an animated aurora backdrop, pulses,
//! and a moving shimmer.
//!
//! These are deterministic functions of time and geometry; they never read the
//! clock, touch the actor, or allocate a widget tree. Applications drive `time`
//! from their snapshot/subscription and paint into the current [`Buffer`].
//!
//! ```no_run
//! # use ferriso_tui::art;
//! # use ratatui::Frame;
//! fn draw(frame: &mut Frame, time: f32) {
//!     art::Aurora::new(time, art::Palette::FERRISO)
//!         .render(frame.area(), frame.buffer_mut());
//! }
//! ```
//!
//! Truecolor is used directly. Terminals that negotiate a smaller palette
//! downgrade it in their own renderer; keeping the art layer on RGB makes the
//! effect independent of terminal detection.
//!
//! Text effects are cell-based. Use [`grapheme_cells`] when painting wide or
//! combined Unicode text so one grapheme keeps one style and advances by its
//! terminal display width.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

/// A grapheme cluster and its width in terminal cells.
///
/// Combining marks stay attached to their base; emoji joins are kept as one
/// cluster. A zero width is possible for control or combining-only clusters.
pub fn grapheme_cells(text: &str) -> impl Iterator<Item = (&str, usize)> + '_ {
    text.graphemes(true)
        .map(|cluster| (cluster, UnicodeWidthStr::width(cluster)))
}
/// Colors for the terminal artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Deep background.
    pub abyss: Color,
    /// Broad ribbon.
    pub glow: Color,
    /// Secondary ribbon.
    pub accent: Color,
    /// Local highlight.
    pub highlight: Color,
}

impl Palette {
    /// Ferriso's cold blue atmosphere.
    pub const FERRISO: Self = Self {
        abyss: Color::Rgb(8, 11, 18),
        glow: Color::Rgb(23, 74, 115),
        accent: Color::Rgb(61, 77, 163),
        highlight: Color::Rgb(102, 163, 250),
    };

    /// Ink, teal, and coral.
    pub const YAZI: Self = Self {
        abyss: Color::Rgb(4, 8, 14),
        glow: Color::Rgb(8, 143, 128),
        accent: Color::Rgb(133, 38, 117),
        highlight: Color::Rgb(115, 250, 217),
    };

    /// Warm charcoal with copper and gold.
    pub const EMBER: Self = Self {
        abyss: Color::Rgb(26, 15, 11),
        glow: Color::Rgb(90, 31, 13),
        accent: Color::Rgb(154, 59, 26),
        highlight: Color::Rgb(255, 180, 92),
    };
}

fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    }
}

/// Mix two colors. Non-RGB colors switch at the midpoint because terminal
/// palette entries cannot be interpolated meaningfully.
pub fn mix(from: Color, to: Color, amount: f32) -> Color {
    let amount = clamp_unit(amount);
    match (rgb(from), rgb(to)) {
        (Some((fr, fg, fb)), Some((tr, tg, tb))) => {
            let channel = |a: u8, b: u8| {
                (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8
            };
            Color::Rgb(channel(fr, tr), channel(fg, tg), channel(fb, tb))
        }
        _ if amount < 0.5 => from,
        _ => to,
    }
}

/// Direction of a [`Gradient`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Axis {
    /// Left-to-right.
    #[default]
    Horizontal,
    /// Top-to-bottom.
    Vertical,
}

/// A two-stop color ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gradient {
    pub from: Color,
    pub to: Color,
    pub axis: Axis,
}

impl Gradient {
    /// Left-to-right ramp.
    pub fn horizontal(from: Color, to: Color) -> Self {
        Self {
            from,
            to,
            axis: Axis::Horizontal,
        }
    }

    /// Top-to-bottom ramp.
    pub fn vertical(from: Color, to: Color) -> Self {
        Self {
            from,
            to,
            axis: Axis::Vertical,
        }
    }

    /// The color at a cell inside `area`.
    pub fn at(&self, area: Rect, x: u16, y: u16) -> Color {
        let (position, span) = match self.axis {
            Axis::Horizontal => (
                u32::from(x.saturating_sub(area.x)),
                u32::from(area.width.saturating_sub(1)),
            ),
            Axis::Vertical => (
                u32::from(y.saturating_sub(area.y)),
                u32::from(area.height.saturating_sub(1)),
            ),
        };
        let amount = if span == 0 {
            0.0
        } else {
            position as f32 / span as f32
        };
        mix(self.from, self.to, amount)
    }

    /// Paint the gradient as the background of every cell in `area`.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_bg(self.at(area, x, y));
                }
            }
        }
    }
}

/// An animated multi-ribbon background.
///
/// `time` is seconds; callers own the clock. The pattern is a pure function of
/// `time` and cell coordinates, which makes snapshots and tests repeatable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aurora {
    pub time: f32,
    pub palette: Palette,
    /// `0.0` is the abyss, `1.0` is full ribbon strength.
    pub intensity: f32,
    /// Ribbon movement in radians per second.
    pub speed: f32,
}

impl Aurora {
    /// Build an aurora with the default speed and full intensity.
    pub fn new(time: f32, palette: Palette) -> Self {
        Self {
            time,
            palette,
            intensity: 1.0,
            speed: 0.65,
        }
    }

    /// Override the ribbon strength.
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = clamp_unit(intensity);
        self
    }

    /// Override the movement speed.
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = if speed.is_finite() {
            speed.max(0.0)
        } else {
            0.0
        };
        self
    }

    /// Color for one cell. Coordinates outside `area` are clamped.
    pub fn color_at(&self, area: Rect, x: u16, y: u16) -> Color {
        let max_x = area.x.saturating_add(area.width.saturating_sub(1));
        let max_y = area.y.saturating_add(area.height.saturating_sub(1));
        let x = x.clamp(area.x, max_x);
        let y = y.clamp(area.top(), max_y);
        let width = f32::from(area.width.max(1));
        let height = f32::from(area.height.max(1));
        let fx = f32::from(x.saturating_sub(area.x)) / width;
        let fy = f32::from(y.saturating_sub(area.y)) / height;
        let phase = self.time * self.speed;

        let ribbon = ((fx * std::f32::consts::TAU * 1.35 + phase + fy * 3.4).sin() + 1.0) * 0.5;
        let secondary =
            ((fx * std::f32::consts::TAU * 2.2 - phase * 0.7 - fy * 5.1).sin() + 1.0) * 0.5;

        let mut color = mix(
            self.palette.abyss,
            self.palette.glow,
            ribbon * self.intensity,
        );
        if secondary > 0.62 {
            color = mix(
                color,
                self.palette.accent,
                ((secondary - 0.62) / 0.38) * self.intensity,
            );
        }
        if ribbon > 0.86 {
            color = mix(
                color,
                self.palette.highlight,
                ((ribbon - 0.86) / 0.14) * self.intensity * 0.7,
            );
        }
        color
    }

    /// Paint the aurora as background.
    pub fn render(&self, area: Rect, buffer: &mut Buffer) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_bg(self.color_at(area, x, y));
                }
            }
        }
    }
}

/// A bounded breathing envelope for focus and activity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    /// Full cycle length in seconds.
    pub period: f32,
    /// Lowest envelope value.
    pub min: f32,
    /// Highest envelope value.
    pub max: f32,
}

impl Default for Pulse {
    fn default() -> Self {
        Self {
            period: 2.4,
            min: 0.0,
            max: 1.0,
        }
    }
}

impl Pulse {
    /// The envelope at `time`, clamped to `min..=max`.
    pub fn at(&self, time: f32) -> f32 {
        if !time.is_finite() || !self.period.is_finite() || self.period <= 0.0 {
            return self.min;
        }
        let low = self.min.min(self.max);
        let high = self.min.max(self.max);
        let phase = (time / self.period * std::f32::consts::TAU).sin();
        let amount = (phase + 1.0) * 0.5;
        low + (high - low) * clamp_unit(amount)
    }

    /// Blend `base` toward `accent` using the envelope.
    pub fn color(&self, time: f32, base: Color, accent: Color) -> Color {
        mix(base, accent, self.at(time))
    }

    /// Style for one cell using the envelope as foreground intensity.
    pub fn style(&self, time: f32, base: Style, accent: Color) -> Style {
        base.fg(self.color(time, base.fg.unwrap_or(Color::Reset), accent))
    }
}

/// A highlight that travels across a run of cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shimmer {
    /// Seconds per full pass.
    pub period: f32,
    /// Width of the highlight in cells.
    pub width: f32,
    pub base: Color,
    pub highlight: Color,
}

impl Default for Shimmer {
    fn default() -> Self {
        Self {
            period: 3.0,
            width: 4.0,
            base: Color::Rgb(120, 130, 150),
            highlight: Color::Rgb(255, 255, 255),
        }
    }
}

impl Shimmer {
    /// Color for a display-cell offset in a run of `len` cells at `time`.
    ///
    /// `index` is a cell offset, not a Unicode scalar index. For text, derive
    /// offsets from [`grapheme_cells`] so wide and combined glyphs keep the
    /// highlight aligned with the terminal.
    pub fn color_at(&self, index: usize, len: usize, time: f32) -> Color {
        if len == 0 || !time.is_finite() || self.period <= 0.0 {
            return self.base;
        }
        let travel = (time / self.period).rem_euclid(1.0) * (len as f32 + self.width);
        let distance = (index as f32 - travel).abs();
        let amount = (1.0 - distance / self.width.max(0.001)).clamp(0.0, 1.0);
        mix(self.base, self.highlight, amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 8, 4)
    }

    #[test]
    fn mix_hits_both_ends_and_clamps() {
        let a = Color::Rgb(0, 10, 20);
        let b = Color::Rgb(100, 110, 120);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, -1.0), a);
        assert_eq!(mix(a, b, 2.0), b);
    }

    #[test]
    fn gradient_endpoints_are_exact() {
        let gradient = Gradient::horizontal(Color::Rgb(0, 0, 0), Color::Rgb(70, 80, 90));
        assert_eq!(gradient.at(area(), 0, 0), Color::Rgb(0, 0, 0));
        assert_eq!(gradient.at(area(), 7, 0), Color::Rgb(70, 80, 90));
    }

    #[test]
    fn aurora_is_deterministic_and_bounded() {
        let aurora = Aurora::new(1.25, Palette::FERRISO).intensity(1.5);
        assert_eq!(aurora.intensity, 1.0);
        assert_eq!(aurora.color_at(area(), 3, 2), aurora.color_at(area(), 3, 2));
        assert_eq!(
            aurora.color_at(area(), u16::MAX, u16::MAX),
            aurora.color_at(area(), 7, 3)
        );
        let Color::Rgb(r, g, b) = aurora.color_at(area(), 3, 2) else {
            panic!("aurora must use RGB");
        };
        assert!(r > 0 || g > 0 || b > 0);
    }

    #[test]
    fn pulse_and_shimmer_stay_in_range() {
        let pulse = Pulse::default();
        for step in 0..100 {
            let value = pulse.at(step as f32 * 0.1);
            assert!((0.0..=1.0).contains(&value));
        }
        let shimmer = Shimmer::default();
        for step in 0..100 {
            let color = shimmer.color_at(3, 20, step as f32 * 0.1);
            assert!(matches!(color, Color::Rgb(..)));
        }
    }
    #[test]
    fn gradients_and_aurora_write_backgrounds() {
        let area = area();
        let mut buffer = Buffer::empty(area);
        let gradient = Gradient::horizontal(Color::Rgb(1, 2, 3), Color::Rgb(4, 5, 6));
        gradient.paint(area, &mut buffer);
        assert_eq!(
            buffer.cell((0, 0)).map(|cell| cell.bg),
            Some(Color::Rgb(1, 2, 3))
        );
        assert_eq!(
            buffer.cell((7, 0)).map(|cell| cell.bg),
            Some(Color::Rgb(4, 5, 6))
        );

        let aurora = Aurora::new(2.0, Palette::FERRISO);
        let mut buffer = Buffer::empty(area);
        aurora.render(area, &mut buffer);
        assert_eq!(
            buffer.cell((3, 2)).map(|cell| cell.bg),
            Some(aurora.color_at(area, 3, 2))
        );
    }

    #[test]
    fn grapheme_cells_preserve_wide_and_joined_clusters() {
        let cells: Vec<_> = grapheme_cells("a中e\u{301}👩‍👩‍👧").collect();
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0], ("a", 1));
        assert_eq!(cells[1], ("中", 2));
        assert_eq!(cells[2], ("e\u{301}", 1));
        assert_eq!(cells[3].0, "👩‍👩‍👧");
        assert!(cells[3].1 > 0);
    }
}
