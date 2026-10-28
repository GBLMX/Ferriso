//! Canvas drawing and mouse translation for the editor.

use ferriso_editor::{EditorView, TokenKind};
use ferriso_gui::iced::{
    self, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        input_method::{InputMethod, Purpose},
        layout,
        widget::{Tree, tree},
    },
    alignment, mouse,
    widget::canvas,
};

use crate::{app::Message, model::EditorMessage, theme};

pub const CELL_WIDTH: f32 = 8.4;
pub const LINE_HEIGHT: f32 = 19.0;
pub const TEXT_TOP: f32 = 8.0;
pub const GUTTER_LEFT: f32 = 12.0;
pub const GUTTER_WIDTH: f32 = 54.0;
pub const TEXT_LEFT: f32 = GUTTER_LEFT + GUTTER_WIDTH;

/// Persistent drag and scroll state owned by the canvas widget.
#[derive(Default)]
pub struct DragState {
    dragging: bool,
    scroll_columns: f32,
    scroll_rows: f32,
}

/// A canvas program that paints one immutable editor snapshot.
pub struct EditorCanvas<'a> {
    pub view: &'a EditorView,
    pub preedit: Option<&'a str>,
    /// Whether to paint the empty-buffer welcome background.
    pub welcome: bool,
}

impl canvas::Program<Message> for EditorCanvas<'_> {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                state.dragging = true;
                Some(
                    canvas::Action::publish(Message::User(EditorMessage::Pointer {
                        row: self.row_at(point.y),
                        column: self.column_at(point.x),
                        extend: false,
                    }))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                let point = cursor.position_from(Point::new(bounds.x, bounds.y))?;
                Some(
                    canvas::Action::publish(Message::User(EditorMessage::Pointer {
                        row: self.row_at(point.y.max(TEXT_TOP)),
                        column: self.column_at(point.x.max(TEXT_LEFT)),
                        extend: true,
                    }))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let (columns, rows) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x, -*y),
                    mouse::ScrollDelta::Pixels { x, y } => (*x / CELL_WIDTH, -*y / LINE_HEIGHT),
                };
                state.scroll_columns += columns;
                state.scroll_rows += rows;
                let columns = state.scroll_columns.trunc() as isize;
                let rows = state.scroll_rows.trunc() as isize;
                state.scroll_columns -= columns as f32;
                state.scroll_rows -= rows as f32;
                if columns == 0 && rows == 0 {
                    return Some(canvas::Action::capture());
                }
                Some(
                    canvas::Action::publish(Message::User(EditorMessage::Scroll { columns, rows }))
                        .and_capture(),
                )
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::new(0.0, 0.0), bounds.size(), theme::CANVAS);
        let text_area = Rectangle::new(
            Point::new(TEXT_LEFT, 0.0),
            Size::new((bounds.width - TEXT_LEFT).max(0.0), bounds.height),
        );
        if self.welcome {
            return vec![frame.into_geometry()];
        }

        frame.fill_rectangle(
            Point::new(GUTTER_LEFT + GUTTER_WIDTH, 0.0),
            Size::new(1.0, bounds.height),
            theme::BORDER_SOFT,
        );

        let visible_rows = ((bounds.height - TEXT_TOP) / LINE_HEIGHT).ceil() as usize;

        for (row_index, row) in self.view.rows.iter().take(visible_rows).enumerate() {
            let y = TEXT_TOP + row_index as f32 * LINE_HEIGHT;
            if row.cursor.is_some() {
                frame.fill_rectangle(
                    Point::new(GUTTER_LEFT, y),
                    Size::new((bounds.width - GUTTER_LEFT).max(0.0), LINE_HEIGHT),
                    theme::CURRENT_LINE,
                );
            }

            if row.continuation {
                frame.fill_text(canvas::Text {
                    content: "↳".to_string(),
                    position: Point::new(GUTTER_LEFT + GUTTER_WIDTH - 10.0, y + 1.0),
                    color: theme::DIM,
                    size: Pixels(12.0),
                    font: Font::MONOSPACE,
                    align_x: alignment::Horizontal::Right.into(),
                    ..canvas::Text::default()
                });
            } else {
                let number = canvas::Text {
                    content: row.number.to_string(),
                    position: Point::new(GUTTER_LEFT + GUTTER_WIDTH - 10.0, y + 1.0),
                    color: if row.cursor.is_some() {
                        theme::ACCENT
                    } else {
                        theme::GUTTER
                    },
                    size: Pixels(12.0),
                    font: Font::MONOSPACE,
                    align_x: alignment::Horizontal::Right.into(),
                    ..canvas::Text::default()
                };
                frame.fill_text(number);
            }

            if let Some((start, end)) = row.selection {
                let left =
                    TEXT_LEFT + (start.saturating_sub(self.view.scroll_col)) as f32 * CELL_WIDTH;
                let right =
                    TEXT_LEFT + (end.saturating_sub(self.view.scroll_col)) as f32 * CELL_WIDTH;
                let left = left.max(TEXT_LEFT);
                let right = right.min(bounds.width);
                if right > left {
                    frame.fill_rectangle(
                        Point::new(left, y),
                        Size::new(right - left, LINE_HEIGHT),
                        theme::SELECTION,
                    );
                }
            }

            for run in &row.runs {
                let x = TEXT_LEFT
                    + (run.column.saturating_sub(self.view.scroll_col)) as f32 * CELL_WIDTH;
                if x >= bounds.width || x + run.width as f32 * CELL_WIDTH < TEXT_LEFT {
                    continue;
                }
                frame.with_clip(text_area, |frame| {
                    frame.fill_text(canvas::Text {
                        content: run.text.clone(),
                        position: Point::new(x, y + 1.0),
                        color: token_color(run.kind),
                        size: Pixels(14.0),
                        font: Font::MONOSPACE,
                        ..canvas::Text::default()
                    });
                });
            }

            if let Some(column) = row.cursor {
                let x =
                    TEXT_LEFT + (column.saturating_sub(self.view.scroll_col)) as f32 * CELL_WIDTH;
                if let Some(preedit) = self.preedit.filter(|text| !text.is_empty()) {
                    frame.with_clip(text_area, |frame| {
                        frame.fill_text(canvas::Text {
                            content: preedit.to_string(),
                            position: Point::new(x + 2.0, y + 1.0),
                            color: theme::TEXT,
                            size: Pixels(14.0),
                            font: Font::MONOSPACE,
                            ..canvas::Text::default()
                        });
                        let width = preedit.chars().count() as f32 * CELL_WIDTH;
                        frame.fill_rectangle(
                            Point::new(x + 2.0, y + LINE_HEIGHT - 4.0),
                            Size::new(width.max(CELL_WIDTH), 1.5),
                            theme::ACCENT,
                        );
                    });
                }
                if x >= TEXT_LEFT && x + 2.0 <= bounds.width {
                    frame.fill_rectangle(
                        Point::new(x, y + 1.0),
                        Size::new(2.0, LINE_HEIGHT - 3.0),
                        theme::ACCENT,
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }
}

impl EditorCanvas<'_> {
    fn row_at(&self, y: f32) -> usize {
        ((y - TEXT_TOP).max(0.0) / LINE_HEIGHT).floor() as usize
    }

    fn column_at(&self, x: f32) -> usize {
        ((x - TEXT_LEFT).max(0.0) / CELL_WIDTH).round() as usize
    }
}

fn token_color(kind: TokenKind) -> Color {
    match kind {
        TokenKind::Plain => theme::TEXT,
        TokenKind::Comment => Color::from_rgb(0.420, 0.600, 0.360),
        TokenKind::String => Color::from_rgb(0.810, 0.580, 0.420),
        TokenKind::Number => Color::from_rgb(0.710, 0.810, 0.560),
        TokenKind::Keyword => Color::from_rgb(0.780, 0.530, 0.760),
        TokenKind::Function => Color::from_rgb(0.860, 0.860, 0.670),
        TokenKind::Type => Color::from_rgb(0.360, 0.790, 0.690),
        TokenKind::Variable => Color::from_rgb(0.610, 0.760, 0.950),
        TokenKind::Constant => Color::from_rgb(0.760, 0.680, 0.980),
        TokenKind::Markup => Color::from_rgb(0.860, 0.680, 0.520),
    }
}

/// A transparent wrapper around the text canvas that advertises an input
/// method cursor and preedit to the runtime. `canvas::Canvas` has no widget-level
/// input-method hook, so this wrapper delegates all widget behavior and adds the
/// IME request during redraws.
pub struct ImeCanvas<'a> {
    inner: canvas::Canvas<EditorCanvas<'a>, Message>,
    cursor: Option<Rectangle>,
    enabled: bool,
}

impl<'a> ImeCanvas<'a> {
    pub fn new(program: EditorCanvas<'a>, cursor: Option<Rectangle>, enabled: bool) -> Self {
        Self {
            inner: canvas::Canvas::new(program)
                .width(Length::Fill)
                .height(Length::Fill),
            cursor,
            enabled,
        }
    }
}

impl<'a> Widget<Message, Theme, iced::Renderer> for ImeCanvas<'a> {
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
        shell: &mut Shell<'_, Message>,
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
        // The canvas draws the composition text itself, so the runtime only
        // needs the caret rectangle and purpose. Passing the preedit here as
        // well would paint it twice.
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
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.inner
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<ImeCanvas<'a>> for Element<'a, Message> {
    fn from(canvas: ImeCanvas<'a>) -> Self {
        Element::new(canvas)
    }
}
