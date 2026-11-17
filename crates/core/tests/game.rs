//! The game model, built move by move through the builder, the only way to make a game.

use shakmaty::fen::Fen;
use shakmaty::san::San;
use shakmaty::{Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position};
use zobrist_core::game::{
    AnnotationSymbol, Counts, Game, GameBuilder, MainLine, Move, MoveError, NoMove, Tag,
    VariationError,
};

fn tag(name: &str, value: &str) -> Tag {
    Tag {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

fn play(builder: &mut GameBuilder, moves: &[&str]) -> Result<(), String> {
    for text in moves {
        let san: San = text.parse().map_err(|error| format!("{text}: {error}"))?;
        builder
            .play(san)
            .map_err(|error| format!("{text}: {error:?}"))?;
    }
    Ok(())
}

fn notation(game: &Game) -> Vec<String> {
    game.steps()
        .map(|step| step.notation().to_string())
        .collect()
}

fn white_wins() -> Outcome {
    Outcome::Known(KnownOutcome::Decisive {
        winner: Color::White,
    })
}

#[test]
fn builds_the_first_game_of_clean_pgn() {
    let mut builder = GameBuilder::new(
        vec![tag("White", "Alpha"), tag("Black", "Beta")],
        Chess::default(),
    );
    play(&mut builder, &["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6"]).unwrap();
    builder.annotate(AnnotationSymbol(4)).unwrap();
    builder.comment("Overlooks the mate.".to_owned());
    builder.begin_variation().unwrap();
    play(&mut builder, &["g6", "Qf3"]).unwrap();
    builder.end_variation();
    play(&mut builder, &["Qxf7#"]).unwrap();
    let game = builder.finish(white_wins());

    assert_eq!(game.tag("White"), Some("Alpha"));
    assert_eq!(game.tag("Black"), Some("Beta"));
    assert_eq!(game.tag("Event"), None);
    assert_eq!(
        notation(&game),
        ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]
    );
    let knight = game.steps().nth(5).unwrap();
    assert_eq!(knight.notation().to_string(), "Nf6");
    assert_eq!(knight.node().annotation_symbols(), [AnnotationSymbol(4)]);
    assert_eq!(knight.node().comments_after(), ["Overlooks the mate."]);
    assert_eq!(knight.node().comments_before(), [] as [String; 0]);
    let variation: Vec<String> = knight
        .variations()
        .flat_map(|steps| steps.map(|step| step.notation().to_string()))
        .collect();
    assert_eq!(variation, ["g6", "Qf3"]);
    let first_of_variation = knight.variations().next().unwrap().next().unwrap();
    assert_eq!(
        u32::from(first_of_variation.position_before().fullmoves()),
        3
    );
    assert_eq!(
        Fen::from_position(first_of_variation.position_before(), EnPassantMode::Legal).to_string(),
        "r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3"
    );
    assert_eq!(game.result(), white_wins());
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 7,
            variations: 1,
            comments: 1,
        }
    );
}

#[test]
fn builds_the_second_game_of_clean_pgn() {
    let mut builder = GameBuilder::new(
        vec![tag("White", "Gamma"), tag("Black", "Delta")],
        Chess::default(),
    );
    play(&mut builder, &["f3", "e5", "g4", "Qh4"]).unwrap();
    let game = builder.finish(Outcome::Known(KnownOutcome::Decisive {
        winner: Color::Black,
    }));

    assert_eq!(notation(&game), ["f3", "e5", "g4", "Qh4#"]);
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 4,
            variations: 0,
            comments: 0,
        }
    );
}

#[test]
fn refuses_an_illegal_move_and_plays_the_next_legal_one() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4", "e5"]).unwrap();

    assert_eq!(
        builder.play("Ke3".parse().unwrap()),
        Err(MoveError::IllegalMove)
    );
    assert_eq!(u32::from(builder.position().fullmoves()), 2);
    play(&mut builder, &["Nf3"]).unwrap();
    assert_eq!(
        notation(&builder.finish(Outcome::Unknown)),
        ["e4", "e5", "Nf3"]
    );
}

#[test]
fn refuses_an_ambiguous_move() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["Nf3", "Nf6", "d4", "d5"]).unwrap();

    assert_eq!(
        builder.play("Nd2".parse().unwrap()),
        Err(MoveError::AmbiguousMove)
    );
}

#[test]
fn plays_a_null_move_and_counts_it_as_a_move() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4", "--", "d4"]).unwrap();
    let game = builder.finish(Outcome::Unknown);

    assert_eq!(notation(&game), ["e4", "--", "d4"]);
    let null = game.steps().nth(1).unwrap();
    assert_eq!(null.node().played(), Move::Null);
    let after_null = game.steps().nth(2).unwrap();
    assert_eq!(
        Fen::from_position(after_null.position_before(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 1 2"
    );
    assert_eq!(game.counts().main_line_moves, 3);
}

#[test]
fn refuses_a_null_move_in_check() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4", "f5", "Qh5+"]).unwrap();

    assert_eq!(
        builder.play("--".parse().unwrap()),
        Err(MoveError::IllegalMove)
    );
    assert_eq!(
        notation(&builder.finish(Outcome::Unknown)),
        ["e4", "f5", "Qh5+"]
    );
}

#[test]
fn refuses_a_variation_and_a_symbol_before_the_first_move() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());

    assert_eq!(builder.begin_variation(), Err(VariationError::NoMove));
    assert_eq!(builder.annotate(AnnotationSymbol(1)), Err(NoMove));

    play(&mut builder, &["e4"]).unwrap();
    builder.begin_variation().unwrap();
    assert_eq!(builder.begin_variation(), Err(VariationError::NoMove));
    assert_eq!(builder.annotate(AnnotationSymbol(1)), Err(NoMove));
}

#[test]
fn refuses_a_variation_nested_deeper_than_255_and_keeps_the_game_at_the_limit_usable() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    for _ in 0..255 {
        builder.begin_variation().unwrap();
        play(&mut builder, &["d4"]).unwrap();
    }

    assert_eq!(builder.begin_variation(), Err(VariationError::TooDeep));
    play(&mut builder, &["d5"]).unwrap();
    let game = builder.finish(Outcome::Unknown);
    let copy = game.clone();
    assert_eq!(copy, game);
    assert!(format!("{game:?}").starts_with("Game { tags: [], start: "));
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 1,
            variations: 255,
            comments: 0,
        }
    );
    drop(copy);
    drop(game);
}

#[test]
fn puts_a_comment_after_a_variation_before_the_next_move() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    builder.begin_variation().unwrap();
    builder.comment("Also good.".to_owned());
    play(&mut builder, &["d4"]).unwrap();
    builder.comment("Queen's pawn.".to_owned());
    builder.end_variation();
    builder.comment("Back to the game.".to_owned());
    play(&mut builder, &["e5"]).unwrap();
    let game = builder.finish(Outcome::Unknown);

    let first = game.steps().next().unwrap();
    assert_eq!(first.node().comments_after(), [] as [String; 0]);
    let variation = first.node().variations()[0].nodes().next().unwrap();
    assert_eq!(variation.comments_before(), ["Also good."]);
    assert_eq!(variation.comments_after(), ["Queen's pawn."]);
    let second = game.steps().nth(1).unwrap();
    assert_eq!(second.node().comments_before(), ["Back to the game."]);
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 2,
            variations: 1,
            comments: 3,
        }
    );
}

#[test]
fn keeps_no_empty_variation_and_hands_its_comment_to_the_line_below() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    builder.begin_variation().unwrap();
    builder.comment("Best by test.".to_owned());
    builder.end_variation();
    let game = builder.finish(Outcome::Unknown);

    let first = game.steps().next().unwrap();
    assert_eq!(first.node().variations().len(), 0);
    assert_eq!(first.node().comments_after(), ["Best by test."]);
}

#[test]
fn closes_an_open_variation_when_the_game_finishes() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    builder.begin_variation().unwrap();
    play(&mut builder, &["d4"]).unwrap();
    let game = builder.finish(Outcome::Unknown);

    assert_eq!(notation(&game), ["e4"]);
    assert_eq!(game.counts().variations, 1);
}

#[test]
fn ignores_the_end_of_a_variation_that_was_never_opened() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    builder.end_variation();
    play(&mut builder, &["e5"]).unwrap();

    assert_eq!(notation(&builder.finish(Outcome::Unknown)), ["e4", "e5"]);
}

#[test]
fn keeps_the_comments_of_a_game_without_moves() {
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    builder.comment("White forfeits.".to_owned());
    let game = builder.finish(Outcome::Known(KnownOutcome::Decisive {
        winner: Color::Black,
    }));

    assert_eq!(
        game.main_line(),
        &MainLine::Empty {
            comments: vec!["White forfeits.".to_owned()]
        }
    );
    assert_eq!(game.steps().count(), 0);
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 0,
            variations: 0,
            comments: 1,
        }
    );
}
