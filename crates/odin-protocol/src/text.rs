use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}
/// LSP offsets are UTF-16 code units, never Rust byte offsets.
pub fn offset(text: &str, position: Position) -> Result<usize> {
    let mut line = 0;
    let mut start = 0;
    let bytes = text.as_bytes();
    while line < position.line {
        let Some(next) = bytes[start..]
            .iter()
            .position(|b| *b == b'\r' || *b == b'\n')
        else {
            bail!("Line outside document");
        };
        start += next;
        start += if bytes[start] == b'\r' && bytes.get(start + 1) == Some(&b'\n') {
            2
        } else {
            1
        };
        line += 1;
    }
    let rest = &text[start..];
    let content = rest.split(['\r', '\n']).next().unwrap_or("");
    let mut units = 0;
    for (i, ch) in content.char_indices() {
        if units == position.character {
            return Ok(start + i);
        }
        units += ch.len_utf16() as u32;
        if units > position.character {
            bail!("Position splits a UTF-16 surrogate pair");
        }
    }
    if units == position.character {
        Ok(start + content.len())
    } else {
        bail!("Column outside document")
    }
}
/// Position immediately after the final character, with LSP line-ending rules.
pub fn end_position(text: &str) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    Position { line, character }
}
pub fn edit(text: &str, edits: &[TextEdit]) -> Result<String> {
    let mut ranges = Vec::new();
    for item in edits {
        let start = offset(text, item.range.start)?;
        let end = offset(text, item.range.end)?;
        if start > end {
            bail!("Reversed range");
        }
        ranges.push((start, end, &item.new_text));
    }
    ranges.sort_by_key(|(start, end, _)| (*start, *end));
    for pair in ranges.windows(2) {
        let same_insert = pair[0].0 == pair[0].1 && pair[1].0 == pair[1].1;
        if pair[0].1 > pair[1].0 || (pair[0].0 == pair[1].0 && !same_insert) {
            bail!("Overlapping or ambiguous edits");
        }
    }
    let mut result = text.to_string();
    for (start, end, new_text) in ranges.into_iter().rev() {
        result.replace_range(start..end, new_text);
    }
    Ok(result)
}
impl Range {
    pub fn contains(&self, p: Position) -> bool {
        self.start <= p && p < self.end
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ascii_offsets() {
        assert_eq!(
            offset(
                "abc\ndef",
                Position {
                    line: 1,
                    character: 2
                }
            )
            .unwrap(),
            6
        );
    }
    #[test]
    fn unicode_utf16() {
        assert_eq!(
            offset(
                "a😀中",
                Position {
                    line: 0,
                    character: 3
                }
            )
            .unwrap(),
            5
        );
        assert!(
            offset(
                "a😀中",
                Position {
                    line: 0,
                    character: 2
                }
            )
            .is_err()
        );
    }
    #[test]
    fn crlf() {
        assert_eq!(
            offset(
                "ab\r\ncd",
                Position {
                    line: 1,
                    character: 1
                }
            )
            .unwrap(),
            5
        );
        assert!(
            offset(
                "ab\r\n",
                Position {
                    line: 0,
                    character: 3
                }
            )
            .is_err()
        );
    }
    #[test]
    fn trailing_empty_line() {
        assert_eq!(
            offset(
                "a\n",
                Position {
                    line: 1,
                    character: 0
                }
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn invalid_line() {
        assert!(
            offset(
                "abc",
                Position {
                    line: 1,
                    character: 0
                }
            )
            .is_err()
        );
    }
    #[test]
    fn disjoint_edits() {
        let t = |a, b, s: &str| TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: a,
                },
                end: Position {
                    line: 0,
                    character: b,
                },
            },
            new_text: s.into(),
        };
        assert_eq!(
            edit("abcde", &[t(0, 1, "X"), t(4, 5, "Y")]).unwrap(),
            "XbcdY"
        );
    }
    #[test]
    fn overlapping_edits_fail() {
        let t = |a, b| TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: a,
                },
                end: Position {
                    line: 0,
                    character: b,
                },
            },
            new_text: "x".into(),
        };
        assert!(edit("abcde", &[t(0, 3), t(2, 4)]).is_err());
    }
}
