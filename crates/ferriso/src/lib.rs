//! Ferriso: one application model, any renderer.
//!
//! The contract is small. An [`App`] owns its state, answers messages with new state and
//! deferred [`Command`]s, and describes a screen with a renderer-neutral [`Node`] tree.
//! `ferriso-ratatui` and `ferriso-iced` render that same tree to a terminal and a window.
//!
//! ```no_run
//! use ferriso::prelude::*;
//!
//! struct Counter {
//!     value: i32,
//! }
//!
//! #[derive(Clone, Debug)]
//! enum Message {
//!     Increment,
//! }
//!
//! impl App for Counter {
//!     type Message = Message;
//!
//!     fn update(&mut self, message: Message) -> Command<Message> {
//!         match message {
//!             Message::Increment => self.value += 1,
//!         }
//!         Command::none()
//!     }
//!
//!     fn view(&self) -> Node<'_, Message> {
//!         column()
//!             .padding(16)
//!             .push(text(format!("count: {}", self.value)).size(20))
//!             .push(button("+1", Message::Increment))
//!             .into()
//!     }
//! }
//! ```
//!
//! ```ignore
//! ferriso_iced::run(Counter { value: 0 });
//! ferriso_ratatui::run(Counter { value: 0 })?;
//! ```

pub mod app;
pub mod command;
pub mod key;
pub mod node;
pub mod theme;

pub use app::App;
pub use command::{BoxFuture, Command};
pub use key::{Key, KeyEvent, Modifiers};
pub use node::{Node, WidgetId};
pub use theme::{Rgb, Theme, Tone};

/// Everything an application normally needs.
pub mod prelude {
    pub use crate::{
        App, Command, Node, Theme, WidgetId,
        key::{Key, KeyEvent, Modifiers},
        node::{
            Align, Button, ButtonKind, Divider, Flex, Image, ImageData, ImageFit, Length, List,
            ListItem, Padding, Progress, Qr, Scroll, Slider, Space, Surface, Text, TextInput,
            TextStyle, Weight, button, column, row, space, text,
        },
        theme::{Rgb, Tone},
    };
}
