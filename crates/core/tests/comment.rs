//! Evaluation, remaining time and prose recorded after a move.

use zobrist_core::comment::MoveReadings;

#[test]
fn reads_the_first_valid_score_and_clock_and_keeps_the_other_text() {
    let mut readings = MoveReadings::default();
    assert_eq!(
        readings.read_comment("Centre. [%eval ?,18] [%csl Ge4] [%eval 0.34,18] [%clk later]"),
        "Centre. [%eval ?,18] [%clk later]"
    );
    assert_eq!(
        readings.read_comment(
            "[%clk 0:04:59.50] [%eval -0.25] [%clk 0:02:04] [%cal Ge2e4] [%source local] More."
        ),
        "[%source local] More."
    );
    assert_eq!(readings.evaluation.as_deref(), Some("+0.34"));
    assert_eq!(readings.clock.as_deref(), Some("0:04:59.50"));
}

#[test]
fn shows_pawn_scores_and_mate_distances_without_the_depth() {
    for (input, expected) in [
        ("[%eval -0.25]", "-0.25"),
        ("[%eval +1.20,21]", "+1.20"),
        ("[%eval 0]", "+0"),
        ("[%eval #-1]", "#-1"),
        ("[%eval #3,15]", "#3"),
    ] {
        let mut readings = MoveReadings::default();
        assert_eq!(readings.read_comment(input), "");
        assert_eq!(readings.evaluation.as_deref(), Some(expected), "{input}");
        assert_eq!(readings.clock, None);
    }
    let mut readings = MoveReadings::default();
    assert_eq!(readings.read_comment("[%clk 12:59:00]"), "");
    assert_eq!(readings.clock.as_deref(), Some("12:59:00"));
    assert_eq!(readings.evaluation, None);
}

#[test]
fn leaves_malformed_commands_in_the_notation() {
    for input in [
        "Plan. [%clk later]",
        "[%clk 1:62:02]",
        "[%clk 0:04:60.5]",
        "[%clk 0:04:59.]",
        "[%clk 0:04:59:01]",
        "[%eval -0.25,x]",
        "[%eval #1.5]",
        "[%eval NaN]",
        "[%eval +]",
        "[%eval 1.2.3]",
        "[%eval --1]",
        "[%eval 0.4",
    ] {
        let mut readings = MoveReadings::default();
        assert_eq!(readings.read_comment(input), input);
        assert_eq!(readings.evaluation, None);
        assert_eq!(readings.clock, None);
    }
}
