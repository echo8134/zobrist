//! Recorded evaluations and clocks in PGN move comments.

use crate::shapes::Shapes;

/// The evaluation and clock collected from the comments after one move.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MoveReadings {
    /// The first valid evaluation from White's perspective, without the search depth.
    pub evaluation: Option<String>,
    /// The first valid remaining time on the mover's clock.
    pub clock: Option<String>,
}

impl MoveReadings {
    /// Collects each first valid reading and returns the text to show in the notation.
    /// Unknown and malformed commands stay in the text.
    #[must_use]
    pub fn read_comment(&mut self, comment: &str) -> String {
        let mut shapes = Shapes::default();
        without_commands(comment, |command| {
            match command.split_once(' ') {
                Some(("eval", value)) => {
                    self.evaluation
                        .get_or_insert(read_evaluation(value.trim())?);
                }
                Some(("clk", value)) => {
                    let time = read_clock(value.trim())?;
                    self.clock.get_or_insert_with(|| time.to_owned());
                }
                _ => return shapes.read_command(command),
            }
            Some(())
        })
    }
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn whole_part(decimal: &str) -> Option<&str> {
    match decimal.split_once('.') {
        Some((whole, fraction)) => digits(fraction).then_some(whole),
        None => Some(decimal),
    }
}

fn read_evaluation(value: &str) -> Option<String> {
    let score = match value.split_once(',') {
        Some((score, depth)) if digits(depth) => score,
        Some(_) => return None,
        None => value,
    };
    let signed = score.strip_prefix('#').unwrap_or(score);
    let magnitude = signed.strip_prefix(['+', '-']).unwrap_or(signed);
    if score.starts_with('#') {
        return digits(magnitude).then(|| score.to_owned());
    }
    digits(whole_part(magnitude)?).then(|| {
        if score.starts_with(['+', '-']) {
            score.to_owned()
        } else {
            format!("+{score}")
        }
    })
}

fn read_clock(time: &str) -> Option<&str> {
    let (hours, rest) = time.split_once(':')?;
    let (minutes, seconds) = rest.split_once(':')?;
    let under_sixty = |field: &str| matches!(field.as_bytes(), [b'0'..=b'5', b'0'..=b'9']);
    (digits(hours) && under_sixty(minutes) && under_sixty(whole_part(seconds)?)).then_some(time)
}

pub(crate) fn without_commands(
    comment: &str,
    mut read_command: impl FnMut(&str) -> Option<()>,
) -> String {
    let mut parts = Vec::new();
    let mut text_start = 0;
    for (start, prefix) in comment.match_indices("[%") {
        let body_start = start + prefix.len();
        let body = &comment[body_start..];
        let Some((length, "]")) = body.match_indices(['[', ']']).next() else {
            continue;
        };
        if read_command(&body[..length]).is_some() {
            parts.push(comment[text_start..start].trim());
            text_start = body_start + length + 1;
        }
    }
    parts.push(comment[text_start..].trim());
    parts.retain(|part| !part.is_empty());
    parts.join(" ")
}
