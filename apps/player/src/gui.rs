//! Iced layout and input translation for the player.
//!
//! This module is deliberately not shared with `tui.rs`: ratatui lays out cells
//! and iced lays out pixels, so "one model, two native views" means duplicate
//! presentation code, not a lowest-common-denominator layout IR.

use ferriso_gui::iced::{
    self, Alignment, Background, Border, Color, Element, Length, Theme,
    keyboard::{self, Key, key::Named},
    widget::{Space, button, column, container, row, scrollable, slider, text, text_input},
};
use ferriso_gui::{Message as GuiMessage, Renderer};

use crate::app::{LoginView, Message, Page, ViewModel, format_duration};

type Msg = GuiMessage<ViewModel, Message>;

const BG: Color = Color::from_rgb8(16, 20, 26);
const SURFACE: Color = Color::from_rgb8(24, 30, 39);
const SURFACE_HOVER: Color = Color::from_rgb8(33, 41, 53);
const BORDER: Color = Color::from_rgb8(48, 57, 71);
const ACCENT: Color = Color::from_rgb8(122, 162, 247);
const TEXT: Color = Color::from_rgb8(220, 226, 236);
const MUTED: Color = Color::from_rgb8(128, 138, 156);
const ERROR: Color = Color::from_rgb8(247, 118, 142);
const WARNING: Color = Color::from_rgb8(224, 175, 104);
const HIGHLIGHT: Color = Color::from_rgb8(34, 46, 64);

pub struct PlayerGui;

impl PlayerGui {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlayerGui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<ViewModel, Message> for PlayerGui {
    fn view<'a>(&self, view: &'a ViewModel) -> Element<'a, Msg> {
        let sidebar = sidebar(view);
        let main = column![top_bar(view), content(view), player_bar(view)]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill);

        container(row![sidebar, main].spacing(10))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .style(|_| container::Style {
                background: Some(Background::Color(BG)),
                text_color: Some(TEXT),
                ..Default::default()
            })
            .into()
    }

    fn subscription(&self, _view: &ViewModel) -> iced::Subscription<Msg> {
        keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                return None;
            };
            if !modifiers.control() {
                return None;
            }
            match key {
                Key::Named(Named::Space) => Some(GuiMessage::User(Message::PlayPause)),
                Key::Named(Named::ArrowLeft) => Some(GuiMessage::User(Message::Previous)),
                Key::Named(Named::ArrowRight) => Some(GuiMessage::User(Message::Next)),
                Key::Character(character) if character.as_str() == "m" => {
                    Some(GuiMessage::User(Message::CycleMode))
                }
                Key::Character(character) if character.as_str() == "l" => {
                    Some(GuiMessage::User(Message::ToggleLike))
                }
                Key::Character(character) if character.as_str() == "q" => Some(GuiMessage::Closed),
                _ => None,
            }
        })
    }
}

fn sidebar(view: &ViewModel) -> Element<'_, Msg> {
    let mut nav = column![].spacing(4);
    for item in &view.nav {
        nav = nav.push(
            button(
                text(item.label)
                    .size(14)
                    .color(if item.selected { ACCENT } else { TEXT }),
            )
            .width(Length::Fill)
            .padding([8, 10])
            .on_press(GuiMessage::User(Message::Navigate(item.page)))
            .style(nav_style(item.selected)),
        );
    }

    let session = match &view.user {
        Some(user) => row![
            text(format!("{user} · 已登录")).size(12).color(MUTED),
            Space::new().width(Length::Fill),
            button(text("退出").size(12).color(MUTED))
                .padding([4, 8])
                .on_press(GuiMessage::User(Message::Logout))
                .style(ghost_style),
        ]
        .align_y(Alignment::Center),
        None => row![
            text("未登录").size(12).color(MUTED),
            Space::new().width(Length::Fill),
            button(text("扫码登录").size(12).color(ACCENT))
                .padding([4, 8])
                .on_press(GuiMessage::User(Message::Navigate(Page::Login)))
                .style(ghost_style),
        ]
        .align_y(Alignment::Center),
    };

    container(
        column![
            text("Ferriso").size(26).color(ACCENT),
            text("actor core · two adapters").size(11).color(MUTED),
            Space::new().height(8),
            nav,
            Space::new().height(Length::Fill),
            session,
            text("状态在 actor · 界面是快照").size(11).color(MUTED),
        ]
        .spacing(10),
    )
    .width(210)
    .height(Length::Fill)
    .padding(14)
    .style(surface_style)
    .into()
}

fn top_bar(view: &ViewModel) -> Element<'_, Msg> {
    let search = text_input("搜索歌曲，回车确认", &view.search)
        .on_input(|value| GuiMessage::User(Message::SearchChanged(value)))
        .on_submit(GuiMessage::User(Message::SearchSubmit))
        .padding(8)
        .width(320);

    let mut banner =
        row![
            text(&view.banner)
                .size(12)
                .color(if view.error { ERROR } else { MUTED })
        ]
        .spacing(12);
    if view.loading {
        banner = banner.push(text("加载中…").size(12).color(WARNING));
    }

    container(
        column![
            row![
                text(&view.page_title).size(24).color(TEXT),
                Space::new().width(Length::Fill),
                search,
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            banner,
        ]
        .spacing(8),
    )
    .padding(14)
    .style(surface_style)
    .into()
}

fn content(view: &ViewModel) -> Element<'_, Msg> {
    let mut header = row![
        text(&view.page_title).size(18).color(TEXT),
        Space::new().width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if view.show_back {
        header = header.push(
            button(text("返回").size(12).color(MUTED))
                .padding([4, 10])
                .on_press(GuiMessage::User(Message::Back))
                .style(ghost_style),
        );
    }
    header = header.push(
        button(text("刷新").size(12).color(MUTED))
            .padding([4, 10])
            .on_press(GuiMessage::User(Message::Refresh))
            .style(ghost_style),
    );

    let body = match view.page {
        Page::Lyrics => lyrics(view),
        Page::Login => login(view),
        Page::Toplists | Page::Playlists => collections(view),
        _ => songs(view),
    };

    container(column![header, body].spacing(10).height(Length::Fill))
        .padding(14)
        .height(Length::Fill)
        .style(surface_style)
        .into()
}

fn songs(view: &ViewModel) -> Element<'_, Msg> {
    if view.songs.is_empty() {
        let hint = if view.loading {
            "加载中…"
        } else if view.page == Page::Search {
            "没有匹配的歌曲，回车搜索全部曲库"
        } else {
            "这里还没有歌曲"
        };
        return text(hint).size(13).color(MUTED).into();
    }

    let mut list = column![].spacing(2).width(Length::Fill);
    for (index, song) in view.songs.iter().enumerate() {
        let marker = if song.playing {
            "▶".to_string()
        } else {
            format!("{:>2}", index + 1)
        };
        let selected = view.selected == Some(index);
        let line = row![
            container(
                text(marker)
                    .size(12)
                    .color(if song.playing { ACCENT } else { MUTED })
            )
            .width(Length::Fixed(28.0)),
            column![
                text(&song.title).size(14).color(TEXT),
                text(&song.subtitle).size(11).color(MUTED),
            ]
            .spacing(2)
            .width(Length::Fill),
            text(&song.duration).size(11).color(MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding([6, 8])
                .on_press(GuiMessage::User(Message::Play(view.page, index)))
                .style(row_style(selected)),
        );
    }

    scrollable(list).height(Length::Fill).into()
}

fn collections(view: &ViewModel) -> Element<'_, Msg> {
    let mut list = column![].spacing(2).width(Length::Fill);
    for (index, collection) in view.collections.iter().enumerate() {
        let line = row![
            container(text(format!("{:>2}", index + 1)).size(12).color(MUTED))
                .width(Length::Fixed(28.0)),
            column![
                text(&collection.title).size(14).color(TEXT),
                text(&collection.kind).size(11).color(MUTED),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding([8, 10])
                .on_press(GuiMessage::User(Message::OpenCollection(index)))
                .style(row_style(view.selected == Some(index))),
        );
    }
    scrollable(list).height(Length::Fill).into()
}

fn lyrics(view: &ViewModel) -> Element<'_, Msg> {
    if !view.player.has_song {
        return text("播放一首歌后这里显示歌词")
            .size(13)
            .color(MUTED)
            .into();
    }
    let mut lines = column![].spacing(12).width(Length::Fill);
    for (index, lyric) in view.lyrics.iter().enumerate() {
        let active = Some(index) == view.active_lyric;
        lines = lines.push(
            text(&lyric.text)
                .size(if active { 20 } else { 14 })
                .color(if active { ACCENT } else { MUTED }),
        );
    }
    container(scrollable(
        container(lines).width(Length::Fill).center_x(Length::Fill),
    ))
    .height(Length::Fill)
    .into()
}

fn login(view: &ViewModel) -> Element<'_, Msg> {
    let body = match &view.login {
        LoginView::LoggedOut => column![
            text("尚未登录").size(20).color(TEXT),
            text("搜索、排行榜不需要登录；每日推荐与「我喜欢」需要。")
                .size(12)
                .color(MUTED),
            button(text("生成登录二维码").size(13).color(Color::WHITE))
                .padding([8, 14])
                .on_press(GuiMessage::User(Message::OfferLoginQr))
                .style(primary_style),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
        LoginView::Qr { data, status } => column![
            qr_grid(data),
            text(status).size(12).color(MUTED),
            row![
                button(text("刷新二维码").size(12).color(MUTED))
                    .padding([6, 12])
                    .on_press(GuiMessage::User(Message::Login))
                    .style(ghost_style),
                button(text("模拟扫码成功").size(12).color(Color::WHITE))
                    .padding([6, 12])
                    .on_press(GuiMessage::User(Message::LoggedIn))
                    .style(primary_style),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
        LoginView::LoggedIn { user } => column![
            text(format!("已登录：{user}")).size(20).color(TEXT),
            text("本地演示会话，不会连接服务器").size(12).color(MUTED),
            button(text("退出登录").size(13).color(Color::WHITE))
                .padding([8, 14])
                .on_press(GuiMessage::User(Message::Logout))
                .style(primary_style),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    };

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn player_bar(view: &ViewModel) -> Element<'_, Msg> {
    let player = &view.player;
    let like = if player.liked {
        "取消喜欢"
    } else {
        "喜欢"
    };
    let controls = row![
        button(text("上一首").size(12).color(MUTED))
            .padding([5, 10])
            .on_press(GuiMessage::User(Message::Previous))
            .style(ghost_style),
        button(
            text(if player.playing { "暂停" } else { "播放" })
                .size(12)
                .color(Color::WHITE)
        )
        .padding([5, 14])
        .on_press(GuiMessage::User(Message::PlayPause))
        .style(primary_style),
        button(text("下一首").size(12).color(MUTED))
            .padding([5, 10])
            .on_press(GuiMessage::User(Message::Next))
            .style(ghost_style),
        button(text(player.mode_label()).size(12).color(MUTED))
            .padding([5, 10])
            .on_press(GuiMessage::User(Message::CycleMode))
            .style(ghost_style),
        button(text(like).size(12).color(MUTED))
            .padding([5, 10])
            .on_press(GuiMessage::User(Message::ToggleLike))
            .style(ghost_style),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    let art = container(text("♪").size(28).color(Color::WHITE))
        .center_x(Length::Fixed(64.0))
        .center_y(Length::Fixed(64.0))
        .style(|_| container::Style {
            background: Some(Background::Color(ACCENT)),
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let info = column![
        text(&player.title).size(16).color(TEXT),
        text(&player.subtitle).size(12).color(MUTED),
        controls,
    ]
    .spacing(4)
    .width(Length::Fill);

    let mut bars = row![].spacing(2).height(36).align_y(Alignment::End);
    for level in &player.spectrum {
        bars = bars.push(accent_block(f32::from(*level) * 0.32));
    }

    let volume = row![
        text("音量").size(11).color(MUTED),
        slider(0.0..=1.0, player.volume, |value| {
            GuiMessage::User(Message::Volume(value))
        })
        .width(100),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let meters = column![bars, volume].spacing(6).align_x(Alignment::End);

    let duration = (player.duration as f64).max(1.0);
    let progress = row![
        text(format_duration((player.position * 1000.0) as u64))
            .size(11)
            .color(MUTED),
        slider(0.0..=duration, player.position, |value| {
            GuiMessage::User(Message::Seek(value))
        })
        .on_release(GuiMessage::User(Message::SeekCommit))
        .width(Length::Fill),
        text(format_duration(player.duration)).size(11).color(MUTED),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(
        column![
            row![art, info, meters]
                .spacing(16)
                .align_y(Alignment::Center),
            progress,
        ]
        .spacing(10),
    )
    .padding(14)
    .style(surface_style)
    .into()
}

fn accent_block(height: f32) -> Element<'static, Msg> {
    container("")
        .width(3)
        .height(height.max(2.0))
        .style(|_| container::Style {
            background: Some(Background::Color(ACCENT)),
            border: Border {
                radius: 1.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn qr_grid(data: &str) -> Element<'static, Msg> {
    let seed = data
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| hash ^ u64::from(*byte));
    let mut grid = column![].spacing(0);
    for y in 0..21 {
        let mut line = row![].spacing(0);
        for x in 0..21 {
            line = line.push(qr_module(
                finder_module(x, y, 21).unwrap_or_else(|| data_bit(seed, x, y)),
            ));
        }
        grid = grid.push(line);
    }
    container(grid)
        .padding(10)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::WHITE)),
            ..Default::default()
        })
        .into()
}

fn qr_module(dark: bool) -> Element<'static, Msg> {
    container("")
        .width(6)
        .height(6)
        .style(move |_| container::Style {
            background: Some(Background::Color(if dark {
                Color::BLACK
            } else {
                Color::WHITE
            })),
            ..Default::default()
        })
        .into()
}

fn data_bit(seed: u64, x: usize, y: usize) -> bool {
    seed.wrapping_mul(6364136223846793005)
        .wrapping_add((x as u64) << 32)
        .wrapping_add(y as u64)
        >> 31
        & 1
        == 1
}

fn finder_module(x: usize, y: usize, modules: usize) -> Option<bool> {
    let inside = |origin_x: usize, origin_y: usize| {
        let dx = x as isize - origin_x as isize;
        let dy = y as isize - origin_y as isize;
        if (0..7).contains(&dx) && (0..7).contains(&dy) {
            Some(
                dx == 0
                    || dx == 6
                    || dy == 0
                    || dy == 6
                    || (2..=4).contains(&dx) && (2..=4).contains(&dy),
            )
        } else {
            None
        }
    };
    inside(0, 0)
        .or_else(|| inside(modules - 7, 0))
        .or_else(|| inside(0, modules - 7))
}

fn surface_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(TEXT),
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

fn nav_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if selected {
                HIGHLIGHT
            } else if hovered {
                SURFACE_HOVER
            } else {
                Color::TRANSPARENT
            })),
            text_color: if selected { ACCENT } else { TEXT },
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

fn row_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if selected || hovered {
                HIGHLIGHT
            } else {
                Color::TRANSPARENT
            })),
            text_color: TEXT,
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

fn ghost_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            SURFACE_HOVER
        } else {
            Color::TRANSPARENT
        })),
        text_color: MUTED,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn primary_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb8(98, 138, 224)
        } else {
            ACCENT
        })),
        text_color: Color::WHITE,
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
