//! Semantic colours. A renderer resolves these against the active [`Theme`], so the same
//! view description is legible on a terminal palette and in a GPU window.

/// A colour role, not a colour value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    /// Primary foreground text.
    Text,
    /// Secondary text, hints, timestamps.
    Muted,
    /// Interactive emphasis: playing row, active navigation item, focused control.
    Accent,
    /// Text drawn on top of [`Tone::Accent`].
    OnAccent,
    Background,
    Surface,
    Raised,
    Border,
    Success,
    Warning,
    Danger,
}

/// 8-bit sRGB. Alpha is deliberately absent: GUI surfaces are opaque and terminals have no
/// alpha channel, so a translucent value could never round-trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn hex(value: u32) -> Self {
        Self {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        }
    }

    pub const WHITE: Self = Self::new(0xff, 0xff, 0xff);
    pub const BLACK: Self = Self::new(0, 0, 0);
}

/// Every colour the view tree can name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub background: Rgb,
    pub surface: Rgb,
    pub raised: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub border: Rgb,
    pub success: Rgb,
    pub warning: Rgb,
    pub danger: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            name: "ferriso-dark",
            background: Rgb::hex(0x131417),
            surface: Rgb::hex(0x1b1d21),
            raised: Rgb::hex(0x25282e),
            text: Rgb::hex(0xe6e8ec),
            muted: Rgb::hex(0x9aa0aa),
            accent: Rgb::hex(0xec4141),
            on_accent: Rgb::WHITE,
            border: Rgb::hex(0x33363d),
            success: Rgb::hex(0x52c785),
            warning: Rgb::hex(0xf2b34b),
            danger: Rgb::hex(0xf25a5a),
        }
    }

    pub const fn light() -> Self {
        Self {
            name: "ferriso-light",
            background: Rgb::hex(0xf6f7f9),
            surface: Rgb::hex(0xffffff),
            raised: Rgb::hex(0xe9ecf1),
            text: Rgb::hex(0x1d2025),
            muted: Rgb::hex(0x626a77),
            accent: Rgb::hex(0xd1242f),
            on_accent: Rgb::WHITE,
            border: Rgb::hex(0xd4d9e1),
            success: Rgb::hex(0x1f8a51),
            warning: Rgb::hex(0x9a6400),
            danger: Rgb::hex(0xc62828),
        }
    }

    pub fn resolve(&self, tone: Tone) -> Rgb {
        match tone {
            Tone::Text => self.text,
            Tone::Muted => self.muted,
            Tone::Accent => self.accent,
            Tone::OnAccent => self.on_accent,
            Tone::Background => self.background,
            Tone::Surface => self.surface,
            Tone::Raised => self.raised,
            Tone::Border => self.border,
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Danger => self.danger,
        }
    }

    /// The foreground that reads on `tone`. Used when a filled surface needs text and the
    /// caller did not name a colour; it keeps contrast correct without guessing in a renderer.
    pub fn contrast_on(&self, tone: Tone) -> Rgb {
        let bg = self.resolve(tone);
        // Rec. 709 luma is enough to pick between the two anchors we actually use.
        let luma = 0.2126 * f32::from(bg.r) + 0.7152 * f32::from(bg.g) + 0.0722 * f32::from(bg.b);
        if luma > 150.0 { Rgb::BLACK } else { Rgb::WHITE }
    }
}
