//! The yazi-desktop iced view.
//!
//! Presentation only: the actor snapshot describes entries and preview data;
//! this module turns it into a glassy three-pane window with an animated WGSL
//! backdrop and a canvas orbit chart.

use std::{sync::Mutex, time::Duration};

use ferriso_diagnostics::{
    Diagnostics, EventKind, ReportMeta, safe_environment, state_dir, unix_seconds,
};
use ferriso_gui::art::{self, Aurora, Palette as AuroraPalette};
use ferriso_gui::iced::{
    self, Alignment, Background, Border, Color, ContentFit, Element, Font, Length, Size, Theme,
    keyboard::{self, Key, key::Named},
    widget::{
        Id, Space, Stack, button, container, image, mouse_area, pane_grid, rule, scrollable, text,
        text_input,
    },
};
use ferriso_gui::{Message as GuiMessage, Renderer};

use crate::app::{Kind, Message, ViewModel};

type Msg = GuiMessage<ViewModel, Message, Local>;

/// Renderer-local messages: animation frames and keyboard decisions.
#[derive(Clone, Debug)]
pub enum Local {
    Frame,
    Key(KeyInput),
    PaneResized(pane_grid::ResizeEvent),
    WindowResized(Size),
}

/// Which region of the split layout a pane represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSlot {
    Files,
    Preview,
}

/// Keyboard input interpreted against the current focus state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Up,
    Down,
    Enter,
    Escape,
    Backspace,
    Home,
    Back,
    ToggleHidden,
    Refresh,
    FocusFilter,
    ToggleDebug,
    ExportDiagnostics,
    TogglePreview,
    Quit,
}

/// The iced renderer for yazi-desktop.
pub struct YaziGui {
    time: f32,
    speed: f32,
    intensity: f32,
    animation: bool,
    filter_active: bool,
    filter_id: Id,
    diagnostics: Option<Diagnostics>,
    debug_visible: bool,
    config_summary: String,
    notice: Option<String>,
    panes: pane_grid::State<PaneSlot>,
    preview_visible: bool,
    preview_ratio: f32,
    window_width: f32,
    requested_width: u32,
    image_cache: Mutex<Option<(u64, image::Handle)>>,
}

impl YaziGui {
    pub fn new(art: &crate::config::Art) -> Self {
        Self {
            time: 0.0,
            speed: art.speed,
            intensity: art.intensity,
            animation: art.enabled,
            filter_active: false,
            filter_id: Id::new("yazi-desktop-filter"),
            diagnostics: None,
            debug_visible: false,
            config_summary: String::new(),
            notice: None,
            panes: two_pane_grid(),
            preview_visible: true,
            preview_ratio: 0.70,
            window_width: 1240.0,
            requested_width: 0,
            image_cache: Mutex::new(None),
        }
    }

    /// Seed the layout with the configured logical window width.
    #[must_use]
    pub fn with_window_width(mut self, width: f32) -> Self {
        if width.is_finite() && width > 0.0 {
            self.window_width = width;
        }
        self
    }

    /// Attach actor diagnostics and, optionally, open the event panel.
    pub fn with_diagnostics(
        art: &crate::config::Art,
        diagnostics: Diagnostics,
        overlay: bool,
        config_summary: String,
    ) -> Self {
        let mut gui = Self::new(art);
        gui.debug_visible = overlay;
        gui.diagnostics = Some(diagnostics);
        gui.config_summary = config_summary;
        gui
    }
}

impl Default for YaziGui {
    fn default() -> Self {
        Self::new(&crate::config::Art::default())
    }
}

fn user(message: Message) -> Msg {
    GuiMessage::User(message)
}

fn local(local: Local) -> Msg {
    GuiMessage::Local(local)
}

const BG: Color = Color::from_rgb(0.016, 0.023, 0.038);
const PANEL: Color = Color::from_rgba(0.045, 0.062, 0.090, 0.72);
const PANEL_STRONG: Color = Color::from_rgba(0.030, 0.045, 0.068, 0.90);
const BORDER: Color = Color::from_rgba(0.42, 0.78, 0.76, 0.16);
const TEXT: Color = Color::from_rgb(0.86, 0.92, 0.93);
const MUTED: Color = Color::from_rgb(0.46, 0.55, 0.62);
const MINT: Color = Color::from_rgb(0.40, 0.90, 0.82);
const BLUE: Color = Color::from_rgb(0.42, 0.64, 0.96);
const PINK: Color = Color::from_rgb(0.96, 0.55, 0.72);
const GOLD: Color = Color::from_rgb(0.96, 0.76, 0.36);
const CORAL: Color = Color::from_rgb(0.97, 0.45, 0.42);

impl Renderer<ViewModel, Message, Local> for YaziGui {
    fn view<'a>(&'a self, view: &'a ViewModel) -> Element<'a, Msg> {
        let body: Element<'a, Msg> = if self.preview_visible {
            pane_grid(&self.panes, move |_pane, slot, _maximized| match slot {
                PaneSlot::Files => pane_grid::Content::new(files_area(view)),
                PaneSlot::Preview => pane_grid::Content::new(preview_panel(self, view, self.time)),
            })
            .spacing(10)
            .on_resize(6, |event| local(Local::PaneResized(event)))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            files_area(view)
        };

        let mut layout =
            iced::widget::column![top_bar(self, view), breadcrumb_bar(view), body,].spacing(10);

        if self.debug_visible
            && let Some(diagnostics) = &self.diagnostics
        {
            layout = layout.push(debug_panel(diagnostics));
        }
        layout = layout.push(status_bar(
            view,
            self.notice.as_deref(),
            self.diagnostics.is_some(),
        ));

        let content = container(layout)
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: if self.animation {
                    None
                } else {
                    Some(Background::Color(BG))
                },
                ..Default::default()
            });

        if self.animation {
            Stack::with_children([
                art::aurora(Aurora {
                    time: self.time,
                    palette: AuroraPalette::YAZI,
                    intensity: self.intensity,
                }),
                content.into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            content.into()
        }
    }

    fn subscription(&self, _view: &ViewModel) -> iced::Subscription<Msg> {
        let mut subscriptions = Vec::with_capacity(3);

        if self.animation || (self.debug_visible && self.diagnostics.is_some()) {
            subscriptions
                .push(iced::time::every(Duration::from_millis(33)).map(|_| local(Local::Frame)));
        }

        subscriptions.push(
            iced::window::resize_events().map(|(_id, size)| local(Local::WindowResized(size))),
        );

        subscriptions.push(keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };

            let input = if modifiers.control() {
                match key {
                    Key::Character(character) => match character.as_str() {
                        "q" | "Q" => Some(KeyInput::Quit),
                        "h" | "H" => Some(KeyInput::ToggleHidden),
                        "r" | "R" => Some(KeyInput::Refresh),
                        "l" | "L" => Some(KeyInput::FocusFilter),
                        "d" | "D" if modifiers.shift() => Some(KeyInput::ExportDiagnostics),
                        "d" | "D" => Some(KeyInput::ToggleDebug),
                        "p" | "P" => Some(KeyInput::TogglePreview),
                        _ => None,
                    },
                    _ => None,
                }
            } else if modifiers.alt() {
                matches!(key, Key::Named(Named::ArrowLeft)).then_some(KeyInput::Back)
            } else {
                match key {
                    Key::Named(Named::ArrowDown) => Some(KeyInput::Down),
                    Key::Named(Named::ArrowUp) => Some(KeyInput::Up),
                    Key::Named(Named::Enter) => Some(KeyInput::Enter),
                    Key::Named(Named::Escape) => Some(KeyInput::Escape),
                    Key::Named(Named::Backspace) => Some(KeyInput::Backspace),
                    Key::Named(Named::Home) => Some(KeyInput::Home),
                    _ => None,
                }
            };

            input.map(|input| local(Local::Key(input)))
        }));

        iced::Subscription::batch(subscriptions)
    }

    fn update(&mut self, local: Local, _view: &ViewModel) -> iced::Task<Msg> {
        match local {
            Local::Frame => {
                self.time = (self.time + 0.033 * self.speed) % 4096.0;
                iced::Task::none()
            }
            Local::Key(input) => self.handle_key(input),
            Local::PaneResized(event) => self.apply_pane_resize(event),
            Local::WindowResized(size) => {
                self.window_width = size.width;
                self.request_preview_width()
            }
        }
    }

    fn on_message(&mut self, message: &Message) {
        self.filter_active = matches!(message, Message::FilterChanged(_));
        self.notice = None;
    }

    fn theme(&self, _view: &ViewModel) -> Theme {
        Theme::custom(
            "yazi-desktop",
            iced::theme::Palette {
                background: BG,
                text: TEXT,
                primary: MINT,
                success: MINT,
                warning: GOLD,
                danger: CORAL,
            },
        )
    }
}

impl YaziGui {
    fn handle_key(&mut self, input: KeyInput) -> iced::Task<Msg> {
        if input == KeyInput::Quit {
            return iced::exit();
        }

        if input == KeyInput::ToggleDebug {
            if self.diagnostics.is_some() {
                self.debug_visible = !self.debug_visible;
            }
            return iced::Task::none();
        }

        if input == KeyInput::ExportDiagnostics {
            if self.diagnostics.is_some() {
                self.export_diagnostics();
            }
            return iced::Task::none();
        }

        if input == KeyInput::TogglePreview {
            self.preview_visible = !self.preview_visible;
            return self.request_preview_width();
        }

        if input == KeyInput::FocusFilter {
            self.filter_active = true;
            return iced::widget::operation::focus(self.filter_id.clone());
        }

        if self.filter_active {
            if matches!(input, KeyInput::Enter | KeyInput::Escape) {
                self.filter_active = false;
                return iced::widget::operation::focus_next();
            }
            return iced::Task::none();
        }

        match input {
            KeyInput::Up => iced::Task::done(user(Message::MoveSelection(-1))),
            KeyInput::Down => iced::Task::done(user(Message::MoveSelection(1))),
            KeyInput::Enter => iced::Task::done(user(Message::Activate)),
            KeyInput::Backspace => iced::Task::done(user(Message::Up)),
            KeyInput::Home => iced::Task::done(user(Message::Home)),
            KeyInput::Back => iced::Task::done(user(Message::Back)),
            KeyInput::ToggleHidden => iced::Task::done(user(Message::ToggleHidden)),
            KeyInput::Refresh => iced::Task::done(user(Message::Refresh)),
            KeyInput::Escape
            | KeyInput::FocusFilter
            | KeyInput::ToggleDebug
            | KeyInput::ExportDiagnostics
            | KeyInput::TogglePreview
            | KeyInput::Quit => iced::Task::none(),
        }
    }

    fn apply_pane_resize(&mut self, event: pane_grid::ResizeEvent) -> iced::Task<Msg> {
        let ratio = event.ratio.clamp(0.20, 0.88);
        self.panes.resize(event.split, ratio);
        self.preview_ratio = ratio;
        self.request_preview_width()
    }

    fn request_preview_width(&mut self) -> iced::Task<Msg> {
        if !self.preview_visible {
            return iced::Task::none();
        }

        let width = preview_width_for(self.window_width, self.preview_ratio);

        if width.abs_diff(self.requested_width) < 24 {
            return iced::Task::none();
        }
        self.requested_width = width;
        iced::Task::done(user(Message::PreviewWidth(width)))
    }

    fn preview_handle(&self, preview: &crate::app::Preview) -> Option<image::Handle> {
        let image = preview.image.as_ref()?;
        let mut cache = self.image_cache.lock().ok()?;

        if cache.as_ref().map(|(generation, _)| *generation) != Some(preview.generation) {
            *cache = Some((
                preview.generation,
                image::Handle::from_rgba(image.width, image.height, image.rgba.as_ref().to_vec()),
            ));
        }

        cache.as_ref().map(|(_, handle)| handle.clone())
    }

    fn export_diagnostics(&mut self) {
        let directory = state_dir("yazi-desktop").unwrap_or_else(std::env::temp_dir);
        let path = directory.join(format!("diagnostics-{}.txt", unix_seconds()));
        self.export_diagnostics_to(path);
    }

    fn export_diagnostics_to(&mut self, path: std::path::PathBuf) {
        let Some(diagnostics) = &self.diagnostics else {
            return;
        };

        let environment = safe_environment();
        let meta = ReportMeta {
            application: "yazi-desktop",
            version: env!("CARGO_PKG_VERSION"),
            config: &self.config_summary,
            environment: &environment,
        };

        self.notice = Some(match diagnostics.export(&path, &meta) {
            Ok(path) => format!("Diagnostics exported to {}", path.display()),
            Err(error) => format!("Diagnostics export failed: {error}"),
        });
    }
}

fn preview_width_for(window_width: f32, ratio: f32) -> u32 {
    let content_width = (window_width - 28.0).max(320.0);
    (content_width * (1.0 - ratio.clamp(0.20, 0.88)) - 10.0)
        .clamp(180.0, 2048.0)
        .round() as u32
}

fn two_pane_grid() -> pane_grid::State<PaneSlot> {
    pane_grid::State::with_configuration(pane_grid::Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.70,
        a: Box::new(pane_grid::Configuration::Pane(PaneSlot::Files)),
        b: Box::new(pane_grid::Configuration::Pane(PaneSlot::Preview)),
    })
}

fn files_area<'a>(view: &'a ViewModel) -> Element<'a, Msg> {
    iced::widget::row![places_panel(view), files_panel(view)]
        .spacing(10)
        .height(Length::Fill)
        .into()
}

fn top_bar<'a>(gui: &'a YaziGui, view: &'a ViewModel) -> Element<'a, Msg> {
    iced::widget::row![
        text("yazi").size(24).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::default()
        }),
        text(".desktop").size(24).color(MUTED),
        Space::new().width(14),
        text(view.cwd.clone()).size(11).color(MUTED),
        Space::new().width(Length::Fill),
        button(
            text(if gui.preview_visible { "▥" } else { "▤" })
                .size(14)
                .color(TEXT)
        )
        .padding([6, 9])
        .on_press(local(Local::Key(KeyInput::TogglePreview)))
        .style(flat_button),
        text_input("Filter…", &view.filter)
            .id(gui.filter_id.clone())
            .on_input(|value| user(Message::FilterChanged(value)))
            .width(220)
            .padding(8)
            .size(13)
            .style(input_style),
        icon_button("↑", Message::Up),
        icon_button("↻", Message::Refresh),
        icon_button("←", Message::Back),
        icon_button("~", Message::Home),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn breadcrumb_bar<'a>(view: &'a ViewModel) -> Element<'a, Msg> {
    let mut crumbs = iced::widget::row![].spacing(2).align_y(Alignment::Center);

    for (index, crumb) in view.crumbs.iter().enumerate() {
        if index > 0 {
            crumbs = crumbs.push(text("/").size(11).color(MUTED));
        }
        crumbs = crumbs.push(
            button(text(crumb.label.clone()).size(11).color(if crumb.current {
                MINT
            } else {
                MUTED
            }))
            .padding([2, 6])
            .on_press(user(Message::Open(crumb.path.clone())))
            .style(flat_button),
        );
    }

    container(crumbs)
        .padding([6, 8])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(PANEL_STRONG)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn places_panel<'a>(view: &'a ViewModel) -> Element<'a, Msg> {
    let mut places =
        iced::widget::column![text("PLACES").size(10).color(MUTED), Space::new().height(4),]
            .spacing(2);

    for place in &view.places {
        places = places.push(
            button(
                text(place.label)
                    .size(13)
                    .color(if place.active { MINT } else { TEXT }),
            )
            .width(Length::Fill)
            .padding([7, 10])
            .on_press(user(Message::Open(place.path.clone())))
            .style(move |theme, status| place_style(theme, status, place.active)),
        );
    }

    places = places
        .push(Space::new().height(10))
        .push(text("VIEW").size(10).color(MUTED))
        .push(Space::new().height(4))
        .push(
            button(
                text(if view.show_hidden {
                    "Hide dotfiles"
                } else {
                    "Show dotfiles"
                })
                .size(12)
                .color(TEXT),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .on_press(user(Message::ToggleHidden))
            .style(flat_button),
        )
        .push(
            button(text("Refresh").size(12).color(TEXT))
                .width(Length::Fill)
                .padding([6, 10])
                .on_press(user(Message::Refresh))
                .style(flat_button),
        );

    container(places)
        .width(Length::Fixed(170.0))
        .height(Length::Fill)
        .padding(12)
        .style(panel)
        .into()
}

fn files_panel<'a>(view: &'a ViewModel) -> Element<'a, Msg> {
    let header = iced::widget::row![
        text("NAME").size(10).color(MUTED),
        Space::new().width(Length::Fill),
        text("KIND · SIZE · MODIFIED").size(10).color(MUTED),
    ]
    .padding([0, 10]);

    let mut list = iced::widget::column![].spacing(2);
    for (index, row) in view.rows.iter().enumerate() {
        list = list.push(file_row(row, index));
    }

    let body: Element<'a, Msg> = if view.rows.is_empty() {
        let message = if view.loading {
            "Reading directory…"
        } else if !view.filter.trim().is_empty() {
            "No entries match this filter."
        } else {
            "This directory is empty."
        };

        container(text(message).size(13).color(MUTED))
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    } else {
        scrollable(list.padding([4, 6]))
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    };

    container(
        iced::widget::column![header, rule::horizontal(1.0), body]
            .spacing(6)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(10)
    .style(panel)
    .into()
}

fn file_row<'a>(row: &'a crate::app::Row, index: usize) -> Element<'a, Msg> {
    let accent = kind_color(row.kind);
    let chip = container(
        text(kind_initial(row.kind))
            .size(10)
            .color(accent)
            .font(Font::MONOSPACE),
    )
    .padding([3, 5])
    .style(move |_| container::Style {
        background: Some(Background::Color(Color { a: 0.14, ..accent })),
        border: Border {
            color: Color { a: 0.28, ..accent },
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    });

    let content = container(
        iced::widget::row![
            chip,
            iced::widget::column![
                text(row.name.clone())
                    .size(13)
                    .color(if row.hidden { MUTED } else { TEXT }),
                text(row.detail.clone()).size(10).color(MUTED),
            ]
            .spacing(1)
            .width(Length::Fill),
            text(if row.is_dir { "›" } else { "" })
                .size(16)
                .color(if row.is_dir { MINT } else { MUTED }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([5, 8])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: row
            .selected
            .then_some(Background::Color(Color { a: 0.10, ..MINT })),
        border: Border {
            color: if row.selected {
                Color { a: 0.30, ..MINT }
            } else {
                Color::TRANSPARENT
            },
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    });

    mouse_area(content)
        .on_press(user(Message::Select(index)))
        .on_double_click(user(Message::Activate))
        .into()
}

fn preview_panel<'a>(gui: &'a YaziGui, view: &'a ViewModel, time: f32) -> Element<'a, Msg> {
    let preview = &view.preview;

    let mut meta = iced::widget::column![].spacing(3);
    for (key, value) in &preview.meta {
        meta = meta.push(
            iced::widget::row![
                text(key.clone()).size(10).color(MUTED),
                Space::new().width(Length::Fill),
                text(value.clone()).size(10).color(TEXT),
            ]
            .align_y(Alignment::Center),
        );
    }

    let image_handle = gui.preview_handle(preview);
    let body: Element<'a, Msg> = if let Some(handle) = image_handle {
        container(
            image::Image::new(handle)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
    } else if preview.image.is_some() {
        container(text("Decoding image…").size(12).color(MUTED))
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    } else if preview.body.is_empty() {
        container(
            text(if preview.subtitle == "Reading…" {
                "Loading preview…"
            } else {
                "Select an entry to preview it."
            })
            .size(12)
            .color(MUTED),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
    } else {
        scrollable(
            iced::widget::column![
                text(preview.body.clone())
                    .size(11)
                    .font(if preview.is_text {
                        Font::MONOSPACE
                    } else {
                        Font::default()
                    })
                    .color(TEXT),
                text(if preview.truncated {
                    "… preview truncated"
                } else {
                    ""
                })
                .size(10)
                .color(GOLD),
            ]
            .spacing(6),
        )
        .height(Length::Fill)
        .into()
    };

    let mut details = iced::widget::column![
        iced::widget::row![
            text(preview.title.clone()).size(16).color(TEXT),
            Space::new().width(Length::Fill),
            text(preview.subtitle.clone())
                .size(10)
                .color(kind_color_from_label(&preview.subtitle)),
            button(text("×").size(14).color(MUTED))
                .padding([1, 6])
                .on_press(local(Local::Key(KeyInput::TogglePreview)))
                .style(flat_button),
        ]
        .align_y(Alignment::Center)
        .spacing(6),
        text(format!(
            "{} dirs · {} files · {}",
            view.stats.dirs,
            view.stats.files,
            super::app::format_size(view.stats.bytes)
        ))
        .size(10)
        .color(MUTED),
    ]
    .spacing(8);

    if preview.image.is_none() {
        details = details.push(orbit(view.stats.buckets, time));
    }
    details = details
        .push(rule::horizontal(1.0))
        .push(meta)
        .push(rule::horizontal(1.0))
        .push(body);

    container(details.height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12)
        .style(panel)
        .into()
}

fn debug_panel<'a>(diagnostics: &'a Diagnostics) -> Element<'a, Msg> {
    let events = diagnostics.events();
    let mut lines = String::new();

    for event in events.iter().rev().take(9).rev() {
        let marker = if event.kind == EventKind::Error {
            "!"
        } else {
            " "
        };
        lines.push_str(&format!(
            "{marker}{:>8.1}ms {:<15} {:<10} task={} {}\n",
            event.at.as_secs_f64() * 1000.0,
            event.kind.label(),
            event.phase,
            event.task,
            truncate(&event.detail, 92)
        ));
    }

    if lines.is_empty() {
        lines.push_str("  no events yet — the actor is running\n");
    }

    container(
        iced::widget::column![
            iced::widget::row![
                text("DEBUG TAP").size(10).color(MINT),
                Space::new().width(Length::Fill),
                text(format!(
                    "{} retained · {} total · {} errors",
                    events.len(),
                    diagnostics.total_events(),
                    diagnostics.total_errors()
                ))
                .size(10)
                .color(MUTED),
                button(text("Export").size(10).color(GOLD))
                    .padding([2, 7])
                    .on_press(local(Local::Key(KeyInput::ExportDiagnostics)))
                    .style(flat_button),
            ]
            .spacing(8),
            text(lines).font(Font::MONOSPACE).size(10).color(TEXT),
        ]
        .spacing(6),
    )
    .width(Length::Fill)
    .height(Length::Fixed(150.0))
    .padding(10)
    .style(|_| container::Style {
        background: Some(Background::Color(PANEL_STRONG)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut truncated: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        truncated.push('…');
    }
    truncated
}

fn status_bar<'a>(
    view: &'a ViewModel,
    notice: Option<&'a str>,
    debug_available: bool,
) -> Element<'a, Msg> {
    let status = notice.unwrap_or(&view.status);
    let shortcuts = if debug_available {
        "↑↓ select · ⏎ open · ⌫ up · ^H hidden · ^R refresh · ^L filter · ^P preview · ^D debug · ^⇧D export · ^Q quit"
    } else {
        "↑↓ select · ⏎ open · ⌫ up · ^H hidden · ^R refresh · ^L filter · ^P preview · ^Q quit"
    };

    container(
        iced::widget::row![
            text(if view.error { "!" } else { "›" })
                .size(13)
                .color(if view.error { CORAL } else { MINT }),
            text(status.to_owned()).size(11).color(if notice.is_some() {
                GOLD
            } else if view.error {
                CORAL
            } else {
                MUTED
            }),
            Space::new().width(Length::Fill),
            text(shortcuts).size(10).color(MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(PANEL_STRONG)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn icon_button<'a>(label: &'a str, message: Message) -> Element<'a, Msg> {
    button(text(label).size(14).color(TEXT))
        .padding([6, 9])
        .on_press(user(message))
        .style(flat_button)
        .into()
}

fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

fn flat_button(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(Color { a: 0.08, ..MINT })),
        text_color: TEXT,
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn place_style(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let mut style = flat_button(theme, status);
    if active {
        style.background = Some(Background::Color(Color { a: 0.12, ..MINT }));
    }
    style
}

fn input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: Background::Color(if focused {
            Color { a: 0.16, ..MINT }
        } else {
            PANEL_STRONG
        }),
        border: Border {
            color: if focused { MINT } else { BORDER },
            width: 1.0,
            radius: 8.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.35, ..MINT },
    }
}

fn kind_color(kind: Kind) -> Color {
    match kind {
        Kind::Dir => MINT,
        Kind::Code => BLUE,
        Kind::Text => GOLD,
        Kind::Image | Kind::Audio | Kind::Video => PINK,
        Kind::Archive | Kind::Binary | Kind::Other => MUTED,
    }
}

fn kind_color_from_label(label: &str) -> Color {
    match label {
        "dir" | "Directory" => MINT,
        "code" => BLUE,
        "text" => GOLD,
        "image" | "Image" | "audio" | "video" => PINK,
        _ => MUTED,
    }
}

fn kind_initial(kind: Kind) -> &'static str {
    match kind {
        Kind::Dir => "D",
        Kind::Code => "C",
        Kind::Text => "T",
        Kind::Image => "I",
        Kind::Audio => "A",
        Kind::Video => "V",
        Kind::Archive => "Z",
        Kind::Binary => "B",
        Kind::Other => "·",
    }
}

fn orbit<'a>(buckets: [usize; 5], time: f32) -> Element<'a, Msg> {
    let colors = [MINT, BLUE, GOLD, PINK, MUTED];
    let total: usize = buckets.iter().sum();
    let segments = buckets
        .iter()
        .zip(colors)
        .filter(|(count, _)| **count > 0)
        .map(|(count, color)| art::OrbitSegment::new(*count as f32, color))
        .collect();

    container(art::orbit(art::Orbit {
        segments,
        time,
        center: total.to_string(),
        label: "items".into(),
        rings: 3,
        gap: 0.03,
        spark: true,
        text_color: TEXT,
        label_color: MUTED,
        guide_color: MINT,
        spark_color: MINT,
    }))
    .width(Length::Fill)
    .height(Length::Fixed(170.0))
    .into()
}

#[cfg(test)]
mod tests {
    use ferriso_diagnostics::{FrameworkError, TaskId};

    use super::*;

    #[test]
    fn debug_toggle_requires_a_diagnostics_tap() {
        let art = crate::config::Art::default();

        let mut plain = YaziGui::new(&art);
        let _ = plain.handle_key(KeyInput::ToggleDebug);
        assert!(!plain.debug_visible);

        let mut gui = YaziGui::with_diagnostics(
            &art,
            Diagnostics::new().with_debug(true),
            false,
            String::new(),
        );
        let _ = gui.handle_key(KeyInput::ToggleDebug);
        assert!(gui.debug_visible);
        let _ = gui.handle_key(KeyInput::ToggleDebug);
        assert!(!gui.debug_visible);
    }

    #[test]
    fn export_writes_errors_config_and_environment() {
        let diagnostics = Diagnostics::new().with_debug(true);
        diagnostics.error(
            "update",
            TaskId::NONE,
            &FrameworkError::ActorPanic {
                phase: "update",
                message: "fixture panic".into(),
                task: 7,
            },
        );

        let art = crate::config::Art::default();
        let mut gui = YaziGui::with_diagnostics(
            &art,
            diagnostics,
            false,
            "window.title=fixture\nlayers:\n  local=yazi-desktop.toml (optional, missing)".into(),
        );

        let directory = std::env::temp_dir().join(format!(
            "ferriso-yazi-export-{}",
            ferriso_diagnostics::unix_seconds()
        ));
        let path = directory.join("report.txt");
        let _ = std::fs::remove_dir_all(&directory);

        gui.export_diagnostics_to(path.clone());

        let notice = gui.notice.expect("export notice");
        assert!(notice.contains("Diagnostics exported"), "{notice}");
        let report = std::fs::read_to_string(&path).expect("report written");
        assert!(report.contains("application: yazi-desktop"));
        assert!(report.contains("window.title=fixture"));
        assert!(report.contains("[ferriso::actor::panic]"));
        assert!(report.contains("fixture panic"));
        assert!(report.contains("os="));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn pane_resize_clamps_both_the_state_and_the_decode_width() {
        let art = crate::config::Art::default();
        let mut gui = YaziGui::new(&art);
        let split = *gui.panes.layout().splits().next().expect("split");

        let _ = gui.apply_pane_resize(pane_grid::ResizeEvent { split, ratio: 0.98 });
        assert!((gui.preview_ratio - 0.88).abs() < f32::EPSILON);
        let pane_grid::Node::Split { ratio, .. } = gui.panes.layout() else {
            panic!("expected split layout");
        };
        assert!((*ratio - 0.88).abs() < f32::EPSILON);

        let _ = gui.apply_pane_resize(pane_grid::ResizeEvent { split, ratio: 0.01 });
        assert!((gui.preview_ratio - 0.20).abs() < f32::EPSILON);
        let pane_grid::Node::Split { ratio, .. } = gui.panes.layout() else {
            panic!("expected split layout");
        };
        assert!((*ratio - 0.20).abs() < f32::EPSILON);
    }

    #[test]
    fn preview_width_tracks_the_split_and_clamps() {
        assert_eq!(preview_width_for(1240.0, 0.70), 354);
        assert_eq!(preview_width_for(1240.0, 0.99), 180);
        assert_eq!(preview_width_for(6000.0, 0.20), 2048);
    }

    #[test]
    fn layout_has_exactly_two_panes() {
        assert_eq!(two_pane_grid().len(), 2);
    }

    #[test]
    fn preview_toggle_flips_visibility() {
        let art = crate::config::Art::default();
        let mut gui = YaziGui::new(&art);
        assert!(gui.preview_visible);
        let _ = gui.handle_key(KeyInput::TogglePreview);
        assert!(!gui.preview_visible);
        let _ = gui.handle_key(KeyInput::TogglePreview);
        assert!(gui.preview_visible);
    }

    #[test]
    fn image_handle_is_reused_until_the_generation_changes() {
        use std::sync::Arc;

        use crate::app::Preview;
        use crate::preview::PreviewImage;

        let art = crate::config::Art::default();
        let gui = YaziGui::new(&art);
        let mut preview = Preview {
            path: std::path::PathBuf::from("fixture.png"),
            title: "fixture.png".into(),
            subtitle: "Image".into(),
            meta: Vec::new(),
            body: String::new(),
            is_text: false,
            truncated: false,
            generation: 3,
            target_width: 120,
            modified: None,
            image: Some(Arc::new(PreviewImage {
                width: 2,
                height: 1,
                rgba: Arc::from(vec![1_u8, 2, 3, 255, 4, 5, 6, 255]),
            })),
        };

        let first = gui.preview_handle(&preview).expect("handle");
        let second = gui.preview_handle(&preview).expect("cached handle");
        assert_eq!(first.id(), second.id());

        preview.generation = 4;
        let third = gui.preview_handle(&preview).expect("new handle");
        assert_ne!(first.id(), third.id());
    }
}
