"""Real Odin MCP -> installed LSP acceptance. Fixture sources, never mock servers.

Python is a development harness dependency, not an Odin runtime dependency.
Reports are immutable by default; choose a fresh --output for every attempt.
"""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

from protocol_harness import Peer


LANGUAGES = ("rust", "python", "typescript", "go", "java", "csharp")
IDE_CAPABILITIES = {
    "completion": "completionProvider", "hover": "hoverProvider",
    "signature_help": "signatureHelpProvider", "call_hierarchy": "callHierarchyProvider",
    "inlay_hints": "inlayHintProvider", "semantic_tokens": "semanticTokensProvider",
    "format_document": "documentFormattingProvider", "workspace_symbols": "workspaceSymbolProvider",
}
RUNTIME_ENV = (
    "PATH", "SYSTEMROOT", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "HOME",
    "USERPROFILE", "TEMP", "TMP", "APPDATA", "LOCALAPPDATA", "RUSTUP_HOME",
    "CARGO_HOME", "JAVA_HOME", "LANG", "LC_ALL",
    "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "ALLUSERSPROFILE",
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def toml(value):
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{json.dumps(k)} = {toml(v)}" for k, v in value.items()) + " }"
    if isinstance(value, list):
        return "[" + ", ".join(toml(v) for v in value) + "]"
    return json.dumps(value, ensure_ascii=False)


def position(text, needle, offset=0):
    index = text.rindex(needle) + offset
    prefix = text[:index]
    return {"line": prefix.count("\n"), "character": len(prefix.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2}


def payload(value):
    return value.get("result") if isinstance(value, dict) and "result" in value else value


def isolation_routes(value):
    if isinstance(value, dict):
        if "launch_route" in value:
            yield value
        for child in value.values():
            yield from isolation_routes(child)
    elif isinstance(value, list):
        for child in value:
            yield from isolation_routes(child)


class BackendError(RuntimeError):
    def __init__(self, response):
        self.response = response
        super().__init__(json.dumps(response, ensure_ascii=False))


@dataclass
class Fixture:
    language: str
    project: str
    root: Path
    source: str
    consumer: str
    original: str
    consumer_original: str
    broken: str
    program: Path
    argv: list[str]
    initialization: dict
    settings: dict

    def hashes(self):
        return {name: digest((self.root / name).read_bytes()) for name in (self.source, self.consumer)}


def installed(tools, name, fallback=None):
    candidates = list(tools.glob(name))
    if not candidates and fallback:
        executable = shutil.which(fallback)
        if executable:
            candidates = [Path(executable)]
    if len(candidates) != 1 or not candidates[0].is_file():
        raise FileNotFoundError(f"Expected exactly one installed backend matching {name}")
    return candidates[0].resolve()


def fixture(base, tools, language, tag):
    project = f"{language}_{tag}"
    root = base / "projects" / project
    root.mkdir(parents=True)
    initialization, settings, extra = {}, {}, {}
    if language == "rust":
        source, consumer = "src/lib.rs", "src/consumer.rs"
        text = 'mod consumer;\npub fn greet(name:&str)->String{format!("Hi {}",name)}\npub fn run()->String{let message=greet("Odin");message}\n'
        caller = 'pub fn consume()->String{crate::greet("Caller")}\n'
        broken = "\npub fn odin_broken( {\n"
        program, argv = installed(tools, "no-installed-rust-proxy", "rust-analyzer"), []
        extra["Cargo.toml"] = f'[package]\nname="odin-ide-{tag}"\nversion="0.1.0"\nedition="2024"\n[workspace]\n'
        extra["Cargo.lock"] = f'version = 4\n\n[[package]]\nname = "odin-ide-{tag}"\nversion = "0.1.0"\n'
        initialization = {"cargo": {"buildScripts": {"enable": False}, "targetDir": str(base / "cache" / project)},
                          "procMacro": {"enable": False}, "checkOnSave": False}
    elif language == "python":
        source, consumer = "sample.py", "consumer.py"
        text = 'def greet(name: str) -> str:\n    return f"Hi {name}"\n\ndef run() -> str:\n    message = greet("Odin")\n    return message\n'
        caller = 'from sample import greet\n\nmessage = greet("Caller")\n'
        broken = "\ndef odin_broken(:\n"
        program, argv = installed(tools, "python/Scripts/basedpyright-langserver.exe"), ["--stdio"]
        settings = {"python": {"pythonPath": str(installed(tools, "python/Scripts/python.exe"))},
                    "basedpyright": {"analysis": {"typeCheckingMode": "standard", "inlayHints": {
                        "variableTypes": True, "functionReturnTypes": True, "callArgumentNames": True}}}}
    elif language == "typescript":
        source, consumer = "sample.ts", "consumer.ts"
        text = 'export function greet(name:string):string{return `Hi ${name}`;}\nexport function run():string{const message=greet("Odin");return message;}\n'
        caller = 'import { greet } from "./sample";\nexport const message=greet("Caller");\n'
        broken = '\nexport const odin_broken: number = "wrong";\n'
        program = Path(shutil.which("node") or "C:/Program Files/nodejs/node.exe").resolve()
        argv = ["--preserve-symlinks", "--preserve-symlinks-main", str(installed(tools, "typescript/node_modules/typescript-language-server/lib/cli.mjs")), "--stdio"]
        initialization = {"tsserver": {"path": str(installed(tools, "typescript/node_modules/typescript/lib/tsserver.js"))},
                          "preferences": {"includeInlayParameterNameHints": "all", "includeInlayVariableTypeHints": True,
                                          "includeInlayFunctionLikeReturnTypeHints": True}}
        extra["tsconfig.json"] = '{"compilerOptions":{"strict":true,"noEmit":true},"include":["*.ts"]}\n'
    elif language == "go":
        source, consumer = "sample.go", "consumer.go"
        text = 'package fixture\nfunc greet(name string) string{return "Hi "+name}\nfunc run() string{message:=greet("Odin");return message}\n'
        caller = 'package fixture\nfunc consume() string{return greet("Caller")}\n'
        broken = "\nfunc odin_broken( {\n"
        program, argv = installed(tools, "go-work/bin/gopls.exe"), []
        extra["go.mod"] = f"module odin.local/ide-{tag}\n\ngo 1.27.0\n"
        settings = {"gopls": {"semanticTokens": True, "hints": {"assignVariableTypes": True, "parameterNames": True, "constantValues": True}}}
        initialization = dict(settings["gopls"])
    elif language == "java":
        source, consumer = "src/fixture/Greeter.java", "src/fixture/Consumer.java"
        text = 'package fixture;\npublic class Greeter {\npublic static String greet(String name){return "Hi "+name;}\npublic static String run(){String message=greet("Odin");return message;}\n}\n'
        caller = 'package fixture;\npublic class Consumer {public static String consume(){return Greeter.greet("Caller");}}\n'
        broken = "\nclass Broken { void odin_broken( }\n"
        program = installed(tools, "jdk/*/bin/java.exe")
        launcher = installed(tools, "jdtls/plugins/org.eclipse.equinox.launcher_*.jar")
        cache = base / "cache" / project
        cache.mkdir(parents=True)
        argv = ["-Declipse.application=org.eclipse.jdt.ls.core.id1", "-Dosgi.bundles.defaultStartLevel=4",
                "-Declipse.product=org.eclipse.jdt.ls.core.product", "-Dlog.level=INFO", "-Xmx1G",
                "--add-modules=ALL-SYSTEM", "--add-opens", "java.base/java.util=ALL-UNNAMED",
                "--add-opens", "java.base/java.lang=ALL-UNNAMED", "-jar", str(launcher),
                "-configuration", str(tools / "jdtls/config_win"), "-data", str(cache)]
        extra[".project"] = f'<?xml version="1.0"?><projectDescription><name>{project}</name><comment/><projects/><buildSpec><buildCommand><name>org.eclipse.jdt.core.javabuilder</name><arguments/></buildCommand></buildSpec><natures><nature>org.eclipse.jdt.core.javanature</nature></natures></projectDescription>'
        extra[".classpath"] = '<?xml version="1.0"?><classpath><classpathentry kind="src" path="src"/><classpathentry kind="con" path="org.eclipse.jdt.launching.JRE_CONTAINER"/><classpathentry kind="output" path="bin"/></classpath>'
        settings = {"java": {"signatureHelp": {"enabled": True}, "import": {"maven": {"enabled": False}, "gradle": {"enabled": False}},
                             "inlayHints": {"parameterNames": {"enabled": "all"}}}}
        initialization = {"settings": settings}
    else:
        source, consumer = "Greeter.cs", "Consumer.cs"
        text = 'namespace Fixture;\npublic static class Greeter {\npublic static string greet(string name){return "Hi "+name;}\npublic static string run(){var message=greet("Odin");return message;}\n}\n'
        caller = 'namespace Fixture;\npublic static class Consumer {public static string consume(){return Greeter.greet("Caller");}}\n'
        broken = "\nclass Broken { void odin_broken( }\n"
        program, argv = installed(tools, "csharp-ls/csharp-ls.exe"), []
        extra["Fixture.csproj"] = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable></PropertyGroup></Project>\n'
        extra["NuGet.Config"] = '<configuration><packageSources><clear/></packageSources></configuration>\n'
    for name, content in {source: text, consumer: caller, **extra}.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8", newline="\n")
    return Fixture(language, project, root, source, consumer, text, caller, broken, program, argv, initialization, settings)


class Run:
    def __init__(self, args, base, report):
        self.args, self.base, self.report = args, base, report
        self.transcript = args.output.with_suffix(".requests.jsonl").open("x", encoding="utf-8", newline="\n")
        self.peer = None
        self.current = None
        self.catalog = {}

    def sanitize(self, value):
        text = json.dumps(value, ensure_ascii=False)
        for actual, label in ((str(self.base), "<fixture>"), (str(self.args.tools_root), "<tools_root>")):
            text = text.replace(json.dumps(actual)[1:-1], label)
            text = text.replace(actual.replace("\\", "/"), label)
        return json.loads(text)

    def record(self, name, status, detail=None):
        item = {"name": name, "status": status}
        if detail is not None:
            item["detail"] = self.sanitize(detail)
        self.current["checks"].append(item)
        print(f'{self.current["project"]}:{name}:{status}', flush=True)
        return status == "PASS"

    def require(self, name, condition, detail=None):
        if not self.record(name, "PASS" if condition else "FAIL", detail):
            raise AssertionError(name)

    def call(self, project, tool, arguments=None):
        started = time.monotonic()
        response = self.peer.request("tools/call", {"name": tool, "arguments": dict(arguments or {}, project=project)}, timeout=self.args.timeout + 10)
        entry = {"project": project, "tool": tool, "arguments": arguments or {},
                 "elapsed_seconds": round(time.monotonic() - started, 3), "response": response}
        self.transcript.write(json.dumps(self.sanitize(entry), ensure_ascii=False) + "\n")
        self.transcript.flush()
        if "error" in response:
            raise BackendError(response["error"])
        result = response["result"]
        body = result.get("structuredContent") or json.loads(result["content"][0]["text"])
        if result.get("isError"):
            raise BackendError(body)
        return body

    def retry(self, operation, predicate, seconds=None):
        deadline = time.monotonic() + (self.args.timeout if seconds is None else seconds)
        last = None
        errors = []
        while True:
            try:
                last = operation()
                if predicate(last):
                    return last
            except BackendError as error:
                errors.append(error.response)
                if any(marker in str(error) for marker in ("CAPABILITY_UNAVAILABLE", "WorkspaceEdit must", "INVALID_ARGUMENT", "PERMISSION_DENIED", "Unknown tool", "Backend process exited")):
                    raise
            if time.monotonic() >= deadline:
                raise TimeoutError(json.dumps({"last_result": last, "attempt_errors": errors[-4:]}))
            time.sleep(.3)

    def envelope(self, value, fixture):
        return (value.get("source") == "lsp" and value.get("freshness") == "current"
                and value.get("position_encoding") == "utf-16"
                and value.get("document", {}).get("sha256") == digest((fixture.root / fixture.source).read_bytes()))

    def optional(self, fixture, capabilities, tool, arguments, meaningful, alternatives=()):
        if tool not in self.catalog:
            self.record(tool, "FAIL", "Odin tool missing from live catalog")
            return None
        capability = capabilities.get(IDE_CAPABILITIES[tool])
        if capability is None or capability is False:
            self.record(tool, "UNAVAILABLE", {"capability": IDE_CAPABILITIES[tool], "advertised": capability})
            return None
        before, observed = fixture.hashes(), None
        def query():
            nonlocal observed
            for parameters in (arguments, *alternatives):
                observed = self.call(fixture.project, tool, parameters)
                if meaningful(observed):
                    break
            return observed
        try:
            value = self.retry(query, meaningful, seconds=min(12, self.args.timeout))
            fresh = self.envelope(value, fixture) if tool != "workspace_symbols" else value.get("freshness") == "current"
            fresh = fresh and fixture.hashes() == before
            self.record(tool, "PASS" if fresh else "FAIL", {"provider": value.get("provider"), "source": value.get("source"),
                        "result_count": len(payload(value)) if isinstance(payload(value), (list, dict)) else None})
            return value
        except Exception as error:
            if (tool == "inlay_hints" and observed and payload(observed) == []
                    and self.envelope(observed, fixture) and fixture.hashes() == before):
                self.record(tool, "NOT_VERIFIED", "Backend advertises hints but returned an empty list for the enabled fixture")
                return observed
            self.record(tool, "FAIL", str(error))
            return None

    def prepare_edit(self, fixture, text):
        preview = self.call(fixture.project, "transaction_prepare", {"changes": [{"path": fixture.source,
                            "expected_sha256": digest((fixture.root / fixture.source).read_bytes()), "content": text}]})
        self.call(fixture.project, "transaction_apply", {"transaction_id": preview["transaction_id"]})
        return preview["transaction_id"]

    def diagnostics(self, fixture, name, errors_expected):
        try:
            value = self.retry(lambda: self.call(fixture.project, "get_diagnostics_for_file", {"relative_path": fixture.source}),
                               lambda v: v.get("available") is True and ((v.get("summary", {}).get("errors", 0) > 0) == errors_expected),
                               seconds=min(18, self.args.timeout))
            current = value.get("version_verified") is True and value.get("summary", {}).get("complete") is True
            self.record(name, "PASS" if current else "NOT_VERIFIED", value)
        except Exception as error:
            self.record(name, "FAIL", str(error))

    def project(self, fixture, fixtures, config, environment):
        self.peer = None
        self.current = {"language": fixture.language, "project": fixture.project, "checks": [],
                        "source_sha256": fixture.hashes(), "isolation": self.args.mode}
        self.report["projects"].append(self.current)
        try:
            if fixture.language == "csharp":
                result = subprocess.run([str(self.args.tools_root / "dotnet/dotnet.exe"), "restore", "Fixture.csproj", "--configfile", "NuGet.Config"],
                                        cwd=fixture.root, env=environment, capture_output=True, text=True, encoding="utf-8", timeout=self.args.timeout)
                self.current["restore"] = {"exit_code": result.returncode, "stdout": self.sanitize(result.stdout), "stderr": self.sanitize(result.stderr)}
                self.require("fixture_offline_restore", result.returncode == 0)
            with Peer([str(self.args.binary), "--config", str(config), "serve"], env=environment) as self.peer:
                initialized = self.peer.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "odin-real-ide", "version": "1"}}, timeout=20)
                self.require("mcp_initialize", initialized.get("result", {}).get("serverInfo", {}).get("name") == "odin")
                self.peer.notify("notifications/initialized")
                self.catalog = {tool["name"]: tool for tool in self.peer.request("tools/list")["result"]["tools"]}
                self.current["binary_version"] = self.call(fixture.project, "get_version")
                probe = self.call(fixture.project, "get_capabilities", {"probe": True, "language": fixture.language})
                self.current["capabilities"] = probe
                language = next(item for item in probe["languages"] if item["id"] == fixture.language)
                self.require("actual_lsp_initialize", language.get("ready") is True, language)
                if self.args.mode == "sandbox":
                    routes = list(isolation_routes(language))
                    self.require("actual_native_sandbox_route", any(route.get("requested") is True
                                 and route.get("launch_route") == "mxc_process_container"
                                 and route.get("hash_verified") is True
                                 and route.get("executor_sha256", "").lower() == self.args.sandbox_sha256.lower()
                                 for route in routes), routes)
                capabilities = language["capabilities"]
                symbols = self.retry(lambda: self.call(fixture.project, "get_symbols_overview", {"relative_path": fixture.source}),
                                     lambda v: "greet" in json.dumps(v))
                matching = [s for s in symbols["symbols"] if s["name"] == "greet" or s["name"].startswith("greet(")]
                self.require("document_symbols", len(matching) == 1, matching)
                name_path = matching[0]["name_path"]
                self.call(fixture.project, "get_symbols_overview", {"relative_path": fixture.consumer})
                references = self.retry(lambda: self.call(fixture.project, "find_referencing_symbols", {"relative_path": fixture.source, "name_path": name_path}),
                                        lambda v: Path(fixture.consumer).name in json.dumps(v))
                self.require("cross_file_references", Path(fixture.consumer).name in json.dumps(references), references)
                content = (fixture.root / fixture.source).read_text(encoding="utf-8")
                at_call = position(content, 'greet("Odin")', 1)
                completion_at = position(content, 'greet("Odin")', 3)
                signature_at = position(content, 'greet("Odin")', len("greet("))
                completions = self.optional(fixture, capabilities, "completion", {"relative_path": fixture.source, "position": completion_at, "limit": 200},
                                            lambda v: any("greet" in x.get("label", "").lower() for x in (payload(v).get("items", []) if isinstance(payload(v), dict) else payload(v) or [])))
                self.optional(fixture, capabilities, "hover", {"relative_path": fixture.source, "position": at_call},
                              lambda v: bool(payload(v)) and "greet" in json.dumps(payload(v)).lower())
                self.optional(fixture, capabilities, "signature_help", {"relative_path": fixture.source, "position": signature_at},
                              lambda v: isinstance(payload(v), dict) and bool(payload(v).get("signatures")),
                              alternatives=({"relative_path": fixture.source, "position": position(content, 'greet("Odin")', len('greet("Odin")') - 1)},))
                self.optional(fixture, capabilities, "workspace_symbols", {"language": fixture.language, "query": "greet", "limit": 100},
                              lambda v: "greet" in json.dumps(payload(v)).lower())
                hints = self.optional(fixture, capabilities, "inlay_hints", {"relative_path": fixture.source}, lambda v: bool(payload(v)))
                self.optional(fixture, capabilities, "semantic_tokens", {"relative_path": fixture.source},
                              lambda v: isinstance(payload(v), dict) and bool(payload(v).get("data")))
                definition = matching[0].get("selection_range", matching[0].get("selectionRange", matching[0]["range"]))["start"]
                self.optional(fixture, capabilities, "call_hierarchy", {"relative_path": fixture.source, "position": definition, "direction": "incoming"},
                              lambda v: any(item.get("related") for item in payload(v) or []))
                # Resolve is inert data, hash-bound, and only tested when advertised.
                for tool, provider, value, key in (("completion_resolve", "completionProvider", completions, "item"),
                                                   ("inlay_hint_resolve", "inlayHintProvider", hints, "hint")):
                    if not isinstance(capabilities.get(provider), dict) or capabilities[provider].get("resolveProvider") is not True:
                        self.record(tool, "UNAVAILABLE", provider + ".resolveProvider")
                        continue
                    values = payload(value) if value else None
                    if isinstance(values, dict):
                        values = values.get("items")
                    if not values:
                        self.record(tool, "NOT_RUN", "No originating item was available")
                        continue
                    try:
                        before_resolve = fixture.hashes()
                        selected = next((value for value in values if "greet" in value.get("label", "").lower()), values[0]) if key == "item" else values[0]
                        resolved = self.call(fixture.project, tool, {"relative_path": fixture.source, key: selected, "expected_sha256": digest((fixture.root / fixture.source).read_bytes())})
                        self.record(tool, "PASS" if self.envelope(resolved, fixture) and fixture.hashes() == before_resolve else "FAIL")
                    except Exception as error:
                        self.record(tool, "FAIL", str(error))
                # Observe the initial clean push before mutations invalidate it.
                # TSLS can suppress repeated empty pushes after apply/rollback.
                self.diagnostics(fixture, "diagnostics_valid", False)
                self.rename(fixture, fixtures, name_path)
                self.format(fixture, capabilities)
                original = (fixture.root / fixture.source).read_text(encoding="utf-8")
                transaction = self.prepare_edit(fixture, original + fixture.broken)
                try:
                    self.diagnostics(fixture, "diagnostics_error", True)
                finally:
                    self.call(fixture.project, "transaction_rollback", {"transaction_id": transaction})
                self.require("diagnostic_fixture_rollback", (fixture.root / fixture.source).read_bytes() == original.encode())
                self.diagnostics(fixture, "diagnostics_recovered", False)
                self.current["final_lsp_status"] = self.call(fixture.project, "get_capabilities", {"probe": True, "language": fixture.language})
                self.current["stderr"] = self.sanitize(self.peer.errors)
        except Exception as error:
            self.record("project_execution", "FAIL", str(error))
            if self.peer:
                self.current["stderr"] = self.sanitize(self.peer.errors)
        self.current["status"] = aggregate(self.current["checks"])

    def rename(self, fixture, fixtures, name_path):
        before = {item.project: item.hashes() for item in fixtures}
        transaction = None
        applied = False
        try:
            preview = self.retry(lambda: self.call(fixture.project, "rename_symbol", {"relative_path": fixture.source, "name_path": name_path, "new_name": "salute"}),
                                 lambda value: bool(value.get("files")))
            transaction = preview["transaction_id"]
            self.require("rename_preview_readonly", all(item.hashes() == before[item.project] for item in fixtures))
            self.call(fixture.project, "transaction_apply", {"transaction_id": transaction})
            applied = True
            self.require("rename_definition_and_consumer", all("salute" in (fixture.root / path).read_text(encoding="utf-8")
                         and "greet(" not in (fixture.root / path).read_text(encoding="utf-8") for path in (fixture.source, fixture.consumer)))
            self.require("rename_other_projects_unchanged", all(item.hashes() == before[item.project] for item in fixtures if item.project != fixture.project))
        except Exception as error:
            self.record("rename_execution", "FAIL", str(error))
        finally:
            if applied:
                self.call(fixture.project, "transaction_rollback", {"transaction_id": transaction})
                self.require("rename_rollback", fixture.hashes() == before[fixture.project])

    def format(self, fixture, capabilities):
        if capabilities.get("documentFormattingProvider") is None or capabilities.get("documentFormattingProvider") is False:
            self.record("format_transaction", "UNAVAILABLE", "documentFormattingProvider")
            return
        transaction, applied = None, False
        before = (fixture.root / fixture.source).read_bytes()
        try:
            preview = self.call(fixture.project, "format_document", {"relative_path": fixture.source, "expected_sha256": digest(before),
                                "options": {"tab_size": 4, "insert_spaces": True}})
            self.require("format_preview_readonly", (fixture.root / fixture.source).read_bytes() == before)
            transaction = preview.get("transaction_id")
            if not transaction:
                self.record("format_transaction", "NOT_VERIFIED", {"reason": "No nonempty formatting edit for intentionally unformatted fixture", "response": preview})
                return
            self.call(fixture.project, "transaction_apply", {"transaction_id": transaction})
            applied = True
            self.require("format_transaction", (fixture.root / fixture.source).read_bytes() != before)
        except Exception as error:
            self.record("format_transaction", "FAIL", str(error))
        finally:
            if applied:
                self.call(fixture.project, "transaction_rollback", {"transaction_id": transaction})
                self.require("format_rollback", (fixture.root / fixture.source).read_bytes() == before)


def aggregate(checks):
    statuses = {item["status"] for item in checks}
    return "FAIL" if "FAIL" in statuses else "PARTIAL" if statuses & {"NOT_RUN", "NOT_VERIFIED"} else "PASS"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tools-root", type=Path, default=Path("E:/odin/.odin/validation-tools"))
    parser.add_argument("--languages", nargs="+", choices=LANGUAGES, default=list(LANGUAGES))
    parser.add_argument("--mode", choices=("baseline", "sandbox"), default="baseline")
    parser.add_argument("--sandbox-executor", type=Path)
    parser.add_argument("--sandbox-sha256")
    parser.add_argument("--sandbox-allow-ui", action="store_true")
    parser.add_argument("--sandbox-typescript-init-temp", action="store_true",
                        help="Use a fixed host-owned bootstrap to initialize MXC's private temp before the pinned TSLS module")
    parser.add_argument("--sandbox-typescript-child-path-flags", action="store_true",
                        help="Set only fixed preserve-symlinks NODE_OPTIONS for the sandboxed TSLS descendant processes")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    args.binary, args.output, args.tools_root = args.binary.resolve(), args.output.resolve(), args.tools_root.resolve()
    if args.output.exists() or args.output.with_suffix(".requests.jsonl").exists():
        parser.error("Output already exists; preserve it and choose a fresh report path")
    if not args.binary.is_file():
        parser.error("Installed Odin binary does not exist")
    if not 1 <= args.timeout <= 300:
        parser.error("Timeout must be between 1 and 300 seconds")
    if args.mode == "sandbox" and (not args.sandbox_executor or not args.sandbox_sha256):
        parser.error("Sandbox mode requires explicit trusted --sandbox-executor and --sandbox-sha256")
    if args.mode == "sandbox" and set(args.languages) - {"rust", "python", "typescript"}:
        parser.error("Select an explicitly tested sandbox language with --languages rust/python/typescript")
    if args.sandbox_typescript_init_temp and (args.mode != "sandbox" or args.languages != ["typescript"]):
        parser.error("The fixed TypeScript temp bootstrap requires --mode sandbox --languages typescript")
    if args.sandbox_typescript_child_path_flags and not args.sandbox_typescript_init_temp:
        parser.error("Fixed TypeScript child path flags require --sandbox-typescript-init-temp")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    report = {"scope": "REAL_ODIN_MCP_SIX_LANGUAGE_IDE", "mode": args.mode, "status": "NOT_RUN", "platform": sys.platform,
              "binary_sha256": digest(args.binary.read_bytes()), "harness_sha256": digest(Path(__file__).read_bytes()),
              "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "projects": [], "setup_errors": [],
              "boundary": "Baseline profiles explicitly run on the host without OS isolation" if args.mode == "baseline" else
                          "Explicit MXC profile; actual LSP results and runtime route recorded, not full OS/UI parity"}
    with tempfile.TemporaryDirectory(prefix="odin-language-ide-") as temporary:
        base = Path(temporary)
        environment = {key: os.environ[key] for key in RUNTIME_ENV if key in os.environ}
        environment.update({"DOTNET_ROOT": str(args.tools_root / "dotnet"), "DOTNET_CLI_HOME": str(args.tools_root / "dotnet-home"),
                            "NUGET_PACKAGES": str(args.tools_root / "nuget"), "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
                            "DOTNET_GENERATE_ASPNET_CERTIFICATE": "false", "GOTELEMETRY": "off", "GOTOOLCHAIN": "local",
                            "GOPATH": str(args.tools_root / "go-work"), "GOCACHE": str(args.tools_root / "go-cache"),
                            "GOMODCACHE": str(args.tools_root / "go-modules"), "GOROOT": str(args.tools_root / "go/go")})
        environment["DOTNET_SKIP_FIRST_TIME_EXPERIENCE"] = "1"
        environment["DOTNET_CLI_UI_LANGUAGE"] = "en-US"
        if args.mode == "sandbox" and "rust" in args.languages:
            environment.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
            environment.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
        jdks = list(args.tools_root.glob("jdk/*/bin/java.exe"))
        if len(jdks) == 1:
            environment["JAVA_HOME"] = str(jdks[0].parent.parent)
        environment["PATH"] = os.pathsep.join([str(args.tools_root / "go/go/bin"), str(args.tools_root / "dotnet"),
                                               str(jdks[0].parent) if len(jdks) == 1 else "", environment.get("PATH", "")])
        fixtures = []
        for language in args.languages:
            for tag in ("a", "b"):
                try:
                    fixtures.append(fixture(base, args.tools_root, language, tag))
                except Exception as error:
                    report["setup_errors"].append({"language": language, "project": f"{language}_{tag}", "status": "FAIL", "error": str(error)})
        lines = [f"state_dir = {toml(str(base / 'state'))}", "[limits]", f"rpc_timeout_secs = {int(args.timeout)}"]
        extensions = {"rust": "rs", "python": "py", "typescript": "ts", "go": "go", "java": "java", "csharp": "cs"}
        for item in fixtures:
            bootstrap = None
            if args.sandbox_typescript_init_temp:
                entry = installed(args.tools_root, "typescript/node_modules/typescript-language-server/lib/cli.mjs")
                entry_sha256 = digest(entry.read_bytes())
                bootstrap = base / "host-support" / item.project / "typescript-bootstrap.mjs"
                bootstrap.parent.mkdir(parents=True)
                bootstrap_source = (
                    "import { mkdirSync, readFileSync } from 'node:fs';\n"
                    "import { tmpdir } from 'node:os';\n"
                    "import { pathToFileURL } from 'node:url';\n"
                    "import { createHash } from 'node:crypto';\n"
                    f"const server = {json.dumps(str(entry))};\n"
                    f"const expectedSha256 = {json.dumps(entry_sha256)};\n"
                    "mkdirSync(tmpdir(), { recursive: true });\n"
                    "if (createHash('sha256').update(readFileSync(server)).digest('hex') !== expectedSha256) {\n"
                    "  throw new Error('Trusted TypeScript server module hash mismatch');\n"
                    "}\n"
                )
                fixed_node_options = "--preserve-symlinks --preserve-symlinks-main" if args.sandbox_typescript_child_path_flags else None
                if fixed_node_options:
                    bootstrap_source += f"process.env.NODE_OPTIONS = {json.dumps(fixed_node_options)};\n"
                bootstrap_source += "process.argv = [process.execPath, server, '--stdio'];\nawait import(pathToFileURL(server).href);\n"
                bootstrap.write_text(bootstrap_source, encoding="utf-8", newline="\n")
                item.argv = ["--preserve-symlinks", "--preserve-symlinks-main", str(bootstrap)]
                report.setdefault("host_support", []).append({
                    "project": item.project, "kind": "explicit_fixed_typescript_private_temp_bootstrap",
                    "relative_path": bootstrap.relative_to(base).as_posix(), "source": bootstrap_source,
                    "source_sha256": digest(bootstrap.read_bytes()), "server_module": str(entry),
                    "server_module_sha256": entry_sha256, "server_argv": [str(item.program), str(entry), "--stdio"],
                    "fixed_descendant_node_options": fixed_node_options,
                    "caller_or_repository_target": False, "fallback": False,
                })
            lines += [f"[projects.{item.project}]", f"root = {toml(str(item.root))}", "allow_write = true", "allow_debug = false", "allow_shell = false",
                      f"[[projects.{item.project}.languages]]", f"id = {toml(item.language)}", f"extensions = [{toml(extensions[item.language])}]",
                      f"program = {toml(str(item.program))}", f"args = {toml(item.argv)}", f"initialization_options = {toml(item.initialization)}", f"settings = {toml(item.settings)}"]
            if args.mode == "sandbox":
                scratch = base / "scratch" / item.project
                scratch.mkdir(parents=True)
                grants = [item.root, item.program.parent]
                if item.language == "typescript":
                    grants.append(args.tools_root / "typescript")
                    if bootstrap:
                        grants.append(bootstrap)
                elif item.language == "python":
                    grants.append(args.tools_root / "python")
                    grants.append(Path((args.tools_root / "python/pyvenv.cfg").read_text(encoding="utf-8").split("home = ", 1)[1].splitlines()[0]))
                elif item.language == "rust":
                    grants += [Path(os.environ["RUSTUP_HOME"]) if "RUSTUP_HOME" in os.environ else Path.home() / ".rustup", Path.home() / ".cargo"]
                unique_grants = sorted({str(path.resolve()) for path in grants if path.exists()})
                report.setdefault("sandbox_profiles", []).append({
                    "project": item.project, "program": str(item.program),
                    "argv": [arg.replace(str(base), "<fixture>") for arg in item.argv],
                    "readonly_paths": [path.replace(str(base), "<fixture>") for path in unique_grants],
                    "readwrite_paths": [], "private_scratch": str(scratch.relative_to(base)),
                    "allow_ui": args.sandbox_allow_ui, "clipboard": "none", "injection": False,
                    "requested_network_policy": "deny_all", "bootstrap_is_readonly_file": bootstrap is not None,
                    "fixed_descendant_node_options": fixed_node_options if bootstrap else None,
                })
                lines += [f"[projects.{item.project}.sandbox]", f"executor = {toml(str(args.sandbox_executor.resolve()))}",
                          f"executor_sha256 = {toml(args.sandbox_sha256)}", f"readonly_paths = {toml(unique_grants)}",
                          "readwrite_paths = []", "denied_paths = []", f"temp_root = {toml(str(scratch))}", f"allow_ui = {toml(args.sandbox_allow_ui)}"]
        config = base / "host.toml"
        config.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
        report["host_config_sha256"] = digest(config.read_bytes())
        run = Run(args, base, report)
        try:
            try:
                launch = subprocess.run([str(args.binary), "--version"], env=environment, capture_output=True,
                                        encoding="utf-8", errors="replace", timeout=20)
                report["binary_launch"] = {"status": "PASS" if launch.returncode == 0 else "BLOCKED",
                    "exit_code": launch.returncode, "stdout": launch.stdout, "stderr": launch.stderr}
            except (OSError, subprocess.TimeoutExpired) as error:
                report["binary_launch"] = {"status": "BLOCKED", "error": str(error), "winerror": getattr(error, "winerror", None)}
            report["backends"] = {}
            for item in fixtures:
                if report["binary_launch"]["status"] != "PASS":
                    break
                if item.language in report["backends"]:
                    continue
                if item.language == "python":
                    command = [str(args.tools_root / "python/Scripts/python.exe"), "-c", "import importlib.metadata;print(importlib.metadata.version('basedpyright'))"]
                elif item.language == "typescript":
                    command = [str(item.program), "--preserve-symlinks", "--preserve-symlinks-main", str(installed(args.tools_root, "typescript/node_modules/typescript-language-server/lib/cli.mjs")), "--version"]
                else:
                    command = [str(item.program), "version" if item.language == "go" else "--version"]
                try:
                    version = subprocess.run(command, env=environment, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=20)
                    report["backends"][item.language] = {"program_sha256": digest(item.program.read_bytes()),
                        "version_argv": run.sanitize(command), "version_exit_code": version.returncode,
                        "version_stdout": run.sanitize(version.stdout), "version_stderr": run.sanitize(version.stderr)}
                    if item.language == "java":
                        jar = installed(args.tools_root, "jdtls/plugins/org.eclipse.jdt.ls.core_*.jar")
                        with zipfile.ZipFile(jar) as archive:
                            manifest = archive.read("META-INF/MANIFEST.MF").decode("utf-8")
                        report["backends"][item.language]["jdtls"] = {"jar_sha256": digest(jar.read_bytes()),
                            "bundle_version": next(line.partition(": ")[2] for line in manifest.splitlines() if line.startswith("Bundle-Version:"))}
                    if item.language == "typescript":
                        package = args.tools_root / "typescript/node_modules/typescript/package.json"
                        report["backends"][item.language]["typescript_version"] = json.loads(package.read_text(encoding="utf-8"))["version"]
                except Exception as error:
                    report["backends"][item.language] = {"version_status": "NOT_VERIFIED", "error": str(error)}
            for item in fixtures:
                if report["binary_launch"]["status"] == "PASS":
                    run.project(item, fixtures, config, environment)
                else:
                    report["projects"].append({"language": item.language, "project": item.project, "status": "BLOCKED", "checks": [
                        {"name": "backend_acceptance", "status": "NOT_RUN", "detail": "Candidate binary launch failed; no backend retry or OS policy change"}]})
        finally:
            run.transcript.close()
        report["counts"] = dict(Counter(check["status"] for project in report["projects"] for check in project["checks"]))
        report["matrix"] = {language: {p["project"]: p["status"] for p in report["projects"] if p["language"] == language} for language in args.languages}
        report["status"] = "FAIL" if report["setup_errors"] else aggregate([{"status": p["status"] if p["status"] != "PARTIAL" else "NOT_VERIFIED"} for p in report["projects"]])
        if report["binary_launch"]["status"] == "BLOCKED":
            report["status"] = "BLOCKED"
        report["actual_lsp_projects_initialized"] = sum(check["name"] == "actual_lsp_initialize" and check["status"] == "PASS"
                                                       for project in report["projects"] for check in project["checks"])
        report["transcript_sha256"] = digest(args.output.with_suffix(".requests.jsonl").read_bytes())
        report["line_endings_normalized_before_hash_binding"] = True
        report["odin_trace_hashes_describe_original_unredacted_payloads"] = True
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"status": report["status"], "counts": report["counts"], "matrix": report["matrix"]}), flush=True)
    return {"PASS": 0, "PARTIAL": 2, "BLOCKED": 3}.get(report["status"], 1)


if __name__ == "__main__":
    raise SystemExit(main())
