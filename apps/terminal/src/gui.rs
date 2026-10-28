use ferriso_gui::iced::{
    self, Alignment, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size,
    Subscription, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        input_method::{InputMethod, Purpose},
        layout,
        widget::{Tree, tree},
    },
    keyboard::{self, Key, Modifiers, key::Named},
    mouse,
    widget::{canvas, container, text},
    window,
};
use ferriso_gui::{Renderer, Settings};

use crate::{
    app::{Message, TerminalView},
    layout::{CELL_HEIGHT, CELL_WIDTH, FONT_SIZE, grid},
};

type GuiMessage = ferriso_gui::Message<TerminalView, Message, ()>;

const BACKGROUND: Color = Color::from_rgb8(30, 30, 46);
const FOREGROUND: Color = Color::from_rgb8(205, 214, 244);

pub struct TerminalGui;

impl TerminalGui {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TerminalGui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<TerminalView, Message> for TerminalGui {
    fn view<'a>(&'a self, view: &'a TerminalView) -> Element<'a, GuiMessage> {
        if let Some(error) = &view.error {
            return container(
                text(error.clone())
                    .size(14)
                    .color(Color::from_rgb8(243, 139, 168)),
            )
            .padding(18)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let cursor = view.cursor.map(|(row, column)| {
            Rectangle::new(
                Point::new(column as f32 * CELL_WIDTH, row as f32 * CELL_HEIGHT),
                Size::new(CELL_WIDTH, CELL_HEIGHT),
            )
        });

        container(TerminalImeCanvas::new(
            TerminalCanvas { view },
            cursor,
            true,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn subscription(&self, _view: &TerminalView) -> Subscription<GuiMessage> {
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };
            input_bytes(&key, modifiers).map(|bytes| GuiMessage::User(Message::Input(bytes)))
        });
        let resize = window::resize_events().map(|(_, size)| {
            let (columns, rows) = grid(size);
            GuiMessage::User(Message::Resize { columns, rows })
        });
        let ime = iced::event::listen_with(|event, _, _| match event {
            Event::InputMethod(iced::advanced::input_method::Event::Commit(text)) => {
                Some(GuiMessage::User(Message::Input(text.into_bytes())))
            }
            _ => None,
        });
        Subscription::batch([keys, resize, ime])
    }
}

struct TerminalCanvas<'a> {
    view: &'a TerminalView,
}

impl canvas::Program<GuiMessage> for TerminalCanvas<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), BACKGROUND);

        for (row_index, row) in self.view.rows.iter().enumerate() {
            for (column_index, cell) in row.iter().enumerate() {
                let x = column_index as f32 * CELL_WIDTH;
                let y = row_index as f32 * CELL_HEIGHT;
                let rect = Rectangle::new(Point::new(x, y), Size::new(CELL_WIDTH, CELL_HEIGHT));
                let background = rgb(cell.background);
                if background != BACKGROUND {
                    frame.fill_rectangle(rect.position(), rect.size(), background);
                }

                if cell.character != '\0' && cell.character != ' ' {
                    frame.fill_text(canvas::Text {
                        content: cell.character.to_string(),
                        position: Point::new(x + CELL_WIDTH * 0.5, y + CELL_HEIGHT * 0.5),
                        color: rgb(cell.foreground),
                        size: Pixels(FONT_SIZE),
                        font: Font::MONOSPACE,
                        align_x: Alignment::Center.into(),
                        align_y: iced::alignment::Vertical::Center,
                        ..canvas::Text::default()
                    });
                }
            }
        }

        if let Some((row, column)) = self.view.cursor {
            frame.fill_rectangle(
                Point::new(column as f32 * CELL_WIDTH, row as f32 * CELL_HEIGHT),
                Size::new(CELL_WIDTH, CELL_HEIGHT),
                Color {
                    a: 0.35,
                    ..FOREGROUND
                },
            );
        }

        vec![frame.into_geometry()]
    }
}

struct TerminalImeCanvas<'a> {
    inner: canvas::Canvas<TerminalCanvas<'a>, GuiMessage>,
    cursor: Option<Rectangle>,
    enabled: bool,
}

impl<'a> TerminalImeCanvas<'a> {
    fn new(program: TerminalCanvas<'a>, cursor: Option<Rectangle>, enabled: bool) -> Self {
        Self {
            inner: canvas::Canvas::new(program)
                .width(Length::Fill)
                .height(Length::Fill),
            cursor,
            enabled,
        }
    }
}

impl Widget<GuiMessage, Theme, iced::Renderer> for TerminalImeCanvas<'_> {
    fn tag(&self) -> tree::Tag {
        self.inner.tag()
    }

    fn state(&self) -> tree::State {
        self.inner.state()
    }

    fn size(&self) -> Size<Length> {
        self.inner.size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner.layout(tree, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.inner
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, GuiMessage>,
        viewport: &Rectangle,
    ) {
        self.inner.update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );

        if !self.enabled {
            shell.request_input_method(&InputMethod::<&str>::Disabled);
            return;
        }

        let Some(cursor) = self.cursor else {
            shell.request_input_method(&InputMethod::<&str>::Disabled);
            return;
        };

        let bounds = layout.bounds();
        shell.request_input_method(&InputMethod::<&str>::Enabled {
            cursor: Rectangle::new(
                Point::new(bounds.x + cursor.x, bounds.y + cursor.y),
                cursor.size(),
            ),
            purpose: Purpose::Normal,
            preedit: None,
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.inner
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn children(&self) -> Vec<Tree> {
        self.inner.children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.inner.diff(tree);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        self.inner.operate(tree, layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, GuiMessage, Theme, iced::Renderer>> {
        self.inner
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<TerminalImeCanvas<'a>> for Element<'a, GuiMessage> {
    fn from(canvas: TerminalImeCanvas<'a>) -> Self {
        Element::new(canvas)
    }
}

fn rgb(color: (u8, u8, u8)) -> Color {
    Color::from_rgb8(color.0, color.1, color.2)
}

pub(crate) fn input_bytes(key: &Key, modifiers: Modifiers) -> Option<Vec<u8>> {
    if let Key::Character(character) = key {
        let bytes = character.as_bytes();
        if modifiers.control() {
            let byte = match *bytes.first()? {
                b'a'..=b'z' => bytes[0] & 0x1f,
                b'A'..=b'Z' => bytes[0] & 0x1f,
                b'[' => 0x1b,
                b'\\' => 0x1c,
                b']' => 0x1d,
                b'^' => 0x1e,
                b'_' => 0x1f,
                b'@' | b' ' => 0,
                byte => byte,
            };
            return Some(vec![byte]);
        }

        let mut output = bytes.to_vec();
        if modifiers.alt() {
            output.insert(0, 0x1b);
        }
        return Some(output);
    }

    let Key::Named(named) = key else {
        return None;
    };

    let bytes: &[u8] = match named {
        Named::Enter => b"\r",
        Named::Backspace => b"\x7f",
        Named::Tab => b"\t",
        Named::Space => b" ",
        Named::Escape => b"\x1b",
        Named::ArrowUp => b"\x1b[A",
        Named::ArrowDown => b"\x1b[B",
        Named::ArrowRight => b"\x1b[C",
        Named::ArrowLeft => b"\x1b[D",
        Named::Home => b"\x1b[H",
        Named::End => b"\x1b[F",
        Named::PageUp => b"\x1b[5~",
        Named::PageDown => b"\x1b[6~",
        Named::Insert => b"\x1b[2~",
        Named::Delete => b"\x1b[3~",
        Named::F1 => b"\x1bOP",
        Named::F2 => b"\x1bOQ",
        Named::F3 => b"\x1bOR",
        Named::F4 => b"\x1bOS",
        Named::F5 => b"\x1b[15~",
        Named::F6 => b"\x1b[17~",
        Named::F7 => b"\x1b[18~",
        Named::F8 => b"\x1b[19~",
        Named::F9 => b"\x1b[20~",
        Named::F10 => b"\x1b[21~",
        Named::F11 => b"\x1b[23~",
        Named::F12 => b"\x1b[24~",
        _ => return None,
    };
    Some(bytes.to_vec())
}

pub fn run() -> ferriso_gui::iced::Result {
    ferriso_gui::run(
        crate::app::TerminalApp::new(),
        TerminalGui::new(),
        Settings {
            title: "Ferriso · Terminal".into(),
            size: (1100.0, 760.0),
            ..Settings::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_control_keys_map_to_terminal_bytes() {
        assert_eq!(
            input_bytes(&Key::Character("a".into()), Modifiers::default()),
            Some(b"a".to_vec())
        );
        assert_eq!(
            input_bytes(&Key::Character("c".into()), Modifiers::CTRL),
            Some(vec![0x03])
        );
        assert_eq!(
            input_bytes(&Key::Named(Named::Enter), Modifiers::default()),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            input_bytes(&Key::Named(Named::Space), Modifiers::default()),
            Some(b" ".to_vec())
        );
    }

    #[test]
    fn arrow_keys_emit_csi_sequences() {
        assert_eq!(
            input_bytes(&Key::Named(Named::ArrowUp), Modifiers::default()),
            Some(b"\x1b[A".to_vec())
        );
    }
}
