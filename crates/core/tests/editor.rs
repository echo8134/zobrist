//! Editing and navigating a game through board moves and PGN.

use std::io;

use shakmaty::Square::{
    A7, A8, B1, C1, C2, C3, C4, C5, C7, D2, D4, D5, D7, E1, E2, E3, E4, E5, E7, F3, G1, H1,
};
use shakmaty::fen::Fen;
use shakmaty::{Chess, Color, EnPassantMode, Position, Role};
use zobrist_core::editor::{GameEditor, NoSuchPlace, Place, PlayError};
use zobrist_core::game::{Game, MAX_VARIATION_DEPTH};
use zobrist_core::pgn::{self, Entry};

fn read_games(source: &[u8]) -> io::Result<Vec<Game>> {
    let mut games = Vec::new();
    pgn::read_games(source, |entry| {
        if let Entry::Game { game, problem, .. } = entry {
            assert_eq!(problem, None);
            games.push(*game);
        }
    })?;
    Ok(games)
}

fn open(text: &str) -> io::Result<GameEditor> {
    let [game] = read_games(text.as_bytes())?
        .try_into()
        .map_err(|_| io::Error::other("expected one game"))?;
    Ok(GameEditor::new(game))
}

fn assert_state(editor: &GameEditor, choices: &[usize], expected: &str) -> io::Result<()> {
    assert_eq!(editor.place(), &Place(choices.to_vec()));
    let mut bytes = Vec::new();
    pgn::write_game(editor.game(), &mut bytes)?;
    assert_eq!(std::str::from_utf8(&bytes), Ok(expected));
    Ok(())
}

#[test]
fn plays_the_game_and_its_variations_one_board_move_at_a_time() -> io::Result<()> {
    let mut editor = open("*")?;
    assert_state(&editor, &[], "*\n")?;
    editor.play(E2, E4, None).unwrap();
    assert_state(&editor, &[0], "1. e4 *\n")?;
    editor.play(E7, E5, None).unwrap();
    assert_state(&editor, &[0, 0], "1. e4 e5 *\n")?;
    editor.play(G1, F3, None).unwrap();
    let main = "1. e4 e5 2. Nf3 *\n";
    assert_state(&editor, &[0, 0, 0], main)?;

    editor.back();
    assert_state(&editor, &[0, 0], main)?;
    editor.back();
    assert_state(&editor, &[0], main)?;
    editor.play(C7, C5, None).unwrap();
    assert_state(&editor, &[0, 1], "1. e4 e5 (1... c5) 2. Nf3 *\n")?;
    editor.play(G1, F3, None).unwrap();
    let sicilian = "1. e4 e5 (1... c5 2. Nf3) 2. Nf3 *\n";
    assert_state(&editor, &[0, 1, 0], sicilian)?;
    editor.back();
    assert_state(&editor, &[0, 1], sicilian)?;
    editor.play(B1, C3, None).unwrap();
    let nested = "1. e4 e5 (1... c5 2. Nf3 (2. Nc3)) 2. Nf3 *\n";
    assert_state(&editor, &[0, 1, 1], nested)?;

    editor.go_to(Place::default()).unwrap();
    assert_state(&editor, &[], nested)?;
    editor.play(E2, E4, None).unwrap();
    assert_state(&editor, &[0], nested)?;
    editor.play(C7, C5, None).unwrap();
    assert_state(&editor, &[0, 1], nested)?;
    editor.go_to(Place::default()).unwrap();
    assert_state(&editor, &[], nested)?;
    editor.play(D2, D4, None).unwrap();
    assert_state(
        &editor,
        &[1],
        "1. e4 (1. d4) 1... e5 (1... c5 2. Nf3 (2. Nc3)) 2. Nf3 *\n",
    )?;
    Ok(())
}

#[test]
fn numbers_first_moves_in_reading_order_and_keeps_their_nesting() -> io::Result<()> {
    let text = "1. e4 (1. d4 (1. c4)) (1. Nf3) *\n";
    let mut editor = open(text)?;
    for (choice, square, role) in [
        (0, E4, Role::Pawn),
        (1, D4, Role::Pawn),
        (2, C4, Role::Pawn),
        (3, F3, Role::Knight),
    ] {
        editor.go_to(Place(vec![choice])).unwrap();
        assert_state(&editor, &[choice], text)?;
        assert_eq!(
            editor.position().board().piece_at(square),
            Some(role.of(Color::White))
        );
    }
    editor.back();
    editor.play(C2, C4, None).unwrap();
    assert_state(&editor, &[2], text)?;
    editor.play(E7, E5, None).unwrap();
    assert_state(&editor, &[2, 0], "1. e4 (1. d4 (1. c4 e5)) (1. Nf3) *\n")?;
    editor.go_to(Place::default()).unwrap();
    editor.play(B1, C3, None).unwrap();
    let extended = "1. e4 (1. d4 (1. c4 e5)) (1. Nf3) (1. Nc3) *\n";
    assert_state(&editor, &[4], extended)?;
    editor.go_to(Place(vec![2])).unwrap();
    assert!(editor.forward());
    assert_state(&editor, &[2, 0], extended)?;
    assert_eq!(
        Fen::from_position(editor.position(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppp1ppp/8/4p3/2P5/8/PP1PPPPP/RNBQKBNR w KQkq - 0 2"
    );
    Ok(())
}

#[test]
fn follows_the_first_copy_of_a_move_in_reading_order() -> io::Result<()> {
    let text = "1. e4 (1. e4) (1. d4 (1. c4 {first})) (1. c4 {second}) *\n";
    let mut editor = open(text)?;
    editor.play(E2, E4, None).unwrap();
    assert_state(&editor, &[0], text)?;
    editor.back();
    editor.play(C2, C4, None).unwrap();
    assert_state(&editor, &[3], text)?;
    Ok(())
}

#[test]
fn an_illegal_move_an_unknown_place_and_the_ends_leave_the_editor_in_place() -> io::Result<()> {
    let text = "1. e4 (1. d4) 1... e5 *\n";
    let mut editor = open(text)?;
    let original = editor.game().clone();
    assert!(!editor.back());
    assert_state(&editor, &[], text)?;
    assert_eq!(editor.play(E2, E5, None), Err(PlayError::IllegalMove));
    assert_state(&editor, &[], text)?;
    assert_eq!(editor.position(), &Chess::default());
    editor.go_to(Place(vec![1])).unwrap();
    assert!(!editor.forward());
    assert_state(&editor, &[1], text)?;
    let after_d4 = editor.position().clone();
    for missing in [vec![2], vec![0, 1], vec![0, 0, 0], vec![1, 0]] {
        assert_eq!(editor.go_to(Place(missing)), Err(NoSuchPlace));
        assert_state(&editor, &[1], text)?;
        assert_eq!(editor.position(), &after_d4);
    }
    assert_eq!(editor.into_game(), original);
    Ok(())
}

#[test]
fn either_castling_gesture_plays_and_selects_the_same_move() -> io::Result<()> {
    for (fen, king_target, rook, notation) in [
        ("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1", G1, H1, "O-O"),
        ("4k3/8/8/8/8/8/8/1R2K3 w B - 0 1", C1, B1, "O-O-O"),
    ] {
        let header = format!("[FEN \"{fen}\"]\n\n");
        let expected = format!("{header}1. {notation} *\n");
        for (first, second) in [(king_target, rook), (rook, king_target)] {
            let mut editor = open(&format!("{header}*"))?;
            editor.play(E1, first, None).unwrap();
            assert_state(&editor, &[0], &expected)?;
            editor.back();
            editor.play(E1, second, None).unwrap();
            assert_state(&editor, &[0], &expected)?;
        }
    }
    Ok(())
}

#[test]
fn plays_en_passant_and_underpromotion_from_the_given_start_position() -> io::Result<()> {
    for (fen, from, to, promotion, notation) in [
        (
            "8/8/8/8/3pP3/8/8/k6K b - e3 0 32",
            D4,
            E3,
            None,
            "32... dxe3",
        ),
        (
            "7k/P7/8/8/8/8/8/7K w - - 0 1",
            A7,
            A8,
            Some(Role::Knight),
            "1. a8=N",
        ),
    ] {
        let header = format!("[FEN \"{fen}\"]\n\n");
        let start = format!("{header}*\n");
        let mut editor = open(&start)?;
        assert_eq!(
            editor.play(from, to, Some(Role::King)),
            Err(PlayError::IllegalMove)
        );
        assert_state(&editor, &[], &start)?;
        editor.play(from, to, promotion).unwrap();
        assert_state(&editor, &[0], &format!("{header}{notation} *\n"))?;
    }
    Ok(())
}

#[test]
fn the_first_move_keeps_the_comments_of_an_empty_game() -> io::Result<()> {
    let mut editor = open("{First} {Second} 1-0")?;
    editor.play(E2, E4, None).unwrap();
    assert_state(&editor, &[0], "{First} {Second} 1. e4 1-0\n")?;
    Ok(())
}

#[test]
fn can_continue_after_an_imported_null_move() -> io::Result<()> {
    let mut editor = open("1. e4 -- *")?;
    editor.go_to(Place(vec![0, 0])).unwrap();
    assert_eq!(
        Fen::from_position(editor.position(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 1 2"
    );
    editor.play(D2, D4, None).unwrap();
    assert_state(&editor, &[0, 0, 0], "1. e4 -- 2. d4 *\n")?;
    Ok(())
}

#[test]
fn enforces_the_actual_variation_depth_even_when_a_place_is_short() -> io::Result<()> {
    let depth = MAX_VARIATION_DEPTH - 1;
    let before = format!(
        "1. e4 {}e5{} *\n",
        "(1. d4 ".repeat(depth),
        ")".repeat(depth)
    );
    let mut editor = open(&before)?;
    editor.go_to(Place(vec![depth])).unwrap();
    assert_state(&editor, &[depth], &before)?;
    editor.play(D7, D5, None).unwrap();
    editor.play(G1, F3, None).unwrap();
    let limit = format!(
        "1. e4 {}e5 (1... d5 2. Nf3){} *\n",
        "(1. d4 ".repeat(depth),
        ")".repeat(depth)
    );
    assert_state(&editor, &[depth, 1, 0], &limit)?;
    editor.back();
    let at_limit = editor.game().clone();
    assert_eq!(editor.play(B1, C3, None), Err(PlayError::TooDeep));
    assert_state(&editor, &[depth, 1], &limit)?;
    editor.play(G1, F3, None).unwrap();
    assert_state(&editor, &[depth, 1, 0], &limit)?;
    assert_eq!(editor.into_game(), at_limit);
    Ok(())
}

#[test]
fn visits_every_place_of_each_clean_fixture_game_and_returns_it_intact() -> io::Result<()> {
    let games = read_games(include_bytes!("../../cli/tests/fixtures/clean.pgn"))?;
    assert_eq!(games.len(), 2);
    for (game, main_plies) in games.into_iter().zip([7, 4]) {
        let mut editor = GameEditor::new(game.clone());
        assert_eq!(editor.place(), &Place::default());
        assert_eq!(editor.position(), game.start_position());
        for ply in 1..=main_plies {
            editor.forward();
            assert_eq!(editor.place(), &Place(vec![0; ply]));
        }
        if main_plies == 7 {
            editor.go_to(Place(vec![0, 0, 0, 0, 0, 1])).unwrap();
            editor.forward();
            assert_eq!(editor.place(), &Place(vec![0, 0, 0, 0, 0, 1, 0]));
        }
        assert_eq!(editor.into_game(), game);
    }
    Ok(())
}
