//! The rules of chess, with castling, en passant, promotion and the draws by repetition, by the fifty-move rule
//! and for lack of material; and the computer player: an alpha-beta search to a fixed depth.

use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    pub const ALL: [PieceKind; 6] = [PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop, PieceKind::Rook, PieceKind::Queen, PieceKind::King];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::White => "White",
            Side::Black => "Black",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub side: Side,
}

/// A square of the board; files and ranks count from 0, so a1 is file 0 of rank 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Square(u8);

impl Square {
    /// The square of a file and a rank, if they are on the board.
    pub fn new(file: i32, rank: i32) -> Option<Square> {
        ((0..8).contains(&file) && (0..8).contains(&rank)).then(|| Square::at(file, rank))
    }

    /// The square of a file and a rank that are known to be on the board.
    const fn at(file: i32, rank: i32) -> Square {
        debug_assert!(0 <= file && file < 8 && 0 <= rank && rank < 8);
        Square((rank * 8 + file) as u8)
    }

    /// The squares of the board, file after file.
    pub fn all() -> impl Iterator<Item = Square> {
        (0..8).flat_map(|file| (0..8).map(move |rank| Square::at(file, rank)))
    }

    pub const fn file(self) -> i32 {
        (self.0 & 7) as i32
    }

    pub const fn rank(self) -> i32 {
        (self.0 >> 3) as i32
    }

    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a move is besides a piece going from one square to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    Plain,
    CastleKingSide,
    CastleQueenSide,
    EnPassant,
    Promotion(PieceKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub kind: MoveKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawReason {
    Repetition,
    FiftyMoves,
    InsufficientMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Check,
    Checkmate,
    Stalemate,
    Draw(DrawReason),
}

// The castlings still possible, as bits.
const WHITE_KING_SIDE: u8 = 1;
const WHITE_QUEEN_SIDE: u8 = 2;
const BLACK_KING_SIDE: u8 = 4;
const BLACK_QUEEN_SIDE: u8 = 8;
const ALL_CASTLINGS: u8 = 15;

const INFINITY: i32 = 1_000_000_000;
const MATE_SCORE: i32 = 1_000_000;

const BACK_RANK: [PieceKind; 8] =
    [PieceKind::Rook, PieceKind::Knight, PieceKind::Bishop, PieceKind::Queen, PieceKind::King, PieceKind::Bishop, PieceKind::Knight, PieceKind::Rook];
const PROMOTIONS: [PieceKind; 4] = [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight];
const KNIGHT_STEPS: [(i32, i32); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
const KING_STEPS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const DIAGONAL_STEPS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const STRAIGHT_STEPS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// What taking a move back needs.
struct Undo {
    made: Move,
    moved: Piece,
    captured: Option<Piece>,
    captured_square: Square,
    castling: u8,
    en_passant: Option<Square>,
    quiet_plies: u32,
}

/// What makes two positions the same: the same pieces on the same squares, the same side to move, and the
/// same castling and en passant captures possible.
#[derive(Clone, PartialEq, Eq)]
struct PositionKey {
    board: [Option<Piece>; 64],
    turn: Side,
    castling: u8,
    en_passant_file: Option<i32>,
}

/// What a search carries along: the sign to stop, and the lists of moves, one for each half-move of depth.
struct Search<'a> {
    stop: &'a AtomicBool,
    nodes: u64,
    lists: Vec<Vec<Move>>,
}

pub struct Game {
    board: [Option<Piece>; 64],
    kings: [Square; 2], // by side
    castling: u8,
    en_passant: Option<Square>,  // the square a pawn has just passed over
    quiet_plies: u32,            // since the last capture or pawn move
    positions: Vec<PositionKey>, // since the last capture or pawn move
    turn: Side,
    status: Status,
    random: Random,
}

impl Game {
    pub fn new() -> Game {
        let mut game = Game {
            board: [None; 64],
            kings: [Square::at(4, 0), Square::at(4, 7)],
            castling: ALL_CASTLINGS,
            en_passant: None,
            quiet_plies: 0,
            positions: Vec::new(),
            turn: Side::White,
            status: Status::Playing,
            random: Random::from_entropy(),
        };
        game.new_game();
        game
    }

    pub fn new_game(&mut self) {
        self.board = [None; 64];
        for (file, kind) in (0..).zip(BACK_RANK) {
            self.board[Square::at(file, 0).index()] = Some(Piece { kind, side: Side::White });
            self.board[Square::at(file, 1).index()] = Some(Piece { kind: PieceKind::Pawn, side: Side::White });
            self.board[Square::at(file, 6).index()] = Some(Piece { kind: PieceKind::Pawn, side: Side::Black });
            self.board[Square::at(file, 7).index()] = Some(Piece { kind, side: Side::Black });
        }
        self.kings = [Square::at(4, 0), Square::at(4, 7)];
        self.castling = ALL_CASTLINGS;
        self.en_passant = None;
        self.quiet_plies = 0;
        self.turn = Side::White;
        self.positions.clear();
        self.positions.push(self.position_key());
        self.update_status();
    }

    /// A copy, for searching on another thread.
    pub fn fork(&mut self) -> Game {
        Game {
            board: self.board,
            kings: self.kings,
            castling: self.castling,
            en_passant: self.en_passant,
            quiet_plies: self.quiet_plies,
            positions: self.positions.clone(),
            turn: self.turn,
            status: self.status,
            random: Random(self.random.next_u64()),
        }
    }

    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.board[square.index()]
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn is_over(&self) -> bool {
        matches!(self.status, Status::Checkmate | Status::Stalemate | Status::Draw(_))
    }

    /// The legal moves of the piece on a square, if it is that side's turn.
    pub fn legal_moves_from(&mut self, square: Square) -> Vec<Move> {
        let mut moves = Vec::new();
        if let Some(piece) = self.piece_at(square).filter(|piece| piece.side == self.turn) {
            self.add_pseudo_moves(square, piece, &mut moves);
            moves.retain(|candidate| self.is_legal(*candidate));
        }
        moves
    }

    /// Plays the move if it is legal.
    pub fn try_make_move(&mut self, wanted: Move) -> bool {
        if self.is_over() || !self.legal_moves_from(wanted.from).contains(&wanted) {
            return false;
        }
        self.make(wanted);
        if self.quiet_plies == 0 {
            self.positions.clear(); // the earlier positions cannot come back
        }
        self.positions.push(self.position_key());
        self.update_status();
        true
    }

    // Moves and their legality ----------------------------------------------------------------------

    fn is_empty(&self, square: Square) -> bool {
        self.board[square.index()].is_none()
    }

    /// Whether the mover's king is safe after the move.
    fn is_legal(&mut self, candidate: Move) -> bool {
        let side = self.turn;
        let undo = self.make(candidate);
        let legal = !self.in_check(side);
        self.unmake(undo);
        legal
    }

    fn in_check(&self, side: Side) -> bool {
        self.is_attacked(self.kings[side as usize], side.opposite())
    }

    /// The moves of the side to move by the movement rules; some may leave its king in check.
    fn generate_moves(&self, moves: &mut Vec<Move>) {
        for index in 0..64 {
            if let Some(piece) = self.board[index].filter(|piece| piece.side == self.turn) {
                self.add_pseudo_moves(Square(index as u8), piece, moves);
            }
        }
    }

    fn add_pseudo_moves(&self, from: Square, piece: Piece, moves: &mut Vec<Move>) {
        match piece.kind {
            PieceKind::Pawn => self.add_pawn_moves(from, piece.side, moves),
            PieceKind::Knight => self.add_steps(from, piece.side, &KNIGHT_STEPS, moves),
            PieceKind::Bishop => self.add_slides(from, piece.side, &DIAGONAL_STEPS, moves),
            PieceKind::Rook => self.add_slides(from, piece.side, &STRAIGHT_STEPS, moves),
            PieceKind::Queen => {
                self.add_slides(from, piece.side, &STRAIGHT_STEPS, moves);
                self.add_slides(from, piece.side, &DIAGONAL_STEPS, moves);
            }
            PieceKind::King => {
                self.add_steps(from, piece.side, &KING_STEPS, moves);
                self.add_castling(piece.side, moves);
            }
        }
    }

    fn add_pawn_moves(&self, from: Square, side: Side, moves: &mut Vec<Move>) {
        let (forward, start) = if side == Side::White { (1, 1) } else { (-1, 6) };
        let (file, rank) = (from.file(), from.rank());
        let Some(next) = Square::new(file, rank + forward) else { return };

        if self.is_empty(next) {
            add_pawn_move(from, next, moves);
            if let Some(to) = Square::new(file, rank + 2 * forward).filter(|to| rank == start && self.is_empty(*to)) {
                moves.push(Move { from, to, kind: MoveKind::Plain });
            }
        }
        for to in [file - 1, file + 1].into_iter().filter_map(|file| Square::new(file, rank + forward)) {
            match self.piece_at(to) {
                Some(victim) if victim.side != side => add_pawn_move(from, to, moves),
                None if self.en_passant == Some(to) => moves.push(Move { from, to, kind: MoveKind::EnPassant }),
                _ => {}
            }
        }
    }

    /// The moves of a knight or a king: a step each way, onto a square its own side does not hold.
    fn add_steps(&self, from: Square, side: Side, steps: &[(i32, i32)], moves: &mut Vec<Move>) {
        for (file_step, rank_step) in steps {
            if let Some(to) = Square::new(from.file() + file_step, from.rank() + rank_step) {
                if self.piece_at(to).is_none_or(|piece| piece.side != side) {
                    moves.push(Move { from, to, kind: MoveKind::Plain });
                }
            }
        }
    }

    /// The moves of a bishop, a rook or a queen: along each direction up to the first piece, which is taken if
    /// it is the other side's.
    fn add_slides(&self, from: Square, side: Side, steps: &[(i32, i32)], moves: &mut Vec<Move>) {
        for (file_step, rank_step) in steps {
            let mut reach = Square::new(from.file() + file_step, from.rank() + rank_step);
            while let Some(to) = reach {
                let target = self.piece_at(to);
                if target.is_none_or(|piece| piece.side != side) {
                    moves.push(Move { from, to, kind: MoveKind::Plain });
                }
                if target.is_some() {
                    break;
                }
                reach = Square::new(to.file() + file_step, to.rank() + rank_step);
            }
        }
    }

    /// The king may not castle out of, through or into check. The rights are lost when the king or the rook
    /// moves, or the rook is taken, so with a right the king and the rook stand on their squares.
    fn add_castling(&self, side: Side, moves: &mut Vec<Move>) {
        let (rank, king_side, queen_side) = if side == Side::White { (0, WHITE_KING_SIDE, WHITE_QUEEN_SIDE) } else { (7, BLACK_KING_SIDE, BLACK_QUEEN_SIDE) };
        let enemy = side.opposite();
        let at = |file: i32| Square::at(file, rank);
        let free = |files: &[i32]| files.iter().all(|file| self.is_empty(at(*file)));
        let safe = |files: &[i32]| files.iter().all(|file| !self.is_attacked(at(*file), enemy));

        if self.castling & (king_side | queen_side) == 0 || !safe(&[4]) {
            return;
        }
        if self.castling & king_side != 0 && free(&[5, 6]) && safe(&[5, 6]) {
            moves.push(Move { from: at(4), to: at(6), kind: MoveKind::CastleKingSide });
        }
        if self.castling & queen_side != 0 && free(&[1, 2, 3]) && safe(&[3, 2]) {
            moves.push(Move { from: at(4), to: at(2), kind: MoveKind::CastleQueenSide });
        }
    }

    /// Whether a piece of the side `by` attacks the square.
    fn is_attacked(&self, square: Square, by: Side) -> bool {
        let (file, rank) = (square.file(), square.rank());
        let holds = |file: i32, rank: i32, kind: PieceKind| self.holds(file, rank, Piece { kind, side: by });
        let pawn_rank = rank - if by == Side::White { 1 } else { -1 };

        holds(file - 1, pawn_rank, PieceKind::Pawn)
            || holds(file + 1, pawn_rank, PieceKind::Pawn)
            || KNIGHT_STEPS.iter().any(|(file_step, rank_step)| holds(file + file_step, rank + rank_step, PieceKind::Knight))
            || KING_STEPS.iter().any(|(file_step, rank_step)| holds(file + file_step, rank + rank_step, PieceKind::King))
            || self.is_attacked_along(square, by, &DIAGONAL_STEPS, PieceKind::Bishop)
            || self.is_attacked_along(square, by, &STRAIGHT_STEPS, PieceKind::Rook)
    }

    fn holds(&self, file: i32, rank: i32, piece: Piece) -> bool {
        Square::new(file, rank).is_some_and(|square| self.piece_at(square) == Some(piece))
    }

    /// Whether the first piece in one of the directions is a queen or the given slider of the side `by`.
    fn is_attacked_along(&self, square: Square, by: Side, steps: &[(i32, i32)], slider: PieceKind) -> bool {
        steps.iter().any(|(file_step, rank_step)| {
            let mut reach = Square::new(square.file() + file_step, square.rank() + rank_step);
            while let Some(at) = reach {
                if let Some(piece) = self.piece_at(at) {
                    return piece.side == by && (piece.kind == slider || piece.kind == PieceKind::Queen);
                }
                reach = Square::new(at.file() + file_step, at.rank() + rank_step);
            }
            false
        })
    }

    fn make(&mut self, made: Move) -> Undo {
        let Move { from, to, kind } = made;
        let moved = self.board[from.index()].expect("a move starts from a piece");
        let captured_square = if kind == MoveKind::EnPassant { Square::at(to.file(), from.rank()) } else { to };
        let captured = self.board[captured_square.index()];
        let undo = Undo { made, moved, captured, captured_square, castling: self.castling, en_passant: self.en_passant, quiet_plies: self.quiet_plies };

        self.board[captured_square.index()] = None;
        self.board[from.index()] = None;
        self.board[to.index()] = Some(match kind {
            MoveKind::Promotion(promoted) => Piece { kind: promoted, side: moved.side },
            _ => moved,
        });
        if moved.kind == PieceKind::King {
            self.kings[moved.side as usize] = to;
        }
        if let Some((rook_from, rook_to)) = rook_castling(made) {
            self.board[rook_to.index()] = self.board[rook_from.index()].take();
        }

        self.castling &= !(castling_lost_at(from) | castling_lost_at(to));
        let two_steps = moved.kind == PieceKind::Pawn && (to.rank() - from.rank()).abs() == 2;
        self.en_passant = two_steps.then(|| Square::at(from.file(), (from.rank() + to.rank()) / 2));
        self.quiet_plies = if moved.kind == PieceKind::Pawn || captured.is_some() { 0 } else { self.quiet_plies + 1 };
        self.turn = self.turn.opposite();
        undo
    }

    fn unmake(&mut self, undo: Undo) {
        let Move { from, to, .. } = undo.made;
        self.board[to.index()] = None;
        self.board[undo.captured_square.index()] = undo.captured;
        self.board[from.index()] = Some(undo.moved);
        if undo.moved.kind == PieceKind::King {
            self.kings[undo.moved.side as usize] = from;
        }
        if let Some((rook_from, rook_to)) = rook_castling(undo.made) {
            self.board[rook_from.index()] = self.board[rook_to.index()].take();
        }

        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.quiet_plies = undo.quiet_plies;
        self.turn = self.turn.opposite();
    }

    // The end of the game ---------------------------------------------------------------------------

    fn update_status(&mut self) {
        let in_check = self.in_check(self.turn);
        self.status = if !self.has_legal_move() {
            if in_check { Status::Checkmate } else { Status::Stalemate }
        } else if let Some(reason) = self.find_draw() {
            Status::Draw(reason)
        } else if in_check {
            Status::Check
        } else {
            Status::Playing
        };
    }

    fn has_legal_move(&mut self) -> bool {
        let mut moves = Vec::new();
        self.generate_moves(&mut moves);
        moves.into_iter().any(|candidate| self.is_legal(candidate))
    }

    fn find_draw(&self) -> Option<DrawReason> {
        let position = self.positions.last();
        if !self.has_mating_material() {
            Some(DrawReason::InsufficientMaterial)
        } else if self.positions.iter().filter(|key| Some(*key) == position).count() >= 3 {
            Some(DrawReason::Repetition)
        } else {
            (self.quiet_plies >= 100).then_some(DrawReason::FiftyMoves)
        }
    }

    /// No mate is possible with the kings and a single minor piece, or with bishops all on one color.
    fn has_mating_material(&self) -> bool {
        let (mut knights, mut bishops, mut bishop_colors) = (0, 0, 0);
        for (index, piece) in self.board.iter().enumerate() {
            match piece.map(|piece| piece.kind) {
                Some(PieceKind::Pawn | PieceKind::Rook | PieceKind::Queen) => return true,
                Some(PieceKind::Knight) => knights += 1,
                Some(PieceKind::Bishop) => {
                    bishops += 1;
                    bishop_colors |= 1 << ((index % 8 + index / 8) & 1);
                }
                Some(PieceKind::King) | None => {}
            }
        }
        knights + bishops > 1 && (knights > 0 || bishop_colors == 3)
    }

    fn position_key(&self) -> PositionKey {
        let en_passant_file = self.en_passant.filter(|square| self.can_capture_en_passant(*square)).map(Square::file);
        PositionKey { board: self.board, turn: self.turn, castling: self.castling, en_passant_file }
    }

    /// Whether a pawn of the side to move stands beside the pawn that has just passed over the square.
    fn can_capture_en_passant(&self, passed: Square) -> bool {
        let pawn = Piece { kind: PieceKind::Pawn, side: self.turn };
        let pawn_rank = passed.rank() - if self.turn == Side::White { 1 } else { -1 };
        self.holds(passed.file() - 1, pawn_rank, pawn) || self.holds(passed.file() + 1, pawn_rank, pawn)
    }

    // The computer player ---------------------------------------------------------------------------

    /// The best move for the side to move by a search `depth` half-moves deep; a random one of the best when
    /// there are several. None when there is no legal move, or the search was told to stop.
    pub fn find_best_move(&mut self, depth: u32, stop: &AtomicBool) -> Option<Move> {
        let mut moves = Vec::new();
        self.generate_moves(&mut moves);
        moves.retain(|candidate| self.is_legal(*candidate));
        self.order_moves(&mut moves);

        let mut search = Search { stop, nodes: 0, lists: vec![Vec::new(); depth as usize + 1] };
        let mut best = Vec::new();
        let mut best_value = -INFINITY;
        for candidate in moves {
            let undo = self.make(candidate);
            // The window starts just below the best value: a move as good gets its exact value, a worse one is
            // cut off below it and cannot pass for a tie.
            let value = self.negamax(depth.saturating_sub(1), -INFINITY, -(best_value - 1), 1, &mut search);
            self.unmake(undo);
            let value = -value?;

            if value > best_value {
                best_value = value;
                best.clear();
            }
            if value == best_value {
                best.push(candidate);
            }
        }
        (!best.is_empty()).then(|| best[self.random.below(best.len())])
    }

    /// The value of the position for the side to move, `depth` half-moves ahead; none once told to stop.
    fn negamax(&mut self, depth: u32, mut alpha: i32, beta: i32, ply: usize, search: &mut Search) -> Option<i32> {
        if depth == 0 {
            return Some(self.evaluate());
        }
        search.nodes += 1;
        if search.nodes & 4095 == 0 && search.stop.load(Ordering::Relaxed) {
            return None;
        }

        let mut moves = std::mem::take(&mut search.lists[ply]);
        moves.clear();
        self.generate_moves(&mut moves);
        self.order_moves(&mut moves);

        let side = self.turn;
        let mut best = -INFINITY;
        let mut any_legal = false;
        for &candidate in &moves {
            let undo = self.make(candidate);
            if self.in_check(side) {
                self.unmake(undo);
                continue;
            }
            any_legal = true;
            let value = self.negamax(depth - 1, -beta, -alpha, ply + 1, search);
            self.unmake(undo);

            best = best.max(-value?);
            alpha = alpha.max(best);
            if alpha >= beta {
                break;
            }
        }
        search.lists[ply] = moves; // kept for the next position at this depth

        Some(if any_legal {
            best
        } else if self.in_check(side) {
            -(MATE_SCORE - ply as i32) // mated: the sooner, the worse
        } else {
            0 // stalemate
        })
    }

    /// Captures of valuable pieces by cheap ones, and promotions, first: the cutoffs come sooner.
    fn order_moves(&self, moves: &mut [Move]) {
        moves.sort_by_cached_key(|candidate| -self.score_move(*candidate));
    }

    fn score_move(&self, candidate: Move) -> i32 {
        let mover = self.piece_at(candidate.from).map_or(0, |piece| piece_value(piece.kind));
        let gain = match (self.piece_at(candidate.to), candidate.kind) {
            (Some(victim), _) => 10 * piece_value(victim.kind),
            (None, MoveKind::EnPassant) => 10 * piece_value(PieceKind::Pawn),
            (None, _) => 0,
        };
        let promotion = if let MoveKind::Promotion(kind) = candidate.kind { piece_value(kind) } else { 0 };
        gain + promotion - mover
    }

    /// Material and a little positional sense, for the side to move.
    fn evaluate(&self) -> i32 {
        let mut score = 0;
        for (index, piece) in self.board.iter().enumerate() {
            if let Some(piece) = piece {
                let value = piece_value(piece.kind) + square_bonus(*piece, Square(index as u8));
                score += if piece.side == self.turn { value } else { -value };
            }
        }
        score
    }
}

/// A pawn reaching the last rank promotes; the queen comes first, so the player's pawns promote to queens.
fn add_pawn_move(from: Square, to: Square, moves: &mut Vec<Move>) {
    if matches!(to.rank(), 0 | 7) {
        moves.extend(PROMOTIONS.map(|kind| Move { from, to, kind: MoveKind::Promotion(kind) }));
    } else {
        moves.push(Move { from, to, kind: MoveKind::Plain });
    }
}

/// Where the rook goes from and to as the king castles.
fn rook_castling(made: Move) -> Option<(Square, Square)> {
    let rank = made.from.rank();
    match made.kind {
        MoveKind::CastleKingSide => Some((Square::at(7, rank), Square::at(5, rank))),
        MoveKind::CastleQueenSide => Some((Square::at(0, rank), Square::at(3, rank))),
        _ => None,
    }
}

/// A move from or to the home square of a king or a rook ends the castling it is part of.
fn castling_lost_at(square: Square) -> u8 {
    match (square.file(), square.rank()) {
        (4, 0) => WHITE_KING_SIDE | WHITE_QUEEN_SIDE,
        (7, 0) => WHITE_KING_SIDE,
        (0, 0) => WHITE_QUEEN_SIDE,
        (4, 7) => BLACK_KING_SIDE | BLACK_QUEEN_SIDE,
        (7, 7) => BLACK_KING_SIDE,
        (0, 7) => BLACK_QUEEN_SIDE,
        _ => 0,
    }
}

fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 20000,
    }
}

/// Knights and bishops like the centre, pawns like advancing, the king likes shelter.
fn square_bonus(piece: Piece, square: Square) -> i32 {
    let (file, rank) = (square.file(), square.rank());
    // 12 on the four squares in the middle, 2 less with every step from them along a file or a rank.
    let centre = 14 - (2 * file - 7).abs() - (2 * rank - 7).abs();
    match piece.kind {
        PieceKind::Pawn => centre / 2 + if piece.side == Side::White { rank } else { 7 - rank } * 4,
        PieceKind::Knight => centre,
        PieceKind::Bishop => centre / 2,
        PieceKind::King => -centre / 2,
        PieceKind::Rook | PieceKind::Queen => 0,
    }
}

/// A small source of random numbers (SplitMix64), seeded differently at every run.
struct Random(u64);

impl Random {
    fn from_entropy() -> Random {
        Random(std::collections::hash_map::RandomState::new().build_hasher().finish())
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 to n - 1.
    fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }
}
