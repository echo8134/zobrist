//! Editing and navigating a game through board moves and PGN.

use std::io;

use shakmaty::Square::{
    A7, A8, B1, C1, C2, C3, C4, C5, C7, D2, D4, D5, D7, E1, E2, E3, E4, E5, E7, F3, G1, H1,
};
use shakmaty::fen::Fen;
use shakmaty::{Chess, Color, EnPassantMode, Position, Role};
use zobrist_core::editor::{
    AnnotationError, EditVariationError, GameEditor, MoveSymbol, NoSuchPlace, Place, PlayError,
    PositionSymbol,
};
use zobrist_core::game::{Game, MAX_VARIATION_DEPTH};
use zobrist_core::pgn::{self, Entry};

fn read_games(source: &[u8]) -> io::Result<Vec<Game>> {
    let mut games = Vec::new();
    pgn::read_games(source, |entry| {
        if let Entry::Game { game, problem } = entry {
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

const NESTED_GAME: &str = "1. e4 e5 (1... c5 2. Nf3 (2. Nc3)) 2. Nf3 Nc6 *\n";
const FIRST_MOVE_VARIATIONS: &str = "1. e4 (1. d4 (1. c4)) (1. Nf3) *\n";

#[test]
fn promotes_the_line_containing_the_current_move_one_level() -> io::Result<()> {
    let sicilian = "1. e4 c5 (1... e5 2. Nf3 Nc6) 2. Nf3 (2. Nc3) *\n";
    for (before, after, expected) in [
        (&[0, 1][..], &[0, 0][..], sicilian),
        (&[0, 1, 0], &[0, 0, 0], sicilian),
        (
            &[0, 1, 1],
            &[0, 1, 0],
            "1. e4 e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3 Nc6 *\n",
        ),
    ] {
        let mut editor = open(NESTED_GAME)?;
        editor.go_to(Place(before.to_vec())).unwrap();
        let position = editor.position().clone();
        editor.promote().unwrap();
        assert_state(&editor, after, expected)?;
        assert_eq!(editor.position(), &position);
    }
    Ok(())
}

#[test]
fn promotion_preserves_comments_and_sibling_variations() -> io::Result<()> {
    let mut editor = open("1. e4 e5 {after} (1... c5 {sicilian}) (1... e6) 2. Nf3 *")?;
    editor.go_to(Place(vec![0, 1])).unwrap();
    editor.promote().unwrap();
    assert_state(
        &editor,
        &[0, 0],
        "1. e4 c5 {sicilian} (1... e5 {after} 2. Nf3) (1... e6) *\n",
    )
}

#[test]
fn promotion_swaps_only_two_choices_among_first_move_variations() -> io::Result<()> {
    let mut editor = open(FIRST_MOVE_VARIATIONS)?;
    editor.go_to(Place(vec![2])).unwrap();
    editor.promote().unwrap();
    assert_state(&editor, &[1], "1. e4 (1. c4 (1. d4)) (1. Nf3) *\n")?;

    let mut editor = open(FIRST_MOVE_VARIATIONS)?;
    editor.go_to(Place(vec![1])).unwrap();
    editor.promote().unwrap();
    let expected = "1. d4 (1. e4) (1. c4) (1. Nf3) *\n";
    assert_state(&editor, &[0], expected)?;
    for (choice, square, role) in [
        (1, E4, Role::Pawn),
        (2, C4, Role::Pawn),
        (3, F3, Role::Knight),
    ] {
        editor.go_to(Place(vec![choice])).unwrap();
        assert_eq!(
            editor.position().board().piece_at(square),
            Some(role.of(Color::White))
        );
    }
    Ok(())
}

#[test]
fn deleting_a_variation_returns_to_the_position_before_its_first_move() -> io::Result<()> {
    for (game, before, after, expected) in [
        (
            NESTED_GAME,
            &[0, 1, 0][..],
            &[0][..],
            "1. e4 e5 2. Nf3 Nc6 *\n",
        ),
        (
            NESTED_GAME,
            &[0, 1, 1],
            &[0, 1],
            "1. e4 e5 (1... c5 2. Nf3) 2. Nf3 Nc6 *\n",
        ),
        (
            FIRST_MOVE_VARIATIONS,
            &[2],
            &[],
            "1. e4 (1. d4) (1. Nf3) *\n",
        ),
    ] {
        let mut editor = open(game)?;
        editor.go_to(Place(after.to_vec())).unwrap();
        let position = editor.position().clone();
        editor.go_to(Place(before.to_vec())).unwrap();
        editor.delete_variation().unwrap();
        assert_state(&editor, after, expected)?;
        assert_eq!(editor.position(), &position);
    }
    Ok(())
}

#[test]
fn deleting_the_rest_removes_all_continuations_from_the_current_place() -> io::Result<()> {
    for (place, expected) in [
        (&[0, 0][..], "1. e4 e5 (1... c5 2. Nf3 (2. Nc3)) *\n"),
        (&[0, 1], "1. e4 e5 (1... c5) 2. Nf3 Nc6 *\n"),
        (&[0], "1. e4 *\n"),
        (&[], "*\n"),
        (&[0, 0, 0, 0], NESTED_GAME),
        (&[0, 1, 1], NESTED_GAME),
    ] {
        let mut editor = open(NESTED_GAME)?;
        editor.go_to(Place(place.to_vec())).unwrap();
        let position = editor.position().clone();
        editor.delete_rest();
        assert_state(&editor, place, expected)?;
        assert_eq!(editor.position(), &position);
    }
    Ok(())
}

#[test]
fn deleting_all_moves_discards_move_comments_and_keeps_the_tags_and_result() -> io::Result<()> {
    let mut editor = open("[Event \"Final\"]\n\n{before} 1. e4 {after} 1-0")?;
    editor.delete_rest();
    assert_state(&editor, &[], "[Event \"Final\"]\n\n1-0\n")
}

#[test]
fn variation_edits_refuse_the_start_and_main_line() -> io::Result<()> {
    let mut editor = open(NESTED_GAME)?;
    for place in [&[][..], &[0, 0]] {
        editor.go_to(Place(place.to_vec())).unwrap();
        let position = editor.position().clone();
        assert_eq!(editor.promote(), Err(EditVariationError::NoVariation));
        assert_state(&editor, place, NESTED_GAME)?;
        assert_eq!(editor.position(), &position);
        assert_eq!(
            editor.delete_variation(),
            Err(EditVariationError::NoVariation)
        );
        assert_state(&editor, place, NESTED_GAME)?;
        assert_eq!(editor.position(), &position);
    }
    Ok(())
}

#[test]
fn promotion_checks_the_depth_of_the_demoted_continuation() -> io::Result<()> {
    for depth in [MAX_VARIATION_DEPTH - 1, MAX_VARIATION_DEPTH] {
        let replies = format!("e5{}{}", " (1... c5".repeat(depth), ")".repeat(depth));
        let before = format!("1. e4 (1. d4) 1... {replies} *\n");
        let mut editor = open(&before)?;
        editor.go_to(Place(vec![1])).unwrap();
        let position = editor.position().clone();
        if depth == MAX_VARIATION_DEPTH {
            assert_eq!(editor.promote(), Err(EditVariationError::TooDeep));
            assert_state(&editor, &[1], &before)?;
        } else {
            editor.promote().unwrap();
            assert_state(&editor, &[0], &format!("1. d4 (1. e4 {replies}) *\n"))?;
        }
        assert_eq!(editor.position(), &position);
    }
    Ok(())
}

#[test]
fn a_sibling_at_the_depth_limit_keeps_its_place_during_promotion() -> io::Result<()> {
    let nested = format!(
        "{}{}",
        " (1. c4".repeat(MAX_VARIATION_DEPTH),
        ")".repeat(MAX_VARIATION_DEPTH)
    );
    let mut editor = open(&format!("1. e4{nested} (1. d4) *"))?;
    editor.go_to(Place(vec![MAX_VARIATION_DEPTH + 1])).unwrap();
    editor.promote().unwrap();
    assert_state(&editor, &[0], &format!("1. d4{nested} (1. e4) *\n"))
}

const ANNOTATION_GAME: &str = "1. e4 e5 (1... c5) 2. Nf3 *\n";

#[test]
fn sets_replaces_and_removes_the_comment_at_a_place() -> io::Result<()> {
    let mut editor = open(ANNOTATION_GAME)?;
    editor.go_to(Place(vec![0])).unwrap();
    for (text, expected) in [
        (
            "Best by test",
            "1. e4 {Best by test} 1... e5 (1... c5) 2. Nf3 *\n",
        ),
        (
            "  King's pawn\t",
            "1. e4 {King's pawn} 1... e5 (1... c5) 2. Nf3 *\n",
        ),
        ("", ANNOTATION_GAME),
    ] {
        editor.set_comment(text).unwrap();
        assert_state(&editor, &[0], expected)?;
    }
    for (place, text, expected) in [
        (&[][..], "Start", "{Start} 1. e4 e5 (1... c5) 2. Nf3 *\n"),
        (&[0, 1], "Sharp", "1. e4 e5 (1... c5 {Sharp}) 2. Nf3 *\n"),
    ] {
        let mut editor = open(ANNOTATION_GAME)?;
        editor.go_to(Place(place.to_vec())).unwrap();
        editor.set_comment(text).unwrap();
        assert_state(&editor, place, expected)?;
    }
    Ok(())
}

#[test]
fn a_game_without_moves_can_keep_and_clear_a_comment() -> io::Result<()> {
    let mut editor = open("*")?;
    editor.set_comment("Only a comment").unwrap();
    assert_state(&editor, &[], "{Only a comment} *\n")?;
    editor.set_comment(" \t\n ").unwrap();
    assert_state(&editor, &[], "*\n")
}

#[test]
fn replacing_comments_leaves_those_before_the_next_move_intact() -> io::Result<()> {
    for (game, place, text, expected) in [
        (
            "1. e4 {one} {two} *",
            &[0][..],
            "three",
            "1. e4 {three} *\n",
        ),
        (
            "1. e4 e5 (1... c5) {After the variation} 2. Nf3 *",
            &[0, 0],
            "x",
            "1. e4 e5 {x} (1... c5) {After the variation} 2. Nf3 *\n",
        ),
    ] {
        let mut editor = open(game)?;
        editor.go_to(Place(place.to_vec())).unwrap();
        editor.set_comment(text).unwrap();
        assert_state(&editor, place, expected)?;
    }
    Ok(())
}

#[test]
fn move_and_position_symbols_can_be_set_and_removed_independently() -> io::Result<()> {
    let mut editor = open(ANNOTATION_GAME)?;
    editor.go_to(Place(vec![0])).unwrap();
    editor.set_move_symbol(Some(MoveSymbol::Good)).unwrap();
    assert_state(&editor, &[0], "1. e4 $1 1... e5 (1... c5) 2. Nf3 *\n")?;
    editor.set_move_symbol(Some(MoveSymbol::Blunder)).unwrap();
    assert_state(&editor, &[0], "1. e4 $4 1... e5 (1... c5) 2. Nf3 *\n")?;

    let mut editor = open(ANNOTATION_GAME)?;
    editor.go_to(Place(vec![0, 0])).unwrap();
    editor.set_move_symbol(Some(MoveSymbol::Dubious)).unwrap();
    assert_state(&editor, &[0, 0], "1. e4 e5 $6 (1... c5) 2. Nf3 *\n")?;
    editor
        .set_position_symbol(Some(PositionSymbol::WhiteSlight))
        .unwrap();
    assert_state(&editor, &[0, 0], "1. e4 e5 $6 $14 (1... c5) 2. Nf3 *\n")?;
    editor.set_move_symbol(None).unwrap();
    assert_state(&editor, &[0, 0], "1. e4 e5 $14 (1... c5) 2. Nf3 *\n")
}

#[test]
fn replacing_imported_symbols_preserves_every_glyph_of_other_kinds_in_order() -> io::Result<()> {
    let mut editor = open("1. e4 $1 $3 $10 $16 $7 *")?;
    editor.go_to(Place(vec![0])).unwrap();
    editor.set_move_symbol(Some(MoveSymbol::Mistake)).unwrap();
    assert_state(&editor, &[0], "1. e4 $2 $10 $16 $7 *\n")?;
    editor.set_position_symbol(None).unwrap();
    assert_state(&editor, &[0], "1. e4 $2 $7 *\n")
}

#[test]
fn refused_annotations_preserve_the_place_and_the_game() -> io::Result<()> {
    let original = "{before} 1. e4 {after} *\n";
    let mut editor = open(original)?;
    assert_eq!(
        editor.set_move_symbol(Some(MoveSymbol::Good)),
        Err(AnnotationError::NoMove)
    );
    assert_state(&editor, &[], original)?;
    assert_eq!(
        editor.set_position_symbol(None),
        Err(AnnotationError::NoMove)
    );
    assert_state(&editor, &[], original)?;

    editor.go_to(Place(vec![0])).unwrap();
    let too_long = "x".repeat((1 << 20) + 1);
    for text in ["contains }", "contains \u{feff}[", &too_long] {
        assert_eq!(
            editor.set_comment(text),
            Err(AnnotationError::InvalidComment)
        );
        assert_state(&editor, &[0], original)?;
    }
    Ok(())
}

#[test]
fn undo_and_redo_restore_moves_variations_and_places() -> io::Result<()> {
    let mut editor = open("*")?;
    let first = "1. e4 *\n";
    let main = "1. e4 e5 *\n";
    let variation = "1. e4 e5 (1... c5) *\n";
    editor.play(E2, E4, None).unwrap();
    assert_state(&editor, &[0], first)?;
    editor.play(E7, E5, None).unwrap();
    assert_state(&editor, &[0, 0], main)?;

    assert!(editor.undo());
    assert_state(&editor, &[0], first)?;
    assert!(editor.undo());
    assert_state(&editor, &[], "*\n")?;
    assert!(!editor.undo());
    assert_state(&editor, &[], "*\n")?;

    assert!(editor.redo());
    assert_state(&editor, &[0], first)?;
    assert!(editor.redo());
    assert_state(&editor, &[0, 0], main)?;
    assert!(!editor.redo());
    assert_state(&editor, &[0, 0], main)?;

    assert!(editor.back());
    assert_state(&editor, &[0], main)?;
    editor.play(C7, C5, None).unwrap();
    assert_state(&editor, &[0, 1], variation)?;
    assert!(editor.undo());
    assert_state(&editor, &[0], main)?;
    editor.play(E7, E5, None).unwrap();
    assert_state(&editor, &[0, 0], main)?;
    assert!(editor.can_redo());
    assert!(editor.redo());
    assert_state(&editor, &[0, 1], variation)
}

#[test]
fn undo_after_walking_restores_the_place_from_before_the_edit() -> io::Result<()> {
    let mut editor = open("*")?;
    editor.play(E2, E4, None).unwrap();
    editor.play(E7, E5, None).unwrap();
    assert!(editor.back());
    assert!(editor.back());
    assert!(editor.undo());
    assert_state(&editor, &[0], "1. e4 *\n")?;

    assert!(editor.redo());
    assert_state(&editor, &[], "1. e4 e5 *\n")?;
    editor.play(E2, E4, None).unwrap();
    assert!(editor.undo());
    assert_state(&editor, &[0], "1. e4 *\n")?;
    editor.play(C7, C5, None).unwrap();
    assert!(!editor.can_redo());
    assert!(!editor.redo());
    assert_state(&editor, &[0, 0], "1. e4 c5 *\n")
}

#[test]
fn refused_and_unchanged_edits_leave_history_alone() -> io::Result<()> {
    let mut editor = open("*")?;
    editor.play(E2, E4, None).unwrap();
    assert_eq!(editor.play(E2, E5, None), Err(PlayError::IllegalMove));
    assert!(editor.undo());
    assert_state(&editor, &[], "*\n")?;

    let annotated = "1. e4 $1 $14 {test} *\n";
    let mut editor = open(annotated)?;
    assert!(editor.forward());
    editor.play(E7, E5, None).unwrap();
    assert!(editor.undo());
    editor.set_comment(" test ").unwrap();
    editor.set_move_symbol(Some(MoveSymbol::Good)).unwrap();
    editor
        .set_position_symbol(Some(PositionSymbol::WhiteSlight))
        .unwrap();
    editor.delete_rest();
    assert_eq!(
        editor.delete_variation(),
        Err(EditVariationError::NoVariation)
    );
    assert!(!editor.can_undo());
    assert!(editor.can_redo());
    assert_state(&editor, &[0], annotated)?;
    assert!(editor.redo());
    assert_state(&editor, &[0, 0], "1. e4 $1 $14 {test} 1... e5 *\n")
}

fn assert_edit_round_trip(
    before: &[usize],
    edit: impl FnOnce(&mut GameEditor),
    after: &[usize],
    expected: &str,
) -> io::Result<()> {
    let mut editor = open(ANNOTATION_GAME)?;
    assert_eq!(editor.go_to(Place(before.to_vec())), Ok(()));
    let before_position = editor.position().clone();
    edit(&mut editor);
    assert_state(&editor, after, expected)?;
    let after_position = editor.position().clone();
    assert!(editor.undo());
    assert_state(&editor, before, ANNOTATION_GAME)?;
    assert_eq!(editor.position(), &before_position);
    assert!(editor.redo());
    assert_state(&editor, after, expected)?;
    assert_eq!(editor.position(), &after_position);
    Ok(())
}

#[test]
fn undo_and_redo_restore_an_edited_variation_and_its_place() -> io::Result<()> {
    assert_edit_round_trip(
        &[0, 1],
        |editor| editor.promote().unwrap(),
        &[0, 0],
        "1. e4 c5 (1... e5 2. Nf3) *\n",
    )?;
    assert_edit_round_trip(
        &[0, 1],
        |editor| editor.delete_variation().unwrap(),
        &[0],
        "1. e4 e5 2. Nf3 *\n",
    )?;
    assert_edit_round_trip(&[0], GameEditor::delete_rest, &[0], "1. e4 *\n")
}

#[test]
fn undo_and_redo_restore_comments_and_both_kinds_of_symbol() -> io::Result<()> {
    let place = &[0, 1];
    assert_edit_round_trip(
        place,
        |editor| editor.set_comment("Sharp").unwrap(),
        place,
        "1. e4 e5 (1... c5 {Sharp}) 2. Nf3 *\n",
    )?;
    assert_edit_round_trip(
        place,
        |editor| editor.set_move_symbol(Some(MoveSymbol::Good)).unwrap(),
        place,
        "1. e4 e5 (1... c5 $1) 2. Nf3 *\n",
    )?;
    assert_edit_round_trip(
        place,
        |editor| {
            editor
                .set_position_symbol(Some(PositionSymbol::Equal))
                .unwrap();
        },
        place,
        "1. e4 e5 (1... c5 $10) 2. Nf3 *\n",
    )
}

#[test]
fn the_current_line_determines_whether_a_move_follows_the_place() -> io::Result<()> {
    let mut editor =
        open("1. e4 (1. d4 (1. c4 e5)) 1... e5 (1... c5 2. Nf3 (2. Nc3)) 2. Nf3 Nc6 *")?;
    for (place, in_variation, has_next) in [
        (&[][..], false, true),
        (&[1], true, false),
        (&[2], true, true),
        (&[2, 0], true, false),
        (&[0, 0], false, true),
        (&[0, 1], true, true),
        (&[0, 1, 0], true, false),
        (&[0, 1, 1], true, false),
        (&[0, 0, 0, 0], false, false),
    ] {
        editor.go_to(Place(place.to_vec())).unwrap();
        assert_eq!(editor.in_variation(), in_variation);
        assert_eq!(editor.has_next_move(), has_next);
    }
    Ok(())
}
