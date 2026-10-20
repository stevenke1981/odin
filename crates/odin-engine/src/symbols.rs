use crate::{boolean, number, optional_string, string};
use odin_core::{Fault, Result};
use odin_protocol::lsp::Symbol;
use serde_json::Value;
use std::collections::BTreeSet;

pub struct Filter {
    pattern: regex::Regex,
    kinds: BTreeSet<u64>,
    excluded: BTreeSet<u64>,
    scope: Option<String>,
    depth: u64,
    pub offset: usize,
    pub limit: usize,
}
impl Filter {
    pub fn new(args: &Value, maximum: usize) -> Result<Self> {
        let query = string(args, "query")?;
        if query.len() > 4096 {
            return Err(Fault::invalid("Symbol query exceeds 4096 bytes"));
        }
        let pattern = match optional_string(args, "match_mode").unwrap_or("substring") {
            "substring" => regex::escape(query),
            "exact" => format!("^{}$", regex::escape(query)),
            "regex" => query.to_owned(),
            "glob" => {
                let mut result = String::from("^");
                for ch in query.chars() {
                    match ch {
                        '*' => result.push_str(".*"),
                        '?' => result.push('.'),
                        c => result.push_str(&regex::escape(&c.to_string())),
                    }
                }
                result.push('$');
                result
            }
            _ => return Err(Fault::invalid("Unknown symbol match mode")),
        };
        let kinds = |key: &str| -> BTreeSet<u64> {
            args[key]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default()
        };
        Ok(Self {
            pattern: regex::RegexBuilder::new(&pattern)
                .case_insensitive(!boolean(args, "case_sensitive", true))
                .size_limit(2 * 1024 * 1024)
                .build()?,
            kinds: kinds("include_kinds"),
            excluded: kinds("exclude_kinds"),
            scope: optional_string(args, "scope").map(str::to_owned),
            depth: number(args, "depth", u64::MAX),
            offset: number(args, "offset", 0) as usize,
            limit: (number(args, "limit", maximum as u64) as usize).min(maximum),
        })
    }
    pub fn accepts(&self, symbol: &Symbol) -> bool {
        self.pattern.is_match(&symbol.name_path)
            && (self.kinds.is_empty() || self.kinds.contains(&symbol.kind))
            && !self.excluded.contains(&symbol.kind)
            && symbol.name_path.matches('/').count() as u64 <= self.depth
            && self.scope.as_ref().is_none_or(|scope| {
                symbol.name_path == *scope || symbol.name_path.starts_with(&format!("{scope}/"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_protocol::text::{Position, Range};
    use serde_json::json;
    fn symbol(name: &str, kind: u64) -> Symbol {
        let pos = Position {
            line: 0,
            character: 0,
        };
        let range = Range {
            start: pos,
            end: pos,
        };
        Symbol {
            name: name.into(),
            name_path: name.into(),
            kind,
            relative_path: "x.rs".into(),
            range,
            selection_range: range,
        }
    }
    #[test]
    fn filters_kind_depth_scope_and_name() {
        let filter=Filter::new(&json!({"query":"module/*","match_mode":"glob","depth":1,"include_kinds":[12],"exclude_kinds":[13],"scope":"module"}),20).unwrap();
        assert!(filter.accepts(&symbol("module/run", 12)));
        assert!(!filter.accepts(&symbol("module/type/run", 12)));
        assert!(!filter.accepts(&symbol("module/run", 13)));
        assert!(!filter.accepts(&symbol("else/run", 12)));
    }
    #[test]
    fn exact_and_case_insensitive_matching() {
        let filter = Filter::new(
            &json!({"query":"RUN","match_mode":"exact","case_sensitive":false}),
            20,
        )
        .unwrap();
        assert!(filter.accepts(&symbol("run", 12)));
        assert!(!filter.accepts(&symbol("module/run", 12)));
    }
}
