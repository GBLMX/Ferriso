//! A music-player application used to exercise Ferriso end to end.
//!
//! There is no network here on purpose: the data is local and asynchronous loading is simulated,
//! so both renderers can be run and compared without an account or a server. The structure is
//! the one boxpigma's desktop window uses — sidebar, page, player bar — and every widget the
//! real app needs is present: navigation, search input, virtual rows, sliders, a QR login code,
//! lyrics, and a live spectrum.

use std::time::Duration;

use ferriso::prelude::*;

const TICK: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Discover,
    Toplists,
    Playlists,
    Search,
    Queue,
    Lyrics,
    Login,
    Detail,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Discover => "每日推荐",
            Self::Toplists => "排行榜",
            Self::Playlists => "推荐歌单",
            Self::Search => "搜索",
            Self::Queue => "播放队列",
            Self::Lyrics => "歌词",
            Self::Login => "登录",
            Self::Detail => "歌单详情",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u64,
}

impl Song {
    fn new(title: &str, artist: &str, album: &str, duration: u64) -> Self {
        Self {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            duration,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub at: f64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    Navigate(Page),
    SearchChanged(String),
    SearchSubmit,
    Refresh,
    SongsLoaded(Vec<Song>),
    SearchLoaded(Vec<Song>),
    Play(Page, usize),
    Selected(usize),
    PlayPause,
    Next,
    Previous,
    CycleMode,
    Seek(f64),
    SeekCommit,
    Volume(f64),
    ToggleLike,
    OpenCollection(usize),
    Back,
    Login,
    QrReady(String),
    LoggedIn,
    Logout,
    OfferLoginQr,
    Tick,
}

pub struct Player {
    page: Page,
    discover: Vec<Song>,
    search_results: Vec<Song>,
    collections: Vec<(String, String)>,
    detail: Option<(String, Vec<Song>, Page)>,
    queue: Vec<Song>,
    current: Option<usize>,
    selected: Option<usize>,
    search: String,
    search_query: String,
    playing: bool,
    position: f64,
    volume: f64,
    mode: u8,
    liked: bool,
    user: Option<String>,
    qr: Option<String>,
    qr_status: String,
    banner: String,
    error: bool,
    loading: bool,
    lyrics: Vec<LyricLine>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            page: Page::Discover,
            discover: catalog(),
            search_results: Vec::new(),
            collections: vec![
                ("飙升榜 · 每小时更新".into(), "热歌".into()),
                ("新歌榜 · 每日更新".into(), "新歌".into()),
                ("原创榜 · 每周更新".into(), "原创".into()),
                ("华语金曲 · 经典".into(), "华语".into()),
            ],
            detail: None,
            queue: Vec::new(),
            current: None,
            selected: Some(0),
            search: String::new(),
            search_query: String::new(),
            playing: false,
            position: 0.0,
            volume: 0.7,
            mode: 0,
            liked: false,
            user: None,
            qr: None,
            qr_status: String::new(),
            banner: "回车播放 · Ctrl+空格 暂停 · Ctrl+←/→ 切歌".into(),
            error: false,
            loading: false,
            lyrics: lyrics(),
        }
    }

    fn songs(&self) -> &[Song] {
        match self.page {
            Page::Search => &self.search_results,
            Page::Queue => &self.queue,
            Page::Detail => self
                .detail
                .as_ref()
                .map(|(_, songs, _)| songs.as_slice())
                .unwrap_or(&[]),
            _ => &self.discover,
        }
    }

    fn is_playing_row(&self, page: Page, index: usize) -> bool {
        if page == Page::Queue {
            return self.current == Some(index);
        }
        self.current
            .and_then(|current| self.queue.get(current))
            .zip(self.songs().get(index))
            .is_some_and(|(playing, shown)| playing.title == shown.title)
    }

    fn play(&mut self, page: Page, index: usize) {
        let (key, songs) = match page {
            Page::Search => ("搜索".to_string(), self.search_results.clone()),
            Page::Queue => {
                if index < self.queue.len() {
                    self.current = Some(index);
                    self.position = 0.0;
                    self.playing = true;
                    self.liked = false;
                }
                return;
            }
            Page::Detail => match &self.detail {
                Some((title, songs, _)) => (title.clone(), songs.clone()),
                None => return,
            },
            _ => ("每日推荐".to_string(), self.discover.clone()),
        };
        if index >= songs.len() {
            return;
        }
        self.queue = songs;
        self.current = Some(index);
        self.selected = Some(index);
        self.position = 0.0;
        self.playing = true;
        self.liked = false;
        self.banner = format!("正在播放 · {key}");
        self.error = false;
    }

    fn current_song(&self) -> Option<&Song> {
        self.current.and_then(|index| self.queue.get(index))
    }

    fn next(&mut self) {
        let Some(current) = self.current else {
            return;
        };
        if self.queue.is_empty() {
            return;
        }
        self.current = Some((current + 1) % self.queue.len());
        self.position = 0.0;
        self.playing = true;
        self.liked = false;
        self.selected = self.current;
    }

    fn previous(&mut self) {
        let Some(current) = self.current else {
            return;
        };
        if self.queue.is_empty() {
            return;
        }
        self.current = Some((current + self.queue.len() - 1) % self.queue.len());
        self.position = 0.0;
        self.playing = true;
        self.liked = false;
        self.selected = self.current;
    }

    fn load_songs(&mut self) -> Command<Message> {
        self.loading = true;
        Command::perform(
            async {
                tokio::time::sleep(Duration::from_millis(350)).await;
                catalog()
            },
            Message::SongsLoaded,
        )
    }

    fn search(&mut self) -> Command<Message> {
        let query = self.search.trim().to_string();
        if query.is_empty() {
            return Command::none();
        }
        self.search_query = query.clone();
        self.loading = true;
        self.page = Page::Search;
        Command::perform(
            async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let all = catalog();
                all.into_iter()
                    .filter(|song| {
                        song.title.contains(&query)
                            || song.artist.contains(&query)
                            || song.album.contains(&query)
                    })
                    .collect()
            },
            Message::SearchLoaded,
        )
    }

    fn login(&mut self) -> Command<Message> {
        self.qr_status = "正在生成二维码…".into();
        Command::perform(
            async {
                tokio::time::sleep(Duration::from_millis(250)).await;
                "https://music.163.com/login?codekey=ferriso-demo".to_string()
            },
            Message::QrReady,
        )
    }

    fn active_lyric(&self) -> Option<usize> {
        if self.lyrics.is_empty() {
            return None;
        }
        match self.lyrics.partition_point(|line| line.at <= self.position) {
            0 => None,
            index => Some(index - 1),
        }
    }

    fn content(&self) -> Node<'_, Message> {
        match self.page {
            Page::Lyrics => self.lyrics_page(),
            Page::Login => self.login_page(),
            Page::Toplists | Page::Playlists => self.collections_page(),
            _ => self.songs_page(),
        }
    }

    fn songs_page(&self) -> Node<'_, Message> {
        let songs = self.songs();
        let page = self.page;
        let title = match page {
            Page::Detail => self
                .detail
                .as_ref()
                .map(|(title, _, _)| title.as_str())
                .unwrap_or("歌单详情"),
            Page::Search if !self.search_query.is_empty() => self.search_query.as_str(),
            other => other.label(),
        };

        let mut header = row()
            .spacing(8)
            .align_y(Align::Center)
            .push(text(title).size(18).bold())
            .push(text(format!("{} 首", songs.len())).size(12).tone(Tone::Muted))
            .push(space());
        if self.page == Page::Detail {
            header = header.push(
                button("返回", Message::Back)
                    .kind(ButtonKind::Ghost)
                    .padding(Padding::symmetric(8, 2)),
            );
        }
        header = header.push(
            button("刷新", Message::Refresh)
                .kind(ButtonKind::Ghost)
                .padding(Padding::symmetric(8, 2)),
        );

        if songs.is_empty() {
            let hint = if self.loading {
                "加载中…"
            } else if self.page == Page::Search {
                "没有匹配的歌曲，回车搜索全部曲库"
            } else {
                "这里还没有歌曲"
            };
            return column()
                .spacing(10)
                .height(Length::Fill)
                .padding(Padding::symmetric(4, 2))
                .push(header)
                .push(text(hint).size(13).tone(Tone::Muted))
                .into();
        }

        let items = songs
            .iter()
            .enumerate()
            .map(|(index, song)| {
                let playing = self.is_playing_row(page, index);
                ListItem::new(song.title.clone())
                    .leading(if playing {
                        "▶".to_string()
                    } else {
                        format!("{:>2}", index + 1)
                    })
                    .subtitle(format!("{} · {}", song.artist, song.album))
                    .trailing(format_duration(song.duration))
                    .accent(playing)
            })
            .collect::<Vec<_>>();

        let list = List::new(items)
            .selected(self.selected)
            .on_select(Message::Selected)
            .on_activate(move |index| Message::Play(page, index))
            .height(Length::Fill);

        column()
            .spacing(8)
            .height(Length::Fill)
            .padding(Padding::symmetric(4, 2))
            .push(header)
            .push(list)
            .into()
    }

    fn collections_page(&self) -> Node<'_, Message> {
        let items = self
            .collections
            .iter()
            .enumerate()
            .map(|(index, (title, kind))| {
                ListItem::new(title.clone())
                    .leading(format!("{:>2}", index + 1))
                    .subtitle(kind.clone())
                    .accent(index == 0)
            })
            .collect::<Vec<_>>();
        let list = List::new(items)
            .on_activate(Message::OpenCollection)
            .height(Length::Fill);
        column()
            .spacing(8)
            .height(Length::Fill)
            .padding(Padding::symmetric(4, 2))
            .push(
                row()
                    .spacing(8)
                    .align_y(Align::Center)
                    .push(text(self.page.label()).size(18).bold())
                    .push(text("点击一项打开").size(12).tone(Tone::Muted)),
            )
            .push(list)
            .into()
    }

    fn lyrics_page(&self) -> Node<'_, Message> {
        if self.current_song().is_none() {
            return column()
                .padding(Padding::all(12))
                .push(text("播放一首歌后这里显示歌词").tone(Tone::Muted))
                .into();
        }
        let active = self.active_lyric();
        let mut lines = column().spacing(10).padding(Padding::symmetric(24, 10));
        for (index, line) in self.lyrics.iter().enumerate() {
            let is_active = active == Some(index);
            lines = lines.push(
                text(line.text.clone())
                    .size(if is_active { 20 } else { 14 })
                    .tone(if is_active { Tone::Accent } else { Tone::Muted })
                    .weight(if is_active { Weight::Bold } else { Weight::Normal }),
            );
        }
        Scroll::new(lines).height(Length::Fill).into()
    }

    fn login_page(&self) -> Node<'_, Message> {
        let content = if let Some(user) = &self.user {
            column()
                .spacing(8)
                .align_x(Align::Center)
                .push(text(format!("已登录：{user}")).size(18).bold())
                .push(text("本地演示会话，不会连接服务器").size(12).tone(Tone::Muted))
                .push(button("退出登录", Message::Logout))
        } else if let Some(qr) = &self.qr {
            column()
                .spacing(8)
                .align_x(Align::Center)
                .push(Qr::new(qr.clone()).size(240).quiet_zone(2))
                .push(text(self.qr_status.clone()).size(13).tone(Tone::Muted))
                .push(
                    row()
                        .spacing(8)
                        .push(button("刷新二维码", Message::Login))
                        .push(
                            button("模拟扫码成功", Message::LoggedIn)
                                .kind(ButtonKind::Primary),
                        ),
                )
        } else {
            column()
                .spacing(8)
                .align_x(Align::Center)
                .push(text("尚未登录").size(18).bold())
                .push(
                    text("搜索、排行榜不需要登录；每日推荐与「我喜欢」需要。")
                        .size(12)
                        .tone(Tone::Muted),
                )
                .push(button("生成登录二维码", Message::OfferLoginQr))
        };
        column()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Align::Center)
            .align_y(Align::Center)
            .push(content)
            .into()
    }

    fn sidebar(&self) -> Node<'_, Message> {
        let mut nav = column().spacing(4);
        for page in [
            Page::Discover,
            Page::Toplists,
            Page::Playlists,
            Page::Search,
            Page::Queue,
            Page::Lyrics,
        ] {
            let selected = self.page == page
                || (self.page == Page::Detail
                    && self
                        .detail
                        .as_ref()
                        .is_some_and(|(_, _, back)| *back == page));
            nav = nav.push(
                button(page.label(), Message::Navigate(page))
                    .width(Length::Fill)
                    .padding(Padding::symmetric(10, 6))
                    .kind(if selected {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Ghost
                    }),
            );
        }

        let session = if self.user.is_some() {
            button("退出登录", Message::Logout)
                .width(Length::Fill)
                .padding(Padding::symmetric(10, 6))
        } else {
            button(
                if self.page == Page::Login {
                    "登录中…"
                } else {
                    "扫码登录"
                },
                Message::Navigate(Page::Login),
            )
            .width(Length::Fill)
            .padding(Padding::symmetric(10, 6))
        };

        column()
            .width(Length::Fixed(190))
            .height(Length::Fill)
            .fill(Tone::Surface)
            .padding(Padding::all(14))
            .spacing(8)
            .push(text("Ferriso").size(22).bold().tone(Tone::Accent))
            .push(text("one model · two renderers").size(11).tone(Tone::Muted))
            .push(nav)
            .push(space())
            .push(session)
            .push(
                text("Ctrl+空格 播放/暂停\nCtrl+←/→ 上一首/下一首\nTab 移动焦点 · Enter 激活")
                    .size(11)
                    .tone(Tone::Muted),
            )
            .into()
    }

    fn top_bar(&self) -> Node<'_, Message> {
        let badge = match &self.user {
            Some(user) => format!("{user} · 已登录"),
            None => "未登录".to_string(),
        };
        let mut bar = column()
            .spacing(6)
            .push(
                row()
                    .spacing(8)
                    .align_y(Align::Center)
                    .push(text(self.page.label()).size(22).bold())
                    .push(
                        TextInput::new(self.search.as_str())
                            .placeholder("搜索歌曲，回车确认")
                            .on_input(Message::SearchChanged)
                            .on_submit(Message::SearchSubmit)
                            .padding(Padding::symmetric(10, 6)),
                    )
                    .push(text(badge).size(12).tone(Tone::Muted)),
            )
            .push(
                text(self.banner.clone())
                    .size(12)
                    .tone(if self.error { Tone::Danger } else { Tone::Muted }),
            );
        if self.loading {
            bar = bar.push(text("加载中…").size(12).tone(Tone::Warning));
        }
        bar.into()
    }

    fn player_bar(&self) -> Node<'_, Message> {
        let song = self.current_song();
        let title = song
            .map(|song| song.title.clone())
            .unwrap_or_else(|| "未在播放".to_string());
        let artist = song
            .map(|song| format!("{} · {}", song.artist, song.album))
            .unwrap_or_default();
        let duration = song.map(|song| song.duration).unwrap_or(0);

        let art = Image::new(None)
            .alt("♪")
            .width(Length::Fixed(64))
            .height(Length::Fixed(64))
            .radius(8);

        let play_label = if self.playing { "暂停" } else { "播放" };
        let like_label = if self.liked { "取消喜欢" } else { "喜欢" };
        let mode = ["顺序播放", "单曲循环", "随机播放"][usize::from(self.mode) % 3];

        let controls = row()
            .spacing(6)
            .align_y(Align::Center)
            .push(
                button("上一首", Message::Previous)
                    .kind(ButtonKind::Ghost)
                    .padding(Padding::symmetric(10, 4)),
            )
            .push(
                button(play_label, Message::PlayPause)
                    .kind(ButtonKind::Primary)
                    .padding(Padding::symmetric(14, 4)),
            )
            .push(
                button("下一首", Message::Next)
                    .kind(ButtonKind::Ghost)
                    .padding(Padding::symmetric(10, 4)),
            )
            .push(
                button(mode, Message::CycleMode)
                    .kind(ButtonKind::Ghost)
                    .padding(Padding::symmetric(10, 4)),
            )
            .push(
                button(like_label, Message::ToggleLike)
                    .kind(ButtonKind::Ghost)
                    .padding(Padding::symmetric(10, 4)),
            );

        let info = column()
            .spacing(4)
            .width(Length::Fill)
            .push(text(title).size(16).bold())
            .push(text(artist).size(12).tone(Tone::Muted))
            .push(controls);

        let volume = row()
            .spacing(6)
            .width(Length::Fixed(140))
            .align_y(Align::Center)
            .push(text("音量").size(11).tone(Tone::Muted))
            .push(
                Slider::new(0.0, 1.0, self.volume)
                    .on_change(Message::Volume)
                    .width(Length::Fixed(96)),
            );

        let position = self.position.clamp(0.0, (duration as f64).max(0.0));
        let progress = row()
            .spacing(8)
            .align_y(Align::Center)
            .push(
                text(format_duration((position * 1000.0) as u64))
                    .size(11)
                    .tone(Tone::Muted),
            )
            .push(
                Slider::new(0.0, (duration as f64).max(0.001), position)
                    .on_change(Message::Seek)
                    .on_release(Message::SeekCommit),
            )
            .push(
                text(format_duration(duration))
                    .size(11)
                    .tone(Tone::Muted),
            );

        column()
            .spacing(8)
            .padding(Padding::all(12))
            .fill(Tone::Surface)
            .radius(10)
            .push(
                row()
                    .spacing(16)
                    .align_y(Align::Center)
                    .push(art)
                    .push(info)
                    .push(self.spectrum())
                    .push(volume),
            )
            .push(progress)
            .into()
    }

    fn spectrum(&self) -> Node<'_, Message> {
        let mut bars = row().spacing(2).height(Length::Fixed(40)).align_y(Align::End);
        if !self.playing {
            return bars.width(Length::Fixed(120)).into();
        }
        for index in 0..28 {
            let phase = self.position * 5.0 + f64::from(index) * 0.7;
            let level = (phase.sin() * 0.5 + 0.5).mul_add(0.7, 0.3);
            let height = (level * 36.0).max(2.0) as u16;
            bars = bars.push(
                column()
                    .width(Length::Fixed(4))
                    .height(Length::Fixed(height))
                    .fill(Tone::Accent)
                    .radius(1),
            );
        }
        bars.width(Length::Fixed(120)).into()
    }
}

impl App for Player {
    type Message = Message;

    fn init(&mut self) -> Command<Message> {
        self.load_songs()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Navigate(page) => {
                self.page = page;
                self.selected = Some(0);
                if page == Page::Login && self.user.is_none() && self.qr.is_none() {
                    return self.login();
                }
            }
            Message::SearchChanged(value) => self.search = value,
            Message::SearchSubmit => return self.search(),
            Message::Refresh => {
                if self.page == Page::Search {
                    return self.search();
                }
                return self.load_songs();
            }
            Message::SongsLoaded(songs) => {
                self.loading = false;
                self.discover = songs;
                self.selected = Some(0);
                self.banner = "每日推荐已更新".into();
                self.error = false;
            }
            Message::SearchLoaded(songs) => {
                self.loading = false;
                self.search_results = songs;
                self.selected = Some(0);
                self.banner = format!("搜索「{}」完成", self.search_query);
                self.error = false;
            }
            Message::Play(page, index) => self.play(page, index),
            Message::Selected(index) => self.selected = Some(index),
            Message::PlayPause => {
                if self.current.is_some() {
                    self.playing = !self.playing;
                } else if !self.discover.is_empty() {
                    self.play(Page::Discover, 0);
                    return Command::none();
                }
                self.banner = if self.playing {
                    "正在播放".into()
                } else {
                    "已暂停".into()
                };
                self.error = false;
            }
            Message::Next => self.next(),
            Message::Previous => self.previous(),
            Message::CycleMode => {
                self.mode = (self.mode + 1) % 3;
                self.banner = format!(
                    "播放模式：{}",
                    ["顺序播放", "单曲循环", "随机播放"][usize::from(self.mode)]
                );
                self.error = false;
            }
            Message::Seek(value) => self.position = value,
            Message::SeekCommit => {
                self.banner = format!("跳转到 {}", format_duration((self.position * 1000.0) as u64));
                self.error = false;
            }
            Message::Volume(value) => self.volume = value.clamp(0.0, 1.0),
            Message::ToggleLike => {
                if self.user.is_none() {
                    self.banner = "登录后才能收藏".into();
                    self.error = true;
                } else if self.current.is_some() {
                    self.liked = !self.liked;
                    self.banner = if self.liked {
                        "已加入我喜欢".into()
                    } else {
                        "已取消喜欢".into()
                    };
                    self.error = false;
                }
            }
            Message::OpenCollection(index) => {
                let Some((title, kind)) = self.collections.get(index).cloned() else {
                    return Command::none();
                };
                let songs = catalog()
                    .into_iter()
                    .filter(|song| {
                        kind == "热歌"
                            || song.album.contains(&kind)
                            || song.artist.contains(&kind)
                    })
                    .collect::<Vec<_>>();
                let (back, songs) = if songs.is_empty() {
                    (Page::Toplists, catalog())
                } else {
                    (Page::Toplists, songs)
                };
                self.detail = Some((title.clone(), songs, back));
                self.page = Page::Detail;
                self.selected = Some(0);
                self.banner = format!("打开歌单：{title}");
                self.error = false;
            }
            Message::Back => {
                if let Some((_, _, back)) = self.detail.take() {
                    self.page = back;
                } else {
                    self.page = Page::Discover;
                }
            }
            Message::OfferLoginQr => return self.login(),
            Message::Login => return self.login(),
            Message::QrReady(qr) => {
                self.qr = Some(qr);
                self.qr_status = "等待扫码…（演示二维码）".into();
                self.banner = "请用网易云音乐 App 扫码".into();
                self.error = false;
            }
            Message::LoggedIn => {
                self.user = Some("Ferriso 用户".into());
                self.qr = None;
                self.page = Page::Discover;
                self.banner = "登录成功（演示）".into();
                self.error = false;
            }
            Message::Logout => {
                self.user = None;
                self.qr = None;
                self.qr_status.clear();
                self.banner = "已退出登录".into();
                self.error = false;
            }
            Message::Tick => {
                if !self.playing {
                    return Command::none();
                }
                let Some(song) = self.current_song() else {
                    self.playing = false;
                    return Command::none();
                };
                let duration = song.duration as f64;
                self.position += TICK.as_secs_f64();
                if self.position >= duration {
                    if self.mode == 1 {
                        self.position = 0.0;
                    } else {
                        self.next();
                    }
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Node<'_, Message> {
        let right = column()
            .spacing(8)
            .height(Length::Fill)
            .push(self.top_bar())
            .push(self.content())
            .push(self.player_bar());

        row()
            .height(Length::Fill)
            .push(self.sidebar())
            .push(right)
            .into()
    }

    fn title(&self) -> &str {
        "Ferriso · music"
    }

    fn tick_interval(&self) -> Option<Duration> {
        Some(TICK)
    }

    fn tick(&mut self) -> Command<Message> {
        self.update(Message::Tick)
    }

    fn on_key(&mut self, event: KeyEvent) -> Command<Message> {
        if !event.is_press() || !event.modifiers.is_shortcut() {
            return Command::none();
        }
        let message = match event.key {
            Key::Space => Message::PlayPause,
            Key::Left => Message::Previous,
            Key::Right => Message::Next,
            _ => return Command::none(),
        };
        self.update(message)
    }
}

fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn catalog() -> Vec<Song> {
    vec![
        Song::new("夏日漱石", "橘子海", "浪潮上岸", 264_000),
        Song::new("我用什么把你留住", "福禄寿", "我用什么把你留住", 315_000),
        Song::new("海底", "一支榴莲", "海底", 256_000),
        Song::new("漠河舞厅", "柳爽", "漠河舞厅", 326_000),
        Song::new("这世界那么多人", "莫文蔚", "这世界那么多人", 271_000),
        Song::new("孤勇者", "陈奕迅", "孤勇者", 256_000),
        Song::new("起风了", "买辣椒也用券", "起风了", 325_000),
        Song::new("泸沽湖", "麻园诗人", "泸沽湖", 289_000),
        Song::new("程艾影", "赵雷", "署前街少年", 312_000),
        Song::new("我记得", "赵雷", "署前街少年", 329_000),
        Song::new("New Boy", "朴树", "我去2000年", 216_000),
        Song::new("火车驶向云外，梦安魂于九霄", "刺猬", "生之响往", 345_000),
    ]
}

fn lyrics() -> Vec<LyricLine> {
    vec![
        LyricLine { at: 0.0, text: "（前奏）".into() },
        LyricLine { at: 12.0, text: "这是一段本地演示歌词".into() },
        LyricLine { at: 20.0, text: "渲染器换了，歌词的时钟没有换".into() },
        LyricLine { at: 28.0, text: "同一个 model，同一个 view".into() },
        LyricLine { at: 36.0, text: "终端里是字符，窗口里是像素".into() },
        LyricLine { at: 44.0, text: "焦点、滚动和文本光标由后端各自处理".into() },
        LyricLine { at: 52.0, text: "状态只经过一次 update".into() },
        LyricLine { at: 60.0, text: "—— Ferriso".into() },
    ]
}
