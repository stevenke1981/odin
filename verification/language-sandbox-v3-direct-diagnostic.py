"""Direct native backend diagnostics, never Odin/MCP acceptance evidence."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
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
    parser.add_argument("--enumerate-runtime-ancestors", action="store_true")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Choose a fresh report")
    executor = Path("C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe")
    assert sha(executor) == "8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259"
    tools = Path("E:/odin/.odin/validation-tools")
    report = {"scope": "DIRECT_NATIVE_BACKEND_DIAGNOSTIC_ONLY", "odin_mcp_checks": 0, "probe": args.probe,
              "source_sha256": sha(__file__), "executor_sha256": sha(executor), "status": "NOT_RUN"}
    with ExitStack() as resources:
        temporary = resources.enter_context(tempfile.TemporaryDirectory(prefix="odin-native-diagnostic-"))
        base = Path(temporary)
        language = args.probe.split("-", 1)[0]
        environment = {key: os.environ[key] for key in RUNTIME_ENV if key in os.environ}
        jdks = list(tools.glob("jdk/*/bin/java.exe"))
        environment["PATH"] = os.pathsep.join([str(tools / "go/go/bin"), str(tools / "dotnet"),
                                               str(jdks[0].parent) if len(jdks) == 1 else "", environment.get("PATH", "")])
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
        if args.enumerate_runtime_ancestors:
            native_probe = subprocess.run([str(executor), "--probe"], capture_output=True, text=True, encoding="utf-8", timeout=10)
            report["native_capability_probe"] = {"exit_code": native_probe.returncode, "stdout": native_probe.stdout, "stderr": native_probe.stderr}
            capabilities = json.loads(native_probe.stdout)
            if (native_probe.returncode != 0 or capabilities.get("tier") != "base-container"
                    or capabilities.get("needsDaclAugmentation") is not False
                    or capabilities.get("warnings") != []
                    or capabilities.get("probes", {}).get("baseContainerSupportsDenyPaths") is not True
                    or capabilities.get("probes", {}).get("baseContainerSupportsEnumeratePaths") is not True):
                report["status"] = "UNSUPPORTED_HOST"
                args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
                return 0
            enumerate_paths = sorted({str(parent) for path in readonly if path.drive.lower() in ("c:", "e:")
                                      for parent in path.resolve().parents})
            policy["processContainer"] = {"filesystem": {"enumeratePaths": enumerate_paths}}
            report["enumerate_metadata_paths"] = enumerate_paths
            secrets = []
            for outside_parent in (base.parent, tools.parent):
                outside = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix="language-sandbox-v3-outside-", dir=outside_parent)))
                assert outside.resolve().parent == outside_parent.resolve() and outside.name.startswith("language-sandbox-v3-outside-")
                secret = outside / "outside-sentinel.txt"
                secret.write_text("outside-content-must-not-be-readable\n", encoding="utf-8", newline="\n")
                secrets.append((secret, sha(secret)))
            source = item.root / item.source
            source_hash = sha(source)
            positive = (writable[0] if writable else scratch) / "native-positive-write.txt"
            listener = resources.enter_context(socket.socket(socket.AF_INET, socket.SOCK_STREAM))
            listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(.1)
            accepted, stop = [], threading.Event()
            resources.callback(stop.set)
            def listen():
                while not stop.is_set():
                    try:
                        connection, _ = listener.accept(); accepted.append(True); connection.close()
                    except socket.timeout:
                        continue
                    except OSError:
                        break
            thread = threading.Thread(target=listen, daemon=True); thread.start()
            support = base / "fixed-boundary-check.mjs"
            boundary_source = (
                "import fs from 'node:fs';\nimport net from 'node:net';\nimport {createHash} from 'node:crypto';\n"
                f"const source={json.dumps(str(source))};const outside={json.dumps([str(path) for path, _ in secrets])};\n"
                f"const positive={json.dumps(str(positive))};const policy={json.dumps(str(policy_path))};\n"
                "const checks=[];const denied=(name,action)=>{try{action();checks.push({name,status:'FAIL'});}catch(error){checks.push({name,status:'PASS',error:error.code});}};\n"
                f"checks.push({{name:'permitted_source_read',status:createHash('sha256').update(fs.readFileSync(source)).digest('hex')==={json.dumps(source_hash)}?'PASS':'FAIL'}});\n"
                "fs.writeFileSync(positive,'explicit-private-write');checks.push({name:'explicit_writable_positive',status:fs.readFileSync(positive,'utf8')==='explicit-private-write'?'PASS':'FAIL'});\n"
                "for(const path of outside){const drive=path.slice(0,1);denied('outside_content_read_'+drive,()=>fs.readFileSync(path));denied('outside_overwrite_'+drive,()=>fs.writeFileSync(path,'denied'));}\n"
                "denied('readonly_source_overwrite',()=>fs.writeFileSync(source,'denied'));\n"
                "denied('readonly_source_create',()=>fs.writeFileSync(source+'.forbidden','denied'));\n"
                "denied('protected_policy_read',()=>fs.readFileSync(policy));\n"
                f"for(const path of {json.dumps([p for p in enumerate_paths if Path(p).parent == Path(p)])}){{checks.push({{name:'volume_metadata_query_'+path.slice(0,1),status:fs.statSync(path).isDirectory()?'PASS':'FAIL'}});}}\n"
                f"const loopbackDenied=await new Promise(resolve=>{{const socket=net.createConnection({{host:'127.0.0.1',port:{listener.getsockname()[1]}}});socket.once('connect',()=>{{socket.destroy();resolve(false);}});socket.once('error',()=>resolve(true));socket.setTimeout(700,()=>{{socket.destroy();resolve(true);}});}});\n"
                "checks.push({name:'loopback_connection_denied',status:loopbackDenied?'PASS':'FAIL'});\n"
                "console.log(JSON.stringify({checks}));process.exit(checks.every(c=>c.status==='PASS')?0:1);\n"
            )
            support.write_text(boundary_source, encoding="utf-8", newline="\n")
            node = Path("C:/Program Files/nodejs/node.exe")
            boundary_policy = json.loads(json.dumps(policy))
            boundary_policy["filesystem"]["readonlyPaths"] += [str(node.parent), str(support)]
            boundary_path = policies / "boundary-policy.json"
            boundary_path.write_text(json.dumps(boundary_policy), encoding="utf-8", newline="\n")
            boundary_command = [str(executor), "--config", str(boundary_path), "--", str(node), "--preserve-symlinks", "--preserve-symlinks-main", str(support)]
            boundary = subprocess.run(boundary_command, cwd=policies, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=35)
            stop.set(); thread.join(timeout=1)
            report["metadata_boundary"] = {"command": boundary_command, "policy_sha256": sha(boundary_path),
                "source": boundary_source, "source_sha256": sha(support), "exit_code": boundary.returncode,
                "stdout": boundary.stdout, "stderr": boundary.stderr,
                "outside_host_hash_unchanged": all(sha(path) == expected for path, expected in secrets),
                "outside_drive_host_hashes": {path.drive: {"path": str(path), "unchanged": sha(path) == expected, "sha256": expected} for path, expected in secrets},
                "readonly_source_host_hash_unchanged": sha(source) == source_hash,
                "host_loopback_accepts": len(accepted),
                "positive_host_write_verified": positive.read_text(encoding="utf-8") == "explicit-private-write" if positive.exists() else False}
            if (boundary.returncode or any(sha(path) != expected for path, expected in secrets) or sha(source) != source_hash or accepted
                    or report["metadata_boundary"]["positive_host_write_verified"] is not True):
                report["status"] = "METADATA_BOUNDARY_FAILED"
                args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
                return 0
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
