//! Reading and writing board annotations in a PGN comment.

use shakmaty::Square::{self, A3, B1, B4, C2, C3, C4, D1, D4, D8, E2, E4, E5};
use zobrist_core::shapes::Color::{self, Blue, Green, Red, Yellow};
use zobrist_core::shapes::{Arrow, ColoredSquare, Shapes, join, split};

fn square(color: Color, square: Square) -> ColoredSquare {
    ColoredSquare { color, square }
}

#[test]
fn splits_commands_in_comment_order() {
    for (comment, text, squares, arrows) in [
        (
            "A good square [%csl Gd4] [%cal Ge2e4,Rd1d8]",
            "A good square",
            vec![square(Green, D4)],
            vec![
                Arrow::new(Green, E2, E4).unwrap(),
                Arrow::new(Red, D1, D8).unwrap(),
            ],
        ),
        (
            "[%cal Yb1c3]",
            "",
            vec![],
            vec![Arrow::new(Yellow, B1, C3).unwrap()],
        ),
        (
            "[%csl Ra3,Bb4][%cal Gc2c4] Plan",
            "Plan",
            vec![square(Red, A3), square(Blue, B4)],
            vec![Arrow::new(Green, C2, C4).unwrap()],
        ),
        (
            "Before [%csl Gd4] after",
            "Before after",
            vec![square(Green, D4)],
            vec![],
        ),
        ("No shapes here", "No shapes here", vec![], vec![]),
        (
            "[%clk 1:00:00] [%csl Gd4]",
            "[%clk 1:00:00]",
            vec![square(Green, D4)],
            vec![],
        ),
        (
            "[%csl Gd4] [%csl Re5]",
            "",
            vec![square(Green, D4), square(Red, E5)],
            vec![],
        ),
        (
            "[%csl Gd4,Gd4] [%cal Ge2e4,Ge2e4]",
            "",
            vec![square(Green, D4)],
            vec![Arrow::new(Green, E2, E4).unwrap()],
        ),
    ] {
        let (actual_text, shapes) = split(comment);
        assert_eq!(actual_text, text, "{comment}");
        assert_eq!(shapes.squares(), squares, "{comment}");
        assert_eq!(shapes.arrows(), arrows, "{comment}");
    }
}

#[test]
fn keeps_each_malformed_command_as_text() {
    for comment in [
        "[%csl Xd4]",
        "[%csl Gd9]",
        "[%cal Ge2]",
        "[%cal Ge2e2]",
        "[%csl Gd4,]",
        "[%csl]",
        "[%csl Gd4",
        "[%cal Ge2e4,Rd1d1]",
    ] {
        assert_eq!(split(comment), (comment.to_owned(), Shapes::default()));
    }
}

#[test]
fn preserves_text_spacing_away_from_shapes() {
    let (text, shapes) = split("  A  plan\t[%cal Ge2e4] \n [%csl Bb4]\tfor  later.  ");
    assert_eq!(text, "A  plan for  later.");
    assert_eq!(shapes.squares(), [square(Blue, B4)]);
    assert_eq!(shapes.arrows(), [Arrow::new(Green, E2, E4).unwrap()]);
}

#[test]
fn joins_the_parts_then_splits_them_back() {
    for (text, squares, arrows, comment) in [
        (
            "A good square",
            vec![square(Green, D4)],
            vec![
                Arrow::new(Green, E2, E4).unwrap(),
                Arrow::new(Red, D1, D8).unwrap(),
            ],
            "A good square [%csl Gd4] [%cal Ge2e4,Rd1d8]",
        ),
        (
            "",
            vec![],
            vec![Arrow::new(Yellow, B1, C3).unwrap()],
            "[%cal Yb1c3]",
        ),
        (
            "Plan",
            vec![square(Red, A3), square(Blue, B4)],
            vec![],
            "Plan [%csl Ra3,Bb4]",
        ),
        ("Plan", vec![], vec![], "Plan"),
        ("", vec![], vec![], ""),
    ] {
        let mut shapes = Shapes::default();
        for square in squares {
            shapes.add_square(square);
        }
        for arrow in arrows {
            shapes.add_arrow(arrow);
        }
        let joined = join(text, &shapes);
        assert_eq!(joined, comment);
        assert_eq!(split(&joined), (text.to_owned(), shapes));
    }
}

#[test]
fn constructing_shapes_keeps_only_the_first_copy_of_each() {
    assert_eq!(Arrow::new(Red, E2, E2), None);
    let mut shapes = Shapes::default();
    for color in [Green, Red, Green] {
        shapes.add_square(square(color, D4));
        shapes.add_arrow(Arrow::new(color, E2, E4).unwrap());
    }
    assert_eq!(shapes.squares(), [square(Green, D4), square(Red, D4)]);
    assert_eq!(
        shapes.arrows(),
        [
            Arrow::new(Green, E2, E4).unwrap(),
            Arrow::new(Red, E2, E4).unwrap()
        ]
    );
}
