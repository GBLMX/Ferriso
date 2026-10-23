//! Interactive gallery for Ferriso's art, skins, and interaction templates.
//!
//! Run with:
//! ```bash
//! cargo run -p ferriso-gui --example design-gallery
//! ```

use std::time::Duration;

use ferriso_gui::art::{self, Aurora, Palette as ArtPalette};
use ferriso_gui::design::{self, ButtonKind, Skin, Surface, Tokens};
use ferriso_gui::iced::{
    self, Alignment, Background, Border, Color, Element, Font, Length, Size, Subscription, Task,
    Theme,
    keyboard::{self, Key},
    widget::{Space, Stack, button, column, container, row, text},
};
use ferriso_gui::interaction::{
    Command, CommandPalette, PaletteEvent, PaletteKey, ToastTone, Toasts,
};

fn main() -> iced::Result {
    iced::application(Gallery::boot, Gallery::update, Gallery::view)
        .title("Ferriso · design gallery")
        .subscription(Gallery::subscription)
        .theme(Gallery::theme)
        .window(iced::window::Settings {
            size: Size::new(1240.0, 820.0),
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone)]
enum GalleryMessage {
    Frame,
    Tick,
    TogglePalette,
    Query(String),
    Palette(PaletteKey),
    Activate(Action),
    SetStyle(Style),
    DemoToast(ToastTone),
    DismissToast(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Deploy,
    Audit,
    Export,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Ferriso,
    Yazi,
    Windows11,
    Macos,
    Windows95,
    MacClassic,
    Cde,
    Ember,
}

impl Style {
    const ALL: [Self; 8] = [
        Self::Ferriso,
        Self::Yazi,
        Self::Windows11,
        Self::Macos,
        Self::Windows95,
        Self::MacClassic,
        Self::Cde,
        Self::Ember,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Ferriso => "FERRISO",
            Self::Yazi => "YAZI",
            Self::Windows11 => "WIN 11",
            Self::Macos => "macOS",
            Self::Windows95 => "WIN 95",
            Self::MacClassic => "MAC CLASSIC",
            Self::Cde => "CDE",
            Self::Ember => "EMBER",
        }
    }

    fn skin(self) -> Skin {
        match self {
            Self::Ferriso => Skin::FERRISO,
            Self::Yazi => Skin::YAZI,
            Self::Windows11 => Skin::WINDOWS_11,
            Self::Macos => Skin::MACOS,
            Self::Windows95 => Skin::WINDOWS_95,
            Self::MacClassic => Skin::MAC_OS_CLASSIC,
            Self::Cde => Skin::CDE,
            Self::Ember => Skin::EMBER,
        }
    }

    fn aurora(self) -> Option<(ArtPalette, f32)> {
        match self {
            Self::Ferriso => Some((ArtPalette::FERRISO, 0.95)),
            Self::Yazi => Some((ArtPalette::YAZI, 0.92)),
            Self::Windows11 => Some((ArtPalette::WINDOWS, 0.55)),
            Self::Macos => Some((ArtPalette::MACOS, 0.85)),
            Self::Ember => Some((ArtPalette::EMBER, 0.95)),
            Self::Windows95 | Self::MacClassic | Self::Cde => None,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Ferriso => Self::Yazi,
            Self::Yazi => Self::Windows11,
            Self::Windows11 => Self::Macos,
            Self::Macos => Self::Windows95,
            Self::Windows95 => Self::MacClassic,
            Self::MacClassic => Self::Cde,
            Self::Cde => Self::Ember,
            Self::Ember => Self::Ferriso,
        }
    }

    fn backdrop_label(self) -> &'static str {
        if self.aurora().is_some() {
            "AURORA"
        } else {
            "DESKTOP"
        }
    }

    fn panel_label(self) -> &'static str {
        let skin = self.skin();
        if skin.bevel {
            "BEVEL"
        } else if skin.light {
            "PLATINUM"
        } else {
            "GLASS"
        }
    }
}

struct Gallery {
    style: Style,
    skin: Skin,
    time: f32,
    palette_open: bool,
    palette: CommandPalette<Action>,
    toasts: Toasts,
}

impl Gallery {
    fn boot() -> (Self, Task<GalleryMessage>) {
        (Self::new(), Task::none())
    }

    fn new() -> Self {
        let palette = CommandPalette::new([
            Command::new(Action::Deploy, "Deploy workspace")
                .detail("Push the current actor snapshot to staging")
                .group("Workspace")
                .shortcut("Ctrl ↵"),
            Command::new(Action::Audit, "Run surface audit")
                .detail("Check contrast, focus states, and panel geometry")
                .group("Quality")
                .shortcut("Ctrl A"),
            Command::new(Action::Export, "Export diagnostics")
                .detail("Write the bounded event ring to the state directory")
                .group("Diagnostics")
                .shortcut("Ctrl E"),
            Command::new(Action::Theme, "Cycle style")
                .detail("Step through Windows, macOS, and classic skins")
                .group("Appearance")
                .shortcut("Ctrl T"),
        ]);

        let mut toasts = Toasts::new();
        toasts.push(
            ToastTone::Info,
            "Renderer local",
            "Renderer clocks run at 33 ms without waking the actor.",
            Duration::from_secs(8),
        );

        let style = Style::Windows11;
        Self {
            style,
            skin: style.skin(),
            time: 0.0,
            palette_open: false,
            palette,
            toasts,
        }
    }

    fn update(&mut self, message: GalleryMessage) -> Task<GalleryMessage> {
        match message {
            GalleryMessage::Frame => {
                self.time = (self.time + 0.033) % 4096.0;
            }
            GalleryMessage::Tick => {
                self.toasts.tick(Duration::from_millis(250));
            }
            GalleryMessage::TogglePalette => {
                self.palette_open = !self.palette_open;
                if self.palette_open {
                    return iced::widget::operation::focus(self.palette.input_id());
                }
            }
            GalleryMessage::Query(query) => self.palette.set_query(query),
            GalleryMessage::Palette(key) if self.palette_open => {
                match self.palette.handle_key(key) {
                    PaletteEvent::Changed => {}
                    PaletteEvent::Activate(action) => {
                        self.activate(action);
                        self.palette_open = false;
                    }
                    PaletteEvent::Dismiss => self.palette_open = false,
                    PaletteEvent::Ignored => {}
                }
            }
            GalleryMessage::Palette(_) => {}
            GalleryMessage::Activate(action) => {
                self.activate(action);
                self.palette_open = false;
            }
            GalleryMessage::SetStyle(style) => {
                self.style = style;
                self.skin = style.skin();
            }
            GalleryMessage::DemoToast(tone) => {
                self.toasts.push(
                    tone,
                    tone_title(tone),
                    "Generated by the gallery interaction template.",
                    Duration::from_secs(5),
                );
            }
            GalleryMessage::DismissToast(id) => {
                self.toasts.dismiss(id);
            }
        }
        Task::none()
    }

    fn activate(&mut self, action: Action) {
        let (tone, title, body) = match action {
            Action::Deploy => (
                ToastTone::Success,
                "Workspace queued",
                "The actor will publish a new snapshot when the deploy command settles.",
            ),
            Action::Audit => (
                ToastTone::Warning,
                "Surface audit",
                "Two muted labels are below the recommended contrast; the skin remains unchanged.",
            ),
            Action::Export => (
                ToastTone::Info,
                "Diagnostics exported",
                "The report is ready in the state directory.",
            ),
            Action::Theme => {
                let next = self.style.next();
                self.style = next;
                self.skin = next.skin();
                (
                    ToastTone::Info,
                    "Style switched",
                    "Windows, macOS, and classic skins are renderer-local.",
                )
            }
        };
        self.toasts.push(tone, title, body, Duration::from_secs(5));
    }

    fn subscription(&self) -> Subscription<GalleryMessage> {
        let animation = iced::time::every(Duration::from_millis(33)).map(|_| GalleryMessage::Frame);
        let expiry = iced::time::every(Duration::from_millis(250)).map(|_| GalleryMessage::Tick);
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };

            if modifiers.command()
                && matches!(&key, Key::Character(character) if character.eq_ignore_ascii_case("k"))
            {
                return Some(GalleryMessage::TogglePalette);
            }
            match PaletteKey::from_key(&key, modifiers) {
                Some(
                    key @ (PaletteKey::Next
                    | PaletteKey::Previous
                    | PaletteKey::First
                    | PaletteKey::Last
                    | PaletteKey::PageNext
                    | PaletteKey::PagePrevious
                    | PaletteKey::Activate
                    | PaletteKey::Dismiss),
                ) => Some(GalleryMessage::Palette(key)),
                _ => None,
            }
        });

        Subscription::batch([animation, expiry, keys])
    }

    fn theme(&self) -> Theme {
        let tokens = self.skin.tokens;
        Theme::custom(
            "ferriso-gallery",
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

    fn view(&self) -> Element<'_, GalleryMessage> {
        let mut layers: Vec<Element<'_, GalleryMessage>> = vec![self.backdrop(), self.content()];

        if self.palette_open {
            let palette = self.palette.view(
                self.skin,
                "Search actions…",
                GalleryMessage::Query,
                GalleryMessage::Activate,
            );
            let scrim = container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color {
                        a: 0.46,
                        ..Color::BLACK
                    })),
                    ..Default::default()
                });
            let modal = container(palette)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill);
            layers.push(Stack::with_children([scrim.into(), modal.into()]).into());
        }

        if !self.toasts.is_empty() {
            let toasts = container(self.toasts.view(self.skin, GalleryMessage::DismissToast))
                .width(Length::Fixed(360.0));
            layers.push(
                container(toasts)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(24)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End)
                    .into(),
            );
        }

        Stack::with_children(layers).into()
    }

    fn backdrop(&self) -> Element<'_, GalleryMessage> {
        if let Some((palette, intensity)) = self.style.aurora() {
            art::aurora(Aurora {
                time: self.time,
                palette,
                intensity,
            })
        } else {
            let color = self.skin.tokens.abyss;
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(color)),
                    ..Default::default()
                })
                .into()
        }
    }

    fn content(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let palette_label = if self.palette_open {
            "Close palette"
        } else {
            "Open palette"
        };

        let hero = row![
            column![
                text("Native surfaces.\nLiving light.")
                    .size(44)
                    .color(tokens.text),
                text("Artwork, interaction templates, and platform skins for native Ferriso frontends. Everything here is renderer-local; the actor remains the only owner of domain state.")
                    .size(14)
                    .color(tokens.muted)
                    .width(Length::Fixed(620.0)),
                row![
                    button(text(palette_label).size(13))
                        .padding([9, 16])
                        .style(move |_, status| design::button(
                            self.skin,
                            ButtonKind::Solid,
                            status
                        ))
                        .on_press(GalleryMessage::TogglePalette),
                    button(text("Notify").size(13))
                        .padding([9, 16])
                        .style(move |_, status| design::button(
                            self.skin,
                            ButtonKind::Soft,
                            status
                        ))
                        .on_press(GalleryMessage::DemoToast(ToastTone::Success)),
                    button(text("Warn").size(13))
                        .padding([9, 16])
                        .style(move |_, status| design::button(
                            self.skin,
                            ButtonKind::Ghost,
                            status
                        ))
                        .on_press(GalleryMessage::DemoToast(ToastTone::Warning)),
                ]
                .spacing(10),
                row![
                    design::key_hint("↑↓", "select", self.skin),
                    design::key_hint("↵", "activate", self.skin),
                    design::key_hint("esc", "dismiss", self.skin),
                ]
                .spacing(16),
            ]
            .spacing(18)
            .width(Length::Fill),
            self.orbit_panel(),
        ]
        .spacing(28)
        .height(Length::Fixed(410.0));

        let cards = row![
            card(
                "SURFACES",
                "Glass, strong, inset, elevated, modal",
                "Five panel treatments with skin-aware borders and shadows.",
                self.skin,
                tokens.accent,
            ),
            card(
                "SKINS",
                "Windows, macOS, Win 95, Mac Classic, CDE",
                "The same widget tree, different geometry and material rules.",
                self.skin,
                tokens.accent_alt,
            ),
            card(
                "SIGNAL",
                "Bars, gauge, and orbit",
                "Sampled locally; renderer clocks never wake the core actor.",
                self.skin,
                tokens.warning,
            ),
        ]
        .spacing(14);

        let mut body = column![].spacing(22).padding(30);
        if let Some(chrome) = self.window_chrome() {
            body = body.push(chrome);
        }
        body = body
            .push(self.header())
            .push(self.style_toolbar())
            .push(hero)
            .push(cards);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn header(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let chip_accent = if self.skin.light {
            tokens.text
        } else {
            tokens.accent
        };
        row![
            self.brand(),
            Space::new().width(Length::Fill),
            chip(self.style.backdrop_label(), self.skin, chip_accent),
            chip("ORBIT", self.skin, chip_accent),
            chip(self.style.panel_label(), self.skin, chip_accent),
            design::key_hint("Ctrl K", "palette", self.skin),
            design::key_hint("esc", "close", self.skin),
        ]
        .align_y(Alignment::Center)
        .spacing(8)
        .into()
    }

    fn brand(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let name = column![
            text("FERRISO")
                .size(11)
                .font(Font::MONOSPACE)
                .color(if self.skin.light {
                    tokens.text
                } else {
                    tokens.accent
                }),
            text("DESIGN LAB").size(26).color(tokens.text),
        ]
        .spacing(2);

        if self.style == Style::Macos {
            row![
                traffic_light(tokens.danger),
                traffic_light(tokens.warning),
                traffic_light(tokens.success),
                name,
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        } else {
            name.into()
        }
    }

    fn style_toolbar(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let mut items = row![design::section_label("STYLE", tokens)]
            .spacing(4)
            .align_y(Alignment::Center);

        for style in Style::ALL {
            let selected = self.style == style;
            items = items.push(
                button(text(style.label()).size(10).font(Font::MONOSPACE))
                    .padding([4, 8])
                    .style(move |_, status| {
                        design::button(
                            self.skin,
                            if selected {
                                ButtonKind::Solid
                            } else {
                                ButtonKind::Ghost
                            },
                            status,
                        )
                    })
                    .on_press(GalleryMessage::SetStyle(style)),
            );
        }

        container(items)
            .width(Length::Fill)
            .padding([6, 8])
            .style(move |_| design::surface(self.skin, Surface::Inset))
            .into()
    }

    fn window_chrome(&self) -> Option<Element<'_, GalleryMessage>> {
        match self.style {
            Style::Windows95 | Style::Cde => Some(self.classic_title_bar()),
            Style::MacClassic => Some(self.mac_title_bar()),
            _ => None,
        }
    }

    fn classic_title_bar(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        container(
            row![
                text("▣").size(11).color(self.skin.on_accent),
                text("Ferriso Design Lab")
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(self.skin.on_accent),
                Space::new().width(Length::Fill),
                chrome_control("_", self.skin),
                chrome_control("□", self.skin),
                chrome_control("×", self.skin),
            ]
            .spacing(3)
            .align_y(Alignment::Center),
        )
        .padding([3, 4])
        .style(move |_| container::Style {
            background: Some(Background::Color(tokens.accent)),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn mac_title_bar(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let close = container(Space::new())
            .width(Length::Fixed(11.0))
            .height(Length::Fixed(11.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(tokens.surface_strong)),
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });
        let zoom = container(Space::new())
            .width(Length::Fixed(11.0))
            .height(Length::Fixed(11.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(tokens.surface_strong)),
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        container(
            row![
                close,
                Space::new().width(Length::Fill),
                text("Ferriso Design Lab")
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(tokens.text),
                Space::new().width(Length::Fill),
                zoom,
            ]
            .padding([3, 4])
            .align_y(Alignment::Center),
        )
        .style(move |_| design::surface(self.skin, Surface::Strong))
        .into()
    }

    fn orbit_panel(&self) -> Element<'_, GalleryMessage> {
        let tokens = self.skin.tokens;
        let segments = vec![
            art::OrbitSegment::new(36.0, tokens.accent),
            art::OrbitSegment::new(28.0, tokens.accent_alt),
            art::OrbitSegment::new(22.0, tokens.warning),
            art::OrbitSegment::new(18.0, tokens.success),
            art::OrbitSegment::new(24.0, tokens.danger),
        ];
        let total: f32 = segments.iter().map(|segment| segment.value).sum();

        let orbit = art::Orbit {
            segments,
            time: self.time,
            center: format!("{total:.0}"),
            label: "objects".into(),
            rings: 3,
            gap: 0.035,
            spark: true,
            text_color: tokens.text,
            label_color: tokens.muted,
            guide_color: tokens.accent,
            spark_color: tokens.accent,
        };
        let signal = art::Bars {
            values: (0..36)
                .map(|index| {
                    let phase = self.time * 0.9 + index as f32 * 0.37;
                    0.22 + 0.78 * (phase.sin() * 0.5 + 0.5)
                })
                .collect(),
            color: tokens.accent,
            latest_color: Some(tokens.accent_alt),
            max: Some(1.0),
            gap: 2.0,
            radius: 1.5,
            baseline: design::rgba(tokens.border, 0.20),
        };
        let gauge = art::Gauge {
            value: 0.72,
            center: "72%".into(),
            label: "SYNC".into(),
            track: design::rgba(tokens.border, 0.20),
            fill: tokens.success,
            text_color: tokens.text,
            label_color: tokens.muted,
            thickness: 7.0,
        };

        container(
            column![
                row![
                    design::section_label("COMPOSITION", tokens),
                    Space::new().width(Length::Fill),
                    chip("LIVE", self.skin, tokens.success),
                ]
                .align_y(Alignment::Center),
                row![
                    container(art::orbit(orbit))
                        .width(Length::Fill)
                        .height(Length::Fixed(170.0)),
                    container(art::gauge(gauge))
                        .width(Length::Fixed(112.0))
                        .height(Length::Fixed(170.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                container(art::bars(signal)).height(Length::Fixed(52.0)),
                stat("Surfaces", "6", tokens),
                stat("Actor wakes", "0", tokens),
            ]
            .spacing(10),
        )
        .width(Length::Fixed(360.0))
        .padding(16)
        .style(move |_| design::surface(self.skin, Surface::Glass))
        .into()
    }
}

fn tone_title(tone: ToastTone) -> &'static str {
    match tone {
        ToastTone::Info => "Information",
        ToastTone::Success => "Saved",
        ToastTone::Warning => "Attention",
        ToastTone::Danger => "Failed",
    }
}

fn traffic_light(color: Color) -> Element<'static, GalleryMessage> {
    container(Space::new())
        .width(Length::Fixed(10.0))
        .height(Length::Fixed(10.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                color: Color {
                    a: 0.55,
                    ..Color::BLACK
                },
                width: 1.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn chrome_control<'a>(label: &str, skin: Skin) -> Element<'a, GalleryMessage> {
    let tokens = skin.tokens;
    container(
        text(label.to_owned())
            .size(9)
            .font(Font::MONOSPACE)
            .color(tokens.text),
    )
    .width(Length::Fixed(18.0))
    .height(Length::Fixed(15.0))
    .center_x(Length::Fixed(18.0))
    .center_y(Length::Fixed(15.0))
    .style(move |_| container::Style {
        background: Some(Background::Color(tokens.surface)),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn chip<'a>(label: &str, skin: Skin, accent: Color) -> Element<'a, GalleryMessage> {
    container(
        text(label.to_owned())
            .size(9)
            .font(Font::MONOSPACE)
            .color(accent),
    )
    .padding([4, 8])
    .style(move |_| design::chip(skin, accent))
    .into()
}

fn stat<'a>(label: &str, value: &str, tokens: Tokens) -> Element<'a, GalleryMessage> {
    row![
        text(label.to_owned()).size(11).color(tokens.muted),
        Space::new().width(Length::Fill),
        text(value.to_owned())
            .size(11)
            .font(Font::MONOSPACE)
            .color(tokens.text),
    ]
    .into()
}

fn card<'a>(
    label: &str,
    title: &str,
    body: &str,
    skin: Skin,
    accent: Color,
) -> Element<'a, GalleryMessage> {
    let tokens = skin.tokens;
    container(
        column![
            design::section_label(label.to_owned(), tokens),
            text(title.to_owned()).size(16).color(tokens.text),
            text(body.to_owned()).size(12).color(tokens.muted),
            Space::new().height(Length::Fixed(4.0)),
            chip("TEMPLATE", skin, accent),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(16)
    .style(move |_| design::surface(skin, Surface::Glass))
    .into()
}
