"""Real binary CLI/MCP startup-workspace acceptance. No language parity claim."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from protocol_harness import Peer


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    checks = []
    error = None

    def check(name, ok):
        checks.append({"name": name, "status": "PASS" if ok else "FAIL"})
        if not ok:
            raise AssertionError(name)

    def cli(argv, cwd, success=True):
        proc = subprocess.run([binary] + argv, cwd=cwd, capture_output=True, timeout=60)
        check("cli_exit:" + " ".join(argv[-2:]), (proc.returncode == 0) == success)
        return json.loads(proc.stdout) if success else None

    def start(command, cwd):
        peer = Peer(command, cwd=cwd)
        result = peer.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "workspace-smoke", "version": "1"}})
        check("initialize", result.get("result", {}).get("serverInfo", {}).get("name") == "odin")
        peer.notify("notifications/initialized")
        return peer

    def call(peer, tool, arguments=None, expect_error=False):
        result = peer.request("tools/call", {"name": tool, "arguments": arguments or {}})["result"]
        check("tool:" + tool, bool(result.get("isError")) == expect_error)
        return result.get("structuredContent") or json.loads(result["content"][0]["text"])

    try:
        with tempfile.TemporaryDirectory(prefix="odin-workspace-") as directory:
            base = Path(directory)
            a, b, legacy = base / "專案 alpha", base / "beta", base / "sigma"
            for path in (a, b, legacy):
                path.mkdir()
            for path, value in ((a, "alpha"), (b, "beta"), (legacy, "sigma")):
                (path / "sentinel.txt").write_text(value, encoding="utf-8")
            (a / "Cargo.toml").write_text("[package]\nname='alpha'\n", encoding="utf-8")
            (a / "main.rs").write_text("fn alpha() {}", encoding="utf-8")
            (a / "odin.toml").write_text("malicious = 'not host config'\n", encoding="utf-8")
            host = base / "host.toml"
            host.write_text("state_dir = " + json.dumps(str(base / "state")) + "\n[projects.sigma]\nroot = " + json.dumps(str(legacy)) + "\n[workspace]\nallowed_roots = [" + json.dumps(str(a)) + "," + json.dumps(str(b)) + "]\n[[workspace.languages]]\nid='rust'\nextensions=['rs']\nprogram='must-not-start-for-analysis'\n", encoding="utf-8")
            prefix = ["--config", str(host), "--workspace", "."]
            analysis = cli(prefix + ["analyze"], a)
            check("analysis_live_no_execution", analysis["backends_started"] is False and analysis["tests"] == "NOT_RUN")
            check("no_grants", not any(analysis["permissions"].values()))
            check("absolute_host_required", cli(["--config", "odin.toml", "--workspace", ".", "analyze"], a, False) is None)
            cli(["--config", str(a / "odin.toml"), "--workspace", ".", "analyze"], a, False)
            cli(prefix + ["analyze"], base, False)
            setup = cli(prefix + ["setup", "--client", "codex"], a)
            import tomllib
            entry = tomllib.loads(setup["content"])["mcp_servers"]["odin"]
            index = entry["args"].index("--workspace")
            check("setup_keeps_literal_dot", entry["args"][index + 1] == ".")
            command = [entry["command"]] + entry["args"]
            # Two running servers use exactly the SAME generated client entry.
            with start(command, a) as first, start(command, b) as second:
                first_projects = call(first, "list_projects")["projects"]
                second_projects = call(second, "list_projects")["projects"]
                id_a, id_b = first_projects[0]["id"], second_projects[0]["id"]
                check("one_launch_project", len(first_projects) == len(second_projects) == 1)
                check("distinct_stable_ids", id_a != id_b and id_a == analysis["project"])
                check("cwd_alpha", call(first, "read_file", {"relative_path": "sentinel.txt"})["content"] == "alpha")
                check("cwd_beta", call(second, "read_file", {"relative_path": "sentinel.txt"})["content"] == "beta")
                check("legacy_not_selected", id_a != "sigma" and id_b != "sigma")
                call(second, "read_file", {"project": id_a, "relative_path": "sentinel.txt"}, True)
                call(first, "create_text_file", {"relative_path": "forbidden", "content": "x", "expected_sha256": None}, True)
                before = call(first, "project_analysis")
                (a / "Cargo.toml").write_text("[package]\nname='updated'\n", encoding="utf-8")
                (a / "new.py").write_text("print('no execution')", encoding="utf-8")
                (a / "main.rs").unlink()
                after = call(first, "project_analysis")
                check("live_add_delete", "rs" not in after["extensions"] and after["extensions"]["py"] == 1)
                check("live_manifest_hash", before["structure_sha256"] != after["structure_sha256"])
                check("backend_gap", "python" in after["unconfigured_language_hints"])
                call(first, "project_analysis", {"root": str(b)}, True)
                check("state_namespaces", all((base / "state" / "projects" / i / "index.sqlite3").is_file() for i in (id_a, id_b)))
            # Output guards retain all registered roots and allowed parents,
            # even though only one runtime project is selected.
            cli(prefix + ["setup", "--client", "codex", "--output", str(b / "client.toml"), "--apply"], a, False)
            cli(prefix + ["hook", "--output", str(legacy / "AGENTS.md"), "--apply"], a, False)
            check("other_roots_not_written", not (b / "client.toml").exists() and not (legacy / "AGENTS.md").exists())
            # This host explicitly allows the private state's parent. The root
            # guard must still deny private journals and memory descendants.
            broad_host = base / "broad-host.toml"
            broad_host.write_text("state_dir = " + json.dumps(str(base / "state")) + "\n[workspace]\nallowed_roots = [" + json.dumps(str(base)) + "]\n", encoding="utf-8")
            private = base / "state" / "projects" / id_a / "transactions"
            cli(["--config", str(broad_host), "--workspace", str(private), "analyze"], a, False)
            registered = cli(prefix + ["analyze"], legacy)
            check("registered_selection", registered["project"] == "sigma")
        status = "PASS"
    except Exception as exc:
        status, error = "FAIL", str(exc)
    report = {"scope": "REAL_WINDOWS_OR_NATIVE_BINARY_WORKSPACE_CLI_AND_MCP", "status": status,
              "platform": platform.platform(), "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
              "checks": checks, "error": error, "real_lsp_dap_tested": False, "timestamp_unix": time.time()}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
