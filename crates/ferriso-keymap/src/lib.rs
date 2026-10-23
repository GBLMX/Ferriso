//! Frontend-neutral key chords and shortcut maps for Ferriso.
//!
//! Adapters translate their platform key events into [`Chord`] values; the
//! application maps chords to its own semantic messages with [`Keymap`]. The
//! crate has no UI or runtime dependency, so the terminal and every GUI adapter
//! share the same binding syntax and matching rules.
//!
//! Bindings use `+` between modifiers and the key, for example `Ctrl+Shift+P`
//! or `ArrowLeft`. Modifier order is normalized. Plain character shortcuts are
//! case-insensitive when Shift is the only modifier; shortcuts with an explicit
//! modifier chord stay exact.
//!
//! ```
//! use ferriso_keymap::{Chord, Keymap};
//!
//! #[derive(Debug, PartialEq)]
//! enum Message { Palette }
//!
//! let mut keys = Keymap::new();
//! keys.bind_str("Ctrl+Shift+P", Message::Palette).unwrap();
//!
//! let chord: Chord = "Ctrl+Shift+P".parse().unwrap();
//! assert_eq!(keys.resolve(&chord), Some(&Message::Palette));
//! ```

#![forbid(unsafe_code)]

use std::{error::Error, fmt, str::FromStr};

/// Keyboard modifiers tracked by a shortcut chord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// Control (or the platform's control key).
    pub const CONTROL: Self = Self(1 << 0);
    /// Alt / Option.
    pub const ALT: Self = Self(1 << 1);
    /// Shift.
    pub const SHIFT: Self = Self(1 << 2);
    /// Logo / Super / Command / Windows.
    pub const LOGO: Self = Self(1 << 3);

    /// Whether Control is present.
    pub const fn control(self) -> bool {
        self.contains(Self::CONTROL)
    }

    /// Whether Alt is present.
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }

    /// Whether Shift is present.
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    /// Whether Logo is present.
    pub const fn logo(self) -> bool {
        self.contains(Self::LOGO)
    }

    /// Whether no modifier is present.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Add Control.
    pub const fn with_control(self) -> Self {
        self.union(Self::CONTROL)
    }

    /// Add Alt.
    pub const fn with_alt(self) -> Self {
        self.union(Self::ALT)
    }

    /// Add Shift.
    pub const fn with_shift(self) -> Self {
        self.union(Self::SHIFT)
    }

    /// Add Logo.
    pub const fn with_logo(self) -> Self {
        self.union(Self::LOGO)
    }

    /// Combine two modifier sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

/// A named key that has no printable character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
}

impl NamedKey {
    const fn label(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Tab => "Tab",
            Self::Space => "Space",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::Insert => "Insert",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
        }
    }
}

impl fmt::Display for NamedKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A logical key, independent of the UI toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character. The character is the layout-produced value, not
    /// a physical key code.
    Character(char),
    /// A named non-printable key.
    Named(NamedKey),
    /// A function key, `F1..=F24`.
    Function(u8),
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character('+') => formatter.write_str("Plus"),
            Self::Character(character) => write!(formatter, "{character}"),
            Self::Named(named) => write!(formatter, "{named}"),
            Self::Function(number) => write!(formatter, "F{number}"),
        }
    }
}

/// One key press with its modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The logical key.
    pub key: Key,
    /// Modifiers held for this key.
    pub modifiers: Modifiers,
}

impl Chord {
    /// Build a chord with no modifiers.
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::NONE,
        }
    }

    /// Replace the modifiers.
    pub const fn with_modifiers(self, modifiers: Modifiers) -> Self {
        Self { modifiers, ..self }
    }

    /// Parse a binding such as `Ctrl+Shift+P`, `Alt+ArrowLeft`, or `F5`.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        input.parse()
    }
}

impl From<Key> for Chord {
    fn from(key: Key) -> Self {
        Self::new(key)
    }
}

impl From<char> for Chord {
    fn from(character: char) -> Self {
        Self::new(Key::Character(character))
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (modifiers, label) in [
            (Modifiers::CONTROL, "Ctrl"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::LOGO, "Logo"),
        ] {
            if self.modifiers.contains(modifiers) {
                if !first {
                    formatter.write_str("+")?;
                }
                formatter.write_str(label)?;
                first = false;
            }
        }
        if !first {
            formatter.write_str("+")?;
        }
        write!(formatter, "{}", self.key)
    }
}

impl FromStr for Chord {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ParseError::Empty);
        }

        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(ParseError::EmptyPart(input.to_owned()));
        }

        let (key, modifiers) = parts.split_last().expect("non-empty binding");
        let mut modifiers_value = Modifiers::NONE;
        for modifier in modifiers {
            modifiers_value = modifiers_value.union(parse_modifier(modifier)?);
        }

        Ok(Self {
            key: parse_key(key)?,
            modifiers: modifiers_value,
        })
    }
}

/// A binding parse error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The binding was empty or whitespace.
    Empty,
    /// A `+` was followed by an empty part.
    EmptyPart(String),
    /// The modifier name was not recognized.
    UnknownModifier(String),
    /// The key name was not recognized.
    UnknownKey(String),
    /// A function key was outside `F1..=F24`.
    FunctionRange(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("binding is empty"),
            Self::EmptyPart(binding) => {
                write!(formatter, "binding contains an empty part: {binding:?}")
            }
            Self::UnknownModifier(modifier) => {
                write!(formatter, "unknown modifier: {modifier:?}")
            }
            Self::UnknownKey(key) => write!(formatter, "unknown key: {key:?}"),
            Self::FunctionRange(key) => {
                write!(formatter, "function key must be F1..=F24, got {key:?}")
            }
        }
    }
}

impl Error for ParseError {}

fn parse_modifier(input: &str) -> Result<Modifiers, ParseError> {
    match input.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Ok(Modifiers::CONTROL),
        "alt" | "option" => Ok(Modifiers::ALT),
        "shift" => Ok(Modifiers::SHIFT),
        "logo" | "super" | "cmd" | "command" | "win" | "windows" | "meta" => Ok(Modifiers::LOGO),
        _ => Err(ParseError::UnknownModifier(input.to_owned())),
    }
}

fn parse_key(input: &str) -> Result<Key, ParseError> {
    let normalized = input.to_ascii_lowercase();
    if normalized == "plus" {
        return Ok(Key::Character('+'));
    }
    let named = match normalized.as_str() {
        "enter" | "return" => Some(NamedKey::Enter),
        "escape" | "esc" => Some(NamedKey::Escape),
        "tab" => Some(NamedKey::Tab),
        "space" | "spacebar" => Some(NamedKey::Space),
        "backspace" | "back" => Some(NamedKey::Backspace),
        "delete" | "del" => Some(NamedKey::Delete),
        "insert" | "ins" => Some(NamedKey::Insert),
        "home" => Some(NamedKey::Home),
        "end" => Some(NamedKey::End),
        "pageup" | "pgup" => Some(NamedKey::PageUp),
        "pagedown" | "pgdn" => Some(NamedKey::PageDown),
        "arrowup" | "up" => Some(NamedKey::ArrowUp),
        "arrowdown" | "down" => Some(NamedKey::ArrowDown),
        "arrowleft" | "left" => Some(NamedKey::ArrowLeft),
        "arrowright" | "right" => Some(NamedKey::ArrowRight),
        _ => None,
    };
    if let Some(named) = named {
        return Ok(Key::Named(named));
    }

    if let Some(number) = normalized.strip_prefix('f')
        && !number.is_empty()
        && number.chars().all(|character| character.is_ascii_digit())
    {
        let number = number
            .parse::<u8>()
            .map_err(|_| ParseError::FunctionRange(input.to_owned()))?;
        return if (1..=24).contains(&number) {
            Ok(Key::Function(number))
        } else {
            Err(ParseError::FunctionRange(input.to_owned()))
        };
    }

    let mut characters = input.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        return Ok(Key::Character(character));
    }

    Err(ParseError::UnknownKey(input.to_owned()))
}

/// One shortcut binding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding<Message> {
    chord: Chord,
    message: Message,
}

impl<Message> Binding<Message> {
    /// The key chord.
    pub fn chord(&self) -> Chord {
        self.chord
    }

    /// The semantic message.
    pub fn message(&self) -> &Message {
        &self.message
    }
}

/// A frontend-neutral shortcut map.
///
/// The map is `Hash` when the message type is, so adapters can carry it
/// through iced's `Subscription::with` without capturing a closure.
#[derive(Clone, Debug, Hash)]
pub struct Keymap<Message> {
    bindings: Vec<Binding<Message>>,
}

impl<Message> Default for Keymap<Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Message> Keymap<Message> {
    /// Create an empty keymap.
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Bind a chord to a message, replacing an existing binding.
    ///
    /// Returns the previous message for a duplicate chord.
    pub fn bind(&mut self, chord: Chord, message: Message) -> Option<Message> {
        if let Some(existing) = self
            .bindings
            .iter_mut()
            .find(|binding| binding.chord == chord)
        {
            return Some(std::mem::replace(&mut existing.message, message));
        }
        self.bindings.push(Binding { chord, message });
        None
    }

    /// Parse and bind a shortcut string.
    ///
    /// Returns the previous message for a duplicate chord.
    pub fn bind_str(
        &mut self,
        binding: &str,
        message: Message,
    ) -> Result<Option<Message>, ParseError> {
        let chord = Chord::parse(binding)?;
        Ok(self.bind(chord, message))
    }

    /// Resolve a chord to its message.
    ///
    /// Plain character shortcuts are case-insensitive when Shift is the only
    /// modifier. Any explicit modifier chord must match exactly.
    pub fn resolve(&self, chord: &Chord) -> Option<&Message> {
        if let Some(message) = self.resolve_exact(chord) {
            return Some(message);
        }

        if chord.modifiers == Modifiers::SHIFT
            && let Key::Character(character) = chord.key
        {
            let character = character.to_lowercase().next().unwrap_or(character);
            return self.resolve_exact(&Chord::new(Key::Character(character)));
        }

        None
    }

    /// Iterate over bindings in insertion order.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding<Message>> {
        self.bindings.iter()
    }

    /// Number of bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether no bindings exist.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    fn resolve_exact(&self, chord: &Chord) -> Option<&Message> {
        self.bindings
            .iter()
            .find(|binding| binding.chord == *chord)
            .map(|binding| &binding.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_keys_and_function_keys() {
        assert_eq!(
            Chord::parse("Ctrl+Shift+P"),
            Ok(Chord {
                key: Key::Character('P'),
                modifiers: Modifiers::CONTROL | Modifiers::SHIFT,
            })
        );
        assert_eq!(
            Chord::parse("alt+space").map(|chord| chord.key),
            Ok(Key::Named(NamedKey::Space))
        );
        assert_eq!(
            Chord::parse("Cmd+ArrowLeft").map(|chord| chord.modifiers),
            Ok(Modifiers::LOGO)
        );
        assert_eq!(
            Chord::parse("F24").map(|chord| chord.key),
            Ok(Key::Function(24))
        );
        assert_eq!(
            Chord::parse("Ctrl+Plus").map(|chord| chord.to_string()),
            Ok("Ctrl+Plus".to_owned())
        );
    }

    #[test]
    fn display_round_trips_and_normalizes_modifier_order() {
        let chord = Chord::parse("Shift+Ctrl+Alt+Logo+P").expect("valid binding");
        assert_eq!(chord.to_string(), "Ctrl+Alt+Shift+Logo+P");
        assert_eq!(Chord::parse(&chord.to_string()), Ok(chord));
        assert_eq!(
            Chord::parse("Ctrl+Enter").map(|chord| chord.to_string()),
            Ok("Ctrl+Enter".to_owned())
        );
    }

    #[test]
    fn reports_malformed_bindings() {
        assert_eq!(Chord::parse(""), Err(ParseError::Empty));
        assert_eq!(
            Chord::parse("Ctrl++P"),
            Err(ParseError::EmptyPart("Ctrl++P".to_owned()))
        );
        assert_eq!(
            Chord::parse("Hyper+P"),
            Err(ParseError::UnknownModifier("Hyper".to_owned()))
        );
        assert_eq!(
            Chord::parse("Ctrl+Wat"),
            Err(ParseError::UnknownKey("Wat".to_owned()))
        );
        assert_eq!(
            Chord::parse("F25"),
            Err(ParseError::FunctionRange("F25".to_owned()))
        );
    }

    #[test]
    fn resolves_plain_characters_case_insensitively() {
        let mut keys = Keymap::new();
        keys.bind_str("r", "restart").expect("valid binding");
        assert_eq!(
            keys.resolve(&Chord {
                key: Key::Character('R'),
                modifiers: Modifiers::SHIFT,
            }),
            Some(&"restart")
        );
        assert_eq!(
            keys.resolve(&Chord {
                key: Key::Character('R'),
                modifiers: Modifiers::CONTROL | Modifiers::SHIFT,
            }),
            None
        );
    }

    #[test]
    fn explicit_shift_bindings_and_overrides_are_exact() {
        let mut keys = Keymap::new();
        keys.bind_str("Ctrl+P", "palette").expect("valid binding");
        keys.bind_str("Ctrl+Shift+P", "private")
            .expect("valid binding");
        assert_eq!(
            keys.resolve(&Chord::parse("Ctrl+P").expect("chord")),
            Some(&"palette")
        );
        assert_eq!(
            keys.resolve(&Chord::parse("Ctrl+Shift+P").expect("chord")),
            Some(&"private")
        );

        let previous = keys
            .bind_str("Ctrl+P", "replacement")
            .expect("valid binding");
        assert_eq!(previous, Some("palette"));
        assert_eq!(keys.len(), 2);
        assert_eq!(
            keys.resolve(&Chord::parse("Ctrl+P").expect("chord")),
            Some(&"replacement")
        );
    }
}
