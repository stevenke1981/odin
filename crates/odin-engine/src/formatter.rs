//! Explicit host formatter profiles. Stdin/stdout proposals only; no fallback.
use crate::{Engine, Project, inspections, optional_string, read_expected, string};
use odin_core::{Fault, Result, sha256, transaction::Change};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

pub fn profiles(project: &Project) -> Value {
    json!(project.config.formatters.iter().map(|(name,profile)|json!({"name":name,"provider":profile.provider,"extensions":profile.extensions,
        "enabled":project.config.allow_write&&project.config.allow_formatters&&project.config.sandbox.is_some(),"requires_sandbox":true,
        "selection":"explicit_formatter_profile","range_supported":false,"caller_options_supported":false})).collect::<Vec<_>>())
}

fn denied(message: &str) -> Fault {
    Fault::denied(message)
}

pub async fn prepare(engine: &Arc<Engine>, project: &Arc<Project>, args: &Value) -> Result<Value> {
    if !project.config.allow_write || !project.config.allow_formatters {
        return Err(denied(
            "Native formatting requires host write and formatter permissions",
        ));
    }
    let name = string(args, "formatter_profile")?;
    let profile = project
        .config
        .formatters
        .get(name)
        .ok_or_else(|| denied("Unknown host formatter profile"))?;
    if args.get("range").is_some()
        || args.get("options").is_some_and(|options| {
            options
                .as_object()
                .is_none_or(|options| !options.is_empty())
        })
    {
        return Err(Fault::new(
            "CAPABILITY_UNAVAILABLE",
            "Native Ruff profiles format whole documents with fixed host options; range and caller formatting options are unsupported",
        ));
    }
    let sandbox = project.config.sandbox.as_ref().ok_or_else(|| {
        Fault::new(
            "SANDBOX_REQUIRED",
            "Native formatting requires a host-configured OS sandbox",
        )
    })?;
    inspections::validate_sandbox(engine, sandbox)?;
    let path = string(args, "relative_path")?;
    let absolute = project.root.resolve(path, false)?;
    let path = project.root.relative(&absolute)?;
    let extension = Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if !profile
        .extensions
        .iter()
        .any(|configured| configured.eq_ignore_ascii_case(extension))
    {
        return Err(Fault::new(
            "CAPABILITY_UNAVAILABLE",
            "Formatter profile does not cover this file extension",
        ));
    }
    let before = read_expected(project, &path, string(args, "expected_sha256")?)?;
    let mut command = profile.command.clone();
    command.args = profile.resolved_args(&absolute)?;
    let result = odin_core::execution::run_with_stdin(
        &command,
        project.root.root(),
        (
            project.root.max_file_bytes(),
            engine.config.limits.max_output_bytes,
        ),
        Some((sandbox, project.state.as_path())),
        &before,
    )
    .await?;
    stage(project, args, &before, &result)
}

fn validated_output(result: &Value, maximum: usize) -> Result<&str> {
    if result["sandbox"] != true {
        return Err(Fault::new(
            "SANDBOX_UNAVAILABLE",
            "Formatter did not execute inside the required sandbox",
        ));
    }
    if result["timed_out"] == true {
        return Err(Fault::new("TIMEOUT", "Native formatter timed out"));
    }
    if result["output_truncated"] == true {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Native formatter output exceeded the host limit",
        ));
    }
    if result["exit_code"] != 0 || result["success"] != true || result["stdin_complete"] != true {
        return Err(Fault::new(
            "FORMATTER_FAILED",
            "Native formatter did not complete successfully with the entire source input; source was not staged",
        ));
    }
    if result["stdout_utf8_valid"] != true {
        return Err(Fault::new(
            "INVALID_BACKEND_OUTPUT",
            "Native formatter stdout is not valid UTF-8",
        ));
    }
    let after = result["stdout"].as_str().ok_or_else(|| {
        Fault::new(
            "INVALID_BACKEND_OUTPUT",
            "Native formatter stdout is missing",
        )
    })?;
    if after.len() > maximum {
        return Err(Fault::new(
            "LIMIT_EXCEEDED",
            "Formatted source exceeds the file limit",
        ));
    }
    Ok(after)
}

fn stage(project: &Project, args: &Value, before: &str, result: &Value) -> Result<Value> {
    let after = validated_output(result, project.root.max_file_bytes())?;
    let path = project.root.relative(
        &project
            .root
            .resolve(string(args, "relative_path")?, false)?,
    )?;
    let name = string(args, "formatter_profile")?;
    if project.root.read(&path)? != before {
        return Err(Fault::conflict("Source changed during native formatting"));
    }
    let formatted_hash = sha256(after.as_bytes());
    let mut preview = if after == before {
        json!({"changed":false,"sha256":formatted_hash})
    } else {
        project.transactions.prepare(
            vec![Change {
                path,
                expected_sha256: Some(string(args, "expected_sha256")?.into()),
                content: Some(after.into()),
            }],
            optional_string(args, "request_id"),
        )?
    };
    preview["provider"] = json!("ruff");
    preview["source"] = json!("native_formatter");
    preview["formatter_profile"] = json!(name);
    preview["source_modified"] = json!(false);
    preview["source_sha256"] = json!(string(args, "expected_sha256")?);
    preview["formatted_sha256"] = json!(formatted_hash);
    preview["execution"] = json!({"exit_code":result["exit_code"],"sandbox":true,"sandbox_status":result["sandbox_status"]});
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic process outcomes exercise the output/transaction boundary only.
    // Actual Ruff and native OS execution are separately accepted end to end.
    fn outcome(output: &str) -> Value {
        json!({"sandbox":true,"sandbox_status":"fixture_only","timed_out":false,"output_truncated":false,"exit_code":0,"success":true,"stdin_complete":true,"stdout_utf8_valid":true,"stdout":output})
    }
    fn setup(source: &str) -> (tempfile::TempDir, tempfile::TempDir, Arc<Engine>) {
        let root = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("source.py"), source).unwrap();
        let config=serde_json::from_value(json!({"state_dir":state.path(),"projects":{"one":{"root":root.path(),"allow_write":true}}})).unwrap();
        (root, state, Engine::new(config).unwrap())
    }
    fn arguments(before: &str) -> Value {
        json!({"relative_path":"source.py","expected_sha256":sha256(before.as_bytes()),"formatter_profile":"python"})
    }
    #[test]
    fn invalid_process_outcomes_never_become_source_text() {
        for (field, value, code) in [
            ("sandbox", json!(false), "SANDBOX_UNAVAILABLE"),
            ("timed_out", json!(true), "TIMEOUT"),
            ("output_truncated", json!(true), "LIMIT_EXCEEDED"),
            ("exit_code", json!(2), "FORMATTER_FAILED"),
            ("success", json!(false), "FORMATTER_FAILED"),
            ("success", Value::Null, "FORMATTER_FAILED"),
            ("stdin_complete", json!(false), "FORMATTER_FAILED"),
            ("stdin_complete", Value::Null, "FORMATTER_FAILED"),
            ("stdout_utf8_valid", json!(false), "INVALID_BACKEND_OUTPUT"),
            ("stdout_utf8_valid", Value::Null, "INVALID_BACKEND_OUTPUT"),
            ("stdout", Value::Null, "INVALID_BACKEND_OUTPUT"),
        ] {
            let mut result = outcome("good");
            result[field] = value;
            assert_eq!(validated_output(&result, 10).unwrap_err().code, code);
        }
        assert_eq!(
            validated_output(&outcome("😀"), 3).unwrap_err().code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(validated_output(&outcome("😀"), 4).unwrap(), "😀");
        for field in ["success", "stdin_complete"] {
            let mut result = outcome("partial but valid UTF-8");
            result.as_object_mut().unwrap().remove(field);
            assert_eq!(
                validated_output(&result, 100).unwrap_err().code,
                "FORMATTER_FAILED"
            );
        }
    }
    #[test]
    fn unicode_bom_and_crlf_preview_preserves_bytes_until_apply_and_rollback() {
        let before = "\u{feff}x={'emoji':'😀'}\r\n";
        let after = "\u{feff}x = {\"emoji\": \"😀\"}\r\n";
        let (root, _state, engine) = setup(before);
        let project = &engine.projects["one"];
        let preview = stage(project, &arguments(before), before, &outcome(after)).unwrap();
        assert_eq!(preview["provider"], "ruff");
        assert_eq!(preview["source_modified"], false);
        assert_eq!(preview["formatted_sha256"], sha256(after.as_bytes()));
        assert_eq!(
            std::fs::read(root.path().join("source.py")).unwrap(),
            before.as_bytes()
        );
        let id = preview["transaction_id"].as_str().unwrap();
        project.transactions.apply(id).unwrap();
        assert_eq!(
            std::fs::read(root.path().join("source.py")).unwrap(),
            after.as_bytes()
        );
        project.transactions.rollback(id).unwrap();
        assert_eq!(
            std::fs::read(root.path().join("source.py")).unwrap(),
            before.as_bytes()
        );
    }
    #[test]
    fn noop_has_no_journal_and_external_writes_are_never_overwritten() {
        let before = "x=1\n";
        let (root, _state, engine) = setup(before);
        let project = &engine.projects["one"];
        let noop = stage(project, &arguments(before), before, &outcome(before)).unwrap();
        assert_eq!(noop["changed"], false);
        assert!(noop.get("transaction_id").is_none());
        let preview = stage(project, &arguments(before), before, &outcome("x = 1\n")).unwrap();
        std::fs::write(root.path().join("source.py"), "external\n").unwrap();
        assert_eq!(
            project
                .transactions
                .apply(preview["transaction_id"].as_str().unwrap())
                .unwrap_err()
                .code,
            "CONFLICT"
        );
        assert_eq!(
            stage(project, &arguments(before), before, &outcome("x = 1\n"))
                .unwrap_err()
                .code,
            "CONFLICT"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("source.py")).unwrap(),
            "external\n"
        );
    }
    #[tokio::test]
    async fn invalid_output_is_rejected_before_a_transaction_is_prepared() {
        let before = "x=1\n";
        let (_root, _state, engine) = setup(before);
        let project = &engine.projects["one"];
        let mut result = outcome("replacement character �");
        result["stdout_utf8_valid"] = json!(false);
        assert_eq!(
            stage(project, &arguments(before), before, &result)
                .unwrap_err()
                .code,
            "INVALID_BACKEND_OUTPUT"
        );
        assert_eq!(
            project.transactions.list().unwrap(),
            json!({"transactions":[]})
        );
    }
}
