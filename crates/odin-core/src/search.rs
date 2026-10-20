use crate::{Fault, Result, paths::SafeRoot, sha256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileList {
    pub files: Vec<String>,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub path: String,
    pub line: usize,
    pub text: String,
    pub sha256: String,
}

pub fn files(root: &SafeRoot, max: usize) -> Result<FileList> {
    let mut builder = ignore::WalkBuilder::new(root.root());
    builder
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false);
    builder.filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        entry.depth() == 0
            || ![
                ".git",
                ".odin",
                "target",
                "node_modules",
                ".venv",
                "__pycache__",
            ]
            .contains(&name.as_ref())
    });
    let mut found = Vec::new();
    let mut truncated = false;
    for entry in builder.build() {
        let entry = entry.map_err(|e| Fault::new("WALK_ERROR", e.to_string()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(relative) = root.relative(entry.path()) else {
            continue;
        };
        if found.len() == max {
            truncated = true;
            break;
        }
        found.push(relative);
    }
    found.sort();
    Ok(FileList {
        files: found,
        truncated,
    })
}
pub fn pattern(
    root: &SafeRoot,
    expression: &str,
    max_files: usize,
    max_matches: usize,
) -> Result<serde_json::Value> {
    if expression.len() > 4096 {
        return Err(Fault::invalid("Regex is too long"));
    }
    let regex = regex::RegexBuilder::new(expression)
        .size_limit(2 * 1024 * 1024)
        .build()?;
    let listing = files(root, max_files)?;
    let mut matches = Vec::new();
    let mut skipped = 0;
    let mut truncated = listing.truncated;
    'files: for path in listing.files {
        let content = match root.read(&path) {
            Ok(s) => s,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let hash = sha256(content.as_bytes());
        for (line, text) in content.lines().enumerate() {
            if regex.is_match(text) {
                if matches.len() == max_matches {
                    truncated = true;
                    break 'files;
                }
                matches.push(Match {
                    path: path.clone(),
                    line: line + 1,
                    text: text.chars().take(1000).collect(),
                    sha256: hash.clone(),
                });
            }
        }
    }
    Ok(
        serde_json::json!({"matches":matches,"truncated":truncated,"skipped_nontext_or_large_files":skipped,"line_base":1}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_returns_live_hash_and_line() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.rs"), "one\ntwo\n").unwrap();
        let r = SafeRoot::new(d.path(), &[], 1000).unwrap();
        let value = pattern(&r, "two", 10, 10).unwrap();
        assert_eq!(value["matches"][0]["line"], 2);
        assert_eq!(value["matches"][0]["sha256"], sha256(b"one\ntwo\n"));
    }
    #[test]
    fn match_limit_is_explicit() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), "a\na\n").unwrap();
        let r = SafeRoot::new(d.path(), &[], 1000).unwrap();
        assert_eq!(pattern(&r, "a", 10, 1).unwrap()["truncated"], true);
    }
}
