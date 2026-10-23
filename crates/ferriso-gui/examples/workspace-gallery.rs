//! Interactive demonstration of the toolkit-neutral workspace models from
//! `ferriso-workspace`.
//!
//! Run with:
//! ```bash
//! cargo run -p ferriso-gui --example workspace-gallery
//! ```

use ferriso_gui::design::{self, ButtonKind, Skin, Surface};
use ferriso_gui::iced::{
    self, Alignment, Color, Element, Font, Length, Size, Subscription, Task, Theme,
    keyboard::{self, Key, key::Named},
    widget::{Space, button, column, container, row, scrollable, slider, text},
};
use ferriso_workspace::{PaneFocus, SplitAxis, SplitPane, TableState, Tabs};

const ROW_HEIGHT: f32 = 28.0;
const TABLE_HEIGHT: f32 = 360.0;
const ROW_COUNT: usize = 10_000;

fn main() -> iced::Result {
    iced::application(Lab::new, Lab::update, Lab::view)
        .title("Ferriso · workspace lab")
        .subscription(Lab::subscription)
        .theme(Lab::theme)
        .window(iced::window::Settings {
            size: Size::new(1080.0, 760.0),
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone)]
struct Document {
    name: String,
    kind: &'static str,
}

struct Lab {
    skin: Skin,
    tabs: Tabs<Document>,
    panes: SplitPane<&'static str>,
    table: TableState,
    next_tab: usize,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    CloseTab(usize),
    NewTab,
    ToggleAxis,
    NextTab,
    PreviousTab,
    CloseActive,
    Focus(PaneFocus),
    Ratio(f32),
    SelectRow(usize),
    TableScroll(f32),
}

impl Lab {
    fn new() -> Self {
        let mut tabs = Tabs::from_tabs([
            Document {
                name: "events.log".into(),
                kind: "log",
            },
            Document {
                name: "actors.ron".into(),
                kind: "config",
            },
            Document {
                name: "query.sql".into(),
                kind: "query",
            },
        ]);
        let _ = tabs.select(0);

        let mut table = TableState::new(ROW_HEIGHT, TABLE_HEIGHT);
        table.set_row_count(ROW_COUNT);
        table.select(0);

        let next_tab = tabs.len() + 1;
        Self {
            skin: Skin::FERRISO,
            tabs,
            panes: SplitPane::new("TABLE", "INSPECTOR", SplitAxis::Columns),
            table,
            next_tab,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(index) => {
                self.tabs.select(index);
            }
            Message::CloseTab(index) => {
                self.tabs.close(index);
            }
            Message::NewTab => {
                self.tabs.push(Document {
                    name: format!("untitled-{}", self.next_tab),
                    kind: "scratch",
                });
                self.next_tab += 1;
            }
            Message::ToggleAxis => {
                let axis = match self.panes.axis() {
                    SplitAxis::Columns => SplitAxis::Rows,
                    SplitAxis::Rows => SplitAxis::Columns,
                };
                self.panes.set_axis(axis);
            }
            Message::NextTab => {
                self.tabs.select_next();
            }
            Message::PreviousTab => {
                self.tabs.select_previous();
            }
            Message::CloseActive => {
                self.tabs.close_active();
            }
            Message::Focus(focus) => self.panes.focus_pane(focus),
            Message::Ratio(ratio) => self.panes.set_ratio(ratio),
            Message::SelectRow(index) => self.table.select(index),
            Message::TableScroll(offset) => self.table.set_scroll_offset(offset),
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };
            match key {
                Key::Named(Named::Tab) if modifiers.control() => Some(if modifiers.shift() {
                    Message::PreviousTab
                } else {
                    Message::NextTab
                }),
                Key::Character(character)
                    if modifiers.control() && character.eq_ignore_ascii_case("w") =>
                {
                    Some(Message::CloseActive)
                }
                _ => None,
            }
        })
    }

    fn theme(&self) -> Theme {
        let tokens = self.skin.tokens;
        Theme::custom(
            "ferriso-workspace",
            iced::theme::Palette {
                background: tokens.abyss,
                text: tokens.text,
                primary: tokens.accent,
                success: tokens.success,
                warning: tokens.warning,
                danger: tokens.danger,
            },
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let body = self.split_view();
        let content = column![
            self.header(),
            self.tab_strip(),
            body,
            self.pane_toolbar(),
            self.footer(),
        ]
        .spacing(12)
        .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Gradient(
                    iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
                        .add_stop(0.0, tokens.abyss)
                        .add_stop(1.0, tokens.surface)
                        .into(),
                )),
                text_color: Some(tokens.text),
                ..Default::default()
            })
            .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        row![
            column![
                text("FERRISO")
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(tokens.accent),
                text("WORKSPACE LAB").size(26).color(tokens.text),
                text("tabs · split panes · virtual rows")
                    .size(12)
                    .color(tokens.muted),
            ]
            .spacing(2),
            Space::new().width(Length::Fill),
            design::key_hint("Ctrl Tab", "switch", self.skin),
            design::key_hint("Ctrl W", "close", self.skin),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    fn tab_strip(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let mut strip = row![].spacing(4).align_y(Alignment::Center);
        for (index, document) in self.tabs.tabs().iter().enumerate() {
            let active = self.tabs.active() == Some(index);
            strip = strip.push(
                row![
                    button(
                        row![
                            text(if active { "●" } else { "○" })
                                .size(9)
                                .color(if active { tokens.accent } else { tokens.muted }),
                            text(document.name.clone())
                                .size(12)
                                .font(Font::MONOSPACE)
                                .color(if active { tokens.text } else { tokens.muted }),
                            text(document.kind).size(10).color(tokens.muted),
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 10])
                    .style(move |_, status| {
                        design::button(
                            self.skin,
                            if active {
                                ButtonKind::Soft
                            } else {
                                ButtonKind::Ghost
                            },
                            status,
                        )
                    })
                    .on_press(Message::SelectTab(index)),
                    button(text("×").size(14).color(tokens.muted))
                        .padding([2, 7])
                        .style(move |_, status| design::button(
                            self.skin,
                            ButtonKind::Ghost,
                            status
                        ))
                        .on_press(Message::CloseTab(index)),
                ]
                .spacing(1)
                .align_y(Alignment::Center),
            );
        }
        strip = strip.push(
            button(text("＋").size(16).color(tokens.accent))
                .padding([4, 10])
                .style(move |_, status| design::button(self.skin, ButtonKind::Ghost, status))
                .on_press(Message::NewTab),
        );

        container(strip)
            .width(Length::Fill)
            .padding([4, 6])
            .style(move |_| design::surface(self.skin, Surface::Inset))
            .into()
    }

    fn split_view(&self) -> Element<'_, Message> {
        let first = self.table_panel(self.panes.focus() == PaneFocus::First);
        let second = self.inspector_panel(self.panes.focus() == PaneFocus::Second);
        let ratio = (self.panes.ratio() * 1000.0).round() as u16;
        let rest = 1000 - ratio;

        match self.panes.axis() {
            SplitAxis::Columns => row![
                container(first).width(Length::FillPortion(ratio)),
                container(second).width(Length::FillPortion(rest)),
            ]
            .spacing(10)
            .height(Length::Fill)
            .into(),
            SplitAxis::Rows => column![
                container(first).height(Length::FillPortion(ratio)),
                container(second).height(Length::FillPortion(rest)),
            ]
            .spacing(10)
            .width(Length::Fill)
            .into(),
        }
    }

    fn table_panel(&self, focused: bool) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let range = self.table.visible_range();
        let mut rows = column![].spacing(0).width(Length::Fill);

        if range.start > 0 {
            rows = rows.push(
                Space::new().height(Length::Fixed(range.start as f32 * self.table.row_height())),
            );
        }
        for index in range.clone() {
            let selected = self.table.selected() == Some(index);
            rows = rows.push(
                button(row![
                    text(format!("{index:05}"))
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(tokens.muted)
                        .width(Length::Fixed(64.0)),
                    text(format!("record-{index:05}"))
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(tokens.text)
                        .width(Length::Fill),
                    text(["actor", "event", "trace"][index % 3])
                        .size(11)
                        .color(tokens.muted)
                        .width(Length::Fixed(64.0)),
                ])
                .height(Length::Fixed(self.table.row_height()))
                .width(Length::Fill)
                .padding([0, 8])
                .style(move |_, status| {
                    design::button(
                        self.skin,
                        if selected {
                            ButtonKind::Soft
                        } else {
                            ButtonKind::Ghost
                        },
                        status,
                    )
                })
                .on_press(Message::SelectRow(index)),
            );
        }
        let tail = self.table.row_count().saturating_sub(range.end);
        if tail > 0 {
            rows = rows
                .push(Space::new().height(Length::Fixed(tail as f32 * self.table.row_height())));
        }

        let list = scrollable(rows)
            .height(Length::Fixed(TABLE_HEIGHT))
            .on_scroll(|viewport| Message::TableScroll(viewport.absolute_offset().y));

        let header = row![
            design::section_label("ROWS", tokens),
            Space::new().width(Length::Fill),
            text(format!(
                "{} total · {} built",
                self.table.row_count(),
                range.len()
            ))
            .size(10)
            .color(tokens.muted),
        ]
        .align_y(Alignment::Center);

        panel(
            self.skin,
            focused,
            column![header, list, self.table_footer()].spacing(8),
        )
    }

    fn table_footer(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let selected = self
            .table
            .selected()
            .map(|index| format!("row {index:05}"))
            .unwrap_or_else(|| "no selection".into());
        row![
            text(selected).size(11).color(tokens.text),
            Space::new().width(Length::Fill),
            text(format!(
                "viewport {:.0}–{:.0}px",
                self.table.scroll_offset(),
                self.table.scroll_offset() + self.table.viewport_height()
            ))
            .size(11)
            .font(Font::MONOSPACE)
            .color(tokens.muted),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    fn inspector_panel(&self, focused: bool) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let document = self
            .tabs
            .active_tab()
            .map(|document| document.name.as_str())
            .unwrap_or("no tab");
        let selected = self.table.selected();

        panel(
            self.skin,
            focused,
            column![
                row![
                    design::section_label("INSPECTOR", tokens),
                    Space::new().width(Length::Fill),
                    text(document).size(11).color(tokens.accent),
                ]
                .align_y(Alignment::Center),
                metric("Row", selected.map_or_else(|| "—".into(), |row| format!("{row:05}")), tokens),
                metric("Kind", selected.map_or_else(|| "—".into(), |row| ["actor", "event", "trace"][row % 3].into()), tokens),
                metric("Offset", format!("{:.0}px", self.table.scroll_offset()), tokens),
                metric("Ratio", format!("{:.0}%", self.panes.ratio() * 100.0), tokens),
                Space::new().height(Length::Fill),
                text("Close the active tab and the model selects its neighbor. Split ratios clamp; the table builds only the visible rows.")
                    .size(12)
                    .color(tokens.muted),
            ]
            .spacing(10),
        )
    }

    fn pane_toolbar(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        let axis = match self.panes.axis() {
            SplitAxis::Columns => "COLUMNS",
            SplitAxis::Rows => "ROWS",
        };
        row![
            button(text(axis).size(10).font(Font::MONOSPACE))
                .padding([5, 9])
                .style(move |_, status| design::button(self.skin, ButtonKind::Soft, status))
                .on_press(Message::ToggleAxis),
            button(text("FOCUS TABLE").size(10))
                .padding([5, 9])
                .style(move |_, status| design::button(self.skin, ButtonKind::Ghost, status))
                .on_press(Message::Focus(PaneFocus::First)),
            button(text("FOCUS INSPECTOR").size(10))
                .padding([5, 9])
                .style(move |_, status| design::button(self.skin, ButtonKind::Ghost, status))
                .on_press(Message::Focus(PaneFocus::Second)),
            Space::new().width(Length::Fill),
            text("SPLIT").size(10).color(tokens.muted),
            slider(0.1..=0.9, self.panes.ratio(), Message::Ratio)
                .width(180)
                .style(move |_, _| slider::Style {
                    rail: iced::widget::slider::Rail {
                        backgrounds: (
                            iced::Background::Color(tokens.accent),
                            iced::Background::Color(design::rgba(tokens.border, 0.18)),
                        ),
                        width: 4.0,
                        border: iced::Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 2.0.into(),
                        },
                    },
                    handle: iced::widget::slider::Handle {
                        shape: iced::widget::slider::HandleShape::Circle { radius: 7.0 },
                        background: iced::Background::Color(tokens.text),
                        border_width: 2.0,
                        border_color: tokens.accent,
                    },
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn footer(&self) -> Element<'_, Message> {
        let tokens = self.skin.tokens;
        row![
            text("ferriso-workspace")
                .size(10)
                .font(Font::MONOSPACE)
                .color(tokens.accent),
            text("state models only · rendering remains application-owned")
                .size(10)
                .color(tokens.muted),
            Space::new().width(Length::Fill),
            text(format!("{} tabs", self.tabs.len()))
                .size(10)
                .color(tokens.muted),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }
}

fn panel<'a>(
    skin: Skin,
    focused: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let tokens = skin.tokens;
    container(content)
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| {
            let mut style = design::surface(skin, Surface::Glass);
            if focused {
                style.border.color = tokens.accent;
                style.border.width = 2.0;
            }
            style
        })
        .into()
}

fn metric<'a>(label: &'static str, value: String, tokens: design::Tokens) -> Element<'a, Message> {
    row![
        text(label).size(11).color(tokens.muted),
        Space::new().width(Length::Fill),
        text(value)
            .size(11)
            .font(Font::MONOSPACE)
            .color(tokens.text),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_selects_first_tab_and_first_row() {
        let lab = Lab::new();
        assert_eq!(lab.tabs.active(), Some(0));
        assert_eq!(lab.table.selected(), Some(0));
        assert_eq!(lab.table.visible_range(), 0..17);
        assert_eq!(lab.panes.ratio(), 0.5);
    }

    #[test]
    fn updates_keep_tab_and_pane_models_consistent() {
        let mut lab = Lab::new();
        let _ = lab.update(Message::CloseActive);
        assert_eq!(
            lab.tabs.active_tab().map(|document| document.name.as_str()),
            Some("actors.ron")
        );
        let _ = lab.update(Message::ToggleAxis);
        assert_eq!(lab.panes.axis(), SplitAxis::Rows);
        let _ = lab.update(Message::SelectRow(9_999));
        assert_eq!(lab.table.selected(), Some(9_999));
        assert_eq!(lab.table.scroll_offset(), 279_640.0);
    }
}
