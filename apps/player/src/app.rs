//! A music-player application used to exercise Ferriso end to end.
//!
//! The state, messages, and view model here are frontend-neutral. The terminal
//! and iced renderers live in `tui.rs` and `gui.rs`; they share this data, not
//! a widget tree or a layout engine.

use std::time::Duration;

use ferriso_core::{App, Command, Subscription};

use crate::config::PlayerConfig;

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
    pub const NAV: [Self; 6] = [
        Self::Discover,
        Self::Toplists,
        Self::Playlists,
        Self::Search,
        Self::Queue,
        Self::Lyrics,
    ];

    pub fn label(self) -> &'static str {
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
    SeekRelative(f64),
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
    tick: Duration,
}

impl Player {
    pub fn new() -> Self {
        Self::with_config(PlayerConfig::default())
    }

    /// Build the model from layered configuration.
    pub fn with_config(mut config: PlayerConfig) -> Self {
        config.sanitize();
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
            volume: config.volume,
            mode: 0,
            liked: false,
            user: None,
            qr: None,
            qr_status: String::new(),
            banner: "回车播放 · Ctrl+空格 暂停 · Ctrl+←/→ 切歌".into(),
            error: false,
            loading: false,
            lyrics: lyrics(),
            tick: config.tick(),
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

    fn advance(&mut self) {
        if !self.playing {
            return;
        }
        let Some(song) = self.current_song() else {
            self.playing = false;
            return;
        };
        let duration = song.duration as f64;
        self.position += self.tick.as_secs_f64();
        if self.position >= duration {
            if self.mode == 1 {
                self.position = 0.0;
            } else {
                self.next();
            }
        }
    }

    fn view_model(&self) -> ViewModel {
        let page_title = match self.page {
            Page::Detail => self
                .detail
                .as_ref()
                .map(|(title, _, _)| title.clone())
                .unwrap_or_else(|| Page::Detail.label().to_string()),
            Page::Search if !self.search_query.is_empty() => {
                format!("搜索：{}", self.search_query)
            }
            page => page.label().to_string(),
        };

        let songs = self
            .songs()
            .iter()
            .enumerate()
            .map(|(index, song)| SongRow {
                title: song.title.clone(),
                subtitle: format!("{} · {}", song.artist, song.album),
                duration: format_duration(song.duration),
                playing: self.is_playing_row(self.page, index),
            })
            .collect();

        let nav = Page::NAV
            .iter()
            .map(|&page| NavItem {
                page,
                label: page.label(),
                selected: self.page == page
                    || (self.page == Page::Detail
                        && self
                            .detail
                            .as_ref()
                            .is_some_and(|(_, _, back)| *back == page)),
            })
            .collect();

        let lyrics = self
            .lyrics
            .iter()
            .map(|line| LyricRow {
                text: line.text.clone(),
            })
            .collect();

        let login = match (&self.user, &self.qr) {
            (Some(user), _) => LoginView::LoggedIn { user: user.clone() },
            (None, Some(data)) => LoginView::Qr {
                data: data.clone(),
                status: self.qr_status.clone(),
            },
            (None, None) => LoginView::LoggedOut,
        };

        let song = self.current_song();
        let spectrum = if self.playing {
            (0..28)
                .map(|index| {
                    let phase = self.position * 5.0 + f64::from(index) * 0.7;
                    let level = (phase.sin() * 0.5 + 0.5).mul_add(0.7, 0.3);
                    (level * 100.0) as u8
                })
                .collect()
        } else {
            vec![0; 28]
        };

        ViewModel {
            page: self.page,
            page_title,
            nav,
            search: self.search.clone(),
            user: self.user.clone(),
            banner: self.banner.clone(),
            error: self.error,
            loading: self.loading,
            songs,
            selected: self.selected,
            show_back: self.page == Page::Detail,
            collections: self
                .collections
                .iter()
                .map(|(title, kind)| CollectionRow {
                    title: title.clone(),
                    kind: kind.clone(),
                })
                .collect(),
            lyrics,
            active_lyric: self.active_lyric(),
            login,
            player: PlayerView {
                title: song
                    .map(|song| song.title.clone())
                    .unwrap_or_else(|| "未在播放".to_string()),
                subtitle: song
                    .map(|song| format!("{} · {}", song.artist, song.album))
                    .unwrap_or_default(),
                has_song: song.is_some(),
                playing: self.playing,
                position: self.position,
                duration: song.map(|song| song.duration).unwrap_or(0),
                volume: self.volume,
                mode: self.mode,
                liked: self.liked,
                spectrum,
            },
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Player {
    type Message = Message;
    type View = ViewModel;

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
            Message::Tick => self.advance(),
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
            Message::SeekRelative(delta) => {
                if let Some(song) = self.current_song() {
                    self.position = (self.position + delta).clamp(0.0, song.duration as f64);
                }
            }
            Message::SeekCommit => {
                self.banner = format!(
                    "跳转到 {}",
                    format_duration((self.position * 1000.0) as u64)
                );
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
                        kind == "热歌" || song.album.contains(&kind) || song.artist.contains(&kind)
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
        }
        Command::none()
    }

    fn view(&self) -> ViewModel {
        self.view_model()
    }

    fn subscriptions(&self) -> Subscription<Message> {
        Subscription::interval(1, self.tick, Message::Tick)
    }
}

#[derive(Clone, Debug)]
pub struct ViewModel {
    pub page: Page,
    pub page_title: String,
    pub nav: Vec<NavItem>,
    pub search: String,
    pub user: Option<String>,
    pub banner: String,
    pub error: bool,
    pub loading: bool,
    pub songs: Vec<SongRow>,
    pub selected: Option<usize>,
    pub show_back: bool,
    pub collections: Vec<CollectionRow>,
    pub lyrics: Vec<LyricRow>,
    pub active_lyric: Option<usize>,
    pub login: LoginView,
    pub player: PlayerView,
}

#[derive(Clone, Debug)]
pub struct NavItem {
    pub page: Page,
    pub label: &'static str,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct SongRow {
    pub title: String,
    pub subtitle: String,
    pub duration: String,
    pub playing: bool,
}

#[derive(Clone, Debug)]
pub struct CollectionRow {
    pub title: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct LyricRow {
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum LoginView {
    LoggedOut,
    Qr { data: String, status: String },
    LoggedIn { user: String },
}

#[derive(Clone, Debug)]
pub struct PlayerView {
    pub title: String,
    pub subtitle: String,
    pub has_song: bool,
    pub playing: bool,
    pub position: f64,
    pub duration: u64,
    pub volume: f64,
    pub mode: u8,
    pub liked: bool,
    pub spectrum: Vec<u8>,
}

impl PlayerView {
    pub fn mode_label(&self) -> &'static str {
        ["顺序播放", "单曲循环", "随机播放"][usize::from(self.mode) % 3]
    }
}

pub fn format_duration(milliseconds: u64) -> String {
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
        LyricLine {
            at: 0.0,
            text: "（前奏）".into(),
        },
        LyricLine {
            at: 12.0,
            text: "这是一段本地演示歌词".into(),
        },
        LyricLine {
            at: 20.0,
            text: "状态在 actor 里，界面只是快照".into(),
        },
        LyricLine {
            at: 28.0,
            text: "同一个 view model，两套原生布局".into(),
        },
        LyricLine {
            at: 36.0,
            text: "终端里是字符，窗口里是像素".into(),
        },
        LyricLine {
            at: 44.0,
            text: "焦点、滚动和文本光标由前端各自处理".into(),
        },
        LyricLine {
            at: 52.0,
            text: "领域状态只经过一次 update".into(),
        },
        LyricLine {
            at: 60.0,
            text: "—— Ferriso".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playing_updates_the_view_and_tick_advances_it() {
        let mut player = Player::new();
        player.update(Message::Play(Page::Discover, 0));

        let view = player.view();
        assert_eq!(view.player.title, "夏日漱石");
        assert!(view.player.playing);
        assert!(view.songs[0].playing);
        assert_eq!(view.selected, Some(0));

        player.update(Message::Tick);
        let view = player.view();
        assert!((view.player.position - player.tick.as_secs_f64()).abs() < f64::EPSILON);
        assert!(view.lyrics.len() > 1);
    }

    #[test]
    fn configuration_drives_volume_and_tick() {
        let mut player = Player::with_config(PlayerConfig {
            volume: 0.25,
            tick_ms: 250,
        });
        player.update(Message::Play(Page::Discover, 0));
        player.update(Message::Tick);

        let view = player.view();
        assert_eq!(view.player.volume, 0.25);
        assert!((view.player.position - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn login_is_a_snapshot_state_not_a_renderer_flag() {
        let mut player = Player::new();
        player.update(Message::OfferLoginQr);
        let view = player.view();
        assert!(matches!(view.login, LoginView::LoggedOut));

        player.update(Message::QrReady("https://example.invalid/qr".into()));
        let view = player.view();
        assert!(matches!(view.login, LoginView::Qr { .. }));

        player.update(Message::LoggedIn);
        let view = player.view();
        assert!(matches!(view.login, LoginView::LoggedIn { .. }));
        assert_eq!(view.user.as_deref(), Some("Ferriso 用户"));
    }
}
