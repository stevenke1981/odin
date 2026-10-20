//! Source-hash-bound occurrence selection for explicit text edits, not semantic refactoring.
use crate::{Project, boolean, number, optional_string, read_expected, string};
use odin_core::{Fault, Result, sha256, transaction::Change};
use serde_json::{Value, json};
use std::collections::BTreeSet;

struct Occurrence {
    id: String,
    start: usize,
    end: usize,
    replacement: String,
}

fn matches(path: &str, text: &str, args: &Value) -> Result<Vec<Occurrence>> {
    let needle = string(args, "needle")?;
    if needle.is_empty() || needle.len() > 4096 {
        return Err(Fault::invalid("Empty or oversized pattern"));
    }
    let regex = boolean(args, "regex", false);
    let pattern = if regex {
        needle.to_owned()
    } else {
        regex::escape(needle)
    };
    let expression = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!boolean(args, "case_sensitive", true))
        .multi_line(boolean(args, "multiline", false))
        .dot_matches_new_line(boolean(args, "dot_matches_new_line", false))
        .size_limit(2 * 1024 * 1024)
        .build()?;
    let hash = sha256(text.as_bytes());
    let replacement = string(args, "replacement")?;
    let mut output = Vec::new();
    for captures in expression.captures_iter(text) {
        let found = captures
            .get(0)
            .ok_or_else(|| Fault::invalid("Missing full match"))?;
        if found.is_empty() {
            return Err(Fault::invalid("Zero-width matches cannot be edited"));
        }
        if output.len() >= 10_000 {
            return Err(Fault::new("LIMIT_EXCEEDED", "More than 10000 occurrences"));
        }
        let identity = json!({"path":path,"source_sha256":hash,"start":found.start(),"end":found.end(),
            "needle":needle,"regex":regex,"case_sensitive":boolean(args,"case_sensitive",true),
            "multiline":boolean(args,"multiline",false),"dot_matches_new_line":boolean(args,"dot_matches_new_line",false)});
        let mut expanded = String::new();
        if regex {
            captures.expand(replacement, &mut expanded);
        } else {
            expanded.push_str(replacement);
        }
        output.push(Occurrence {
            id: sha256(&serde_json::to_vec(&identity)?),
            start: found.start(),
            end: found.end(),
            replacement: expanded,
        });
    }
    Ok(output)
}

pub fn prepare(
    project: &Project,
    args: &Value,
    preview: bool,
    max_matches: usize,
) -> Result<Value> {
    let files = args["files"]
        .as_array()
        .ok_or_else(|| Fault::invalid("files must be an array"))?;
    if files.is_empty() {
        return Err(Fault::invalid("At least one file is required"));
    }
    let selected = args
        .get("selected_occurrence_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .map(|id| {
                    id.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| Fault::invalid("Occurrence IDs must be strings"))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    if selected.as_ref().is_some_and(Vec::is_empty) {
        return Err(Fault::invalid("Empty occurrence selection"));
    }
    let selected_set = selected
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<BTreeSet<_>>());
    if selected
        .as_ref()
        .zip(selected_set.as_ref())
        .is_some_and(|(ids, set)| ids.len() != set.len())
    {
        return Err(Fault::invalid("Duplicate occurrence ID"));
    }
    let mut seen_files = BTreeSet::new();
    let mut found_ids = BTreeSet::new();
    let mut occurrences = Vec::new();
    let mut changes = Vec::new();
    let mut count = 0;
    let mut chosen_count = 0;
    for file in files {
        let path = string(file, "relative_path")?;
        let normalized = project.root.relative(&project.root.resolve(path, false)?)?;
        #[cfg(windows)]
        let normalized = normalized.to_lowercase();
        if !seen_files.insert(normalized) {
            return Err(Fault::invalid("Duplicate file path"));
        }
        let before = read_expected(project, path, string(file, "expected_sha256")?)?;
        let matches = matches(path, &before, args)?;
        count += matches.len();
        if count > max_matches {
            return Err(Fault::new(
                "LIMIT_EXCEEDED",
                "Occurrence list exceeds max_matches; narrow the query",
            ));
        }
        let mut after = before.clone();
        let mut chosen = Vec::new();
        for item in matches {
            let is_selected = selected_set
                .as_ref()
                .is_none_or(|ids| ids.contains(&item.id));
            found_ids.insert(item.id.clone());
            let prefix = &before[..item.start];
            let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
            occurrences.push(json!({"occurrence_id":item.id,"relative_path":path,"sha256":sha256(before.as_bytes()),
                "start_byte":item.start,"end_byte":item.end,"line":prefix.bytes().filter(|b|*b==b'\n').count()+1,
                "utf16_column":before[line_start..item.start].encode_utf16().count(),"line_base":1,"column_base":0,
                "before":&before[item.start..item.end],"after":item.replacement,"selected":is_selected}));
            if is_selected {
                chosen.push(item);
                chosen_count += 1;
            }
        }
        for item in chosen.iter().rev() {
            after.replace_range(item.start..item.end, &item.replacement);
        }
        if !chosen.is_empty() {
            changes.push(Change {
                path: path.into(),
                expected_sha256: Some(sha256(before.as_bytes())),
                content: Some(after),
            });
        }
    }
    if selected_set
        .as_ref()
        .is_some_and(|ids| !ids.is_subset(&found_ids))
    {
        return Err(Fault::conflict(
            "Occurrence IDs do not match these file hashes and search options",
        ));
    }
    if let Some(expected) = args.get("expected_total_matches")
        && expected.as_u64() != Some(count as u64)
    {
        return Err(Fault::conflict(format!(
            "Expected {} total matches, found {count}",
            number(args, "expected_total_matches", 0)
        )));
    }
    if count == 0 {
        return Err(Fault::conflict("No matching occurrences"));
    }
    if !boolean(args, "allow_multiple", true) && chosen_count > 1 {
        return Err(Fault::conflict(
            "Multiple selected occurrences require allow_multiple",
        ));
    }
    if preview {
        return Ok(
            json!({"occurrences":occurrences,"total_matches":count,"selected_matches":chosen_count,"source_modified":false}),
        );
    }
    let mut result = project
        .transactions
        .prepare(changes, optional_string(args, "request_id"))?;
    result["occurrences"] = json!(occurrences);
    result["total_matches"] = json!(count);
    result["selected_matches"] = json!(chosen_count);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regex_expands_only_selected_capture_and_preserves_crlf() {
        let args = json!({"needle":"(?m)^foo(\\d+)\\r$","replacement":"bar$1\r","regex":true});
        let found = matches("x", "foo1\r\nfoo2\r\n", &args).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].replacement, "bar2\r");
        assert_eq!(&"foo1\r\nfoo2\r\n"[found[1].start..found[1].end], "foo2\r");
    }
    #[test]
    fn empty_regex_matches_are_rejected() {
        assert!(
            matches(
                "x",
                "abc",
                &json!({"needle":"a*","replacement":"x","regex":true})
            )
            .is_err()
        );
    }
    #[test]
    fn occurrence_ids_change_with_content_or_flags() {
        let a = json!({"needle":"a","replacement":"b"});
        assert_ne!(
            matches("x", "a", &a).unwrap()[0].id,
            matches("x", "a!", &a).unwrap()[0].id
        );
        let mut b = a.clone();
        b["case_sensitive"] = json!(false);
        assert_ne!(
            matches("x", "a", &a).unwrap()[0].id,
            matches("x", "a", &b).unwrap()[0].id
        );
    }
}
