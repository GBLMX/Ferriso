use ferriso_gui::iced::{
    Background, Color, Element, Length, Subscription,
    keyboard::{self, Key, key::Named},
    widget::{button, column, container, row, text, text_input},
    window,
};
use ferriso_gui::{Renderer, Settings};

use crate::app::{DataWorkbench, Message, WorkbenchView};

type GuiMessage = ferriso_gui::Message<WorkbenchView, Message, ()>;

const GUI_ROW_HEIGHT: f32 = 30.0;
const GUI_CHROME_HEIGHT: f32 = 200.0;

fn viewport_rows(height: f32) -> usize {
    ((height - GUI_CHROME_HEIGHT).max(GUI_ROW_HEIGHT) / GUI_ROW_HEIGHT) as usize
}

pub struct DataWorkbenchGui;

impl DataWorkbenchGui {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for DataWorkbenchGui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<WorkbenchView, Message> for DataWorkbenchGui {
    fn view<'a>(&'a self, view: &'a WorkbenchView) -> Element<'a, GuiMessage> {
        let heading = row![
            text("DATA WORKBENCH")
                .size(22)
                .color(Color::from_rgb(0.9, 0.9, 0.9)),
            text(format!("{} of {} rows", view.filtered, view.total))
                .size(12)
                .color(Color::from_rgb(0.55, 0.6, 0.7)),
        ]
        .spacing(14)
        .align_y(ferriso_gui::iced::Alignment::Center);

        let filter = text_input("Filter by title, author, loader, or status", &view.query)
            .on_input(|query| GuiMessage::User(Message::QueryChanged(query)))
            .padding(8)
            .width(Length::Fill);

        let header = row![
            text("ID").size(11).width(Length::Fixed(56.0)),
            text("TITLE").size(11).width(Length::FillPortion(4)),
            text("AUTHOR").size(11).width(Length::Fixed(112.0)),
            text("KIND").size(11).width(Length::Fixed(110.0)),
            text("STATUS").size(11).width(Length::Fixed(100.0)),
            text("DOWNLOADS").size(11).width(Length::Fixed(92.0)),
            text("UPDATED").size(11).width(Length::Fixed(86.0)),
        ]
        .spacing(8)
        .padding(6);

        let mut rows = column![].spacing(1);
        let viewport_end = view.viewport_start.saturating_add(view.viewport_rows);
        for data in view
            .rows
            .iter()
            .filter(|data| data.index >= view.viewport_start && data.index < viewport_end)
        {
            let selected = data.selected;
            let content = row![
                text(format!("{:05}", data.id))
                    .size(11)
                    .width(Length::Fixed(56.0)),
                text(data.title.clone())
                    .size(11)
                    .width(Length::FillPortion(4)),
                text(data.author.clone())
                    .size(11)
                    .width(Length::Fixed(112.0)),
                text(data.kind.clone()).size(11).width(Length::Fixed(110.0)),
                text(data.status.clone())
                    .size(11)
                    .width(Length::Fixed(100.0)),
                text(format!("{:>9}", data.downloads))
                    .size(11)
                    .width(Length::Fixed(92.0)),
                text(data.updated.clone())
                    .size(11)
                    .width(Length::Fixed(86.0)),
            ]
            .spacing(8)
            .padding(4);
            rows = rows.push(
                button(content)
                    .width(Length::Fill)
                    .height(Length::Fixed(30.0))
                    .style(move |_, _| ferriso_gui::iced::widget::button::Style {
                        background: Some(Background::Color(if selected {
                            Color::from_rgb(0.18, 0.55, 0.62)
                        } else {
                            Color::from_rgb(0.09, 0.11, 0.15)
                        })),
                        text_color: if selected {
                            Color::BLACK
                        } else {
                            Color::from_rgb(0.85, 0.88, 0.92)
                        },
                        border: ferriso_gui::iced::Border {
                            color: if selected {
                                Color::from_rgb(0.3, 0.9, 0.9)
                            } else {
                                Color::from_rgb(0.18, 0.21, 0.28)
                            },
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..Default::default()
                    })
                    .on_press(GuiMessage::User(Message::Select(data.index))),
            );
        }

        let footer = text(format!(
            "{}  •  ↑/↓ move  PgUp/PgDn page  Home/End  Esc clear",
            view.status
        ))
        .size(11)
        .color(Color::from_rgb(0.5, 0.55, 0.65));

        container(
            column![heading, filter, header, rows, footer]
                .spacing(10)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18)
        .into()
    }

    fn subscription(&self, _view: &WorkbenchView) -> Subscription<GuiMessage> {
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };
            if modifiers.control() || modifiers.alt() || modifiers.logo() {
                return None;
            }
            let message = match key {
                Key::Named(Named::ArrowDown) => Message::Move(1),
                Key::Named(Named::ArrowUp) => Message::Move(-1),
                Key::Named(Named::PageDown) => Message::Page(1),
                Key::Named(Named::PageUp) => Message::Page(-1),
                Key::Named(Named::Home) => Message::First,
                Key::Named(Named::End) => Message::Last,
                Key::Named(Named::Escape) => Message::ClearFilter,
                _ => return None,
            };
            Some(GuiMessage::User(message))
        });
        let resize = window::resize_events()
            .map(|(_, size)| GuiMessage::User(Message::Viewport(viewport_rows(size.height))));
        Subscription::batch([keys, resize])
    }
}

pub fn run() -> ferriso_gui::iced::Result {
    ferriso_gui::run(
        DataWorkbench::new(),
        DataWorkbenchGui::new(),
        Settings {
            title: "Ferriso · Data Workbench".into(),
            size: (1100.0, 760.0),
            ..Settings::default()
        },
    )
}
