//! A minimal 2048 application used to exercise the Ferriso GUI path.
//!
//! The board rules and tile type come from the upstream `twozero48` crate. The
//! Ferriso application owns the board, score, status, and random generator;
//! the iced renderer sees only an immutable [`View`].

use std::fmt;

use ferriso_core::{App, Command};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use twozero48::{Board, Tile};

const SIZE: usize = 4;
const WINNING: Tile = Tile::new(11);

/// Semantic input accepted by the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Message {
    Move(Move),
    Restart,
}

/// A directional move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
}

/// Game status exposed to the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Won,
    Lost,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Playing => "keep going",
            Self::Won => "you won",
            Self::Lost => "game over",
        })
    }
}

/// Immutable snapshot consumed by the iced renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub cells: [[u32; SIZE]; SIZE],
    pub score: usize,
    pub status: Status,
    pub winning: u32,
    /// The direction of the last accepted move, for renderer-side motion.
    pub last_move: Option<Move>,
    /// Monotonic counter bumped on every accepted move or restart.
    pub move_seq: u64,
    /// Cells occupied by a freshly spawned tile in this snapshot.
    pub spawned: [[bool; SIZE]; SIZE],
}

/// Actor-owned game state.
pub struct TwoZero48 {
    board: Board,
    score: usize,
    status: Status,
    rng: StdRng,
    last_move: Option<Move>,
    move_seq: u64,
    spawned: [[bool; SIZE]; SIZE],
}

impl TwoZero48 {
    pub fn new() -> Self {
        let mut rng = StdRng::from_rng(&mut rand::rng());
        let mut board = Board::new(SIZE).expect("constant board size is valid");
        let mut spawned = [[false; SIZE]; SIZE];
        for _ in 0..2 {
            if let Some((row, column)) = spawn(&mut board, &mut rng) {
                spawned[row][column] = true;
            }
        }
        Self {
            board,
            score: 0,
            status: Status::Playing,
            rng,
            last_move: None,
            move_seq: 0,
            spawned,
        }
    }

    fn restart(&mut self) {
        self.board = Board::new(SIZE).expect("constant board size is valid");
        self.score = 0;
        self.status = Status::Playing;
        self.last_move = None;
        self.move_seq += 1;
        self.spawned = [[false; SIZE]; SIZE];
        for _ in 0..2 {
            if let Some((row, column)) = spawn(&mut self.board, &mut self.rng) {
                self.spawned[row][column] = true;
            }
        }
    }

    fn apply(&mut self, direction: Move) {
        if self.status != Status::Playing {
            return;
        }

        let before = self.board.clone();
        let score = match direction {
            Move::Left => self.board.move_left(),
            Move::Right => self.board.move_right(),
            Move::Up => self.board.move_up(),
            Move::Down => self.board.move_down(),
        };

        if self.board == before {
            return;
        }

        self.score += score;
        self.last_move = Some(direction);
        self.move_seq += 1;
        self.spawned = [[false; SIZE]; SIZE];
        if let Some((row, column)) = spawn(&mut self.board, &mut self.rng) {
            self.spawned[row][column] = true;
        }
        self.status = if self.board.contains(WINNING) {
            Status::Won
        } else if self.board.is_locked() {
            Status::Lost
        } else {
            Status::Playing
        };
    }
}

fn spawn(board: &mut Board, rng: &mut StdRng) -> Option<(usize, usize)> {
    let empty: Vec<_> = (0..SIZE)
        .flat_map(|row| (0..SIZE).map(move |col| (row, col)))
        .filter(|&(row, col)| board[(row, col)] == Tile::EMPTY)
        .collect();
    if empty.is_empty() {
        return None;
    }
    let &(row, col) = &empty[rng.random_range(0..empty.len())];
    board[(row, col)] = if rng.random_bool(0.1) {
        Tile::FOUR
    } else {
        Tile::TWO
    };
    Some((row, col))
}

impl Default for TwoZero48 {
    fn default() -> Self {
        Self::new()
    }
}

impl App for TwoZero48 {
    type Message = Message;
    type View = View;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Move(direction) => self.apply(direction),
            Message::Restart => self.restart(),
        }
        Command::none()
    }

    fn view(&self) -> View {
        let mut cells = [[0; SIZE]; SIZE];
        for (row, cells) in cells.iter_mut().enumerate() {
            for (col, cell) in cells.iter_mut().enumerate() {
                *cell = self.board[(row, col)].score() as u32;
            }
        }
        View {
            cells,
            score: self.score,
            status: self.status,
            winning: WINNING.score() as u32,
            last_move: self.last_move,
            move_seq: self.move_seq,
            spawned: self.spawned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_game_has_two_tiles_and_no_score() {
        let game = TwoZero48::new();
        let view = game.view();
        assert_eq!(view.score, 0);
        assert_eq!(view.status, Status::Playing);
        assert_eq!(
            view.cells
                .iter()
                .flatten()
                .filter(|cell| **cell != 0)
                .count(),
            2
        );
    }

    #[test]
    fn accepted_move_reports_direction_and_a_single_spawn() {
        let mut game = TwoZero48::new();
        game.board = Board::new(SIZE).expect("board");
        game.board[(0, 0)] = Tile::TWO;
        game.board[(0, 1)] = Tile::TWO;
        let before = game.view().move_seq;
        game.apply(Move::Left);

        let view = game.view();
        assert_eq!(view.last_move, Some(Move::Left));
        assert!(view.move_seq > before);
        assert_eq!(
            view.spawned
                .iter()
                .flatten()
                .filter(|spawned| **spawned)
                .count(),
            1
        );
    }

    #[test]
    fn restart_marks_both_starting_tiles_as_spawned() {
        let mut game = TwoZero48::new();
        game.restart();
        let view = game.view();
        assert_eq!(
            view.spawned
                .iter()
                .flatten()
                .filter(|spawned| **spawned)
                .count(),
            2
        );
        assert_eq!(view.last_move, None);
    }

    #[test]
    fn a_merge_adds_the_promoted_tile_value_to_the_score() {
        let mut game = TwoZero48::new();
        game.board = Board::new(SIZE).expect("board");
        game.board[(0, 0)] = Tile::TWO;
        game.board[(0, 1)] = Tile::TWO;
        game.apply(Move::Left);
        assert_eq!(game.score, 4);
        assert_eq!(game.board[(0, 0)].score(), 4);
    }

    #[test]
    fn restart_clears_the_board_and_score() {
        let mut game = TwoZero48::new();
        game.score = 99;
        game.restart();
        assert_eq!(game.score, 0);
        let view = game.view();
        assert_eq!(
            view.cells
                .iter()
                .flatten()
                .filter(|cell| **cell != 0)
                .count(),
            2
        );
    }
}
