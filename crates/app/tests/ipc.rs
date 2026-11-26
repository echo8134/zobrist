//! Requests and complete answers in the format that the view sends and receives.

use serde_json::{Value, json};
use zobrist_app::ipc::{Games, Request};

fn ask(games: &mut Games, request: Value) -> serde_json::Result<Value> {
    let request = serde_json::from_value(request)?;
    let mut response = serde_json::to_value(games.answer(request))?;
    // The board offers these moves as a set. Compare every entry, including duplicates.
    if let Some(moves) = response
        .pointer_mut("/picture/legalMoves")
        .and_then(Value::as_array_mut)
    {
        moves.sort_by(|left, right| {
            (left["from"].as_str(), left["to"].as_str())
                .cmp(&(right["from"].as_str(), right["to"].as_str()))
        });
    }
    Ok(response)
}

#[track_caller]
fn assert_picture(games: &mut Games, request: Value, picture: &Value) -> serde_json::Result<()> {
    let expected = json!({"type": request["type"], "picture": picture});
    assert_eq!(ask(games, request)?, expected);
    Ok(())
}

fn play(
    games: &mut Games,
    coordinates: &str,
    promotion: Option<&str>,
) -> serde_json::Result<Value> {
    let (from, to) = coordinates.split_at(2);
    ask(
        games,
        json!({"type": "PlayMove", "game": 1, "from": from, "to": to, "promotion": promotion}),
    )
}

fn play_line(games: &mut Games, moves: &str) -> serde_json::Result<Value> {
    let mut answer = ask(games, json!({"type": "NewGame"}))?;
    for coordinates in moves.split_whitespace() {
        answer = play(games, coordinates, None)?;
    }
    Ok(answer)
}

fn legal_moves(coordinates: &str) -> Vec<Value> {
    coordinates
        .split_whitespace()
        .map(|entry| {
            let (from, to) = entry.trim_end_matches('=').split_at(2);
            json!({"from": from, "to": to, "promotion": entry.ends_with('=')})
        })
        .collect()
}

fn main_line(notation: &str) -> Vec<Value> {
    notation
        .split_whitespace()
        .enumerate()
        .map(|(ply, text)| {
            let place = vec![0; ply + 1];
            let side = if ply.is_multiple_of(2) {
                "white"
            } else {
                "black"
            };
            json!({"place": place, "number": ply / 2 + 1, "side": side, "text": text,
                   "commentsBefore": [], "commentsAfter": [], "symbols": [], "variations": []})
        })
        .collect()
}

fn start_picture() -> Value {
    json!({
        "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "turn": "white", "inCheck": false,
        "legalMoves": legal_moves("a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d2d3 d2d4
                                  e2e3 e2e4 f2f3 f2f4 g1f3 g1h3 g2g3 g2g4 h2h3 h2h4"),
        "startComments": [], "moveSymbol": null, "positionSymbol": null,
        "lastMove": null, "place": [], "notation": []
    })
}

fn after_e4() -> Value {
    json!({
        "fen": "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        "turn": "black", "inCheck": false,
        "legalMoves": legal_moves("a7a5 a7a6 b7b5 b7b6 b8a6 b8c6 c7c5 c7c6 d7d5 d7d6
                                  e7e5 e7e6 f7f5 f7f6 g7g5 g7g6 g8f6 g8h6 h7h5 h7h6"),
        "startComments": [], "moveSymbol": null, "positionSymbol": null,
        "lastMove": {"from": "e2", "to": "e4"}, "place": [0],
        "notation": [{"place": [0], "number": 1, "side": "white", "text": "e4",
                      "commentsBefore": [], "commentsAfter": [], "symbols": [], "variations": []}]
    })
}

#[test]
fn answers_the_version_request_with_the_version_of_the_crate() -> serde_json::Result<()> {
    // A release PR changes the version and nothing else, so a literal here would fail it.
    assert_eq!(
        ask(&mut Games::default(), json!({"type": "Version"}))?,
        json!({"type": "Version", "version": env!("CARGO_PKG_VERSION")})
    );
    Ok(())
}

#[test]
fn each_new_game_has_its_own_position_and_number_even_after_closing_a_game()
-> serde_json::Result<()> {
    let mut games = Games::default();
    for number in [1, 2] {
        assert_eq!(
            ask(&mut games, json!({"type": "NewGame"}))?,
            json!({"type": "NewGame", "game": number, "picture": start_picture()})
        );
    }

    assert_eq!(
        play(&mut games, "e2e5", None)?,
        json!({"type": "PlayMove", "picture": start_picture()})
    );
    assert_eq!(
        play(&mut games, "e2e4", None)?,
        json!({"type": "PlayMove", "picture": after_e4()})
    );
    assert_eq!(
        ask(&mut games, json!({"type": "GoTo", "game": 2, "place": []}))?,
        json!({"type": "GoTo", "picture": start_picture()})
    );

    assert_eq!(
        ask(&mut games, json!({"type": "CloseGame", "game": 1}))?,
        json!({"type": "CloseGame"})
    );
    assert_eq!(
        play(&mut games, "e2e4", None)?,
        json!({"type": "PlayMove", "picture": null})
    );
    assert_eq!(
        ask(&mut games, json!({"type": "GoTo", "game": 1, "place": []}))?,
        json!({"type": "GoTo", "picture": null})
    );
    for request in [
        json!({"type": "SetComment", "game": 1, "text": "Closed"}),
        json!({"type": "SetMoveSymbol", "game": 1, "symbol": "!"}),
        json!({"type": "SetPositionSymbol", "game": 1, "symbol": "+-"}),
    ] {
        assert_picture(&mut games, request, &Value::Null)?;
    }
    assert_eq!(
        ask(&mut games, json!({"type": "GoTo", "game": 2, "place": []}))?,
        json!({"type": "GoTo", "picture": start_picture()})
    );
    assert_eq!(
        ask(&mut games, json!({"type": "NewGame"}))?,
        json!({"type": "NewGame", "game": 3, "picture": start_picture()})
    );
    Ok(())
}

#[test]
fn rejects_a_malformed_square_place_promotion_or_symbol_at_the_wire() {
    for request in [
        json!({"type": "PlayMove", "game": 1, "from": "e9", "to": "e4", "promotion": null}),
        json!({"type": "PlayMove", "game": 1, "from": "e2", "to": "z4", "promotion": null}),
        json!({"type": "PlayMove", "game": 1, "from": "h7", "to": "g8", "promotion": "king"}),
        json!({"type": "PlayMove", "game": 1, "from": "h7", "to": "g8", "promotion": "pawn"}),
        json!({"type": "GoTo", "game": 1, "place": [-1]}),
        json!({"type": "SetMoveSymbol", "game": 1, "symbol": "="}),
        json!({"type": "SetPositionSymbol", "game": 1, "symbol": "!"}),
    ] {
        assert!(serde_json::from_value::<Request>(request).is_err());
    }
}

#[test]
fn comments_and_symbols_follow_the_current_move() -> serde_json::Result<()> {
    let mut games = Games::default();
    play_line(&mut games, "e2e4")?;
    let mut expected = after_e4();
    expected["notation"][0]["commentsAfter"] = json!(["Best by test"]);
    for text in ["Best by test", "invalid }"] {
        assert_picture(
            &mut games,
            json!({"type": "SetComment", "game": 1, "text": text}),
            &expected,
        )?;
    }

    expected["moveSymbol"] = json!("!");
    expected["notation"][0]["symbols"] = json!(["!"]);
    assert_picture(
        &mut games,
        json!({"type": "SetMoveSymbol", "game": 1, "symbol": "!"}),
        &expected,
    )?;
    expected["positionSymbol"] = json!("+-");
    expected["notation"][0]["symbols"] = json!(["!", "+-"]);
    assert_picture(
        &mut games,
        json!({"type": "SetPositionSymbol", "game": 1, "symbol": "+-"}),
        &expected,
    )?;

    let mut start = start_picture();
    start["notation"] = expected["notation"].clone();
    assert_picture(
        &mut games,
        json!({"type": "GoTo", "game": 1, "place": []}),
        &start,
    )?;
    start["notation"][0]["commentsBefore"] = json!(["Start"]);
    assert_picture(
        &mut games,
        json!({"type": "SetComment", "game": 1, "text": "Start"}),
        &start,
    )?;
    expected["notation"] = start["notation"].clone();
    assert_picture(
        &mut games,
        json!({"type": "GoTo", "game": 1, "place": [0]}),
        &expected,
    )?;

    expected["moveSymbol"] = Value::Null;
    expected["notation"][0]["symbols"] = json!(["+-"]);
    assert_picture(
        &mut games,
        json!({"type": "SetMoveSymbol", "game": 1, "symbol": null}),
        &expected,
    )?;
    expected["positionSymbol"] = Value::Null;
    expected["notation"][0]["symbols"] = json!([]);
    assert_picture(
        &mut games,
        json!({"type": "SetPositionSymbol", "game": 1, "symbol": null}),
        &expected,
    )?;
    expected["notation"][0]["commentsAfter"] = json!([]);
    assert_picture(
        &mut games,
        json!({"type": "SetComment", "game": 1, "text": ""}),
        &expected,
    )?;
    Ok(())
}

#[test]
fn the_start_of_an_empty_game_has_a_comment_and_refuses_symbols() -> serde_json::Result<()> {
    let mut games = Games::default();
    ask(&mut games, json!({"type": "NewGame"}))?;
    let mut start = start_picture();
    start["startComments"] = json!(["Start"]);
    assert_picture(
        &mut games,
        json!({"type": "SetComment", "game": 1, "text": "Start"}),
        &start,
    )?;
    for (kind, symbol) in [("SetMoveSymbol", "!"), ("SetPositionSymbol", "∞")] {
        assert_picture(
            &mut games,
            json!({"type": kind, "game": 1, "symbol": symbol}),
            &start,
        )?;
    }
    let mut expected = after_e4();
    expected["notation"][0]["commentsBefore"] = json!(["Start"]);
    assert_eq!(
        play(&mut games, "e2e4", None)?,
        json!({"type": "PlayMove", "picture": expected})
    );
    Ok(())
}

#[test]
fn the_board_offers_one_promotion_and_the_player_chooses_the_piece() -> serde_json::Result<()> {
    let mut games = Games::default();
    let ready = json!({"type": "PlayMove", "picture": {
        "fen": "rnbqkbnr/1ppppp1P/8/p7/8/8/PPPPPPP1/RNBQKBNR w KQkq - 0 5",
        "turn": "white", "inCheck": false,
        "legalMoves": legal_moves("a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d2d3 d2d4 e2e3 e2e4
                                  f2f3 f2f4 g1f3 g1h3 g2g3 g2g4 h1h2 h1h3 h1h4 h1h5 h1h6 h7g8="),
        "startComments": [], "moveSymbol": null, "positionSymbol": null,
        "lastMove": {"from": "a6", "to": "a5"}, "place": [0, 0, 0, 0, 0, 0, 0, 0],
        "notation": main_line("h4 g5 hxg5 h6 gxh6 a6 h7 a5")
    }});
    assert_eq!(
        play_line(&mut games, "h2h4 g7g5 h4g5 h7h6 g5h6 a7a6 h6h7 a6a5")?,
        ready
    );
    assert_eq!(play(&mut games, "h7g8", None)?, ready);
    assert_eq!(
        play(&mut games, "h7g8", Some("knight"))?,
        json!({"type": "PlayMove", "picture": {
            "fen": "rnbqkbNr/1ppppp2/8/p7/8/8/PPPPPPP1/RNBQKBNR b KQkq - 0 5",
            "turn": "black", "inCheck": false,
            "legalMoves": legal_moves("a5a4 a8a6 a8a7 b7b5 b7b6 b8a6 b8c6 c7c5 c7c6 d7d5 d7d6 e7e5 e7e6
                                      f7f5 f7f6 f8g7 f8h6 h8g8 h8h1 h8h2 h8h3 h8h4 h8h5 h8h6 h8h7"),
            "startComments": [], "moveSymbol": null, "positionSymbol": null,
            "lastMove": {"from": "h7", "to": "g8"}, "place": [0, 0, 0, 0, 0, 0, 0, 0, 0],
            "notation": main_line("h4 g5 hxg5 h6 gxh6 a6 h7 a5 hxg8=N")
        }})
    );
    Ok(())
}

#[test]
fn both_castling_gestures_give_the_same_picture_and_the_board_offers_one() -> serde_json::Result<()>
{
    for gesture in ["e1g1", "e1h1"] {
        let mut games = Games::default();
        assert_eq!(
            play_line(&mut games, "e2e4 e7e5 g1f3 b8c6 f1c4 f8c5")?,
            json!({"type": "PlayMove", "picture": {
                "fen": "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
                "turn": "white", "inCheck": false,
                "legalMoves": legal_moves("a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c4a6 c4b3 c4b5 c4d3 c4d5 c4e2
                                          c4e6 c4f1 c4f7 d1e2 d2d3 d2d4 e1e2 e1f1 e1g1 f3d4 f3e5 f3g1 f3g5
                                          f3h4 g2g3 g2g4 h1f1 h1g1 h2h3 h2h4"),
                "startComments": [], "moveSymbol": null, "positionSymbol": null,
                "lastMove": {"from": "f8", "to": "c5"}, "place": [0, 0, 0, 0, 0, 0],
                "notation": main_line("e4 e5 Nf3 Nc6 Bc4 Bc5")
            }})
        );
        assert_eq!(
            play(&mut games, gesture, None)?,
            json!({"type": "PlayMove", "picture": {
                "fen": "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 b kq - 5 4",
                "turn": "black", "inCheck": false,
                "legalMoves": legal_moves("a7a5 a7a6 a8b8 b7b5 b7b6 c5a3 c5b4 c5b6 c5d4 c5d6 c5e3 c5e7 c5f2
                                          c5f8 c6a5 c6b4 c6b8 c6d4 c6e7 d7d5 d7d6 d8e7 d8f6 d8g5 d8h4
                                          e8e7 e8f8 f7f5 f7f6 g7g5 g7g6 g8e7 g8f6 g8h6 h7h5 h7h6"),
                "startComments": [], "moveSymbol": null, "positionSymbol": null,
                "lastMove": {"from": "e1", "to": "g1"}, "place": [0, 0, 0, 0, 0, 0, 0],
                "notation": main_line("e4 e5 Nf3 Nc6 Bc4 Bc5 O-O")
            }})
        );
    }
    Ok(())
}

#[test]
fn a_mated_side_is_in_check_and_has_no_legal_move() -> serde_json::Result<()> {
    let mut games = Games::default();
    assert_eq!(
        play_line(&mut games, "e2e4 e7e5 d1h5 b8c6 f1c4 g8f6 h5f7")?,
        json!({"type": "PlayMove", "picture": {
            "fen": "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4",
            "turn": "black", "inCheck": true, "legalMoves": [],
            "startComments": [], "moveSymbol": null, "positionSymbol": null,
            "lastMove": {"from": "h5", "to": "f7"}, "place": [0, 0, 0, 0, 0, 0, 0],
            "notation": main_line("e4 e5 Qh5 Nc6 Bc4 Nf6 Qxf7#")
        }})
    );
    Ok(())
}

#[test]
fn going_back_keeps_the_notation_and_a_different_reply_adds_a_variation() -> serde_json::Result<()>
{
    let mut games = Games::default();
    play_line(&mut games, "e2e4 e7e5")?;
    let mut earlier = after_e4();
    earlier["notation"] = json!(main_line("e4 e5"));
    assert_eq!(
        ask(&mut games, json!({"type": "GoTo", "game": 1, "place": [0]}))?,
        json!({"type": "GoTo", "picture": earlier})
    );

    let variation = json!({
        "fen": "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
        "turn": "white", "inCheck": false,
        "legalMoves": legal_moves("a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d1e2 d1f3 d1g4 d1h5 d2d3 d2d4
                                  e1e2 e4e5 f1a6 f1b5 f1c4 f1d3 f1e2 f2f3 f2f4 g1e2 g1f3 g1h3 g2g3 g2g4 h2h3 h2h4"),
        "startComments": [], "moveSymbol": null, "positionSymbol": null,
        "lastMove": {"from": "c7", "to": "c5"}, "place": [0, 1],
        "notation": [
            {"place": [0], "number": 1, "side": "white", "text": "e4",
             "commentsBefore": [], "commentsAfter": [], "symbols": [], "variations": []},
            {"place": [0, 0], "number": 1, "side": "black", "text": "e5",
             "commentsBefore": [], "commentsAfter": [], "symbols": [], "variations": [
                [{"place": [0, 1], "number": 1, "side": "black", "text": "c5",
                  "commentsBefore": [], "commentsAfter": [], "symbols": [], "variations": []}]
            ]}
        ]
    });
    assert_eq!(
        play(&mut games, "c7c5", None)?,
        json!({"type": "PlayMove", "picture": variation})
    );
    assert_eq!(
        ask(
            &mut games,
            json!({"type": "GoTo", "game": 1, "place": [0, 2]})
        )?,
        json!({"type": "GoTo", "picture": variation})
    );
    Ok(())
}
