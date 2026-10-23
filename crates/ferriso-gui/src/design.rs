//! Reusable visual tokens and widget styles for Ferriso GUIs.
//!
//! The adapter deliberately owns no application palette. This module provides
//! a small, coherent baseline that applications can adopt or copy: translucent
//! surfaces, state-aware buttons, input styling, chips, labels, and keyboard
//! hints. Advanced artwork lives in [`crate::art`]; interaction models live in
//! [`crate::interaction`].
//!
//! [`Skin`] couples a [`Tokens`] palette with geometry: corner radius, border
//! weight, opacity, and classic bevels. The presets cover the Ferriso/YAZI
//! palettes, Windows 11 Fluent, macOS, Windows 95, Mac OS Classic, and CDE.
//!
//! ```no_run
//! use ferriso_gui::design::{Skin, Surface, surface};
//! use ferriso_gui::iced::widget::container;
//!
//! # fn example<'a, Message: 'a>(body: ferriso_gui::iced::Element<'a, Message>) {
//! let skin = Skin::WINDOWS_11;
//! let _card = container(body).style(move |_| surface(skin, Surface::Glass));
//! # }
//! ```

use iced::{
    Color, Element, Font, Shadow, Vector,
    widget::{
        button::{Status as ButtonStatus, Style as ButtonStyle},
        container, row, text,
        text_input::{Status as InputStatus, Style as InputStyle},
    },
};

/// Semantic colors and surfaces used by the templates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// Deepest background color.
    pub abyss: Color,
    /// Default panel surface.
    pub surface: Color,
    /// Stronger panel surface for overlays and inputs.
    pub surface_strong: Color,
    /// Hairline and outline color.
    pub border: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text and metadata.
    pub muted: Color,
    /// Primary accent.
    pub accent: Color,
    /// Secondary accent for charts and highlights.
    pub accent_alt: Color,
    /// Positive state.
    pub success: Color,
    /// Caution state.
    pub warning: Color,
    /// Negative state.
    pub danger: Color,
}

impl Tokens {
    /// Deep blue, cold highlight: the player and framework palette.
    pub const FERRISO: Self = Self {
        abyss: Color::from_rgb(0.032, 0.042, 0.070),
        surface: Color::from_rgb(0.060, 0.078, 0.112),
        surface_strong: Color::from_rgb(0.082, 0.105, 0.148),
        border: Color::from_rgb(0.400, 0.640, 0.980),
        text: Color::from_rgb(0.880, 0.920, 0.980),
        muted: Color::from_rgb(0.500, 0.560, 0.670),
        accent: Color::from_rgb(0.400, 0.640, 0.980),
        accent_alt: Color::from_rgb(0.540, 0.380, 0.960),
        success: Color::from_rgb(0.360, 0.820, 0.560),
        warning: Color::from_rgb(0.960, 0.740, 0.360),
        danger: Color::from_rgb(0.960, 0.420, 0.460),
    };

    /// Ink, teal, and coral: the yazi-desktop palette.
    pub const YAZI: Self = Self {
        abyss: Color::from_rgb(0.016, 0.023, 0.038),
        surface: Color::from_rgb(0.045, 0.062, 0.090),
        surface_strong: Color::from_rgb(0.030, 0.045, 0.068),
        border: Color::from_rgb(0.420, 0.780, 0.760),
        text: Color::from_rgb(0.860, 0.920, 0.930),
        muted: Color::from_rgb(0.460, 0.550, 0.620),
        accent: Color::from_rgb(0.400, 0.900, 0.820),
        accent_alt: Color::from_rgb(0.960, 0.550, 0.720),
        success: Color::from_rgb(0.400, 0.900, 0.820),
        warning: Color::from_rgb(0.960, 0.760, 0.360),
        danger: Color::from_rgb(0.970, 0.450, 0.420),
    };

    /// Ember and steel: a warmer dashboard palette.
    pub const EMBER: Self = Self {
        abyss: Color::from_rgb(0.055, 0.035, 0.035),
        surface: Color::from_rgb(0.095, 0.060, 0.058),
        surface_strong: Color::from_rgb(0.130, 0.080, 0.076),
        border: Color::from_rgb(0.960, 0.500, 0.360),
        text: Color::from_rgb(0.960, 0.920, 0.880),
        muted: Color::from_rgb(0.640, 0.540, 0.500),
        accent: Color::from_rgb(0.960, 0.480, 0.300),
        accent_alt: Color::from_rgb(0.960, 0.740, 0.360),
        success: Color::from_rgb(0.560, 0.820, 0.520),
        warning: Color::from_rgb(0.980, 0.760, 0.340),
        danger: Color::from_rgb(0.960, 0.320, 0.360),
    };

    /// Windows 11 dark Fluent: neutral acrylic and a blue accent.
    pub const WINDOWS_11: Self = Self {
        abyss: Color::from_rgb(0.122, 0.122, 0.122),
        surface: Color::from_rgb(0.169, 0.169, 0.169),
        surface_strong: Color::from_rgb(0.196, 0.196, 0.196),
        border: Color::from_rgb(0.420, 0.420, 0.420),
        text: Color::from_rgb(1.000, 1.000, 1.000),
        muted: Color::from_rgb(0.650, 0.650, 0.650),
        accent: Color::from_rgb(0.000, 0.471, 0.831),
        accent_alt: Color::from_rgb(0.376, 0.804, 1.000),
        success: Color::from_rgb(0.424, 0.796, 0.373),
        warning: Color::from_rgb(0.988, 0.882, 0.000),
        danger: Color::from_rgb(0.769, 0.169, 0.110),
    };

    /// macOS dark: graphite, vibrancy, and system blue.
    pub const MACOS: Self = Self {
        abyss: Color::from_rgb(0.118, 0.118, 0.118),
        surface: Color::from_rgb(0.173, 0.173, 0.180),
        surface_strong: Color::from_rgb(0.227, 0.227, 0.235),
        border: Color::from_rgb(0.557, 0.557, 0.576),
        text: Color::from_rgb(0.961, 0.961, 0.969),
        muted: Color::from_rgb(0.631, 0.631, 0.651),
        accent: Color::from_rgb(0.039, 0.518, 1.000),
        accent_alt: Color::from_rgb(0.749, 0.353, 0.949),
        success: Color::from_rgb(0.196, 0.843, 0.294),
        warning: Color::from_rgb(1.000, 0.839, 0.039),
        danger: Color::from_rgb(1.000, 0.271, 0.227),
    };

    /// Windows 95: teal desktop, silver controls, navy title bars.
    pub const WINDOWS_95: Self = Self {
        abyss: Color::from_rgb(0.000, 0.502, 0.502),
        surface: Color::from_rgb(0.753, 0.753, 0.753),
        surface_strong: Color::from_rgb(0.831, 0.816, 0.784),
        border: Color::from_rgb(0.502, 0.502, 0.502),
        text: Color::from_rgb(0.000, 0.000, 0.000),
        muted: Color::from_rgb(0.165, 0.165, 0.165),
        accent: Color::from_rgb(0.000, 0.000, 0.502),
        accent_alt: Color::from_rgb(0.502, 0.000, 0.502),
        success: Color::from_rgb(0.000, 0.502, 0.000),
        warning: Color::from_rgb(0.502, 0.502, 0.000),
        danger: Color::from_rgb(0.502, 0.000, 0.000),
    };

    /// Mac OS Classic Platinum: light grey, black rules, system selection.
    pub const MAC_OS_CLASSIC: Self = Self {
        abyss: Color::from_rgb(0.498, 0.498, 0.498),
        surface: Color::from_rgb(0.867, 0.867, 0.867),
        surface_strong: Color::from_rgb(0.933, 0.933, 0.933),
        border: Color::from_rgb(0.000, 0.000, 0.000),
        text: Color::from_rgb(0.000, 0.000, 0.000),
        muted: Color::from_rgb(0.200, 0.200, 0.200),
        accent: Color::from_rgb(0.000, 0.000, 0.000),
        accent_alt: Color::from_rgb(0.333, 0.333, 0.333),
        success: Color::from_rgb(0.000, 0.392, 0.000),
        warning: Color::from_rgb(0.502, 0.376, 0.000),
        danger: Color::from_rgb(0.608, 0.000, 0.000),
    };

    /// CDE/Motif: slate desktop, beige controls, muted red accent.
    pub const CDE: Self = Self {
        abyss: Color::from_rgb(0.361, 0.486, 0.549),
        surface: Color::from_rgb(0.714, 0.714, 0.714),
        surface_strong: Color::from_rgb(0.784, 0.784, 0.784),
        border: Color::from_rgb(0.290, 0.290, 0.290),
        text: Color::from_rgb(0.000, 0.000, 0.000),
        muted: Color::from_rgb(0.145, 0.145, 0.145),
        accent: Color::from_rgb(0.545, 0.227, 0.227),
        accent_alt: Color::from_rgb(0.302, 0.427, 0.549),
        success: Color::from_rgb(0.227, 0.420, 0.227),
        warning: Color::from_rgb(0.545, 0.420, 0.165),
        danger: Color::from_rgb(0.545, 0.165, 0.165),
    };
}

impl Default for Tokens {
    fn default() -> Self {
        Self::FERRISO
    }
}

/// A palette plus the geometry and opacity of a platform style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skin {
    /// Semantic colors.
    pub tokens: Tokens,
    /// Corner radius in logical pixels. Classic skins use `0.0`.
    pub radius: f32,
    /// Border weight in logical pixels.
    pub border_width: f32,
    /// Opacity of [`Surface::Glass`] panels.
    pub glass_alpha: f32,
    /// Text color drawn on saturated accent fills.
    pub on_accent: Color,
    /// Whether surfaces should use solid light colors and light-theme inputs.
    pub light: bool,
    /// Whether controls get a raised, hard-edged classic bevel.
    pub bevel: bool,
}

impl Skin {
    const fn new(
        tokens: Tokens,
        radius: f32,
        border_width: f32,
        glass_alpha: f32,
        on_accent: Color,
        light: bool,
        bevel: bool,
    ) -> Self {
        Self {
            tokens,
            radius,
            border_width,
            glass_alpha,
            on_accent,
            light,
            bevel,
        }
    }

    /// Deep blue Ferriso style.
    pub const FERRISO: Self = Self::new(
        Tokens::FERRISO,
        14.0,
        1.0,
        0.72,
        Tokens::FERRISO.abyss,
        false,
        false,
    );

    /// Ink, teal, and coral YAZI style.
    pub const YAZI: Self = Self::new(
        Tokens::YAZI,
        14.0,
        1.0,
        0.72,
        Tokens::YAZI.abyss,
        false,
        false,
    );

    /// Warm ember dashboard style.
    pub const EMBER: Self = Self::new(Tokens::EMBER, 14.0, 1.0, 0.72, Color::WHITE, false, false);

    /// Windows 11 dark Fluent: 8 px corners, acrylic, blue accent.
    pub const WINDOWS_11: Self = Self::new(
        Tokens::WINDOWS_11,
        8.0,
        1.0,
        0.88,
        Color::WHITE,
        false,
        false,
    );

    /// macOS dark: 12 px corners, vibrancy, system blue.
    pub const MACOS: Self = Self::new(Tokens::MACOS, 12.0, 1.0, 0.58, Color::WHITE, false, false);

    /// Windows 95: square controls, 2 px borders, hard bevels.
    pub const WINDOWS_95: Self =
        Self::new(Tokens::WINDOWS_95, 0.0, 2.0, 1.0, Color::WHITE, true, true);

    /// Mac OS Classic Platinum: square, black rules, no shadow.
    pub const MAC_OS_CLASSIC: Self = Self::new(
        Tokens::MAC_OS_CLASSIC,
        0.0,
        1.0,
        1.0,
        Color::WHITE,
        true,
        false,
    );

    /// CDE/Motif: square, beige controls, hard bevels.
    pub const CDE: Self = Self::new(Tokens::CDE, 0.0, 1.0, 1.0, Color::WHITE, true, true);
}

/// A panel treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Translucent card meant to sit over artwork.
    Glass,
    /// Nearly opaque panel for dense content.
    Strong,
    /// Recessed field or list well.
    Inset,
    /// Raised panel with a soft shadow.
    Elevated,
    /// Overlay or modal surface.
    Modal,
}

/// Build a panel style from the skin.
pub fn surface(skin: Skin, kind: Surface) -> container::Style {
    let tokens = skin.tokens;

    let (background, border_alpha, shadow) = match kind {
        Surface::Glass => (
            if skin.light {
                tokens.surface
            } else {
                rgba(tokens.surface, skin.glass_alpha)
            },
            0.42,
            None,
        ),
        Surface::Strong => (
            if skin.light {
                tokens.surface
            } else {
                rgba(tokens.surface_strong, 0.94)
            },
            0.34,
            None,
        ),
        Surface::Inset => (
            if skin.light {
                tokens.surface_strong
            } else {
                rgba(tokens.abyss, 0.46)
            },
            0.18,
            None,
        ),
        Surface::Elevated => (
            if skin.light {
                tokens.surface
            } else {
                rgba(tokens.surface_strong, 0.97)
            },
            0.30,
            Some(Shadow {
                color: rgba(tokens.abyss, 0.55),
                offset: Vector::new(0.0, 10.0),
                blur_radius: 28.0,
            }),
        ),
        Surface::Modal => (
            if skin.light {
                tokens.surface
            } else {
                rgba(tokens.surface_strong, 0.98)
            },
            0.55,
            Some(Shadow {
                color: rgba(tokens.abyss, 0.72),
                offset: Vector::new(0.0, 18.0),
                blur_radius: 48.0,
            }),
        ),
    };

    let shadow = if skin.bevel {
        Shadow {
            color: rgba(Color::BLACK, 0.45),
            offset: Vector::new(2.0, 2.0),
            blur_radius: 0.0,
        }
    } else {
        shadow.unwrap_or_default()
    };

    container::Style {
        background: Some(iced::Background::Color(background)),
        border: iced::Border {
            color: if skin.light {
                tokens.border
            } else {
                rgba(tokens.border, border_alpha)
            },
            width: skin.border_width,
            radius: skin.radius.into(),
        },
        shadow,
        text_color: Some(tokens.text),
        ..Default::default()
    }
}

/// A compact labeled status pill.
pub fn chip(skin: Skin, accent: Color) -> container::Style {
    let radius = if skin.radius <= 1.0 { 0.0 } else { 999.0 };
    container::Style {
        background: Some(iced::Background::Color(rgba(accent, 0.12))),
        border: iced::Border {
            color: if skin.light {
                accent
            } else {
                rgba(accent, 0.36)
            },
            width: 1.0,
            radius: radius.into(),
        },
        text_color: Some(accent),
        ..Default::default()
    }
}

/// Button intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Transparent until hovered (a classic face on beveled skins).
    Ghost,
    /// Tinted accent background.
    Soft,
    /// Filled accent background.
    Solid,
    /// Filled danger background.
    Danger,
}

/// State-aware button style.
pub fn button(skin: Skin, kind: ButtonKind, status: ButtonStatus) -> ButtonStyle {
    let tokens = skin.tokens;
    let disabled = matches!(status, ButtonStatus::Disabled);
    let hovered = matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);

    let (background, mut text_color, border) = match kind {
        ButtonKind::Ghost if skin.light => (
            Some(iced::Background::Color(if hovered {
                mix(tokens.surface, Color::WHITE, 0.10)
            } else {
                tokens.surface
            })),
            tokens.text,
            tokens.border,
        ),
        ButtonKind::Ghost => (
            hovered.then_some(iced::Background::Color(rgba(tokens.text, 0.07))),
            tokens.text,
            Color::TRANSPARENT,
        ),
        ButtonKind::Soft if skin.light => (
            Some(iced::Background::Color(if hovered {
                mix(tokens.surface_strong, Color::WHITE, 0.10)
            } else {
                tokens.surface_strong
            })),
            tokens.text,
            tokens.border,
        ),
        ButtonKind::Soft => (
            Some(iced::Background::Color(rgba(
                tokens.accent,
                if hovered { 0.22 } else { 0.12 },
            ))),
            tokens.accent,
            rgba(tokens.accent, if hovered { 0.55 } else { 0.26 }),
        ),
        ButtonKind::Solid => (
            Some(iced::Background::Color(if hovered {
                mix(tokens.accent, Color::WHITE, 0.12)
            } else {
                tokens.accent
            })),
            skin.on_accent,
            if skin.bevel {
                tokens.border
            } else {
                Color::TRANSPARENT
            },
        ),
        ButtonKind::Danger => (
            Some(iced::Background::Color(if hovered {
                mix(tokens.danger, Color::WHITE, 0.10)
            } else {
                tokens.danger
            })),
            skin.on_accent,
            if skin.bevel {
                tokens.border
            } else {
                Color::TRANSPARENT
            },
        ),
    };

    if disabled {
        text_color = rgba(text_color, 0.42);
    }

    ButtonStyle {
        background: if disabled { None } else { background },
        text_color,
        border: iced::Border {
            color: border,
            width: if border == Color::TRANSPARENT {
                0.0
            } else {
                skin.border_width
            },
            radius: skin.radius.into(),
        },
        shadow: if skin.bevel {
            Shadow {
                color: rgba(Color::BLACK, 0.45),
                offset: Vector::new(1.0, 1.0),
                blur_radius: 0.0,
            }
        } else {
            Shadow::default()
        },
        ..Default::default()
    }
}

/// State-aware text-input style.
pub fn text_input_style(skin: Skin, status: InputStatus) -> InputStyle {
    let tokens = skin.tokens;
    let focused = matches!(status, InputStatus::Focused { .. });
    InputStyle {
        background: iced::Background::Color(if skin.light {
            Color::WHITE
        } else {
            rgba(tokens.surface_strong, if focused { 0.98 } else { 0.82 })
        }),
        border: iced::Border {
            color: if focused {
                tokens.accent
            } else if skin.light {
                tokens.border
            } else {
                rgba(tokens.border, 0.30)
            },
            width: skin.border_width,
            radius: skin.radius.into(),
        },
        icon: tokens.muted,
        placeholder: rgba(tokens.muted, 0.78),
        value: tokens.text,
        selection: rgba(tokens.accent, 0.35),
    }
}

/// A small uppercase section heading.
pub fn section_label<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: Tokens,
) -> Element<'a, Message> {
    text(label.into())
        .size(10)
        .font(Font::MONOSPACE)
        .color(tokens.muted)
        .into()
}

/// A keyboard key followed by a short action label.
pub fn key_hint<'a, Message: 'a>(
    keys: impl Into<String>,
    label: impl Into<String>,
    skin: Skin,
) -> Element<'a, Message> {
    let tokens = skin.tokens;
    let key = container(
        text(keys.into())
            .size(10)
            .font(Font::MONOSPACE)
            .color(tokens.text),
    )
    .padding([2, 6])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(if skin.light {
            tokens.surface_strong
        } else {
            rgba(tokens.text, 0.06)
        })),
        border: iced::Border {
            color: if skin.light {
                tokens.border
            } else {
                rgba(tokens.border, 0.24)
            },
            width: 1.0,
            radius: if skin.radius <= 1.0 { 0.0 } else { 5.0 }.into(),
        },
        ..Default::default()
    });

    row![key, text(label.into()).size(10).color(tokens.muted)]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

/// Mix two colors in sRGB.
pub fn mix(a: Color, b: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * amount,
        g: a.g + (b.g - a.g) * amount,
        b: a.b + (b.b - a.b) * amount,
        a: a.a + (b.a - a.a) * amount,
    }
}

/// Replace the alpha channel.
pub fn rgba(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_button_contrasts_against_its_background() {
        let style = button(Skin::FERRISO, ButtonKind::Solid, ButtonStatus::Active);
        assert_eq!(style.text_color, Skin::FERRISO.on_accent);
        assert_eq!(style.text_color, Tokens::FERRISO.abyss);
        assert!(style.background.is_some());
    }

    #[test]
    fn hover_changes_ghost_and_soft_surfaces() {
        let ghost = button(Skin::YAZI, ButtonKind::Ghost, ButtonStatus::Active);
        let hovered = button(Skin::YAZI, ButtonKind::Ghost, ButtonStatus::Hovered);
        assert!(ghost.background.is_none());
        assert!(hovered.background.is_some());

        let soft = button(Skin::YAZI, ButtonKind::Soft, ButtonStatus::Active);
        let soft_hover = button(Skin::YAZI, ButtonKind::Soft, ButtonStatus::Hovered);
        assert_ne!(soft.background, soft_hover.background);
    }

    #[test]
    fn modal_surface_is_more_opaque_and_shadowed() {
        let glass = surface(Skin::YAZI, Surface::Glass);
        let modal = surface(Skin::YAZI, Surface::Modal);
        assert!(modal.shadow.blur_radius > glass.shadow.blur_radius);
        assert_ne!(modal.background, glass.background);
    }

    #[test]
    fn light_skins_use_face_buttons() {
        let ghost = button(Skin::WINDOWS_95, ButtonKind::Ghost, ButtonStatus::Active);
        assert!(ghost.background.is_some());
        assert_eq!(ghost.border.width, 2.0);

        let soft = button(Skin::WINDOWS_95, ButtonKind::Soft, ButtonStatus::Active);
        assert_eq!(soft.text_color, Tokens::WINDOWS_95.text);
    }

    #[test]
    fn classic_skins_use_square_beveled_controls() {
        let skin = Skin::WINDOWS_95;
        assert_eq!(skin.radius, 0.0);
        assert!(skin.light);
        assert!(skin.bevel);

        let style = button(skin, ButtonKind::Solid, ButtonStatus::Active);
        assert_eq!(style.border.width, 2.0);
        assert_eq!(style.text_color, Color::WHITE);

        let panel = surface(skin, Surface::Glass);
        assert_eq!(panel.border.width, 2.0);
        assert_eq!(panel.shadow.offset, Vector::new(2.0, 2.0));
    }
}
