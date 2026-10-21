"""Direct native backend diagnostics, never Odin/MCP acceptance evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tests"))
from protocol_harness import Peer
from real_ide_backends import RUNTIME_ENV, configure_sandbox_fixture, fixture


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", choices=("go-lsp", "java-security", "python-runtime-cwd"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Choose a fresh report")
    executor = Path("C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe")
    assert sha(executor) == "8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259"
    tools = Path("E:/odin/.odin/validation-tools")
    report = {"scope": "DIRECT_NATIVE_BACKEND_DIAGNOSTIC_ONLY", "odin_mcp_checks": 0, "probe": args.probe,
              "source_sha256": sha(__file__), "executor_sha256": sha(executor), "status": "NOT_RUN"}
    with tempfile.TemporaryDirectory(prefix="odin-native-diagnostic-") as temporary:
        base = Path(temporary)
        language = args.probe.split("-", 1)[0]
        environment = {key: os.environ[key] for key in RUNTIME_ENV if key in os.environ}
        environment["GOTOOLCHAIN"] = "local"
        item = fixture(base, tools, language, "diagnostic")
        readonly, writable = configure_sandbox_fixture(base, tools, item, environment)
        environment.update(item.environment)
        scratch, policies = base / "private-scratch", base / "policies"
        scratch.mkdir(); policies.mkdir()
        for key in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP"):
            environment[key] = str(scratch)
        environment.update(GOTELEMETRY="off", DOTNET_CLI_TELEMETRY_OPTOUT="1", DOTNET_GENERATE_ASPNET_CERTIFICATE="false")
        cwd = item.root
        if args.probe == "python-runtime-cwd":
            cwd = tools / "python/Lib/site-packages/basedpyright/dist"
        if args.probe == "java-security":
            item.argv = [f"-Djava.io.tmpdir={base / 'backend-cache' / item.project / 'tmp'}",
                         "-Duser.language=en", "-Duser.country=US", "-Djava.security.debug=properties,provider",
                         "-XshowSettings:security", "--version"]
        policy = {"version": "1.0.0", "containment": "processcontainer",
                  "lifecycle": {"destroyOnExit": True, "preservePolicy": False},
                  "process": {"cwd": str(cwd), "env": [f"{key}={value}" for key, value in environment.items()],
                              "inheritDefaultEnv": False, "timeout": 25000},
                  "filesystem": {"readonlyPaths": [str(p.resolve()) for p in readonly],
                                 "readwritePaths": [str(p.resolve()) for p in writable] + [str(scratch)],
                                 "deniedPaths": [str(policies)]},
                  "fallback": {"allowDaclMutation": False},
                  "network": {"egress": {"default": "deny"}, "ingress": {"default": "deny", "hostLoopback": "deny"}},
                  "ui": {"disable": False, "clipboard": "none", "injection": False}, "telemetry": {"enabled": False}}
        policy_path = policies / "fixed-policy.json"
        policy_path.write_text(json.dumps(policy), encoding="utf-8", newline="\n")
        command = [str(executor), "--config", str(policy_path), "--", str(item.program), *item.argv]
        report.update(program_sha256=sha(item.program), policy_sha256=sha(policy_path), command=command,
                      policy=policy, explicit_host_allow_ui=True, profile_info=item.profile_info)
        if args.probe == "java-security":
            result = subprocess.run(command, cwd=policies, capture_output=True, timeout=35)
            report.update(native_exit_code=result.returncode, stdout=result.stdout.decode("utf-8", errors="replace"),
                          stderr=result.stderr.decode("utf-8", errors="replace"),
                          stdout_raw_sha256=hashlib.sha256(result.stdout).hexdigest(),
                          stderr_raw_sha256=hashlib.sha256(result.stderr).hexdigest(), status="OBSERVED")
        else:
            with Peer(command, framing="header", cwd=policies) as peer:
                try:
                    params = {"processId": os.getpid(), "rootUri": item.root.as_uri(),
                              "workspaceFolders": [{"uri": item.root.as_uri(), "name": item.project}],
                              "capabilities": {"general": {"positionEncodings": ["utf-16"]}},
                              "initializationOptions": item.initialization}
                    report["initialize"] = peer.request("initialize", params, timeout=15)
                    peer.notify("initialized", {})
                    peer.notify("workspace/didChangeConfiguration", {"settings": item.settings})
                    peer.notify("textDocument/didOpen", {"textDocument": {"uri": (item.root / item.source).as_uri(),
                                "languageId": language, "version": 1, "text": item.original}})
                    report["symbols"] = peer.request("textDocument/documentSymbol", {"textDocument": {"uri": (item.root / item.source).as_uri()}}, timeout=15)
                    try:
                        report["shutdown"] = peer.request("shutdown", {}, timeout=3)
                        peer.notify("exit", {})
                    except Exception as error:
                        report["shutdown_error"] = str(error)
                    report["status"] = "OBSERVED"
                except Exception as error:
                    report.update(status="OBSERVED_FAILURE", error=str(error))
                finally:
                    time.sleep(.2)
                    report["events"] = list(peer.events)
                    report["stderr"] = list(peer.errors)
            report["native_exit_code"] = peer.process.returncode
        serialized = json.dumps(report, ensure_ascii=False, indent=2)
        serialized = serialized.replace(json.dumps(str(base))[1:-1], "<fixture>")
        args.output.write_text(serialized + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"status": report["status"], "scope": report["scope"], "native_exit_code": report.get("native_exit_code"), "report_sha256": sha(args.output)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
