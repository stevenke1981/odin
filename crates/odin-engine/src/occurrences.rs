//! Source-hash-bound, allocation-bounded text edits, not semantic refactoring.
use crate::{
    Project, boolean, number, optional_string, read_expected, string,
    text_replace::{Hit, Pattern, json_string_bytes},
};
use odin_core::{Fault, Result, sha256, transaction::Change};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn limit() -> Fault {
    Fault::new(
        "LIMIT_EXCEEDED",
        "Occurrence preview or planned file output exceeds the host size limit",
    )
}
fn matches(
    path: &str,
    text: &str,
    args: &Value,
    pattern: &Pattern<'_>,
    limits: (usize, usize),
) -> Result<Vec<(String, Hit)>> {
    let hash = sha256(text.as_bytes());
    pattern.find(text,limits.0,limits.1)?.into_iter().map(|hit|{
        let identity=json!({"path":path,"source_sha256":hash,"start":hit.start,"end":hit.end,
            "needle":string(args,"needle")?,"regex":boolean(args,"regex",false),"case_sensitive":boolean(args,"case_sensitive",true),
            "multiline":boolean(args,"multiline",false),"dot_matches_new_line":boolean(args,"dot_matches_new_line",false)});
        Ok((sha256(&serde_json::to_vec(&identity)?),hit))
    }).collect()
}

pub fn prepare(
    project: &Project,
    args: &Value,
    preview: bool,
    max_matches: usize,
    max_preview: usize,
) -> Result<Value> {
    let files = args["files"]
        .as_array()
        .ok_or_else(|| Fault::invalid("files must be an array"))?;
    if files.is_empty() {
        return Err(Fault::invalid("At least one file is required"));
    }
    let mut selected_set = None;
    if let Some(ids) = args
        .get("selected_occurrence_ids")
        .and_then(Value::as_array)
    {
        if ids.is_empty() {
            return Err(Fault::invalid("Empty occurrence selection"));
        }
        let mut selection = BTreeSet::new();
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| Fault::invalid("Occurrence IDs must be strings"))?;
            if !selection.insert(id) {
                return Err(Fault::invalid("Duplicate occurrence ID"));
            }
        }
        selected_set = Some(selection);
    }
    let max_file = project.root.max_file_bytes();
    let pattern = Pattern::new(
        string(args, "needle")?,
        string(args, "replacement")?,
        boolean(args, "regex", false),
        [
            boolean(args, "case_sensitive", true),
            boolean(args, "multiline", false),
            boolean(args, "dot_matches_new_line", false),
        ],
        max_file,
    )?;
    // Reserve envelope/trace space and conservatively charge per-occurrence JSON
    // metadata, escaped source text and capture pieces before materializing them.
    let mut preview_left = max_preview.min(16 * 1024 * 1024).saturating_sub(512);
    let mut planned_bytes = 0usize;
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
        let matches = matches(
            path,
            &before,
            args,
            &pattern,
            (max_matches.saturating_sub(count), max_file),
        )?;
        count += matches.len();
        let mut hits = Vec::new();
        let mut chosen = Vec::new();
        let mut ids = Vec::new();
        for (id, hit) in matches {
            let selected = selected_set
                .as_ref()
                .is_none_or(|ids| ids.contains(id.as_str()));
            if selected {
                chosen_count += 1;
            }
            let cost = 512usize
                .checked_add(json_string_bytes(path, preview_left)?)
                .and_then(|n| {
                    n.checked_add(
                        json_string_bytes(&before[hit.start..hit.end], preview_left).ok()?,
                    )
                })
                .and_then(|n| {
                    n.checked_add(pattern.preview_bytes(&before, &hit, preview_left).ok()?)
                })
                .ok_or_else(limit)?;
            preview_left = preview_left.checked_sub(cost).ok_or_else(limit)?;
            found_ids.insert(id.clone());
            ids.push(id);
            hits.push(hit);
            chosen.push(selected);
        }
        let output_length = Pattern::output_length(&before, &hits, &chosen, max_file)?;
        if !preview && chosen.iter().any(|chosen| *chosen) {
            planned_bytes = planned_bytes
                .checked_add(before.len())
                .and_then(|n| n.checked_add(output_length))
                .filter(|n| *n <= 16 * 1024 * 1024)
                .ok_or_else(limit)?;
            let after = pattern.render(&before, &hits, &chosen, max_file)?;
            changes.push(Change {
                path: path.into(),
                expected_sha256: Some(sha256(before.as_bytes())),
                content: Some(after),
            });
        }
        let hash = sha256(before.as_bytes());
        let (mut cursor, mut line, mut column) = (0, 1, 0);
        for ((id, hit), selected) in ids.into_iter().zip(hits).zip(chosen) {
            let segment = &before[cursor..hit.start];
            line += segment.bytes().filter(|byte| *byte == b'\n').count();
            column = if let Some(last) = segment.rfind('\n') {
                segment[last + 1..].encode_utf16().count()
            } else {
                column + segment.encode_utf16().count()
            };
            cursor = hit.start;
            let after = pattern.expansion(&before, &hit, max_file)?;
            occurrences.push(json!({"occurrence_id":id,"relative_path":path,"sha256":hash,"start_byte":hit.start,"end_byte":hit.end,
                "line":line,"utf16_column":column,"line_base":1,"column_base":0,"before":&before[hit.start..hit.end],"after":after,"selected":selected}));
        }
    }
    if selected_set
        .as_ref()
        .is_some_and(|ids| ids.iter().any(|id| !found_ids.contains(*id)))
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
        let pattern = Pattern::new(
            string(&args, "needle").unwrap(),
            string(&args, "replacement").unwrap(),
            true,
            [true, false, false],
            1024,
        )
        .unwrap();
        let found = matches("x", "foo1\r\nfoo2\r\n", &args, &pattern, (10, 1024)).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(
            pattern
                .expansion("foo1\r\nfoo2\r\n", &found[1].1, 1024)
                .unwrap(),
            "bar2\r"
        );
    }
    #[test]
    fn empty_regex_matches_are_rejected() {
        let args = json!({"needle":"a*","replacement":"x","regex":true});
        let pattern = Pattern::new("a*", "x", true, [true, false, false], 1024).unwrap();
        assert!(matches("x", "abc", &args, &pattern, (10, 1024)).is_err());
    }
    #[test]
    fn occurrence_ids_change_with_content_or_flags() {
        let a = json!({"needle":"a","replacement":"b"});
        let pattern = Pattern::new("a", "b", false, [true, false, false], 1024).unwrap();
        assert_ne!(
            matches("x", "a", &a, &pattern, (10, 1024)).unwrap()[0].0,
            matches("x", "a!", &a, &pattern, (10, 1024)).unwrap()[0].0
        );
        let mut b = a.clone();
        b["case_sensitive"] = json!(false);
        assert_ne!(
            matches("x", "a", &a, &pattern, (10, 1024)).unwrap()[0].0,
            matches("x", "a", &b, &pattern, (10, 1024)).unwrap()[0].0
        );
    }
}
