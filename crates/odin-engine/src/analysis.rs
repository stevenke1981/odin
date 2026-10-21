//! Live structural evidence. File extensions and marker names are hints, never
//! semantic symbols, executable host policy or proof that a backend/test ran.
use crate::{Engine, Project, number};
use odin_core::{
    Result,
    languages::ADAPTERS,
    search::{self, FileList},
    sha256,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) fn analyze(engine: &Engine, project: &Project, args: &Value) -> Result<Value> {
    let listing = search::files(&project.root, engine.config.limits.max_files)?;
    analyze_listing(engine, project, args, &listing)
}

pub(crate) fn analyze_listing(
    engine: &Engine,
    project: &Project,
    args: &Value,
    listing: &FileList,
) -> Result<Value> {
    let mut extensions = BTreeMap::<String, usize>::new();
    let mut directories = BTreeSet::new();
    let mut marker_names = ADAPTERS
        .iter()
        .flat_map(|a| a.project_markers.iter().copied())
        .collect::<BTreeSet<_>>();
    marker_names.extend([
        "README.md",
        "AGENTS.md",
        "Cargo.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "uv.lock",
        "requirements.txt",
    ]);
    let mut candidates = Vec::new();
    for file in &listing.files {
        let path = Path::new(file);
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            *extensions
                .entry(extension.to_ascii_lowercase())
                .or_default() += 1;
        }
        if let Some(first) = file.split('/').next().filter(|_| file.contains('/')) {
            directories.insert(first.to_owned());
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| marker_names.contains(name))
        {
            candidates.push(file.clone());
        }
    }
    // Root evidence first, then deterministic path order. Contents never leave
    // this helper; only hashes/length or a bounded error code are returned.
    candidates.sort_by_key(|path| (path.matches('/').count(), path.clone()));
    let max_evidence = number(args, "max_evidence", 20) as usize;
    let mut evidence = Vec::new();
    for path in candidates.iter().take(max_evidence) {
        evidence.push(match project.root.read(path) {
            Ok(body) => json!({"path":path,"status":"READ","sha256":sha256(body.as_bytes()),"bytes":body.len()}),
            Err(error) => json!({"path":path,"status":"UNREADABLE","error_code":error.code}),
        });
    }
    let mut languages = Vec::new();
    let mut gaps = Vec::new();
    for adapter in ADAPTERS {
        let source_count: usize = adapter
            .extensions
            .iter()
            .map(|ext| extensions.get(*ext).copied().unwrap_or_default())
            .sum();
        let markers = listing
            .files
            .iter()
            .filter(|file| {
                Path::new(file)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| adapter.project_markers.contains(&name))
            })
            .take(20)
            .collect::<Vec<_>>();
        if source_count == 0 && markers.is_empty() {
            continue;
        }
        let observed = adapter
            .extensions
            .iter()
            .copied()
            .filter(|ext| extensions.contains_key(*ext))
            .collect::<Vec<_>>();
        let uncovered = observed
            .iter()
            .copied()
            .filter(|ext| {
                !project.config.languages.iter().any(|language| {
                    language
                        .extensions
                        .iter()
                        .any(|configured| configured.eq_ignore_ascii_case(ext))
                })
            })
            .collect::<Vec<_>>();
        let configured = if observed.is_empty() {
            project
                .config
                .languages
                .iter()
                .any(|language| language.id == adapter.id)
        } else {
            uncovered.is_empty()
        };
        if !uncovered.is_empty() {
            gaps.push(adapter.id);
        }
        languages.push(json!({"id":adapter.id,"source_file_count":source_count,
            "detection":if source_count > 0 {"EXTENSION_HINT"} else {"MARKER_HINT_ONLY"},"marker_hints":markers,
            "host_backend_configured":configured,"observed_extensions":observed,"unconfigured_extensions":uncovered,
            "backend_probe":"NOT_RUN","suggested_server":adapter.server}));
    }
    let configured = project
        .config
        .languages
        .iter()
        .map(
            |language| json!({"id":language.id,"extensions":language.extensions,"probe":"NOT_RUN"}),
        )
        .collect::<Vec<_>>();
    // This digest describes listed paths and the bounded evidence read above,
    // NOT every source file's content. LSP queries synchronize source separately.
    let structure_sha256 = sha256(&serde_json::to_vec(&(&listing.files, &evidence))?);
    let mut next_steps =
        vec![json!({"tool":"get_capabilities","project":project.id,"probe":false})];
    if let Some(file) = listing.files.iter().find(|file| {
        project.config.languages.iter().any(|language| {
            Path::new(file)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    language
                        .extensions
                        .iter()
                        .any(|configured| configured.eq_ignore_ascii_case(ext))
                })
        })
    }) {
        next_steps
            .push(json!({"tool":"get_symbols_overview","project":project.id,"relative_path":file}));
    }
    Ok(
        json!({"project":project.id,"root":project.root.root(),"analysis_kind":"LIVE_STRUCTURE_AND_CONFIGURATION",
        "snapshot":"LIVE_NON_ATOMIC","structure_sha256":structure_sha256,"file_count":listing.files.len(),
        "scan_truncated":listing.truncated,"scan_limit":engine.config.limits.max_files,
        "sample_files":listing.files.iter().take(number(args,"sample_files",20) as usize).collect::<Vec<_>>(),
        "top_level_directories":directories.iter().take(100).collect::<Vec<_>>(),"directories_truncated":directories.len()>100,
        "extensions":extensions,"language_hints":languages,"configured_backends":configured,"unconfigured_language_hints":gaps,
        "evidence":evidence,"evidence_truncated":candidates.len()>max_evidence,
        "permissions":{"write":project.config.allow_write,"shell":project.config.allow_shell,"debug":project.config.allow_debug,
            "inspections":project.config.allow_inspections,"formatters":project.config.allow_formatters},
        "backends_started":false,"tests":"NOT_RUN","inspections":"NOT_RUN","next_steps":next_steps,
        "limitations":["Structural hints are not semantic symbol analysis or dependency parsing.",
            "Ignore rules, path policy and scan/evidence limits restrict visibility; absent evidence is not proof of absence.",
            "No background watcher or atomic multi-file snapshot. structure_sha256 covers paths and returned evidence only.",
            "No repository commands/configuration executed and no backend installed or probed. Semantic work requires a host-configured LSP."]}),
    )
}
