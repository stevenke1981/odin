//! Native Clippy/Ruff/ESLint inspection output adapters. No shell text or regex semantics.
use crate::{Engine, Project, number, optional_string, string, text_replace::json_string_bytes};
use odin_core::{Fault, Result, paths::SafeRoot, search, sha256, transaction::Change};
use odin_protocol::text::{Position, Range, TextEdit, offset, position_from_offset};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> Fault {
    Fault::new("INVALID_BACKEND_OUTPUT", message)
}
fn limited(message: &str) -> Fault {
    Fault::new("LIMIT_EXCEEDED", message)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    Clippy,
    Ruff,
    Eslint,
}
impl Provider {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "clippy" => Ok(Self::Clippy),
            "ruff" => Ok(Self::Ruff),
            "eslint" => Ok(Self::Eslint),
            _ => Err(Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "Unknown native inspection provider",
            )),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Clippy => "clippy",
            Self::Ruff => "ruff",
            Self::Eslint => "eslint",
        }
    }
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Clippy => &["rs", "toml"],
            Self::Ruff => &["py", "pyi"],
            Self::Eslint => &["js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts"],
        }
    }
}
struct NativeFix {
    path: String,
    hash: String,
    edits: Vec<TextEdit>,
}
struct Report {
    value: Value,
    fixes: BTreeMap<String, NativeFix>,
}
struct Normalizer<'a> {
    provider: Provider,
    root: &'a SafeRoot,
    snapshots: &'a BTreeMap<String, String>,
    hashes: BTreeMap<String, String>,
    version: &'a str,
    findings: Vec<Value>,
    fixes: BTreeMap<String, NativeFix>,
    maximum: usize,
    remaining: usize,
    fix_remaining: usize,
    complete: bool,
}
impl<'a> Normalizer<'a> {
    fn source(&self, path: &str) -> Result<Option<(String, &'a str)>> {
        let Some(path) = relative(self.root, path)? else {
            return Ok(None);
        };
        Ok(self
            .snapshots
            .get(&path)
            .map(|source| (path, source.as_str())))
    }
    fn push(
        &mut self,
        rule: &str,
        message: &str,
        severity: u64,
        source: Option<(String, &str)>,
        range: Option<Range>,
        edits: Vec<TextEdit>,
    ) -> Result<()> {
        if self.findings.len() >= self.maximum {
            return Err(limited("Inspection finding count exceeds the host limit"));
        }
        if rule.len() > 256 || message.len() > 8192 {
            return Err(limited("Inspection rule/message exceeds the output limit"));
        }
        let path = source.as_ref().map(|(path, _)| path.as_str());
        let hash = source
            .as_ref()
            .and_then(|(path, _)| self.hashes.get(path))
            .cloned();
        let mut cost = 1024usize;
        for text in [message, rule, self.version] {
            cost = cost
                .checked_add(json_string_bytes(text, self.remaining)?)
                .ok_or_else(|| limited("Inspection result size overflow"))?;
        }
        if let Some(path) = path {
            cost = cost
                .checked_add(json_string_bytes(path, self.remaining)?)
                .ok_or_else(|| limited("Inspection result size overflow"))?;
        }
        self.remaining = self
            .remaining
            .checked_sub(cost)
            .ok_or_else(|| limited("Inspection report exceeds the host output budget"))?;
        let identity = json!({"provider":self.provider.name(),"rule":rule,"path":path,"range":range,"sha256":hash,"message":message,"edits":edits});
        let id = sha256(&serde_json::to_vec(&identity)?);
        let fix_available = source.is_some() && range.is_some() && !edits.is_empty();
        if fix_available {
            let (path, source) = source
                .as_ref()
                .ok_or_else(|| invalid("Fix has no source"))?;
            validate_edits(source, &edits, self.root.max_file_bytes())?;
            self.fixes.insert(
                id.clone(),
                NativeFix {
                    path: path.clone(),
                    hash: hash.clone().ok_or_else(|| invalid("Fix has no hash"))?,
                    edits,
                },
            );
        }
        if source.is_none() || range.is_none() {
            self.complete = false;
        }
        self.findings.push(json!({"finding_id":id,"provider":self.provider.name(),"source":self.provider.name(),"rule_id":rule,
            "relative_path":path,"range":range,"severity":severity,"message":message,"version":self.version,"sha256":hash,
            "document_version":null,"freshness":if source.is_some(){"current"}else{"unavailable"},"position_encoding":"utf-16","fix_available":fix_available}));
        Ok(())
    }
    fn ruff(&mut self, items: &[Value]) -> Result<()> {
        for item in items {
            let source = self.source(required(item, "filename")?)?;
            let range = source
                .as_ref()
                .map(|(_, source)| {
                    row_range(source, &item["location"], &item["end_location"], false)
                })
                .transpose()?;
            let mut edits = Vec::new();
            if item["fix"]["applicability"] == "safe"
                && item["cell"].is_null()
                && let (Some((_, source)), Some(raw)) =
                    (source.as_ref(), item["fix"]["edits"].as_array())
            {
                if raw.len() > 256 {
                    return Err(limited("Too many inspection fix edits"));
                }
                for edit in raw {
                    edits.push(TextEdit {
                        range: row_range(source, &edit["location"], &edit["end_location"], false)?,
                        new_text: replacement(
                            edit,
                            "content",
                            self.root.max_file_bytes(),
                            &mut self.fix_remaining,
                        )?,
                    });
                }
            }
            let severity = match item["severity"].as_str() {
                Some("warning") => 2,
                Some("info") | Some("information") => 3,
                _ => 1,
            };
            self.push(
                item["code"]
                    .as_str()
                    .or_else(|| item["name"].as_str())
                    .unwrap_or("ruff/unknown"),
                required(item, "message")?,
                severity,
                source,
                range,
                edits,
            )?;
        }
        Ok(())
    }
    fn eslint(&mut self, files: &[Value]) -> Result<()> {
        for file in files {
            let source = self.source(required(file, "filePath")?)?;
            let messages = file["messages"]
                .as_array()
                .ok_or_else(|| invalid("ESLint messages must be an array"))?;
            for message in messages {
                let range = if message.get("line").is_none() || message.get("column").is_none() {
                    None
                } else {
                    source
                        .as_ref()
                        .map(|(_, source)| eslint_range(source, message))
                        .transpose()?
                };
                let mut edits = Vec::new();
                if let (Some((_, source)), Some(fix)) = (source.as_ref(), message.get("fix")) {
                    let span = fix["range"]
                        .as_array()
                        .filter(|span| span.len() == 2)
                        .ok_or_else(|| invalid("ESLint fix range is invalid"))?;
                    let start = js_index(source, integer_value(&span[0])?)?;
                    let end = js_index(source, integer_value(&span[1])?)?;
                    edits.push(TextEdit {
                        range: byte_range(source, start, end)?,
                        new_text: replacement(
                            fix,
                            "text",
                            self.root.max_file_bytes(),
                            &mut self.fix_remaining,
                        )?,
                    });
                }
                let severity = match message["severity"].as_u64() {
                    Some(2) => 1,
                    Some(1) => 2,
                    _ => return Err(invalid("Invalid ESLint severity")),
                };
                self.push(
                    message["ruleId"].as_str().unwrap_or("eslint/parse"),
                    required(message, "message")?,
                    severity,
                    source.clone(),
                    range,
                    edits,
                )?;
            }
        }
        Ok(())
    }
    fn clippy(&mut self, output: &str) -> Result<bool> {
        let mut finished = None;
        let mut saw_message = false;
        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            let frame: Value = serde_json::from_str(line)
                .map_err(|error| invalid(format!("Invalid Cargo JSON line: {error}")))?;
            if frame["reason"] == "build-finished" {
                finished = frame["success"].as_bool();
                continue;
            }
            let message = if frame["reason"] == "compiler-message" {
                &frame["message"]
            } else if frame["$message_type"] == "diagnostic" {
                &frame
            } else {
                continue;
            };
            saw_message = true;
            let severity = match required(message, "level")? {
                "error" | "error: internal compiler error" => 1,
                "warning" => 2,
                "note" => 3,
                "help" => 4,
                "failure-note" => 1,
                _ => return Err(invalid("Invalid Clippy diagnostic level")),
            };
            let spans = message["spans"]
                .as_array()
                .ok_or_else(|| invalid("Clippy spans must be an array"))?;
            let primaries = spans
                .iter()
                .filter(|span| span["is_primary"] == true)
                .collect::<Vec<_>>();
            if primaries.is_empty() {
                self.push(
                    message["code"]["code"].as_str().unwrap_or("rustc"),
                    required(message, "message")?,
                    severity,
                    None,
                    None,
                    Vec::new(),
                )?;
                continue;
            }
            for primary in primaries {
                let source = self.source(required(primary, "file_name")?)?;
                let range = source
                    .as_ref()
                    .map(|(_, source)| {
                        byte_range(
                            source,
                            integer(primary, "byte_start")?,
                            integer(primary, "byte_end")?,
                        )
                    })
                    .transpose()?;
                let mut edits = Vec::new();
                if let (Some((path, source)), Some(children)) =
                    (source.as_ref(), message["children"].as_array())
                {
                    for child in children {
                        let Some(suggestions) =
                            child["spans"].as_array().filter(|items| !items.is_empty())
                        else {
                            continue;
                        };
                        if suggestions.len() > 256 {
                            return Err(limited("Too many Clippy suggestion edits"));
                        }
                        if !suggestions.iter().all(|span| {
                            span["suggestion_applicability"] == "MachineApplicable"
                                && span["suggested_replacement"].is_string()
                        }) {
                            continue;
                        }
                        let mut group = Vec::new();
                        let mut same_file = true;
                        for span in suggestions {
                            if self
                                .source(required(span, "file_name")?)?
                                .as_ref()
                                .map(|(target, _)| target)
                                != Some(path)
                            {
                                same_file = false;
                                break;
                            }
                            group.push(TextEdit {
                                range: byte_range(
                                    source,
                                    integer(span, "byte_start")?,
                                    integer(span, "byte_end")?,
                                )?,
                                new_text: replacement(
                                    span,
                                    "suggested_replacement",
                                    self.root.max_file_bytes(),
                                    &mut self.fix_remaining,
                                )?,
                            });
                        }
                        if same_file {
                            edits = group;
                            break;
                        }
                    }
                }
                self.push(
                    message["code"]["code"].as_str().unwrap_or("rustc"),
                    required(message, "message")?,
                    severity,
                    source,
                    range,
                    edits,
                )?;
            }
        }
        if finished.is_none() && !saw_message {
            return Err(invalid(
                "Cargo output did not contain a completion or diagnostic record",
            ));
        }
        Ok(finished == Some(true))
    }
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| invalid(format!("Missing analyzer field {key}")))
}
fn integer(value: &Value, key: &str) -> Result<usize> {
    integer_value(&value[key])
}
fn integer_value(value: &Value) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| invalid("Analyzer location must be a nonnegative integer"))
}
fn replacement(value: &Value, key: &str, maximum: usize, remaining: &mut usize) -> Result<String> {
    let text = required(value, key)?;
    if text.len() > maximum {
        return Err(limited("Inspection replacement exceeds file limit"));
    }
    *remaining = remaining
        .checked_sub(
            text.len()
                .checked_add(128)
                .ok_or_else(|| limited("Inspection replacement size overflow"))?,
        )
        .ok_or_else(|| limited("Inspection fixes exceed the aggregate host output budget"))?;
    Ok(text.into())
}
fn byte_range(source: &str, start: usize, end: usize) -> Result<Range> {
    if start > end {
        return Err(invalid("Reversed analyzer range"));
    }
    Ok(Range {
        start: position_from_offset(source, start).map_err(|e| invalid(e.to_string()))?,
        end: position_from_offset(source, end).map_err(|e| invalid(e.to_string()))?,
    })
}
fn analysis_source(source: &str) -> (&str, usize) {
    if let Some(source) = source.strip_prefix('\u{feff}') {
        (source, 3)
    } else {
        (source, 0)
    }
}
fn row_position(source: &str, row: usize, column: usize, utf16: bool) -> Result<Position> {
    if row == 0 || column == 0 {
        return Err(invalid("Analyzer row/column must be 1-based"));
    }
    let (text, bom) = analysis_source(source);
    let line = u32::try_from(row - 1).map_err(|_| invalid("Analyzer row overflow"))?;
    let start =
        offset(text, Position { line, character: 0 }).map_err(|e| invalid(e.to_string()))?;
    let byte = if utf16 {
        offset(
            text,
            Position {
                line,
                character: u32::try_from(column - 1)
                    .map_err(|_| invalid("Analyzer column overflow"))?,
            },
        )
        .map_err(|e| invalid(e.to_string()))?
    } else {
        let content = text[start..].split(['\r', '\n']).next().unwrap_or("");
        let wanted = column - 1;
        if wanted == content.chars().count() {
            start + content.len()
        } else {
            start
                + content
                    .char_indices()
                    .nth(wanted)
                    .map(|(index, _)| index)
                    .ok_or_else(|| invalid("Analyzer column outside source"))?
        }
    };
    position_from_offset(source, byte + bom).map_err(|e| invalid(e.to_string()))
}
fn row_range(source: &str, start: &Value, end: &Value, utf16: bool) -> Result<Range> {
    let start = row_position(
        source,
        integer(start, "row")?,
        integer(start, "column")?,
        utf16,
    )?;
    let end = if end.is_null() {
        start
    } else {
        row_position(source, integer(end, "row")?, integer(end, "column")?, utf16)?
    };
    if start > end {
        return Err(invalid("Reversed analyzer range"));
    }
    Ok(Range { start, end })
}
fn eslint_range(source: &str, item: &Value) -> Result<Range> {
    let start = row_position(
        source,
        integer(item, "line")?,
        integer(item, "column")?,
        true,
    )?;
    let end = if item.get("endLine").is_some() && item.get("endColumn").is_some() {
        row_position(
            source,
            integer(item, "endLine")?,
            integer(item, "endColumn")?,
            true,
        )?
    } else {
        start
    };
    if start > end {
        return Err(invalid("Reversed ESLint range"));
    }
    Ok(Range { start, end })
}
fn js_index(source: &str, index: usize) -> Result<usize> {
    let (text, bom) = analysis_source(source);
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == index {
            return Ok(byte + bom);
        }
        units += ch.len_utf16();
        if units > index {
            return Err(invalid("ESLint fix splits a UTF-16 surrogate pair"));
        }
    }
    if units == index {
        Ok(text.len() + bom)
    } else {
        Err(invalid("ESLint fix outside source"))
    }
}
fn relative(root: &SafeRoot, path: &str) -> Result<Option<String>> {
    let original = PathBuf::from(path);
    let local = if original.is_absolute() {
        let base = url::Url::from_directory_path(root.root())
            .map_err(|_| invalid("Invalid root URI"))?
            .to_file_path()
            .map_err(|_| invalid("Invalid root path"))?;
        let mut components = original.components();
        for expected in base.components() {
            let Some(actual) = components.next() else {
                return Ok(None);
            };
            let same = if cfg!(windows) {
                actual
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
            } else {
                actual == expected
            };
            if !same {
                return Ok(None);
            }
        }
        components
            .collect::<PathBuf>()
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        path.replace('\\', "/")
    };
    let resolved = match root.resolve(&local, false) {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    Ok(Some(root.relative(&resolved)?))
}
fn validate_edits(source: &str, edits: &[TextEdit], maximum: usize) -> Result<()> {
    let mut size = source.len();
    let mut ranges = Vec::new();
    for edit in edits {
        let start = offset(source, edit.range.start).map_err(|e| invalid(e.to_string()))?;
        let end = offset(source, edit.range.end).map_err(|e| invalid(e.to_string()))?;
        if start > end {
            return Err(invalid("Reversed inspection edit"));
        }
        size = size
            .checked_sub(end - start)
            .and_then(|n| n.checked_add(edit.new_text.len()))
            .ok_or_else(|| limited("Inspection edit size overflow"))?;
        ranges.push((start, end));
    }
    ranges.sort_unstable();
    if ranges
        .windows(2)
        .any(|pair| pair[0].1 > pair[1].0 || pair[0].0 == pair[1].0)
    {
        return Err(invalid("Overlapping inspection edits"));
    }
    if size > maximum {
        return Err(limited("Inspection fix exceeds the file size limit"));
    }
    Ok(())
}
fn normalize(
    provider: Provider,
    root: &SafeRoot,
    snapshots: &BTreeMap<String, String>,
    output: &str,
    version: &str,
    limits: (usize, usize),
) -> Result<Report> {
    if output.len() > limits.1.min(MAX_REPORT_BYTES) {
        return Err(limited("Analyzer output exceeds the host output budget"));
    }
    let mut normalizer = Normalizer {
        provider,
        root,
        snapshots,
        hashes: snapshots
            .iter()
            .map(|(path, source)| (path.clone(), sha256(source.as_bytes())))
            .collect(),
        version,
        findings: Vec::new(),
        fixes: BTreeMap::new(),
        maximum: limits.0,
        remaining: limits.1.min(MAX_REPORT_BYTES).saturating_sub(2048),
        fix_remaining: limits.1.min(MAX_REPORT_BYTES),
        complete: true,
    };
    if provider == Provider::Clippy {
        let complete = normalizer.clippy(output)?;
        normalizer.complete &= complete;
    } else {
        let output: Value = serde_json::from_str(output)
            .map_err(|error| invalid(format!("Invalid analyzer JSON: {error}")))?;
        let items = output
            .as_array()
            .ok_or_else(|| invalid("Analyzer output must be a JSON array"))?;
        if provider == Provider::Ruff {
            normalizer.ruff(items)?;
        } else {
            normalizer.eslint(items)?;
        }
    }
    let errors = normalizer
        .findings
        .iter()
        .filter(|finding| finding["severity"] == 1)
        .count();
    let warnings = normalizer
        .findings
        .iter()
        .filter(|finding| finding["severity"] == 2)
        .count();
    Ok(Report {
        value: json!({"provider":provider.name(),"version":version,"findings":normalizer.findings,"position_encoding":"utf-16",
        "summary":{"errors":errors,"warnings":warnings,"total":normalizer.findings.len(),"complete":normalizer.complete},"scope":"host_fixed_profile","whole_project_verified":false}),
        fixes: normalizer.fixes,
    })
}

fn snapshots(
    project: &Project,
    provider: Provider,
    maximum: usize,
) -> Result<BTreeMap<String, String>> {
    let listing = search::files(&project.root, maximum)?;
    if listing.truncated {
        return Err(limited("Inspection snapshot would omit files"));
    }
    let mut bytes = 0usize;
    let mut result = BTreeMap::new();
    for path in listing.files {
        let extension = Path::new(&path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !provider
            .extensions()
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        {
            continue;
        }
        let source = project.root.read(&path)?;
        bytes = bytes
            .checked_add(source.len())
            .filter(|bytes| *bytes <= MAX_SNAPSHOT_BYTES)
            .ok_or_else(|| limited("Inspection source snapshot exceeds 32 MiB"))?;
        result.insert(path, source);
    }
    Ok(result)
}
fn validate_no_writes(args: &[String]) -> Result<()> {
    for argument in args {
        let flag = argument.split('=').next().unwrap_or(argument);
        if [
            "--fix",
            "--fix-only",
            "--fix-dry-run",
            "--unsafe-fixes",
            "--add-noqa",
            "--output-file",
            "--output",
            "-o",
            "--cache",
            "--emit",
            "--explain",
            "--save",
            "--write",
            "--init",
        ]
        .contains(&flag)
            || ["fix", "format", "clean"].contains(&argument.as_str())
            || (argument.starts_with("-o") && !argument.starts_with("--") && argument.len() > 2)
        {
            return Err(Fault::denied(
                "Inspection profiles must not request direct file writes",
            ));
        }
    }
    Ok(())
}
fn path_contains(parent: &Path, child: &Path) -> bool {
    let mut children = child.components();
    parent.components().all(|expected| {
        children.next().is_some_and(|actual| {
            if cfg!(windows) {
                actual
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
            } else {
                actual == expected
            }
        })
    })
}
fn canonical_scope(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(Fault::denied(
            "Inspection sandbox paths must be normalized absolute host paths",
        ));
    }
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    existing
                        .file_name()
                        .ok_or_else(|| Fault::denied("Sandbox path has no existing ancestor"))?
                        .to_os_string(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| Fault::denied("Sandbox path has no parent"))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut result = existing.canonicalize()?;
    for component in suffix.into_iter().rev() {
        result.push(component);
    }
    Ok(result)
}
/// Require transaction-producing subprocesses to have read-only source/state.
pub(crate) fn validate_sandbox(
    engine: &Engine,
    sandbox: &odin_core::sandbox_policy::MxcSandboxConfig,
) -> Result<()> {
    let state = canonical_scope(&engine.config.state_dir)?;
    let roots = engine
        .projects
        .values()
        .map(|project| project.root.root())
        .chain(std::iter::once(state.as_path()))
        .collect::<Vec<_>>();
    for grant in sandbox
        .readwrite_paths
        .iter()
        .chain(std::iter::once(&sandbox.temp_root))
    {
        let grant = canonical_scope(grant)?;
        if roots
            .iter()
            .any(|root| path_contains(root, &grant) || path_contains(&grant, root))
        {
            return Err(Fault::denied(
                "Writable/temp paths must not overlap any configured project or Odin state",
            ));
        }
    }
    Ok(())
}
fn provider_version(provider: Provider, output: &str) -> Option<String> {
    let line = output.lines().find(|line| !line.trim().is_empty())?.trim();
    let valid = match provider {
        Provider::Clippy => line.starts_with("clippy "),
        Provider::Ruff => line.starts_with("ruff "),
        Provider::Eslint => line
            .strip_prefix('v')
            .and_then(|line| line.chars().next())
            .is_some_and(|ch| ch.is_ascii_digit()),
    };
    (valid && !line.chars().any(char::is_control)).then(|| line.chars().take(512).collect())
}
fn validate_profile(provider: Provider, args: &[String], engine: &Engine) -> Result<()> {
    validate_no_writes(args)?;
    if provider == Provider::Ruff
        && !["--no-fix", "--no-cache"]
            .iter()
            .all(|required| args.iter().any(|argument| argument == required))
    {
        return Err(Fault::denied(
            "Ruff inspection profiles require --no-fix and --no-cache",
        ));
    }
    if provider == Provider::Clippy && !args.iter().any(|argument| argument == "--locked") {
        return Err(Fault::denied("Clippy inspection profiles require --locked"));
    }
    if provider == Provider::Eslint {
        if !args
            .iter()
            .any(|argument| argument == "--no-config-lookup" || argument == "--no-eslintrc")
        {
            return Err(Fault::denied(
                "ESLint inspection profiles must disable repository executable configuration lookup",
            ));
        }
        for (index, argument) in args.iter().enumerate() {
            let configured = argument.strip_prefix("--config=").or_else(|| {
                if argument == "--config" || argument == "-c" {
                    args.get(index + 1).map(String::as_str)
                } else {
                    None
                }
            });
            if let Some(configured) = configured {
                let configured = Path::new(configured);
                if !configured.is_absolute() {
                    return Err(Fault::denied(
                        "ESLint explicit config must be an absolute trusted host path",
                    ));
                }
                let configured = configured.canonicalize()?;
                if engine
                    .projects
                    .values()
                    .any(|project| path_contains(project.root.root(), &configured))
                {
                    return Err(Fault::denied(
                        "ESLint executable configuration cannot be loaded from any configured project",
                    ));
                }
            }
        }
    }
    Ok(())
}

async fn analyze(engine: &Arc<Engine>, project: &Arc<Project>, args: &Value) -> Result<Report> {
    if !project.config.allow_inspections {
        return Err(Fault::denied("Native inspections are disabled by the host"));
    }
    let profile = project
        .config
        .inspections
        .get(string(args, "profile")?)
        .ok_or_else(|| Fault::denied("Unknown host inspection profile"))?;
    let provider = Provider::parse(
        serde_json::to_value(profile.provider)?
            .as_str()
            .ok_or_else(|| invalid("Invalid host inspection provider"))?,
    )?;
    validate_profile(provider, &profile.command.args, engine)?;
    validate_no_writes(&profile.version_args)?;
    let sandbox = project.config.sandbox.as_ref().ok_or_else(|| {
        Fault::new(
            "SANDBOX_REQUIRED",
            "Native inspection execution requires a host-configured OS sandbox",
        )
    })?;
    validate_sandbox(engine, sandbox)?;
    let before = snapshots(project, provider, engine.config.limits.max_files)?;
    let mut version_command = profile.command.clone();
    version_command.args = profile.version_args.clone();
    version_command.timeout_secs = version_command.timeout_secs.min(10);
    let version_result = odin_core::execution::run(
        &version_command,
        project.root.root(),
        4096,
        Some((sandbox, project.state.as_path())),
    )
    .await?;
    if version_result["sandbox"] != true {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "Inspection version probe did not execute inside the required sandbox",
        ));
    }
    let version = provider_version(provider, version_result["stdout"].as_str().unwrap_or(""));
    let version_known = version_result["exit_code"] == 0
        && version_result["timed_out"] != true
        && version_result["output_truncated"] != true
        && version.is_some();
    let version = if version_known {
        version.unwrap_or_else(|| "UNKNOWN".into())
    } else {
        "UNKNOWN".into()
    };
    let outcome = odin_core::execution::run(
        &profile.command,
        project.root.root(),
        engine.config.limits.max_output_bytes.min(MAX_REPORT_BYTES),
        Some((sandbox, project.state.as_path())),
    )
    .await?;
    if outcome["sandbox"] != true {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "Inspection process did not execute inside the required sandbox",
        ));
    }
    if outcome["timed_out"] == true {
        return Err(Fault::new("TIMEOUT", "Native inspection process timed out"));
    }
    if outcome["output_truncated"] == true {
        return Err(limited(
            "Native inspection output was truncated; cannot claim a complete report",
        ));
    }
    let mut report = normalize(
        provider,
        &project.root,
        &before,
        outcome["stdout"]
            .as_str()
            .ok_or_else(|| invalid("Missing analyzer stdout"))?,
        &version,
        (
            (number(args, "limit", 100) as usize).min(engine.config.limits.max_matches),
            engine.config.limits.max_output_bytes,
        ),
    )?;
    let after = snapshots(project, provider, engine.config.limits.max_files)?;
    let current = before == after;
    let normal_exit = if provider == Provider::Clippy {
        outcome["exit_code"] == 0
    } else {
        outcome["exit_code"] == 0
            || (outcome["exit_code"] == 1
                && report.value["summary"]["total"]
                    .as_u64()
                    .is_some_and(|total| total > 0))
    };
    let complete = report.value["summary"]["complete"] == true
        && current
        && normal_exit
        && version_known
        && !version.is_empty();
    report.value["freshness"] = json!(if current { "current" } else { "stale" });
    report.value["status"] = json!(if complete { "complete" } else { "incomplete" });
    report.value["summary"]["complete"] = json!(complete);
    report.value["execution"] = json!({"exit_code":outcome["exit_code"],"sandbox":outcome["sandbox"],"sandbox_status":outcome["sandbox_status"],"version_verified":version_known,"source_modified_by_odin":false});
    if !current {
        report.fixes.clear();
        if let Some(findings) = report.value["findings"].as_array_mut() {
            for finding in findings {
                finding["freshness"] = json!("stale");
                finding["fix_available"] = json!(false);
            }
        }
    }
    Ok(report)
}
pub async fn dispatch(
    engine: &Arc<Engine>,
    project: &Arc<Project>,
    tool: &str,
    args: &Value,
) -> Result<Value> {
    if tool == "list_inspection_providers" {
        return Ok(
            json!({"providers":["clippy","ruff","eslint"],"profiles":project.config.inspections.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider,
            "enabled":project.config.allow_inspections&&project.config.sandbox.is_some(),"requires_sandbox":true})).collect::<Vec<_>>(),"allow_inspections":project.config.allow_inspections}),
        );
    }
    let mut report = analyze(engine, project, args).await?;
    if tool == "run_inspections" {
        return Ok(report.value);
    }
    if tool != "prepare_inspection_fix" {
        return Err(Fault::new("UNKNOWN_TOOL", "Unknown inspection tool"));
    }
    if report.value["summary"]["complete"] != true {
        return Err(Fault::new(
            "CAPABILITY_UNAVAILABLE",
            "Inspection report is incomplete; cannot prepare a native fix",
        ));
    }
    let fix = report
        .fixes
        .remove(string(args, "finding_id")?)
        .ok_or_else(|| {
            Fault::new(
                "CAPABILITY_UNAVAILABLE",
                "This current inspection finding has no safe single-file fix",
            )
        })?;
    if fix.hash != string(args, "expected_sha256")? {
        return Err(Fault::conflict(
            "Inspection source hash does not match expected_sha256",
        ));
    }
    let source = crate::read_expected(project, &fix.path, &fix.hash)?;
    validate_edits(&source, &fix.edits, project.root.max_file_bytes())?;
    let after = odin_protocol::text::edit(&source, &fix.edits)
        .map_err(|error| invalid(error.to_string()))?;
    let mut preview = project.transactions.prepare(
        vec![Change {
            path: fix.path,
            expected_sha256: Some(fix.hash),
            content: Some(after),
        }],
        optional_string(args, "request_id"),
    )?;
    preview["provider"] = report.value["provider"].clone();
    preview["finding_id"] = json!(string(args, "finding_id")?);
    preview["source_modified"] = json!(false);
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    const UNICODE: &str = "\u{feff}😀ab\r\nz\n";
    fn setup(
        path: &str,
        source: &str,
        maximum: usize,
    ) -> (tempfile::TempDir, SafeRoot, BTreeMap<String, String>) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(path), source).unwrap();
        let root = SafeRoot::new(directory.path(), &[], maximum).unwrap();
        (
            directory,
            root,
            BTreeMap::from([(path.into(), source.into())]),
        )
    }
    fn ruff(path: &str, applicability: &str) -> Value {
        json!({"filename":path,"code":"F821","message":"fixture analyzer finding","location":{"row":1,"column":3},"end_location":{"row":1,"column":4},"fix":{"applicability":applicability,"edits":[{"location":{"row":1,"column":3},"end_location":{"row":1,"column":4},"content":"X"}]}})
    }
    fn clippy(path: &str, replacement: &str, primaries: usize) -> String {
        let primary = json!({"file_name":path,"byte_start":8,"byte_end":9,"is_primary":true});
        let diagnostic = json!({"reason":"compiler-message","message":{"level":"warning","message":"fixture clippy finding","code":{"code":"clippy::fixture"},"spans":vec![primary;primaries],
            "children":[{"spans":[{"file_name":path,"byte_start":8,"byte_end":9,"suggestion_applicability":"MachineApplicable","suggested_replacement":replacement}]}]}});
        format!("{diagnostic}\n{{\"reason\":\"build-finished\",\"success\":true}}\n")
    }
    fn first_fix(report: &mut Report) -> NativeFix {
        let id = report.value["findings"][0]["finding_id"].as_str().unwrap();
        report.fixes.remove(id).unwrap()
    }
    #[test]
    fn ruff_scalar_columns_bom_and_crlf_convert_to_utf16_with_safe_fix() {
        let (_directory, root, snapshots) = setup("source.py", UNICODE, 10000);
        let mut report = normalize(
            Provider::Ruff,
            &root,
            &snapshots,
            &json!([ruff("source.py", "safe")]).to_string(),
            "ruff fixture",
            (20, 65536),
        )
        .unwrap();
        let finding = &report.value["findings"][0];
        assert_eq!(
            finding["range"],
            json!({"start":{"line":0,"character":4},"end":{"line":0,"character":5}})
        );
        assert_eq!(finding["provider"], "ruff");
        assert_eq!(finding["rule_id"], "F821");
        assert_eq!(finding["fix_available"], true);
        assert_eq!(finding["version"], "ruff fixture");
        assert_eq!(finding["sha256"], sha256(UNICODE.as_bytes()));
        assert_eq!(report.value["summary"]["complete"], true);
        assert_eq!(
            odin_protocol::text::edit(UNICODE, &first_fix(&mut report).edits).unwrap(),
            "\u{feff}😀aX\r\nz\n"
        );
        assert_eq!(root.read("source.py").unwrap(), UNICODE);
    }
    #[test]
    fn eslint_utf16_columns_and_global_fix_indices_preserve_bom() {
        let (_directory, root, snapshots) = setup("source.js", UNICODE, 10000);
        let output = json!([{"filePath":"source.js","messages":[{"ruleId":"fixture/rule","severity":1,"message":"fixture eslint finding","line":1,"column":4,"endLine":1,"endColumn":5,"fix":{"range":[3,4],"text":"X"}}]}]);
        let mut report = normalize(
            Provider::Eslint,
            &root,
            &snapshots,
            &output.to_string(),
            "eslint fixture",
            (20, 65536),
        )
        .unwrap();
        assert_eq!(
            report.value["findings"][0]["range"]["start"]["character"],
            4
        );
        assert_eq!(report.value["findings"][0]["severity"], 2);
        assert_eq!(
            odin_protocol::text::edit(UNICODE, &first_fix(&mut report).edits).unwrap(),
            "\u{feff}😀aX\r\nz\n"
        );
    }
    #[test]
    fn clippy_machine_applicable_bytes_are_converted_and_build_completion_is_required() {
        let (_directory, root, snapshots) = setup("source.rs", UNICODE, 10000);
        let output = clippy("source.rs", "X", 1);
        let mut report = normalize(
            Provider::Clippy,
            &root,
            &snapshots,
            &output,
            "clippy fixture",
            (20, 65536),
        )
        .unwrap();
        assert_eq!(report.value["summary"]["complete"], true);
        assert_eq!(
            report.value["findings"][0]["range"]["start"]["character"],
            4
        );
        assert_eq!(
            odin_protocol::text::edit(UNICODE, &first_fix(&mut report).edits).unwrap(),
            "\u{feff}😀aX\r\nz\n"
        );
        let incomplete = output.lines().next().unwrap();
        assert_eq!(
            normalize(
                Provider::Clippy,
                &root,
                &snapshots,
                incomplete,
                "v",
                (20, 65536)
            )
            .unwrap()
            .value["summary"]["complete"],
            false
        );
        assert!(normalize(Provider::Clippy, &root, &snapshots, "", "v", (20, 65536)).is_err());
    }
    #[test]
    fn unsafe_suggestions_cross_file_locations_and_unknown_sources_never_become_safe_fixes() {
        let (_directory, root, snapshots) = setup("source.py", UNICODE, 10000);
        let report = normalize(
            Provider::Ruff,
            &root,
            &snapshots,
            &json!([ruff("source.py", "unsafe")]).to_string(),
            "v",
            (20, 65536),
        )
        .unwrap();
        assert!(report.fixes.is_empty());
        assert_eq!(report.value["findings"][0]["fix_available"], false);
        let external = tempfile::tempdir().unwrap();
        let external = external.path().join("secret.py");
        let report = normalize(
            Provider::Ruff,
            &root,
            &snapshots,
            &json!([ruff(external.to_str().unwrap(), "safe")]).to_string(),
            "v",
            (20, 65536),
        )
        .unwrap();
        assert_eq!(report.value["summary"]["complete"], false);
        assert_eq!(report.value["findings"][0]["relative_path"], Value::Null);
        assert_eq!(report.value["findings"][0]["freshness"], "unavailable");
        assert!(report.fixes.is_empty());
        assert!(!report.value.to_string().contains("secret.py"));
    }
    #[test]
    fn malformed_analyzer_coordinates_and_overlapping_edits_are_rejected() {
        let (_directory, root, snapshots) = setup("source.js", UNICODE, 10000);
        let output = json!([{"filePath":"source.js","messages":[{"ruleId":"x","severity":1,"message":"x","line":1,"column":2,"fix":{"range":[1,2],"text":"x"}}]}]);
        assert_eq!(
            normalize(
                Provider::Eslint,
                &root,
                &snapshots,
                &output.to_string(),
                "v",
                (20, 65536)
            )
            .err()
            .unwrap()
            .code,
            "INVALID_BACKEND_OUTPUT"
        );
        assert!(byte_range(UNICODE, 4, 8).is_err());
        assert!(byte_range(UNICODE, 10, 11).is_err());
        assert!(row_position(UNICODE, 0, 1, false).is_err());
        assert!(js_index(UNICODE, 1).is_err());
        let edits = vec![
            TextEdit {
                range: byte_range(UNICODE, 8, 9).unwrap(),
                new_text: "x".into()
            };
            2
        ];
        assert!(validate_edits(UNICODE, &edits, 100).is_err());
    }
    #[test]
    fn repeated_primary_clippy_suggestions_have_an_aggregate_preallocation_budget() {
        let (_directory, root, snapshots) = setup("source.rs", UNICODE, 10000);
        let output = clippy("source.rs", &"x".repeat(2048), 50);
        assert!(output.len() < 16000);
        let error = normalize(
            Provider::Clippy,
            &root,
            &snapshots,
            &output,
            "v",
            (100, 32000),
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "LIMIT_EXCEEDED");
        assert!(error.message.contains("aggregate"), "{}", error.message);
    }
    #[test]
    fn parser_count_output_and_resulting_file_size_limits_are_enforced() {
        let (_directory, root, snapshots) = setup("source.py", UNICODE, UNICODE.len());
        let mut finding = ruff("source.py", "safe");
        finding["fix"]["edits"][0]["content"] = json!("xx");
        assert_eq!(
            normalize(
                Provider::Ruff,
                &root,
                &snapshots,
                &json!([finding]).to_string(),
                "v",
                (20, 65536)
            )
            .err()
            .unwrap()
            .code,
            "LIMIT_EXCEEDED"
        );
        let item = ruff("source.py", "unsafe");
        assert_eq!(
            normalize(
                Provider::Ruff,
                &root,
                &snapshots,
                &json!([item.clone(), item]).to_string(),
                "v",
                (1, 65536)
            )
            .err()
            .unwrap()
            .code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            normalize(
                Provider::Ruff,
                &root,
                &snapshots,
                &" ".repeat(1025),
                "v",
                (1, 1024)
            )
            .err()
            .unwrap()
            .code,
            "LIMIT_EXCEEDED"
        );
    }
    #[test]
    fn inspection_profiles_reject_write_flags_and_require_provider_safety_flags() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let config = serde_json::from_value(
            json!({"state_dir":state.path(),"projects":{"one":{"root":directory.path()}}}),
        )
        .unwrap();
        let engine = Engine::new(config).unwrap();
        for flag in [
            "--fix",
            "--fix=true",
            "--output-file=changed",
            "-ofile",
            "format",
            "clean",
        ] {
            assert!(validate_no_writes(&[flag.into()]).is_err());
        }
        assert!(validate_profile(Provider::Ruff, &["check".into()], &engine).is_err());
        assert!(
            validate_profile(
                Provider::Ruff,
                &["check".into(), "--no-fix".into(), "--no-cache".into()],
                &engine
            )
            .is_ok()
        );
        assert!(validate_profile(Provider::Clippy, &["clippy".into()], &engine).is_err());
        assert!(
            validate_profile(
                Provider::Clippy,
                &["clippy".into(), "--locked".into()],
                &engine
            )
            .is_ok()
        );
        assert!(validate_profile(Provider::Eslint, &["--format=json".into()], &engine).is_err());
        let config_path = directory.path().join("eslint.config.mjs");
        std::fs::write(&config_path, "throw Error('never execute')").unwrap();
        assert!(
            validate_profile(
                Provider::Eslint,
                &[
                    "--no-config-lookup".into(),
                    "--config".into(),
                    config_path.to_string_lossy().into()
                ],
                &engine
            )
            .is_err()
        );
        let trusted = state.path().join("eslint.config.mjs");
        std::fs::write(&trusted, "export default []").unwrap();
        assert!(
            validate_profile(
                Provider::Eslint,
                &[
                    "--no-config-lookup".into(),
                    format!("--config={}", trusted.display())
                ],
                &engine
            )
            .is_ok()
        );
    }
    #[test]
    fn sandbox_write_and_temp_grants_cannot_overlap_sources_or_state() {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let config = serde_json::from_value(
            json!({"state_dir":state.path(),"projects":{"one":{"root":directory.path()}}}),
        )
        .unwrap();
        let engine = Engine::new(config).unwrap();
        let mut policy:odin_core::sandbox_policy::MxcSandboxConfig=serde_json::from_value(json!({"executor":external.path().join("executor"),"executor_sha256":"00","temp_root":external.path().join("temp"),"readwrite_paths":[external.path().join("cache")]})).unwrap();
        assert!(validate_sandbox(&engine, &policy).is_ok());
        for grant in [
            directory.path().to_path_buf(),
            directory.path().join("child"),
            directory.path().parent().unwrap().to_path_buf(),
            state.path().to_path_buf(),
        ] {
            policy.readwrite_paths = vec![grant];
            assert!(validate_sandbox(&engine, &policy).is_err());
        }
        policy.readwrite_paths.clear();
        policy.temp_root = state.path().join("tmp");
        assert!(validate_sandbox(&engine, &policy).is_err());
        #[cfg(windows)]
        {
            policy.temp_root = PathBuf::from(directory.path().to_string_lossy().to_uppercase());
            assert!(validate_sandbox(&engine, &policy).is_err());
        }
    }
    #[test]
    fn provider_versions_distinguish_clippy_from_cargo_and_unknown_output() {
        assert_eq!(
            provider_version(Provider::Clippy, "clippy 0.1.90\n"),
            Some("clippy 0.1.90".into())
        );
        assert!(provider_version(Provider::Clippy, "cargo 1.90.0").is_none());
        assert_eq!(
            provider_version(Provider::Ruff, "ruff 0.14.5\n"),
            Some("ruff 0.14.5".into())
        );
        assert_eq!(
            provider_version(Provider::Eslint, "v9.0.0\n"),
            Some("v9.0.0".into())
        );
        assert!(provider_version(Provider::Eslint, "UNKNOWN").is_none());
    }
}
