use std::time::Instant;

use ferriso_gui::iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Radians, Shadow, Subscription,
    Task, Theme, Vector,
    font::Weight,
    gradient, keyboard,
    widget::{
        Space, button,
        button::{Status as ButtonStatus, Style as ButtonStyle},
        column, container,
        float::Float,
        row, text,
    },
    window,
};
use ferriso_gui::{Renderer, Settings};
use ferriso_keymap::Keymap;
use ferriso_twozero48::{Message, Move, Status, TwoZero48, View};

type GuiMessage = ferriso_gui::Message<View, Message, Local>;

const FX_MOVE_MS: f32 = 170.0;
const FX_MERGE_MS: f32 = 140.0;
const FX_SPAWN_MS: f32 = 150.0;
const FX_TOTAL_MS: f32 = 360.0;
const CELL_PITCH: f32 = 106.0;

const INK: Color = Color::from_rgb(0.30, 0.24, 0.19);
const MUTED: Color = Color::from_rgb(0.49, 0.41, 0.34);
const BOARD: Color = Color::from_rgb(0.72, 0.66, 0.59);
const EMPTY: Color = Color::from_rgb(0.85, 0.80, 0.74);
const ACCENT: Color = Color::from_rgb(0.77, 0.36, 0.20);

#[derive(Clone, Debug)]
enum Local {
    Frame(Instant),
}

struct Animation {
    start: Instant,
    spawned: [[bool; 4]; 4],
    merged: [[bool; 4]; 4],
    slides: [[Vector; 4]; 4],
    scored: bool,
}

struct Gui {
    keymap: Keymap<Message>,
    now: Instant,
    previous: Option<[[u32; 4]; 4]>,
    previous_score: usize,
    previous_seq: u64,
    animation: Option<Animation>,
}

impl Gui {
    fn new() -> Self {
        let mut keymap = Keymap::new();
        for (binding, message) in [
            ("ArrowLeft", Message::Move(Move::Left)),
            ("ArrowRight", Message::Move(Move::Right)),
            ("ArrowUp", Message::Move(Move::Up)),
            ("ArrowDown", Message::Move(Move::Down)),
            ("a", Message::Move(Move::Left)),
            ("d", Message::Move(Move::Right)),
            ("w", Message::Move(Move::Up)),
            ("s", Message::Move(Move::Down)),
            ("r", Message::Restart),
        ] {
            keymap
                .bind_str(binding, message)
                .expect("static 2048 binding is valid");
        }
        Self {
            keymap,
            now: Instant::now(),
            previous: None,
            previous_score: 0,
            previous_seq: 0,
            animation: None,
        }
    }

    fn tile_fx(&self, row: usize, column: usize, value: u32) -> (f32, Vector) {
        if value == 0 {
            return (1.0, Vector::ZERO);
        }
        let Some(animation) = &self.animation else {
            return (1.0, Vector::ZERO);
        };
        let elapsed = self
            .now
            .saturating_duration_since(animation.start)
            .as_secs_f32()
            * 1000.0;

        if animation.spawned[row][column] {
            let pop = ((elapsed - FX_MOVE_MS) / FX_SPAWN_MS).clamp(0.0, 1.0);
            return (0.66 + 0.34 * (1.0 - (1.0 - pop).powi(3)), Vector::ZERO);
        }

        let progress = (elapsed / FX_MOVE_MS).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        let offset = animation.slides[row][column] * (1.0 - eased);
        let merge = ((elapsed - FX_MERGE_MS) / FX_SPAWN_MS).clamp(0.0, 1.0);
        let scale = if animation.merged[row][column] {
            1.0 + 0.14 * (std::f32::consts::PI * merge).sin()
        } else {
            1.0
        };
        (scale, offset)
    }

    fn score_scale(&self) -> f32 {
        let Some(animation) = &self.animation else {
            return 1.0;
        };
        if !animation.scored {
            return 1.0;
        }
        let elapsed = self
            .now
            .saturating_duration_since(animation.start)
            .as_secs_f32()
            * 1000.0;
        let progress = (elapsed / 220.0).clamp(0.0, 1.0);
        1.0 + 0.16 * (std::f32::consts::PI * progress).sin()
    }
}

impl Renderer<View, Message, Local> for Gui {
    fn view<'a>(&'a self, view: &'a View) -> Element<'a, GuiMessage> {
        let title = ["2", "0", "4", "8"]
            .iter()
            .fold(row![].spacing(0), |row, digit| {
                row.push(
                    text(*digit)
                        .size(48)
                        .font(Font {
                            weight: Weight::Bold,
                            ..Font::default()
                        })
                        .color(match *digit {
                            "2" => Color::from_rgb(0.35, 0.28, 0.22),
                            "0" => Color::from_rgb(0.72, 0.58, 0.24),
                            "4" => Color::from_rgb(0.82, 0.43, 0.24),
                            _ => Color::from_rgb(0.66, 0.25, 0.16),
                        }),
                )
            });

        let header = row![
            column![title, text("merge · slide · repeat").size(12).color(MUTED)].spacing(2),
            Space::new().width(Length::Fill),
            float(stat("SCORE", view.score), self.score_scale()),
            stat("TARGET", view.winning as usize),
            button("↻")
                .padding([10, 14])
                .style(control_style)
                .on_press(GuiMessage::User(Message::Restart)),
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        let grid = view.cells.iter().enumerate().fold(
            column![].spacing(10),
            |grid, (row_index, cells)| {
                let row =
                    cells
                        .iter()
                        .enumerate()
                        .fold(row![].spacing(10), |row, (column, value)| {
                            let (scale, offset) = self.tile_fx(row_index, column, *value);
                            row.push(float(tile(*value), Animated { scale, offset }))
                        });
                grid.push(row)
            },
        );

        let board = container(grid)
            .padding(14)
            .style(|_theme| container::Style {
                background: Some(BOARD.into()),
                border: Border {
                    color: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
                    width: 1.0,
                    radius: 20.0.into(),
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.30, 0.20, 0.12, 0.28),
                    offset: Vector::new(0.0, 10.0),
                    blur_radius: 24.0,
                },
                ..Default::default()
            });

        let controls = row![
            control("←", Message::Move(Move::Left)),
            control("↑", Message::Move(Move::Up)),
            control("↓", Message::Move(Move::Down)),
            control("→", Message::Move(Move::Right)),
            Space::new().width(Length::Fill),
            status_badge(view.status),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let footer = row![
            text("ARROWS / WASD").size(11).color(MUTED),
            text("R").size(11).color(ACCENT),
            text("RESTART").size(11).color(MUTED),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let content = column![header, board, controls, footer]
            .spacing(18)
            .width(Length::Shrink);
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(28)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(backdrop_style)
            .into()
    }

    fn subscription(&self, _view: &View) -> Subscription<GuiMessage> {
        let keys = keyboard::listen()
            .with(self.keymap.clone())
            .filter_map(|(keymap, event)| {
                let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                    return None;
                };
                let chord = ferriso_gui::keymap::chord(&key, modifiers)?;
                keymap.resolve(&chord).copied().map(GuiMessage::User)
            });
        let frames = if self.animation.is_some() {
            window::frames().map(|now| GuiMessage::Local(Local::Frame(now)))
        } else {
            Subscription::none()
        };
        Subscription::batch([keys, frames])
    }

    fn update(&mut self, local: Local, _view: &View) -> Task<GuiMessage> {
        let Local::Frame(now) = local;
        self.now = now;
        if self.animation.as_ref().is_some_and(|animation| {
            now.duration_since(animation.start).as_secs_f32() * 1000.0 >= FX_TOTAL_MS
        }) {
            self.animation = None;
        }
        Task::none()
    }

    fn on_snapshot(&mut self, view: &View) {
        let previous = self.previous.replace(view.cells);
        let mut spawned = [[false; 4]; 4];
        let mut merged = [[false; 4]; 4];
        let mut changed = false;
        for row in 0..4 {
            for column in 0..4 {
                let old = previous.map_or(0, |previous| previous[row][column]);
                let new = view.cells[row][column];
                spawned[row][column] =
                    view.spawned[row][column] || (previous.is_none() && new != 0);
                merged[row][column] = old != 0 && new > old;
                changed |= spawned[row][column] || merged[row][column];
            }
        }
        let direction = previous
            .and((view.move_seq != self.previous_seq).then_some(view.last_move))
            .flatten();
        let slides = previous
            .zip(direction)
            .map(|(previous, direction)| slide_offsets(&previous, &view.cells, direction, &spawned))
            .unwrap_or([[Vector::ZERO; 4]; 4]);
        let scored = previous.is_some() && view.score != self.previous_score;
        if changed || direction.is_some() || scored {
            self.now = Instant::now();
            self.animation = Some(Animation {
                start: self.now,
                spawned,
                merged,
                slides,
                scored,
            });
        }
        self.previous_score = view.score;
        self.previous_seq = view.move_seq;
    }

    fn title(&self, view: &View) -> Option<String> {
        Some(format!("Ferriso · 2048 · {}", view.score))
    }

    fn theme(&self, _view: &View) -> Theme {
        Theme::Light
    }
}

#[derive(Clone, Copy)]
struct Animated {
    scale: f32,
    offset: Vector,
}

impl From<f32> for Animated {
    fn from(scale: f32) -> Self {
        Self {
            scale,
            offset: Vector::ZERO,
        }
    }
}

fn float<'a>(
    element: Element<'a, GuiMessage>,
    animation: impl Into<Animated>,
) -> Element<'a, GuiMessage> {
    let Animated { scale, offset } = animation.into();
    let offset = if offset == Vector::ZERO && (scale - 1.0).abs() > 0.001 {
        Vector::new(0.001, 0.0)
    } else {
        offset
    };
    if (scale - 1.0).abs() <= 0.001 && offset == Vector::ZERO {
        return element;
    }
    Float::new(element)
        .scale(scale)
        .translate(move |_, _| offset)
        .into()
}

fn stat(label: &'static str, value: usize) -> Element<'static, GuiMessage> {
    container(
        column![
            text(label).size(10).color(MUTED),
            text(value.to_string())
                .size(20)
                .font(Font {
                    weight: Weight::Bold,
                    ..Font::default()
                })
                .color(INK),
        ]
        .spacing(1),
    )
    .padding([7, 12])
    .style(|_theme| container::Style {
        background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.55).into()),
        border: Border {
            color: Color::from_rgba(0.55, 0.45, 0.36, 0.18),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn control(label: &'static str, message: Message) -> Element<'static, GuiMessage> {
    button(text(label).size(18).color(INK))
        .padding([10, 16])
        .style(control_style)
        .on_press(GuiMessage::User(message))
        .into()
}

fn status_badge(status: Status) -> Element<'static, GuiMessage> {
    let (label, color) = match status {
        Status::Playing => ("PLAYING", Color::from_rgb(0.42, 0.34, 0.27)),
        Status::Won => ("YOU WIN", Color::from_rgb(0.24, 0.49, 0.29)),
        Status::Lost => ("GAME OVER", Color::from_rgb(0.66, 0.22, 0.18)),
    };
    container(text(label).size(10).color(Color::WHITE))
        .padding([6, 10])
        .style(move |_theme| container::Style {
            background: Some(color.into()),
            border: Border {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.10),
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn tile(value: u32) -> Element<'static, GuiMessage> {
    let label = if value == 0 {
        String::new()
    } else {
        value.to_string()
    };
    let text_color = if value <= 4 {
        Color::from_rgb(0.42, 0.37, 0.32)
    } else {
        Color::WHITE
    };
    let size = match value {
        0..=64 => 34,
        65..=512 => 28,
        _ => 22,
    };
    container(
        text(label)
            .size(size)
            .font(Font {
                weight: Weight::Bold,
                ..Font::default()
            })
            .color(text_color),
    )
    .width(Length::Fixed(96.0))
    .height(Length::Fixed(96.0))
    .center_x(Length::Fixed(96.0))
    .center_y(Length::Fixed(96.0))
    .style(move |_theme| tile_style(value))
    .into()
}

fn backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(INK),
        background: Some(Background::Gradient(
            gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2))
                .add_stop(0.0, Color::from_rgb(0.98, 0.95, 0.90))
                .add_stop(0.55, Color::from_rgb(0.95, 0.89, 0.81))
                .add_stop(1.0, Color::from_rgb(0.90, 0.82, 0.72))
                .into(),
        )),
        ..Default::default()
    }
}

fn tile_style(value: u32) -> container::Style {
    let color = match value {
        0 => EMPTY,
        2 => Color::from_rgb(0.93, 0.89, 0.83),
        4 => Color::from_rgb(0.92, 0.86, 0.75),
        8 => Color::from_rgb(0.94, 0.68, 0.45),
        16 => Color::from_rgb(0.93, 0.55, 0.35),
        32 => Color::from_rgb(0.91, 0.45, 0.30),
        64 => Color::from_rgb(0.86, 0.34, 0.21),
        128 => Color::from_rgb(0.86, 0.73, 0.36),
        256 => Color::from_rgb(0.84, 0.68, 0.28),
        512 => Color::from_rgb(0.82, 0.64, 0.22),
        _ => Color::from_rgb(0.42, 0.35, 0.26),
    };
    container::Style {
        background: Some(color.into()),
        border: Border {
            color: Color::from_rgba(0.35, 0.25, 0.16, 0.10),
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.35, 0.25, 0.16, 0.18),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

fn control_style(_theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let (background, text_color) = match status {
        ButtonStatus::Hovered => (Color::from_rgb(0.86, 0.53, 0.34), Color::WHITE),
        ButtonStatus::Pressed => (Color::from_rgb(0.72, 0.35, 0.20), Color::WHITE),
        ButtonStatus::Disabled => (Color::from_rgba(0.5, 0.4, 0.3, 0.2), MUTED),
        ButtonStatus::Active => (Color::from_rgb(0.91, 0.87, 0.80), INK),
    };
    ButtonStyle {
        background: Some(background.into()),
        text_color,
        border: Border {
            color: Color::from_rgba(0.35, 0.25, 0.16, 0.16),
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.35, 0.25, 0.16, 0.16),
            offset: Vector::new(0.0, 3.0),
            blur_radius: 6.0,
        },
        ..Default::default()
    }
}

fn slide_offsets(
    previous: &[[u32; 4]; 4],
    current: &[[u32; 4]; 4],
    direction: Move,
    spawned: &[[bool; 4]; 4],
) -> [[Vector; 4]; 4] {
    let mut slides = [[Vector::ZERO; 4]; 4];
    for index in 0..4 {
        let line: [(usize, usize); 4] = match direction {
            Move::Left | Move::Right => {
                let column = |position: usize| {
                    if direction == Move::Left {
                        position
                    } else {
                        3 - position
                    }
                };
                [
                    (index, column(0)),
                    (index, column(1)),
                    (index, column(2)),
                    (index, column(3)),
                ]
            }
            Move::Up | Move::Down => {
                let row = |position: usize| {
                    if direction == Move::Up {
                        position
                    } else {
                        3 - position
                    }
                };
                [
                    (row(0), index),
                    (row(1), index),
                    (row(2), index),
                    (row(3), index),
                ]
            }
        };
        let tiles: Vec<(usize, usize, u32)> = line
            .iter()
            .filter_map(|&(row, column)| {
                (previous[row][column] != 0).then_some((row, column, previous[row][column]))
            })
            .collect();

        let mut source = 0;
        let mut destination = 0;
        while source < tiles.len() {
            let merge = source + 1 < tiles.len() && tiles[source].2 == tiles[source + 1].2;
            let (source_row, source_column) = if merge {
                (
                    (tiles[source].0 + tiles[source + 1].0) as f32 / 2.0,
                    (tiles[source].1 + tiles[source + 1].1) as f32 / 2.0,
                )
            } else {
                (tiles[source].0 as f32, tiles[source].1 as f32)
            };
            let (destination_row, destination_column) = line[destination];
            if current[destination_row][destination_column] != 0
                && !spawned[destination_row][destination_column]
            {
                slides[destination_row][destination_column] = Vector::new(
                    (source_column - destination_column as f32) * CELL_PITCH,
                    (source_row - destination_row as f32) * CELL_PITCH,
                );
            }
            source += if merge { 2 } else { 1 };
            destination += 1;
        }
    }
    slides
}

fn main() -> ferriso_gui::iced::Result {
    ferriso_gui::run(
        TwoZero48::new(),
        Gui::new(),
        Settings {
            title: "Ferriso · 2048".into(),
            size: (560.0, 680.0),
            ..Settings::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_view() -> View {
        View {
            cells: [[0; 4]; 4],
            score: 0,
            status: Status::Playing,
            winning: 2048,
            last_move: None,
            move_seq: 0,
            spawned: [[false; 4]; 4],
        }
    }

    #[test]
    fn boot_snapshot_pops_spawned_tiles_then_settles() {
        let mut gui = Gui::new();
        let mut view = empty_view();
        view.cells[0][0] = 2;
        view.spawned[0][0] = true;
        gui.on_snapshot(&view);

        let animation = gui.animation.as_ref().expect("intro animation");
        assert!(animation.spawned[0][0]);
        gui.now = animation.start;
        let (scale, _) = gui.tile_fx(0, 0, 2);
        assert!(scale < 0.8, "spawn starts small, got {scale}");

        let _ = gui.update(
            Local::Frame(animation.start + std::time::Duration::from_millis(400)),
            &view,
        );
        assert!(gui.animation.is_none());
        assert_eq!(gui.tile_fx(0, 0, 2).0, 1.0);
    }

    #[test]
    fn a_moved_tile_slides_while_the_spawned_tile_pops() {
        let mut gui = Gui::new();
        let mut before = empty_view();
        before.cells[0][0] = 2;
        before.spawned[0][0] = true;
        gui.on_snapshot(&before);

        let mut after = empty_view();
        after.cells[0][3] = 2;
        after.spawned[3][3] = true;
        after.last_move = Some(Move::Right);
        after.move_seq = 1;
        gui.on_snapshot(&after);

        let animation = gui.animation.as_ref().expect("move animation");
        assert!(!animation.spawned[0][3]);
        assert!(animation.spawned[3][3]);
        gui.now = animation.start;

        let (_, offset) = gui.tile_fx(0, 3, 2);
        assert!(offset.x < 0.0, "tile enters from the left");
        let (scale, _) = gui.tile_fx(3, 3, 2);
        assert!(scale < 0.8, "spawn pops from small");
    }

    #[test]
    fn keymap_covers_arrows_wasd_and_restart() {
        use ferriso_keymap::Chord;

        let gui = Gui::new();
        let resolve = |binding: &str| {
            gui.keymap
                .resolve(&Chord::parse(binding).expect("test chord"))
                .copied()
        };

        assert_eq!(resolve("ArrowLeft"), Some(Message::Move(Move::Left)));
        assert_eq!(resolve("w"), Some(Message::Move(Move::Up)));
        assert_eq!(resolve("Shift+D"), Some(Message::Move(Move::Right)));
        assert_eq!(resolve("r"), Some(Message::Restart));
        assert_eq!(resolve("x"), None);
    }
}
