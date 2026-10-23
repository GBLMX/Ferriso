//! Visual tokens and widget styles for the editor shell.

use ferriso_gui::iced::{
    Background, Border, Color, Shadow, Theme, Vector,
    widget::{button, container, text_input},
};

pub const BG: Color = Color::from_rgb(0.043, 0.055, 0.075);
pub const CANVAS: Color = Color::from_rgb(0.043, 0.055, 0.075);
pub const PANEL: Color = Color::from_rgb(0.067, 0.086, 0.118);
pub const PANEL_ALT: Color = Color::from_rgb(0.047, 0.063, 0.090);
pub const PANEL_HOVER: Color = Color::from_rgb(0.090, 0.118, 0.165);
pub const BORDER: Color = Color::from_rgb(0.133, 0.176, 0.235);
pub const BORDER_SOFT: Color = Color::from_rgb(0.086, 0.118, 0.161);
pub const TEXT: Color = Color::from_rgb(0.902, 0.929, 0.969);
pub const MUTED: Color = Color::from_rgb(0.545, 0.608, 0.706);
pub const DIM: Color = Color::from_rgb(0.365, 0.427, 0.525);
pub const ACCENT: Color = Color::from_rgb(0.471, 0.663, 1.0);
pub const ACCENT_DARK: Color = Color::from_rgb(0.105, 0.184, 0.302);
pub const ACCENT_SOFT: Color = Color::from_rgb(0.157, 0.282, 0.463);
pub const GREEN: Color = Color::from_rgb(0.431, 0.906, 0.718);
pub const AMBER: Color = Color::from_rgb(0.984, 0.749, 0.141);
pub const RED: Color = Color::from_rgb(0.984, 0.443, 0.522);
pub const SELECTION: Color = Color::from_rgb(0.118, 0.267, 0.408);
pub const CURRENT_LINE: Color = Color::from_rgb(0.075, 0.102, 0.145);
pub const GUTTER: Color = Color::from_rgb(0.337, 0.396, 0.490);

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: BORDER_SOFT,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn toolbar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: BORDER,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn editor(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CANVAS)),
        ..Default::default()
    }
}

pub fn tab(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(if active {
            PANEL_HOVER
        } else {
            PANEL_ALT
        })),
        border: Border {
            color: if active { ACCENT_SOFT } else { BORDER_SOFT },
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn card(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
        ..Default::default()
    }
}

pub fn search_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_ALT)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.30),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    }
}

pub fn ghost(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (Some(Background::Color(PANEL_HOVER)), TEXT),
        button::Status::Pressed => (Some(Background::Color(ACCENT_DARK)), ACCENT),
        button::Status::Disabled => (None, DIM),
        button::Status::Active => (None, MUTED),
    };
    button::Style {
        background,
        text_color,
        border: Border {
            color: BORDER_SOFT,
            width: 0.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn primary(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => ACCENT,
        button::Status::Pressed => Color::from_rgb(0.365, 0.545, 0.882),
        button::Status::Disabled => DIM,
        button::Status::Active => Color::from_rgb(0.286, 0.443, 0.737),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: BG,
        border: Border {
            color: background,
            width: 0.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn subtle(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, border_color) = match status {
        button::Status::Hovered => (PANEL_HOVER, TEXT, ACCENT_SOFT),
        button::Status::Pressed => (ACCENT_DARK, ACCENT, ACCENT_SOFT),
        button::Status::Disabled => (PANEL_ALT, DIM, BORDER_SOFT),
        button::Status::Active => (PANEL_ALT, MUTED, BORDER),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn active_toggle(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            let background = match status {
                button::Status::Hovered => ACCENT,
                button::Status::Pressed => Color::from_rgb(0.365, 0.545, 0.882),
                button::Status::Disabled => ACCENT_SOFT,
                button::Status::Active => Color::from_rgb(0.286, 0.443, 0.737),
            };
            button::Style {
                background: Some(Background::Color(background)),
                text_color: BG,
                border: Border {
                    color: background,
                    width: 0.0,
                    radius: 7.0.into(),
                },
                ..Default::default()
            }
        } else {
            ghost(_theme, status)
        }
    }
}

pub fn search_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => ACCENT,
        text_input::Status::Hovered => BORDER,
        text_input::Status::Disabled => BORDER_SOFT,
        text_input::Status::Active => BORDER_SOFT,
    };
    text_input::Style {
        background: Background::Color(PANEL),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 7.0.into(),
        },
        icon: MUTED,
        placeholder: DIM,
        value: TEXT,
        selection: ACCENT_SOFT,
    }
}
