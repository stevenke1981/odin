//! Plan bounded textual replacements before constructing any expanded output.
use odin_core::{Fault, Result};
use regex::{Captures, Regex};

enum Part<'a> {
    Literal(&'a str),
    Capture(&'a str),
}
pub struct Pattern<'a> {
    expression: Regex,
    parts: Vec<Part<'a>>,
}
pub struct Hit {
    pub start: usize,
    pub end: usize,
    pub replacement_bytes: usize,
}
fn limit() -> Fault {
    Fault::new(
        "LIMIT_EXCEEDED",
        "Replacement exceeds match, template, file or preview size limit",
    )
}
fn add(total: &mut usize, n: usize, maximum: usize) -> Result<()> {
    *total = total
        .checked_add(n)
        .filter(|total| *total <= maximum)
        .ok_or_else(limit)?;
    Ok(())
}
pub fn json_string_bytes(text: &str, maximum: usize) -> Result<usize> {
    let mut total = 0;
    for byte in text.bytes() {
        add(
            &mut total,
            match byte {
                b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
                0..=0x1f => 6,
                _ => 1,
            },
            maximum,
        )?;
    }
    Ok(total)
}
impl<'a> Pattern<'a> {
    pub fn new(
        needle: &str,
        replacement: &'a str,
        regex: bool,
        flags: [bool; 3],
        maximum: usize,
    ) -> Result<Self> {
        if needle.is_empty() || needle.len() > 4096 {
            return Err(Fault::invalid("Empty or oversized pattern"));
        }
        if replacement.len() > maximum {
            return Err(limit());
        }
        let expression = regex::RegexBuilder::new(&if regex {
            needle.into()
        } else {
            regex::escape(needle)
        })
        .case_insensitive(!flags[0])
        .multi_line(flags[1])
        .dot_matches_new_line(flags[2])
        .size_limit(2 * 1024 * 1024)
        .build()?;
        let mut parts = Vec::new();
        if !regex {
            parts.push(Part::Literal(replacement));
        } else {
            let mut rest = replacement;
            while let Some(index) = rest.find('$') {
                if parts.len() > 65530 {
                    return Err(limit());
                }
                if index > 0 {
                    parts.push(Part::Literal(&rest[..index]));
                }
                rest = &rest[index..];
                if rest.starts_with("$$") {
                    parts.push(Part::Literal(&rest[..1]));
                    rest = &rest[2..];
                    continue;
                }
                let reference = if let Some(braced) = rest.strip_prefix("${") {
                    braced.find('}').map(|end| (&braced[..end], end + 3))
                } else {
                    let end = rest
                        .as_bytes()
                        .iter()
                        .skip(1)
                        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
                        .count()
                        + 1;
                    (end > 1).then(|| (&rest[1..end], end))
                };
                if let Some((name, end)) = reference {
                    parts.push(Part::Capture(name));
                    rest = &rest[end..];
                } else {
                    parts.push(Part::Literal(&rest[..1]));
                    rest = &rest[1..];
                }
            }
            if !rest.is_empty() {
                parts.push(Part::Literal(rest));
            }
        }
        Ok(Self { expression, parts })
    }
    fn pieces<'b>(&'b self, captures: &'b Captures<'b>) -> impl Iterator<Item = &'b str> {
        self.parts.iter().map(move |part| match part {
            Part::Literal(text) => *text,
            Part::Capture(name) => name
                .parse::<usize>()
                .ok()
                .and_then(|index| captures.get(index))
                .or_else(|| {
                    if name.parse::<usize>().is_ok() {
                        None
                    } else {
                        captures.name(name)
                    }
                })
                .map_or("", |found| found.as_str()),
        })
    }
    fn length(&self, captures: &Captures<'_>, maximum: usize, json: bool) -> Result<usize> {
        let mut total = 0;
        for part in self.pieces(captures) {
            add(
                &mut total,
                if json {
                    json_string_bytes(part, maximum)?
                } else {
                    part.len()
                },
                maximum,
            )?;
        }
        Ok(total)
    }
    pub fn find(&self, text: &str, max_matches: usize, max_file: usize) -> Result<Vec<Hit>> {
        let mut hits = Vec::new();
        for captures in self.expression.captures_iter(text) {
            if hits.len() >= max_matches {
                return Err(limit());
            }
            let found = captures
                .get(0)
                .ok_or_else(|| Fault::invalid("Missing full match"))?;
            if found.is_empty() {
                return Err(Fault::invalid("Zero-width matches cannot be edited"));
            }
            let replacement_bytes = self.length(&captures, max_file, false)?;
            hits.push(Hit {
                start: found.start(),
                end: found.end(),
                replacement_bytes,
            });
        }
        Ok(hits)
    }
    pub fn preview_bytes(&self, text: &str, hit: &Hit, maximum: usize) -> Result<usize> {
        let captures = self
            .expression
            .captures_at(text, hit.start)
            .ok_or_else(|| Fault::invalid("Missing planned match"))?;
        self.length(&captures, maximum, true)
    }
    pub fn expansion(&self, text: &str, hit: &Hit, maximum: usize) -> Result<String> {
        if hit.replacement_bytes > maximum {
            return Err(limit());
        }
        let captures = self
            .expression
            .captures_at(text, hit.start)
            .ok_or_else(|| Fault::invalid("Missing planned match"))?;
        let mut output = String::with_capacity(hit.replacement_bytes);
        for part in self.pieces(&captures) {
            let mut next = output.len();
            add(&mut next, part.len(), maximum)?;
            output.push_str(part);
        }
        Ok(output)
    }
    pub fn output_length(
        text: &str,
        hits: &[Hit],
        selected: &[bool],
        maximum: usize,
    ) -> Result<usize> {
        let mut length = text.len();
        for (hit, selected) in hits.iter().zip(selected) {
            if *selected {
                length = length
                    .checked_sub(hit.end - hit.start)
                    .and_then(|n| n.checked_add(hit.replacement_bytes))
                    .ok_or_else(limit)?;
            }
        }
        if length > maximum {
            return Err(limit());
        }
        Ok(length)
    }
    pub fn render(
        &self,
        text: &str,
        hits: &[Hit],
        selected: &[bool],
        maximum: usize,
    ) -> Result<String> {
        let length = Self::output_length(text, hits, selected, maximum)?;
        let mut output = String::with_capacity(length);
        let mut cursor = 0;
        for ((captures, hit), selected) in
            self.expression.captures_iter(text).zip(hits).zip(selected)
        {
            if !*selected {
                continue;
            }
            output.push_str(&text[cursor..hit.start]);
            for part in self.pieces(&captures) {
                let mut next = output.len();
                add(&mut next, part.len(), maximum)?;
                output.push_str(part);
            }
            cursor = hit.end;
        }
        output.push_str(&text[cursor..]);
        if output.len() != length {
            return Err(Fault::invalid("Replacement plan changed"));
        }
        Ok(output)
    }
}

pub fn replace(
    text: &str,
    needle: &str,
    replacement: &str,
    regex: bool,
    limits: (usize, usize, usize),
) -> Result<(String, usize)> {
    let (max_file, max_matches, expected) = limits;
    let pattern = Pattern::new(needle, replacement, regex, [true, false, false], max_file)?;
    let hits = pattern.find(text, max_matches, max_file)?;
    if hits.len() != expected {
        return Err(Fault::conflict(format!(
            "Expected {expected} matches, found {}",
            hits.len()
        )));
    }
    let selected = vec![true; hits.len()];
    let output = pattern.render(text, &hits, &selected, max_file)?;
    Ok((output, hits.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn length_first_rejects_many_literal_and_capture_expansions() {
        for (regex, needle, replacement) in
            [(false, "a", "12345678"), (true, "(a)", "$1$1$1$1$1$1$1$1")]
        {
            assert_eq!(
                replace(
                    "a".repeat(100).as_str(),
                    needle,
                    replacement,
                    regex,
                    (128, 200, 100)
                )
                .unwrap_err()
                .code,
                "LIMIT_EXCEEDED"
            );
        }
        assert_eq!(
            replace("aaaa", "a", "x", false, (128, 2, 4))
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            replace("aa", "a", "long", false, (128, 10, 1))
                .unwrap_err()
                .code,
            "CONFLICT"
        );
    }
    #[test]
    fn bounded_template_matches_upstream_capture_interpolation() {
        let re = Regex::new(r"(?P<word>foo)([0-9]+)?").unwrap();
        for template in [
            "$word:$2",
            "${word}_$0",
            "$$$word",
            "$99-$missing",
            "${}",
            "${word",
            "$word_suffix",
            "$0001",
            "${99999999999999999999999999999}",
            "繁體${word}\\\n",
        ] {
            let pattern = Pattern::new(
                r"(?P<word>foo)([0-9]+)?",
                template,
                true,
                [true, false, false],
                1024,
            )
            .unwrap();
            let captures = re.captures("foo12").unwrap();
            let mut upstream = String::new();
            captures.expand(template, &mut upstream);
            let hit = pattern.find("foo12", 10, 1024).unwrap().remove(0);
            assert_eq!(
                pattern.expansion("foo12", &hit, 1024).unwrap(),
                upstream,
                "{template}"
            );
        }
    }
}
