//! The notation of a position while pieces and starting rights are still being set up.

use shakmaty::{Board, Color, Square};

const CASTLING_LETTERS: [char; 4] = ['K', 'Q', 'k', 'q'];

/// A complete FEN draft. Its position can still be illegal to play.
pub struct PositionDraft {
    /// The six fields of FEN, including the move counters.
    pub fen: String,
    /// Placement of the pieces without the remaining FEN fields.
    pub pieces: String,
    /// The side to play.
    pub turn: Color,
    /// The four castling rights in FEN order (K, Q, k, q).
    pub castling: [bool; 4],
    /// The en passant target, even if no legal capture reaches it.
    pub en_passant: Option<Square>,
}

impl PositionDraft {
    /// Read all six FEN fields without checking whether the position can be played.
    #[must_use]
    pub fn from_fen(text: &str) -> Option<Self> {
        let mut parts = text.split_whitespace();
        let pieces = parts.next()?;
        let turn = match parts.next()? {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return None,
        };
        let rights = parts.next()?;
        let castling = CASTLING_LETTERS.map(|letter| rights.contains(letter));
        let en_passant = parts.next()?;
        let halfmoves = parts.next()?;
        let fullmoves = parts.next()?;
        if parts.next().is_some()
            || rights != castling_text(castling)
            || !halfmoves.bytes().all(|digit| digit.is_ascii_digit())
            || !fullmoves.bytes().all(|digit| digit.is_ascii_digit())
            || fullmoves.starts_with('0')
        {
            return None;
        }
        Self::from_parts(
            pieces,
            turn,
            castling,
            (en_passant != "-").then_some(en_passant),
            halfmoves,
            fullmoves,
        )
    }

    /// Describe edited board fields, with a halfmove clock of zero and move number one.
    #[must_use]
    pub fn from_fields(
        pieces: &str,
        turn: Color,
        castling: [bool; 4],
        en_passant: Option<&str>,
    ) -> Option<Self> {
        Self::from_parts(pieces, turn, castling, en_passant, "0", "1")
    }

    fn from_parts(
        pieces: &str,
        turn: Color,
        castling: [bool; 4],
        en_passant: Option<&str>,
        halfmoves: &str,
        fullmoves: &str,
    ) -> Option<Self> {
        let board = Board::from_ascii_board_fen(pieces.as_bytes()).ok()?;
        if board.board_fen().to_string() != pieces {
            return None;
        }
        let square = en_passant.map(str::parse::<Square>).transpose().ok()?;
        let fen = format!(
            "{pieces} {} {} {} {halfmoves} {fullmoves}",
            turn.char(),
            castling_text(castling),
            en_passant.unwrap_or("-")
        );
        Some(Self {
            fen,
            pieces: pieces.to_owned(),
            turn,
            castling,
            en_passant: square,
        })
    }
}

fn castling_text(rights: [bool; 4]) -> String {
    let letters: String = CASTLING_LETTERS
        .into_iter()
        .zip(rights)
        .filter_map(|(letter, enabled)| enabled.then_some(letter))
        .collect();
    if letters.is_empty() {
        "-".to_owned()
    } else {
        letters
    }
}
