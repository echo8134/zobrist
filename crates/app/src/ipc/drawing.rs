use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zobrist_core::shapes::{self, Shapes};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum ShapeColor {
    Green,
    Red,
    Yellow,
    Blue,
}

impl From<shapes::Color> for ShapeColor {
    fn from(color: shapes::Color) -> Self {
        match color {
            shapes::Color::Green => Self::Green,
            shapes::Color::Red => Self::Red,
            shapes::Color::Yellow => Self::Yellow,
            shapes::Color::Blue => Self::Blue,
        }
    }
}

impl From<ShapeColor> for shapes::Color {
    fn from(color: ShapeColor) -> Self {
        match color {
            ShapeColor::Green => Self::Green,
            ShapeColor::Red => Self::Red,
            ShapeColor::Yellow => Self::Yellow,
            ShapeColor::Blue => Self::Blue,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
struct ColoredSquare {
    color: ShapeColor,
    square: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
struct Arrow {
    color: ShapeColor,
    from: String,
    to: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
pub(super) struct BoardShapes {
    squares: Vec<ColoredSquare>,
    arrows: Vec<Arrow>,
}

impl From<&Shapes> for BoardShapes {
    fn from(shapes: &Shapes) -> Self {
        Self {
            squares: shapes
                .squares()
                .iter()
                .map(|mark| ColoredSquare {
                    color: mark.color.into(),
                    square: mark.square.to_string(),
                })
                .collect(),
            arrows: shapes
                .arrows()
                .iter()
                .map(|arrow| Arrow {
                    color: arrow.color.into(),
                    from: arrow.start().to_string(),
                    to: arrow.end().to_string(),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize, TS)]
#[ts(rename = "ShapeEdit")]
pub(super) struct ShapeEditFields {
    game: usize,
    #[serde(flatten)]
    shapes: BoardShapes,
}

/// The game number and the shapes to put at its current place.
#[derive(Clone, Deserialize)]
#[serde(try_from = "ShapeEditFields")]
pub struct ShapeEdit {
    /// The number from the new-game answer.
    pub game: usize,
    /// The distinct colored squares and arrows for the current place.
    pub shapes: Shapes,
}

impl TryFrom<ShapeEditFields> for ShapeEdit {
    type Error = &'static str;

    fn try_from(fields: ShapeEditFields) -> Result<Self, Self::Error> {
        let mut shapes = Shapes::default();
        for mark in fields.shapes.squares {
            shapes.add_square(shapes::ColoredSquare {
                color: mark.color.into(),
                square: mark.square.parse().map_err(|_| "invalid square")?,
            });
        }
        for arrow in fields.shapes.arrows {
            let from = arrow.from.parse().map_err(|_| "invalid square")?;
            let to = arrow.to.parse().map_err(|_| "invalid square")?;
            shapes.add_arrow(
                shapes::Arrow::new(arrow.color.into(), from, to)
                    .ok_or("an arrow must join two different squares")?,
            );
        }
        Ok(Self {
            game: fields.game,
            shapes,
        })
    }
}
