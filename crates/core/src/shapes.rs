//! Arrows and colored squares in PGN comments.

use shakmaty::Square;

/// A color used to mark a square or draw an arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    /// Green, written `G`.
    Green = b'G',
    /// Red, written `R`.
    Red = b'R',
    /// Yellow, written `Y`.
    Yellow = b'Y',
    /// Blue, written `B`.
    Blue = b'B',
}

impl Color {
    fn from_letter(letter: u8) -> Option<Self> {
        match letter {
            b'G' => Some(Self::Green),
            b'R' => Some(Self::Red),
            b'Y' => Some(Self::Yellow),
            b'B' => Some(Self::Blue),
            _ => None,
        }
    }

    fn letter(self) -> char {
        char::from(self as u8)
    }
}

/// A square marked with a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColoredSquare {
    /// The color of the mark.
    pub color: Color,
    /// The marked square.
    pub square: Square,
}

/// A colored arrow between two distinct squares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrow {
    /// The color of the arrow.
    pub color: Color,
    start: Square,
    end: Square,
}

impl Arrow {
    /// Creates an arrow if its endpoints differ.
    #[must_use]
    pub fn new(color: Color, start: Square, end: Square) -> Option<Self> {
        if start == end {
            None
        } else {
            Some(Self { color, start, end })
        }
    }

    /// The square at the tail of the arrow.
    #[must_use]
    pub fn start(&self) -> Square {
        self.start
    }

    /// The square at the head of the arrow.
    #[must_use]
    pub fn end(&self) -> Square {
        self.end
    }
}

/// The distinct colored squares and arrows of one comment, in order of first appearance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shapes {
    squares: Vec<ColoredSquare>,
    arrows: Vec<Arrow>,
}

impl Shapes {
    /// The colored squares in insertion order.
    #[must_use]
    pub fn squares(&self) -> &[ColoredSquare] {
        &self.squares
    }

    /// The arrows in insertion order.
    #[must_use]
    pub fn arrows(&self) -> &[Arrow] {
        &self.arrows
    }

    /// Adds a colored square on its first occurrence.
    pub fn add_square(&mut self, square: ColoredSquare) {
        if !self.squares.contains(&square) {
            self.squares.push(square);
        }
    }

    /// Adds an arrow on its first occurrence.
    pub fn add_arrow(&mut self, arrow: Arrow) {
        if !self.arrows.contains(&arrow) {
            self.arrows.push(arrow);
        }
    }

    fn read_command(&mut self, command: &str) -> Option<()> {
        let (name, entries) = command.split_once(' ')?;
        match name {
            "csl" => {
                for square in parse_entries(entries, |color, coordinates| {
                    Some(ColoredSquare {
                        color,
                        square: Square::from_ascii(coordinates).ok()?,
                    })
                })? {
                    self.add_square(square);
                }
            }
            "cal" => {
                for arrow in parse_entries(entries, |color, coordinates| {
                    let (start, end) = coordinates.split_at_checked(2)?;
                    Arrow::new(
                        color,
                        Square::from_ascii(start).ok()?,
                        Square::from_ascii(end).ok()?,
                    )
                })? {
                    self.add_arrow(arrow);
                }
            }
            _ => return None,
        }
        Some(())
    }
}

fn parse_entries<T>(entries: &str, parse: impl Fn(Color, &[u8]) -> Option<T>) -> Option<Vec<T>> {
    entries
        .split(',')
        .map(|entry| {
            let (&letter, coordinates) = entry.as_bytes().split_first()?;
            parse(Color::from_letter(letter)?, coordinates)
        })
        .collect()
}

/// Extracts well-formed shape commands and returns the remaining text and the shapes.
#[must_use]
pub fn split(comment: &str) -> (String, Shapes) {
    let mut shapes = Shapes::default();
    let mut parts = Vec::new();
    let mut text_start = 0;
    for (start, prefix) in comment.match_indices("[%") {
        let body_start = start + prefix.len();
        let body = &comment[body_start..];
        let Some((length, "]")) = body.match_indices(['[', ']']).next() else {
            continue;
        };
        if shapes.read_command(&body[..length]).is_some() {
            parts.push(comment[text_start..start].trim());
            text_start = body_start + length + 1;
        }
    }
    parts.push(comment[text_start..].trim());
    parts.retain(|part| !part.is_empty());
    (parts.join(" "), shapes)
}

/// Appends the squares and then the arrows to text that is already free of shape commands.
#[must_use]
pub fn join(text: &str, shapes: &Shapes) -> String {
    let mut comment = text.to_owned();
    append_command(
        &mut comment,
        "[%csl ",
        shapes.squares(),
        |output, square| {
            output.push(square.color.letter());
            square.square.append_to_string(output);
        },
    );
    append_command(&mut comment, "[%cal ", shapes.arrows(), |output, arrow| {
        output.push(arrow.color.letter());
        arrow.start.append_to_string(output);
        arrow.end.append_to_string(output);
    });
    comment
}

fn append_command<T>(
    comment: &mut String,
    prefix: &str,
    entries: &[T],
    append: impl Fn(&mut String, &T),
) {
    let Some((first, remaining)) = entries.split_first() else {
        return;
    };
    if !comment.is_empty() {
        comment.push(' ');
    }
    comment.push_str(prefix);
    append(comment, first);
    for entry in remaining {
        comment.push(',');
        append(comment, entry);
    }
    comment.push(']');
}
