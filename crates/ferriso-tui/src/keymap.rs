//! Translate Crossterm keyboard input into frontend-neutral chords.
//!
//! Applications keep their own [`ferriso_keymap::Keymap`]; this module only
//! bridges Crossterm's event types. Modifiers are preserved exactly, except
//! that Space is normalized to a named key.

use crate::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ferriso_keymap::{Chord, Key as ShortcutKey, Modifiers as ShortcutModifiers, NamedKey};

/// Convert a Crossterm key event into a [`Chord`].
///
/// Returns `None` for keys Ferriso does not model, including media keys and
/// function keys outside `F1..=F24`. Hyper/Meta modifiers are rejected rather
/// than silently folded into another modifier.
pub fn chord(event: &KeyEvent) -> Option<Chord> {
    if event.modifiers.contains(KeyModifiers::HYPER) || event.modifiers.contains(KeyModifiers::META)
    {
        return None;
    }

    let key = match event.code {
        KeyCode::Char(' ') => ShortcutKey::Named(NamedKey::Space),
        KeyCode::Char(character) => ShortcutKey::Character(character),
        KeyCode::Enter => ShortcutKey::Named(NamedKey::Enter),
        KeyCode::Esc => ShortcutKey::Named(NamedKey::Escape),
        KeyCode::Tab | KeyCode::BackTab => ShortcutKey::Named(NamedKey::Tab),
        KeyCode::Backspace => ShortcutKey::Named(NamedKey::Backspace),
        KeyCode::Delete => ShortcutKey::Named(NamedKey::Delete),
        KeyCode::Insert => ShortcutKey::Named(NamedKey::Insert),
        KeyCode::Home => ShortcutKey::Named(NamedKey::Home),
        KeyCode::End => ShortcutKey::Named(NamedKey::End),
        KeyCode::PageUp => ShortcutKey::Named(NamedKey::PageUp),
        KeyCode::PageDown => ShortcutKey::Named(NamedKey::PageDown),
        KeyCode::Up => ShortcutKey::Named(NamedKey::ArrowUp),
        KeyCode::Down => ShortcutKey::Named(NamedKey::ArrowDown),
        KeyCode::Left => ShortcutKey::Named(NamedKey::ArrowLeft),
        KeyCode::Right => ShortcutKey::Named(NamedKey::ArrowRight),
        KeyCode::F(number) if (1..=24).contains(&number) => ShortcutKey::Function(number),
        _ => return None,
    };

    let mut modifiers = ShortcutModifiers::NONE;
    if event.code == KeyCode::BackTab {
        modifiers = modifiers.with_shift();
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers = modifiers.with_control();
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        modifiers = modifiers.with_alt();
    }
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers = modifiers.with_shift();
    }
    if event.modifiers.contains(KeyModifiers::SUPER) {
        modifiers = modifiers.with_logo();
    }

    Some(Chord { key, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_named_and_character_keys() {
        assert_eq!(
            chord(&KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(Chord::parse("Ctrl+ArrowLeft").expect("valid chord"))
        );
        assert_eq!(
            chord(&KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Chord::parse("Space").expect("valid chord"))
        );
        assert_eq!(
            chord(&KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
            Some(Chord::parse("F5").expect("valid chord"))
        );
        assert_eq!(
            chord(&KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
            Some(Chord::parse("Shift+Tab").expect("valid chord"))
        );
    }

    #[test]
    fn rejects_unsupported_modifiers() {
        assert_eq!(
            chord(&KeyEvent::new(KeyCode::Char('p'), KeyModifiers::HYPER)),
            None
        );
    }
}
