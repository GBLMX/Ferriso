//! Terminal layout and input translation for the player.
//!
//! Everything a terminal needs but the core must not know about lives here:
//! focus, list offsets, the in-progress search edit, and key chords.

use ferriso_tui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferriso_tui::ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, LineGauge, List, ListItem, ListState, Paragraph, Wrap},
};
use ferriso_tui::{Outcome, Renderer};

use crate::app::{LoginView, Message, Page, ViewModel, format_duration};

const ACCENT: Color = Color::Rgb(122, 162, 247);
const MUTED: Color = Color::Rgb(128, 138, 156);
const TEXT: Color = Color::Rgb(220, 226, 236);
const SURFACE: Color = Color::Rgb(26, 31, 40);
const HIGHLIGHT: Color = Color::Rgb(38, 48, 64);
const ERROR: Color = Color::Rgb(247, 118, 142);
const WARNING: Color = Color::Rgb(224, 175, 104);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Nav,
    Content,
    Player,
    Search,
}

pub struct PlayerTui {
    focus: Focus,
    nav_index: usize,
    content_index: usize,
    search: String,
    cursor: usize,
}

impl PlayerTui {
    pub fn new() -> Self {
        Self {
            focus: Focus::Content,
            nav_index: 0,
            content_index: 0,
            search: String::new(),
            cursor: 0,
        }
    }

    fn key(&mut self, view: &ViewModel, key: KeyEvent) -> Outcome<Message> {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Outcome::Ignored;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let message = match key.code {
                KeyCode::Char(' ') => Some(Message::PlayPause),
                KeyCode::Left => Some(Message::Previous),
                KeyCode::Right => Some(Message::Next),
                KeyCode::Char('m') => Some(Message::CycleMode),
                KeyCode::Char('l') => Some(Message::ToggleLike),
                _ => None,
            };
            if let Some(message) = message {
                return Outcome::Message(message);
            }
        }

        if key.code == KeyCode::Tab {
            self.cycle_focus(view, !key.modifiers.contains(KeyModifiers::SHIFT));
            return Outcome::Ignored;
        }

        match self.focus {
            Focus::Search => self.search_key(key),
            Focus::Nav => self.nav_key(view, key),
            Focus::Content => self.content_key(view, key),
            Focus::Player => self.player_key(view, key),
        }
    }

    fn cycle_focus(&mut self, view: &ViewModel, forward: bool) {
        let order = [Focus::Nav, Focus::Content, Focus::Player, Focus::Search];
        let at = order
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        self.focus = order[next];
        if self.focus == Focus::Search {
            self.search = view.search.clone();
            self.cursor = self.search.chars().count();
        }
    }

    fn search_key(&mut self, key: KeyEvent) -> Outcome<Message> {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Content;
                Outcome::Ignored
            }
            KeyCode::Enter => Outcome::Message(Message::SearchSubmit),
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::Ignored
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.search.chars().count());
                Outcome::Ignored
            }
            KeyCode::Home => {
                self.cursor = 0;
                Outcome::Ignored
            }
            KeyCode::End => {
                self.cursor = self.search.chars().count();
                Outcome::Ignored
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let start = byte_index(&self.search, self.cursor - 1);
                    let end = byte_index(&self.search, self.cursor);
                    self.search.replace_range(start..end, "");
                    self.cursor -= 1;
                    return Outcome::Message(Message::SearchChanged(self.search.clone()));
                }
                Outcome::Ignored
            }
            KeyCode::Delete => {
                if self.cursor < self.search.chars().count() {
                    let start = byte_index(&self.search, self.cursor);
                    let end = byte_index(&self.search, self.cursor + 1);
                    self.search.replace_range(start..end, "");
                    return Outcome::Message(Message::SearchChanged(self.search.clone()));
                }
                Outcome::Ignored
            }
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                let at = byte_index(&self.search, self.cursor);
                self.search.insert(at, character);
                self.cursor += 1;
                Outcome::Message(Message::SearchChanged(self.search.clone()))
            }
            _ => Outcome::Ignored,
        }
    }

    fn nav_key(&mut self, view: &ViewModel, key: KeyEvent) -> Outcome<Message> {
        let len = view.nav.len();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 {
                    self.nav_index = (self.nav_index + 1) % len;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if len > 0 {
                    self.nav_index = (self.nav_index + len - 1) % len;
                }
            }
            KeyCode::Enter => {
                if let Some(item) = view.nav.get(self.nav_index) {
                    return Outcome::Message(Message::Navigate(item.page));
                }
            }
            KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Content,
            KeyCode::Esc => self.focus = Focus::Content,
            _ => {}
        }
        Outcome::Ignored
    }

    fn content_key(&mut self, view: &ViewModel, key: KeyEvent) -> Outcome<Message> {
        let len = content_len(view);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 {
                    self.content_index = (self.content_index + 1).min(len - 1);
                    return Outcome::Message(Message::Selected(self.content_index));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if len > 0 {
                    self.content_index = self.content_index.saturating_sub(1);
                    return Outcome::Message(Message::Selected(self.content_index));
                }
            }
            KeyCode::Enter => return self.activate(view),
            KeyCode::Backspace | KeyCode::Esc => {
                if view.show_back {
                    return Outcome::Message(Message::Back);
                }
                self.focus = Focus::Nav;
            }
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Nav,
            KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Player,
            _ => {}
        }
        Outcome::Ignored
    }

    fn activate(&mut self, view: &ViewModel) -> Outcome<Message> {
        match view.page {
            Page::Lyrics => Outcome::Ignored,
            Page::Login => {
                let message = match view.login {
                    LoginView::LoggedOut => Message::OfferLoginQr,
                    LoginView::Qr { .. } => Message::LoggedIn,
                    LoginView::LoggedIn { .. } => Message::Logout,
                };
                Outcome::Message(message)
            }
            Page::Toplists | Page::Playlists => {
                if view.collections.is_empty() {
                    Outcome::Ignored
                } else {
                    Outcome::Message(Message::OpenCollection(
                        self.content_index.min(view.collections.len() - 1),
                    ))
                }
            }
            page => {
                if view.songs.is_empty() {
                    Outcome::Ignored
                } else {
                    Outcome::Message(Message::Play(
                        page,
                        self.content_index.min(view.songs.len() - 1),
                    ))
                }
            }
        }
    }

    fn player_key(&mut self, view: &ViewModel, key: KeyEvent) -> Outcome<Message> {
        let message = match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => Some(Message::PlayPause),
            KeyCode::Char('n') | KeyCode::Right => Some(Message::Next),
            KeyCode::Char('p') | KeyCode::Left => Some(Message::Previous),
            KeyCode::Char('m') => Some(Message::CycleMode),
            KeyCode::Char('l') => Some(Message::ToggleLike),
            KeyCode::Char(']') => Some(Message::SeekRelative(5.0)),
            KeyCode::Char('[') => Some(Message::SeekRelative(-5.0)),
            KeyCode::Char('=') | KeyCode::Char('+') => {
                Some(Message::Volume((view.player.volume + 0.05).min(1.0)))
            }
            KeyCode::Char('-') => Some(Message::Volume((view.player.volume - 0.05).max(0.0))),
            KeyCode::Esc => {
                self.focus = Focus::Content;
                None
            }
            _ => None,
        };
        message.map_or(Outcome::Ignored, Outcome::Message)
    }
}

impl Default for PlayerTui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<ViewModel, Message> for PlayerTui {
    fn draw(&mut self, view: &ViewModel, frame: &mut Frame) {
        self.sync(view);
        let area = frame.area();
        let [sidebar, main] =
            Layout::horizontal([Constraint::Length(25), Constraint::Fill(1)]).areas(area);
        let [top, content, player] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Fill(1),
            Constraint::Length(9),
        ])
        .areas(main);

        draw_sidebar(self, view, frame, sidebar);
        draw_top(self, view, frame, top);
        draw_content(self, view, frame, content);
        draw_player(view, frame, player);
    }

    fn handle(&mut self, view: &ViewModel, event: Event) -> Outcome<Message> {
        match event {
            Event::Key(key) => self.key(view, key),
            Event::Paste(text) if self.focus == Focus::Search => {
                let at = byte_index(&self.search, self.cursor);
                self.search.insert_str(at, &text);
                self.cursor += text.chars().count();
                Outcome::Message(Message::SearchChanged(self.search.clone()))
            }
            _ => Outcome::Ignored,
        }
    }
}

impl PlayerTui {
    fn sync(&mut self, view: &ViewModel) {
        if self.focus != Focus::Search {
            self.search = view.search.clone();
            self.cursor = self.search.chars().count();
        }
        if self.focus != Focus::Content {
            self.content_index = view
                .selected
                .unwrap_or(0)
                .min(content_len(view).saturating_sub(1));
        }
        if self.focus != Focus::Nav
            && let Some(index) = view.nav.iter().position(|item| item.selected)
        {
            self.nav_index = index;
        }
    }
}

fn content_len(view: &ViewModel) -> usize {
    match view.page {
        Page::Toplists | Page::Playlists => view.collections.len(),
        Page::Lyrics | Page::Login => 0,
        _ => view.songs.len(),
    }
}

fn draw_sidebar(state: &PlayerTui, view: &ViewModel, frame: &mut Frame, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Ferriso",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "actor core · two adapters",
            Style::default().fg(MUTED),
        )),
        Line::from(""),
    ];
    for (index, item) in view.nav.iter().enumerate() {
        let focused = state.focus == Focus::Nav && index == state.nav_index;
        let style = if focused {
            Style::default().fg(Color::Black).bg(ACCENT)
        } else if item.selected {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(Span::styled(
            format!(" {} {}", if item.selected { "▶" } else { " " }, item.label),
            style,
        )));
    }
    lines.push(Line::from(""));
    let session = view
        .user
        .as_ref()
        .map(|user| format!(" {user}"))
        .unwrap_or_else(|| " 未登录".into());
    lines.push(Line::from(Span::styled(
        session,
        Style::default().fg(MUTED),
    )));
    lines.push(Line::from(Span::styled(
        " Tab 焦点 · Ctrl+空格 播放",
        Style::default().fg(MUTED),
    )));

    let block = Block::bordered()
        .title(" 导航 ")
        .border_style(focus_border(state.focus == Focus::Nav))
        .style(Style::default().bg(SURFACE));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_top(state: &PlayerTui, view: &ViewModel, frame: &mut Frame, area: Rect) {
    let search_style = if state.focus == Focus::Search {
        Style::default().fg(Color::Black).bg(ACCENT)
    } else {
        Style::default().fg(TEXT)
    };
    let before = &state.search[..byte_index(&state.search, state.cursor)];
    let after = &state.search[byte_index(&state.search, state.cursor)..];
    let mut search = vec![
        Span::styled("搜索 ", Style::default().fg(MUTED)),
        Span::styled(before, search_style),
    ];
    if state.focus == Focus::Search {
        search.push(Span::styled("█", Style::default().fg(ACCENT)));
    }
    search.push(Span::styled(after, search_style));
    search.push(Span::raw("    "));
    search.push(Span::styled(
        view.page_title.clone(),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ));
    search.push(Span::raw("  "));
    search.push(Span::styled(
        view.user
            .as_ref()
            .map(|user| format!("{user} · 已登录"))
            .unwrap_or_else(|| "未登录".into()),
        Style::default().fg(MUTED),
    ));

    let banner_style = Style::default().fg(if view.error { ERROR } else { MUTED });
    let mut banner = vec![Span::styled(view.banner.clone(), banner_style)];
    if view.loading {
        banner.push(Span::raw("  "));
        banner.push(Span::styled("加载中…", Style::default().fg(WARNING)));
    }

    let lines = vec![Line::from(search), Line::from(banner)];
    let block = Block::bordered()
        .title(" 搜索与状态 ")
        .border_style(focus_border(state.focus == Focus::Search))
        .style(Style::default().bg(SURFACE));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_content(state: &PlayerTui, view: &ViewModel, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .title(format!(" {} ", view.page_title))
        .border_style(focus_border(state.focus == Focus::Content))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match view.page {
        Page::Lyrics => draw_lyrics(view, frame, inner),
        Page::Login => draw_login(view, frame, inner),
        Page::Toplists | Page::Playlists => draw_collections(state, view, frame, inner),
        _ => draw_songs(state, view, frame, inner),
    }
}

fn draw_songs(state: &PlayerTui, view: &ViewModel, frame: &mut Frame, area: Rect) {
    if view.songs.is_empty() {
        let hint = if view.loading {
            "加载中…"
        } else if view.page == Page::Search {
            "没有匹配的歌曲，回车搜索全部曲库"
        } else {
            "这里还没有歌曲"
        };
        frame.render_widget(Paragraph::new(hint).style(Style::default().fg(MUTED)), area);
        return;
    }

    let items = view
        .songs
        .iter()
        .enumerate()
        .map(|(index, song)| {
            let marker = if song.playing {
                "▶".to_string()
            } else {
                format!("{:>2}", index + 1)
            };
            let title_style = if song.playing {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!(" {marker} "), Style::default().fg(MUTED)),
                    Span::styled(song.title.clone(), title_style),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(song.subtitle.clone(), Style::default().fg(MUTED)),
                    Span::raw("  "),
                    Span::styled(song.duration.clone(), Style::default().fg(MUTED)),
                ]),
            ])
        })
        .collect::<Vec<_>>();

    let selected = if state.focus == Focus::Content {
        Some(state.content_index)
    } else {
        view.selected
    };
    let mut list_state = ListState::default().with_selected(selected);
    let highlight = if state.focus == Focus::Content {
        Style::default().bg(HIGHLIGHT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(HIGHLIGHT)
    };
    let list = List::new(items)
        .highlight_style(highlight)
        .repeat_highlight_symbol(false);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_collections(state: &PlayerTui, view: &ViewModel, frame: &mut Frame, area: Rect) {
    let items = view
        .collections
        .iter()
        .enumerate()
        .map(|(index, collection)| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!(" {:>2} ", index + 1), Style::default().fg(MUTED)),
                    Span::styled(collection.title.clone(), Style::default().fg(TEXT)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(collection.kind.clone(), Style::default().fg(MUTED)),
                ]),
            ])
        })
        .collect::<Vec<_>>();
    let selected = if state.focus == Focus::Content {
        Some(state.content_index.min(items.len().saturating_sub(1)))
    } else {
        Some(0)
    };
    let mut list_state = ListState::default().with_selected(selected);
    let list = List::new(items)
        .highlight_style(Style::default().bg(HIGHLIGHT))
        .repeat_highlight_symbol(false);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_lyrics(view: &ViewModel, frame: &mut Frame, area: Rect) {
    if !view.player.has_song {
        frame.render_widget(
            Paragraph::new("播放一首歌后这里显示歌词").style(Style::default().fg(MUTED)),
            area,
        );
        return;
    }
    let focus_line = view.active_lyric.unwrap_or(0);
    let height = usize::from(area.height);
    let start = focus_line.saturating_sub(height / 2);
    let lines = view
        .lyrics
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, lyric)| {
            let active = Some(index) == view.active_lyric;
            Line::from(Span::styled(
                lyric.text.clone(),
                if active {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(MUTED)
                },
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_login(view: &ViewModel, frame: &mut Frame, area: Rect) {
    let lines = match &view.login {
        LoginView::LoggedOut => vec![
            Line::from(Span::styled(
                "尚未登录",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "搜索、排行榜不需要登录；每日推荐与「我喜欢」需要。",
                Style::default().fg(MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[Enter] 生成登录二维码",
                Style::default().fg(ACCENT),
            )),
        ],
        LoginView::Qr { data, status } => {
            let mut lines = Vec::new();
            for row in pseudo_qr(data) {
                lines.push(Line::from(Span::styled(row, Style::default().fg(TEXT))));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(MUTED),
            )));
            lines.push(Line::from(Span::styled(
                "[Enter] 模拟扫码成功",
                Style::default().fg(ACCENT),
            )));
            lines
        }
        LoginView::LoggedIn { user } => vec![
            Line::from(Span::styled(
                format!("已登录：{user}"),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "本地演示会话，不会连接服务器",
                Style::default().fg(MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[Enter] 退出登录",
                Style::default().fg(ACCENT),
            )),
        ],
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn draw_player(view: &ViewModel, frame: &mut Frame, area: Rect) {
    let player = &view.player;
    let block = Block::bordered()
        .title(" 正在播放 ")
        .border_style(Style::default().fg(Color::Rgb(52, 61, 76)))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [info, progress] =
        Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(inner);
    let [left, meters] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(32)]).areas(info);

    let title = Line::from(Span::styled(
        player.title.clone(),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    ));
    let subtitle = Line::from(Span::styled(
        player.subtitle.clone(),
        Style::default().fg(MUTED),
    ));
    let like = if player.liked {
        "取消喜欢"
    } else {
        "喜欢"
    };
    let controls = Line::from(vec![
        Span::styled(
            if player.playing {
                "[暂停]"
            } else {
                "[播放]"
            },
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" 上一首 ", Style::default().fg(TEXT)),
        Span::styled("下一首 ", Style::default().fg(TEXT)),
        Span::styled(
            format!("{} ", player.mode_label()),
            Style::default().fg(MUTED),
        ),
        Span::styled(like, Style::default().fg(MUTED)),
    ]);
    let hint = Line::from(Span::styled(
        "空格 播放 · n/p 切歌 · m 模式 · l 喜欢 · [ ] 快进",
        Style::default().fg(MUTED),
    ));
    frame.render_widget(Paragraph::new(vec![title, subtitle, controls, hint]), left);

    let spectrum = Line::from(Span::styled(
        sparkline(&player.spectrum),
        Style::default().fg(ACCENT),
    ));
    let volume = Line::from(Span::styled(
        format!("音量 {:.0}%  -/+", player.volume * 100.0),
        Style::default().fg(MUTED),
    ));
    frame.render_widget(
        Paragraph::new(vec![spectrum, volume]).alignment(Alignment::Right),
        meters,
    );

    let ratio = if player.duration == 0 {
        0.0
    } else {
        (player.position / player.duration as f64).clamp(0.0, 1.0)
    };
    let label = format!(
        "{} / {}",
        format_duration((player.position * 1000.0) as u64),
        format_duration(player.duration)
    );
    frame.render_widget(
        LineGauge::default()
            .ratio(ratio)
            .label(label)
            .filled_style(Style::default().fg(ACCENT))
            .unfilled_style(Style::default().fg(Color::Rgb(52, 61, 76))),
        progress,
    );
}

fn focus_border(focused: bool) -> Style {
    Style::default().fg(if focused {
        ACCENT
    } else {
        Color::Rgb(52, 61, 76)
    })
}

fn sparkline(levels: &[u8]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    levels
        .iter()
        .map(|level| BARS[(usize::from(*level) * 7 / 100).min(7)])
        .collect()
}

fn pseudo_qr(data: &str) -> Vec<String> {
    const MODULES: usize = 21;
    let mut state = data
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| hash ^ u64::from(*byte));
    let mut rows = Vec::with_capacity(MODULES);
    for y in 0..MODULES {
        let mut row = String::with_capacity(MODULES * 2);
        for x in 0..MODULES {
            let dark = finder_module(x, y, MODULES).unwrap_or_else(|| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                state >> 33 & 1 == 1
            });
            row.push_str(if dark { "██" } else { "  " });
        }
        rows.push(row);
    }
    rows
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

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use ferriso_core::App;
    use ferriso_tui::ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::app::Player;

    fn screen(view: &ViewModel) -> String {
        let backend = TestBackend::new(110, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut renderer = PlayerTui::new();
        terminal
            .draw(|frame| renderer.draw(view, frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer[(x, y)].symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn terminal_layout_shows_the_shared_view_data() {
        let player = Player::new();
        let output = screen(&player.view());
        let compact: String = output
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(compact.contains("Ferriso"));
        assert!(compact.contains("夏日漱石"), "{output}");
        assert!(compact.contains("橘子海·浪潮上岸"));
        assert!(compact.contains("未在播放"));
        assert!(compact.contains("每日推荐"));
    }

    #[test]
    fn enter_plays_the_selected_row_and_tab_moves_focus() {
        let player = Player::new();
        let view = player.view();
        let mut renderer = PlayerTui::new();

        let outcome = renderer.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        assert!(matches!(
            outcome,
            Outcome::Message(Message::Play(Page::Discover, 0))
        ));

        renderer.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        );
        assert_eq!(renderer.focus, Focus::Player);
        let outcome = renderer.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
        );
        assert!(matches!(outcome, Outcome::Message(Message::PlayPause)));
    }

    #[test]
    fn focused_search_edits_locally_and_sends_the_draft() {
        let player = Player::new();
        let mut view = player.view();
        view.search = "海".into();
        let mut renderer = PlayerTui::new();
        renderer.focus = Focus::Search;
        renderer.search = "海".into();
        renderer.cursor = 1;

        let outcome = renderer.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Char('底'), KeyModifiers::NONE)),
        );
        assert!(matches!(
            outcome,
            Outcome::Message(Message::SearchChanged(text)) if text == "海底"
        ));

        let outcome = renderer.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert!(matches!(
            outcome,
            Outcome::Message(Message::SearchChanged(text)) if text == "海"
        ));
    }
}
