//! Translate iced keyboard input into frontend-neutral chords.
//!
//! Applications keep their own [`ferriso_keymap::Keymap`]; this module only
//! bridges iced's event types. Modifiers are preserved exactly.

use ferriso_keymap::{Chord, Key as ShortcutKey, Modifiers as ShortcutModifiers, NamedKey};
use iced::keyboard::{Key, Modifiers, key::Named};

/// Convert an iced key and modifier state into a [`Chord`].
///
/// Returns `None` for unidentified keys, multi-character strings, and function
/// keys outside `F1..=F24`.
pub fn chord(key: &Key, modifiers: Modifiers) -> Option<Chord> {
    let key = match key {
        Key::Character(characters) => {
            let mut characters = characters.chars();
            let character = characters.next()?;
            if characters.next().is_some() {
                return None;
            }
            ShortcutKey::Character(character)
        }
        Key::Named(named) => shortcut_key(*named)?,
        Key::Unidentified => return None,
    };

    Some(Chord {
        key,
        modifiers: shortcut_modifiers(modifiers),
    })
}

fn shortcut_modifiers(modifiers: Modifiers) -> ShortcutModifiers {
    let mut shortcut = ShortcutModifiers::NONE;
    if modifiers.control() {
        shortcut = shortcut.with_control();
    }
    if modifiers.alt() {
        shortcut = shortcut.with_alt();
    }
    if modifiers.shift() {
        shortcut = shortcut.with_shift();
    }
    if modifiers.logo() {
        shortcut = shortcut.with_logo();
    }
    shortcut
}

fn shortcut_key(named: Named) -> Option<ShortcutKey> {
    let named = match named {
        Named::Enter => NamedKey::Enter,
        Named::Escape => NamedKey::Escape,
        Named::Tab => NamedKey::Tab,
        Named::Space => NamedKey::Space,
        Named::Backspace => NamedKey::Backspace,
        Named::Delete => NamedKey::Delete,
        Named::Insert => NamedKey::Insert,
        Named::Home => NamedKey::Home,
        Named::End => NamedKey::End,
        Named::PageUp => NamedKey::PageUp,
        Named::PageDown => NamedKey::PageDown,
        Named::ArrowUp => NamedKey::ArrowUp,
        Named::ArrowDown => NamedKey::ArrowDown,
        Named::ArrowLeft => NamedKey::ArrowLeft,
        Named::ArrowRight => NamedKey::ArrowRight,
        Named::F1 => return Some(ShortcutKey::Function(1)),
        Named::F2 => return Some(ShortcutKey::Function(2)),
        Named::F3 => return Some(ShortcutKey::Function(3)),
        Named::F4 => return Some(ShortcutKey::Function(4)),
        Named::F5 => return Some(ShortcutKey::Function(5)),
        Named::F6 => return Some(ShortcutKey::Function(6)),
        Named::F7 => return Some(ShortcutKey::Function(7)),
        Named::F8 => return Some(ShortcutKey::Function(8)),
        Named::F9 => return Some(ShortcutKey::Function(9)),
        Named::F10 => return Some(ShortcutKey::Function(10)),
        Named::F11 => return Some(ShortcutKey::Function(11)),
        Named::F12 => return Some(ShortcutKey::Function(12)),
        Named::F13 => return Some(ShortcutKey::Function(13)),
        Named::F14 => return Some(ShortcutKey::Function(14)),
        Named::F15 => return Some(ShortcutKey::Function(15)),
        Named::F16 => return Some(ShortcutKey::Function(16)),
        Named::F17 => return Some(ShortcutKey::Function(17)),
        Named::F18 => return Some(ShortcutKey::Function(18)),
        Named::F19 => return Some(ShortcutKey::Function(19)),
        Named::F20 => return Some(ShortcutKey::Function(20)),
        Named::F21 => return Some(ShortcutKey::Function(21)),
        Named::F22 => return Some(ShortcutKey::Function(22)),
        Named::F23 => return Some(ShortcutKey::Function(23)),
        Named::F24 => return Some(ShortcutKey::Function(24)),
        _ => return None,
    };
    Some(ShortcutKey::Named(named))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_named_and_character_keys() {
        assert_eq!(
            chord(&Key::Named(Named::ArrowLeft), Modifiers::CTRL),
            Some(Chord::parse("Ctrl+ArrowLeft").expect("valid chord"))
        );
        assert_eq!(
            chord(&Key::Character("A".into()), Modifiers::SHIFT),
            Some(Chord::parse("Shift+A").expect("valid chord"))
        );
        assert_eq!(
            chord(&Key::Named(Named::Space), Modifiers::NONE),
            Some(Chord::parse("Space").expect("valid chord"))
        );
        assert_eq!(
            chord(&Key::Named(Named::F5), Modifiers::NONE),
            Some(Chord::parse("F5").expect("valid chord"))
        );
    }

    #[test]
    fn rejects_unidentified_and_multi_character_keys() {
        assert_eq!(chord(&Key::Unidentified, Modifiers::NONE), None);
        assert_eq!(chord(&Key::Character("ab".into()), Modifiers::NONE), None);
    }
}
