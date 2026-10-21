//! Renderer-neutral key events.

use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        control: false,
        shift: false,
        alt: false,
        logo: false,
    };

    pub const CONTROL: Self = Self {
        control: true,
        ..Self::NONE
    };

    /// Shortcuts that a renderer must not consume for widget navigation. On macOS `command`
    /// is the logo key, elsewhere control is the usual accelerator.
    pub fn is_shortcut(self) -> bool {
        self.control || self.logo || self.alt
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Character(String),
    Space,
    Enter,
    Tab,
    BackTab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Other,
}

impl Key {
    pub fn character(&self) -> Option<&str> {
        match self {
            Self::Character(text) => Some(text),
            Self::Space => Some(" "),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    /// False for key releases, so an app can ignore them without inspecting the renderer.
    pub pressed: bool,
    pub repeat: bool,
    /// Text produced by the key, when the platform knows it. For a character key this is the
    /// input-method result, which can differ from the physical key.
    pub text: Option<String>,
}

impl KeyEvent {
    pub fn press(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            pressed: true,
            repeat: false,
            text: None,
        }
    }

    pub fn is_press(&self) -> bool {
        self.pressed
    }

    pub fn shortcut(&self) -> bool {
        self.modifiers.is_shortcut()
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref().or_else(|| self.key.character())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(text) => write!(f, "{text}"),
            Self::Space => write!(f, "space"),
            Self::Enter => write!(f, "enter"),
            Self::Tab => write!(f, "tab"),
            Self::BackTab => write!(f, "shift+tab"),
            Self::Escape => write!(f, "escape"),
            Self::Backspace => write!(f, "backspace"),
            Self::Delete => write!(f, "delete"),
            Self::Insert => write!(f, "insert"),
            Self::Left => write!(f, "left"),
            Self::Right => write!(f, "right"),
            Self::Up => write!(f, "up"),
            Self::Down => write!(f, "down"),
            Self::Home => write!(f, "home"),
            Self::End => write!(f, "end"),
            Self::PageUp => write!(f, "page-up"),
            Self::PageDown => write!(f, "page-down"),
            Self::Function(number) => write!(f, "f{number}"),
            Self::Other => write!(f, "other"),
        }
    }
}
