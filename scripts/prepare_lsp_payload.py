#!/usr/bin/env python3
"""Build the pinned Windows x64 LSP payload. No repository code is executed."""
from __future__ import annotations

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
import uuid
import zipfile

TARGET = "x86_64-pc-windows-msvc"
REPO = Path(__file__).resolve().parents[1]
LOCK_SHA256 = "58cb6e0a67c4283bc56e29c4cd89a4304bf587bbbba92fe4d38024d35082b4ba"
NATIVE_SOURCES_SHA256 = "bd1f02f0d3565e8e59b2ab0b7b9729dfe4949220281df93dabcd581d9a91a262"
NPM_VERSIONS = {
    "basedpyright": "1.40.2",
    "typescript-language-server": "6.0.2",
    "typescript": "6.0.3",
    "bash-language-server": "5.8.1",
    "smol-toml": "1.9.1",
    "yaml-language-server": "2.0.0",
    "@vtsls/language-server": "0.3.0",
}
NOTICE_COLLECTOR_SHA256 = "b72d7da99cbe6ac6e333f9599b15bf7aa34c248013e3adcfd99aa3a2feb2d154"
GOPLS_VERSION = "v0.24.0"
OFFICIAL_HOSTS = frozenset({
    "nodejs.org", "go.dev", "dl.google.com", "builds.dotnet.microsoft.com",
    "api.nuget.org", "github.com", "release-assets.githubusercontent.com",
    "objects.githubusercontent.com", "raw.githubusercontent.com",
    "codeload.github.com", "static.crates.io",
    "skia.googlesource.com", "chromium.googlesource.com",
})
MAX_ARCHIVE_BYTES = 2 * 1024**3
MAX_EXPANDED_BYTES = 8 * 1024**3
MAX_ARCHIVE_ENTRIES = 150_000
REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
NOTICE_PREFIXES = ("LICENSE", "LICENCE", "NOTICE", "COPYING", "COPYRIGHT", "PATENTS")


class BuildError(RuntimeError):
    """A failed precondition or build step; partial output is never reused."""


@dataclass(frozen=True)
class Artifact:
    id: str
    version: str
    filename: str
    url: str
    algorithm: str
    digest: str


ARTIFACTS = (
    Artifact("node", "24.21.0", "node-v24.21.0-win-x64.zip",
             "https://nodejs.org/dist/v24.21.0/node-v24.21.0-win-x64.zip", "sha256",
             "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541"),
    Artifact("lua-language-server", "3.19.2", "lua-language-server-3.19.2-win32-x64.zip",
             "https://github.com/LuaLS/lua-language-server/releases/download/3.19.2/lua-language-server-3.19.2-win32-x64.zip", "sha256",
             "4a3dc851b3b31e7b75a23f645d1ba4a2137579a55f5fa4edbb17a3e2a6254044"),
    Artifact("go", "1.27.3", "go1.27.3.windows-amd64.zip",
             "https://go.dev/dl/go1.27.3.windows-amd64.zip", "sha256",
             "f26081f438319757f8a1b46d7d0b2c5ec7583d4038e485e0aa725cfde7885fff"),
    Artifact("dotnet", "10.0.402", "dotnet-sdk-10.0.402-win-x64.zip",
             "https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.402/dotnet-sdk-10.0.402-win-x64.zip", "sha512",
             "ae4fd528cb7e9f6af6aaf7de36715a69f761f183744f1bb3025d19f5a92447090ec561410e46266c1d10a63781ed6f3de771760bc76c876d87daabc367b898cd"),
    Artifact("csharp-ls", "0.28.0", "csharp-ls.0.28.0.nupkg",
             "https://api.nuget.org/v3-flatcontainer/csharp-ls/0.28.0/csharp-ls.0.28.0.nupkg", "sha256",
             "8f815e4f22674106a8244beba07e4b4b062e31528e404aa8489312effb8c0e0c"),
)
CSHARP_LICENSE = Artifact(
    "csharp-ls-license", "0.28.0", "csharp-ls-0.28.0-LICENSE",
    "https://raw.githubusercontent.com/razzmatazz/csharp-language-server/72671fc7fa810c158864e72485db68e8b9b25c3d/LICENSE",
    "sha256", "2077a187c02688a6e2e816ea81f846667c50d09c2ee5c7f0f82594b452e63118",
)


def hash_file(path: Path, algorithm: str = "sha256") -> str:
    result = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def reject_reparse(path: Path) -> None:
    """Inspect the lexical path before resolve() can follow a junction."""
    current = Path(os.path.abspath(path))
    for item in (current, *current.parents):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & REPARSE_POINT:
            raise BuildError(f"Symlink/reparse path is forbidden: {item}")


def external_path(path: Path, repo: Path = REPO) -> Path:
    reject_reparse(path)
    absolute = path.absolute().resolve()
    if absolute == repo.resolve() or absolute.is_relative_to(repo.resolve()):
        raise BuildError(f"Build paths must be outside the source repository: {absolute}")
    for item in (absolute, *absolute.parents):
        if (item / ".git").exists():
            raise BuildError(f"Build paths must be outside every Git checkout: {absolute}")
    return absolute


def validate_paths(output: Path, cache: Path, receipt: Path, repo: Path = REPO) -> tuple[Path, Path, Path]:
    output, cache, receipt = (external_path(p, repo) for p in (output, cache, receipt))
    if output.exists():
        raise BuildError(f"Output must be fresh and absent, including an empty directory: {output}")
    if receipt.exists():
        raise BuildError(f"Receipt is create-only and already exists: {receipt}")
    if cache.exists() and not cache.is_dir():
        raise BuildError(f"Cache is not a directory: {cache}")
    if output == cache or output.is_relative_to(cache) or cache.is_relative_to(output):
        raise BuildError("Output and cache must not contain one another")
    if receipt == output or receipt.is_relative_to(output) or receipt.is_relative_to(cache):
        raise BuildError("Receipt must be outside output and cache")
    return output, cache, receipt


def write_new(path: Path, data: bytes) -> None:
    reject_reparse(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    reject_reparse(path.parent)
    with path.open("xb") as stream:
        stream.write(data)


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("utf-8")


class OfficialRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        parsed = urllib.parse.urlsplit(new_url)
        if parsed.scheme != "https" or parsed.hostname not in OFFICIAL_HOSTS or parsed.username or parsed.password:
            raise BuildError(f"Non-official HTTPS redirect rejected: {parsed.hostname}")
        return super().redirect_request(request, fp, code, message, headers, new_url)


def cached_artifact(artifact: Artifact, cache: Path) -> Path:
    target = cache / artifact.filename
    reject_reparse(target)
    if target.exists():
        if not target.is_file() or hash_file(target, artifact.algorithm) != artifact.digest:
            raise BuildError(f"Pinned hash mismatch; cached file was preserved: {target}")
        return target
    parsed = urllib.parse.urlsplit(artifact.url)
    if parsed.scheme != "https" or parsed.hostname not in OFFICIAL_HOSTS or parsed.username or parsed.password:
        raise BuildError("Artifact source must be a fixed official HTTPS URL")
    cache.mkdir(parents=True, exist_ok=True)
    temporary = cache / (artifact.filename + ".part-" + uuid.uuid4().hex)
    try:
        opener = urllib.request.build_opener(OfficialRedirect())
        request = urllib.request.Request(artifact.url, headers={"User-Agent": "Odin-LSP-Payload-Builder/1"})
        with opener.open(request, timeout=60) as response, temporary.open("xb") as destination:
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > MAX_ARCHIVE_BYTES:
                    raise BuildError("Download exceeds the archive byte limit")
                destination.write(chunk)
        if hash_file(temporary, artifact.algorithm) != artifact.digest:
            raise BuildError(f"Downloaded bytes do not match the pinned {artifact.algorithm}: {artifact.id}")
        # Exclusive publication never replaces a cache conflict.
        with temporary.open("rb") as source, target.open("xb") as destination:
            shutil.copyfileobj(source, destination)
        return target
    finally:
        if temporary.exists():
            temporary.unlink()


def safe_member(info: zipfile.ZipInfo, windows_separators: bool = False) -> tuple[str, ...]:
    original = info.orig_filename
    name = original.replace("\\", "/") if windows_separators else info.filename
    if not name or "\x00" in original or (not windows_separators and original != name) or "\x00" in name or "\\" in name or ":" in name or name.startswith("/"):
        raise BuildError(f"Unsafe ZIP member: {name!r}")
    trimmed = name[:-1] if name.endswith("/") else name
    parts = tuple(trimmed.split("/"))
    reserved = {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))}
    for part in parts:
        if not part or part in {".", ".."} or part.endswith((".", " ")) or any(ord(c) < 32 or c in '<>"|?*' for c in part):
            raise BuildError(f"Unsafe ZIP member: {name!r}")
        if part.split(".")[0].upper() in reserved:
            raise BuildError(f"Windows device name in ZIP member: {name!r}")
    unix_mode = (info.external_attr >> 16) & 0xFFFF
    file_type = stat.S_IFMT(unix_mode)
    if file_type not in (0, stat.S_IFREG, stat.S_IFDIR) or (file_type == stat.S_IFDIR and not info.is_dir()) or (file_type == stat.S_IFREG and info.is_dir()):
        raise BuildError(f"ZIP link or special entry rejected: {name!r}")
    if info.external_attr & REPARSE_POINT or info.flag_bits & 1:
        raise BuildError(f"ZIP reparse/encrypted entry rejected: {name!r}")
    return parts


def zip_plan(archive: zipfile.ZipFile, windows_separators: bool = False) -> list[tuple[zipfile.ZipInfo, tuple[str, ...]]]:
    entries = archive.infolist()
    if len(entries) > MAX_ARCHIVE_ENTRIES or sum(i.file_size for i in entries) > MAX_EXPANDED_BYTES:
        raise BuildError("ZIP exceeds entry or expanded byte limit")
    plan = []
    names: dict[str, bool] = {}
    for info in entries:
        parts = safe_member(info, windows_separators)
        folded = "/".join(parts).casefold()
        if folded in names:
            raise BuildError(f"ZIP duplicate/case collision: {info.filename!r}")
        names[folded] = info.is_dir()
        plan.append((info, parts))
    for name in names:
        for parent in PurePosixPath(name).parents:
            key = str(parent)
            if key != "." and key in names and not names[key]:
                raise BuildError(f"ZIP file is also an ancestor: {name!r}")
    return plan


def extract_zip(path: Path, destination: Path, artifact: Artifact, prefix: str = "", windows_separators: bool = False) -> None:
    reject_reparse(path)
    # Hash the same open handle consumed by ZipFile.
    with path.open("rb") as source:
        digest = hashlib.new(artifact.algorithm)
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
        if digest.hexdigest() != artifact.digest:
            raise BuildError(f"Archive changed after cache validation: {path}")
        source.seek(0)
        with zipfile.ZipFile(source) as archive:
            plan = zip_plan(archive, windows_separators)  # Preflight all entries, including those outside prefix.
            selected = 0
            for info, parts in plan:
                member = "/".join(parts)
                if prefix:
                    if member == prefix.rstrip("/") and info.is_dir():
                        continue
                    if not member.startswith(prefix):
                        continue
                    parts = tuple(member[len(prefix):].split("/"))
                if not parts or not parts[0]:
                    continue
                target = destination.joinpath(*parts)
                reject_reparse(target)
                if info.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                reject_reparse(target.parent)
                with archive.open(info) as member_stream, target.open("xb") as output_stream:
                    shutil.copyfileobj(member_stream, output_stream)
                selected += 1
            if selected == 0:
                raise BuildError(f"ZIP has no files for prefix {prefix!r}: {path}")


def validate_lock(package: dict, lock: dict) -> None:
    if package.get("private") is not True or package.get("dependencies") != NPM_VERSIONS:
        raise BuildError("Package must be private with the exact pinned dependency set")
    if any(package.get(key) for key in ("scripts", "devDependencies", "optionalDependencies", "workspaces", "overrides")):
        raise BuildError("Package scripts or unpinned dependency controls are forbidden")
    packages = lock.get("packages")
    if lock.get("lockfileVersion") != 3 or not isinstance(packages, dict) or packages.get("", {}).get("dependencies") != NPM_VERSIONS:
        raise BuildError("Expected the complete v3 lockfile with matching root dependencies")
    for name, entry in packages.items():
        if name == "":
            continue
        if not name.startswith("node_modules/") or entry.get("link") or not isinstance(entry.get("version"), str):
            raise BuildError(f"Unsupported lockfile package: {name}")
        parsed = urllib.parse.urlsplit(entry.get("resolved", ""))
        if parsed.scheme != "https" or parsed.hostname != "registry.npmjs.org" or parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise BuildError(f"Only HTTPS npm registry tarballs are accepted: {name}")
        integrity = entry.get("integrity", "")
        try:
            valid_integrity = integrity.startswith("sha512-") and len(base64.b64decode(integrity[7:], validate=True)) == 64
        except (ValueError, TypeError):
            valid_integrity = False
        if not valid_integrity:
            raise BuildError(f"Missing SHA512 package integrity: {name}")
    for name, version in NPM_VERSIONS.items():
        if packages.get("node_modules/" + name, {}).get("version") != version:
            raise BuildError(f"Lockfile does not pin {name} {version}")


def package_inputs(package_path: Path, lock_path: Path) -> tuple[bytes, bytes, dict]:
    reject_reparse(package_path)
    reject_reparse(lock_path)
    package_bytes, lock_bytes = package_path.read_bytes(), lock_path.read_bytes()
    if hashlib.sha256(lock_bytes).hexdigest() != LOCK_SHA256:
        raise BuildError("Lockfile bytes do not match the reviewed pinned lock SHA256")
    package, lock = json.loads(package_bytes), json.loads(lock_bytes)
    validate_lock(package, lock)
    return package_bytes, lock_bytes, lock


class Runner:
    def __init__(self, work: Path, receipt: dict):
        self.work, self.receipt = work, receipt

    def run(self, argv: list[str], cwd: Path, env: dict[str, str], timeout: int = 1800) -> None:
        index = len(self.receipt["commands"]) + 1
        stdout = self.work / f"command-{index:02d}.stdout.log"
        stderr = self.work / f"command-{index:02d}.stderr.log"
        record = {"argv": [str(a) for a in argv], "cwd": str(cwd), "environment": env,
                  "stdout": str(stdout), "stderr": str(stderr), "exit_code": None}
        self.receipt["commands"].append(record)
        started = time.monotonic()
        with stdout.open("xb") as out, stderr.open("xb") as err:
            process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=out, stderr=err, shell=False)
            try:
                record["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record["timed_out"] = True
                if os.name == "nt":
                    killer = Path(os.environ["SystemRoot"]) / "System32" / "taskkill.exe"
                    cleanup = subprocess.run([str(killer), "/PID", str(process.pid), "/T", "/F"], capture_output=True, check=False)
                    record["timeout_cleanup"] = {"argv": [str(killer), "/PID", str(process.pid), "/T", "/F"], "exit_code": cleanup.returncode}
                process.kill()
                record["exit_code"] = process.wait()
        record["elapsed_seconds"] = round(time.monotonic() - started, 3)
        for key, path in (("stdout", stdout), ("stderr", stderr)):
            record[key + "_sha256"] = hash_file(path)
            record[key + "_bytes"] = path.stat().st_size
        if record.get("timed_out") or record["exit_code"] != 0:
            raise BuildError(f"Command {index} failed with exit {record['exit_code']}; see {stderr}")


def build_environment(output: Path, work: Path) -> dict[str, str]:
    inherited = ("SystemRoot", "WINDIR", "TEMP", "TMP", "USERPROFILE", "LOCALAPPDATA", "APPDATA", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "COMSPEC", "OS", "PROCESSOR_ARCHITECTURE")
    env = {key: os.environ[key] for key in inherited if key in os.environ}
    system_root = Path(os.environ.get("SystemRoot", "C:/Windows"))
    user_config, global_config = work / "user.npmrc", work / "global.npmrc"
    write_new(user_config, b"")
    write_new(global_config, b"")
    for name in ("home", "gopath", "go-cache", "go-modules", "npm-cache", "nuget"):
        (work / name).mkdir()
    env.update({
        "PATH": os.pathsep.join(map(str, (output / "node", output / "go/bin", system_root / "System32", system_root))),
        "HOME": str(work / "home"), "GOROOT": str(output / "go"), "GOBIN": str(output / "gopls"),
        "GOPATH": str(work / "gopath"), "GOCACHE": str(work / "go-cache"), "GOMODCACHE": str(work / "go-modules"),
        "GOTOOLCHAIN": "local", "GOSUMDB": "sum.golang.org", "GOPROXY": "https://proxy.golang.org",
        "GOPRIVATE": "", "GONOPROXY": "", "GONOSUMDB": "", "GOENV": "off", "CGO_ENABLED": "0", "GOTELEMETRY": "off",
        "NPM_CONFIG_USERCONFIG": str(user_config), "NPM_CONFIG_GLOBALCONFIG": str(global_config),
        "NPM_CONFIG_CACHE": str(work / "npm-cache"), "NPM_CONFIG_REGISTRY": "https://registry.npmjs.org/",
        "NPM_CONFIG_IGNORE_SCRIPTS": "true", "NPM_CONFIG_AUDIT": "false", "NPM_CONFIG_FUND": "false",
        "DOTNET_ROOT": str(output / "dotnet"), "DOTNET_MULTILEVEL_LOOKUP": "0",
        "DOTNET_CLI_HOME": str(work / "home"), "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
        "DOTNET_SKIP_FIRST_TIME_EXPERIENCE": "1",
        "DOTNET_ADD_GLOBAL_TOOLS_TO_PATH": "false", "DOTNET_GENERATE_ASPNET_CERTIFICATE": "false",
        "NUGET_PACKAGES": str(work / "nuget"),
    })
    return env


def payload_files(output: Path) -> list[dict[str, str]]:
    paths = []
    for directory, dirs, names in os.walk(output, followlinks=False):
        parent = Path(directory)
        reject_reparse(parent)
        for name in dirs:
            reject_reparse(parent / name)
        paths.extend(parent / name for name in names if parent / name != output / "manifest.json")

    def inspect(path: Path) -> dict[str, str]:
        reject_reparse(path)
        info = path.stat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise BuildError(f"Payload contains a special file or hardlink: {path}")
        return {"path": path.relative_to(output).as_posix(), "sha256": hash_file(path)}

    # The closure can contain tens of thousands of SDK files. Bound concurrency
    # while retaining a deterministic, platform-independent manifest order.
    with ThreadPoolExecutor(max_workers=16) as executor:
        files = list(executor.map(inspect, paths))
    return sorted(files, key=lambda item: item["path"])


def copy_go_notices(module_cache: Path, output: Path) -> None:
    for directory, dirs, names in os.walk(module_cache, followlinks=False):
        parent = Path(directory)
        reject_reparse(parent)
        dirs[:] = sorted(d for d in dirs if not (parent == module_cache and d == "cache"))
        for name in dirs:
            reject_reparse(parent / name)
        for name in sorted(names):
            if name.upper().startswith(NOTICE_PREFIXES):
                source = parent / name
                reject_reparse(source)
                write_new(output / "licenses/go-modules" / source.relative_to(module_cache), source.read_bytes())


def platform_optional_not_applicable(entry: dict) -> bool:
    """Only an explicit npm os/cpu exclusion can justify an absent optional package."""
    if entry.get("optional") is not True:
        return False
    for key, target in (("os", "win32"), ("cpu", "x64")):
        values = entry.get(key)
        if isinstance(values, str):
            values = [values]
        if not isinstance(values, list) or not values or any(not isinstance(v, str) or not v or v == "!" for v in values):
            continue  # Missing/ambiguous platform metadata is not evidence of exclusion.
        if values == ["any"]:
            continue
        if "!" + target in values:
            return True
        allowed = [v for v in values if not v.startswith("!")]
        if allowed and target not in allowed:
            return True
    return False


def npm_inventory(output: Path, lock: dict) -> None:
    inventory = []
    for relative, entry in sorted(lock["packages"].items()):
        if not relative:
            continue
        package_file = output / relative / "package.json"
        reject_reparse(package_file)
        if not package_file.exists():
            if platform_optional_not_applicable(entry) and not package_file.parent.exists():
                inventory.append({"path": relative, "version": entry["version"],
                                  "source": entry["resolved"], "integrity": entry["integrity"],
                                  "os": entry.get("os"), "cpu": entry.get("cpu"),
                                  "status": "NOT_INSTALLED_PLATFORM_OPTIONAL"})
                continue
            raise BuildError(f"Missing required/applicable npm package or incomplete installation: {relative}")
        actual = json.loads(package_file.read_bytes())
        if actual.get("version") != entry["version"]:
            raise BuildError(f"Installed npm package differs from lockfile: {relative}")
        inventory.append({"path": relative, "name": actual.get("name"), "version": entry["version"],
                          "license": actual.get("license"), "source": entry["resolved"], "integrity": entry["integrity"],
                          "status": "INSTALLED"})
    write_new(output / "licenses/npm-packages.json", json_bytes(inventory))


def npm_bin(output: Path, package: str, name: str) -> Path:
    root = output / "node_modules" / package
    data = json.loads((root / "package.json").read_bytes())
    bins = data.get("bin", {})
    relative = bins if isinstance(bins, str) else bins.get(name)
    if not isinstance(relative, str):
        raise BuildError(f"Missing npm executable {name}")
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise BuildError(f"Npm executable escaped its package: {name}")
    reject_reparse(path)
    return path


def native_inputs(path: Path) -> tuple[bytes, list[dict], tuple[Artifact, ...]]:
    reject_reparse(path)
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != NATIVE_SOURCES_SHA256:
        raise BuildError("Native source bytes differ from the reviewed pinned SHA256")
    metadata = json.loads(data)
    components = metadata["components"]
    artifacts = []
    for component in components:
        artifacts.append(Artifact(component["id"], component["version"], component["filename"],
                                  component["url"], "sha256", component["sha256"]))
        for index, license_source in enumerate(component["license_sources"]):
            artifacts.append(Artifact(component["id"] + "-license-" + str(index), component["version"],
                                      license_source["filename"], license_source["url"], "sha256", license_source["sha256"]))
    for source in metadata.get("source_sidecar", {}).get("inputs", []):
        artifacts.append(Artifact(source["id"], "1.18.1", source["filename"],
                                  source["url"], "sha256", source["sha256"]))
    for group in metadata.get("notice_sets", []):
        for source in group["sources"]:
            artifacts.append(Artifact(source["id"], source["version"], source["filename"], source["url"], "sha256", source["sha256"]))
            for notice in source.get("supplemental_notice_sources", []):
                artifacts.append(Artifact(notice["id"], source["version"], notice["filename"], notice["url"], "sha256", notice["sha256"]))
    unique = {}
    for artifact in artifacts:
        previous = unique.get(artifact.id)
        if previous is not None and previous != artifact:
            raise BuildError("Conflicting pinned source identity: " + artifact.id)
        unique[artifact.id] = artifact
    return data, components, tuple(unique.values())


def extract_native(sources: dict[str, Path], components: list[dict], output: Path) -> None:
    for component in components:
        artifact = Artifact(component["id"], component["version"], component["filename"],
                            component["url"], "sha256", component["sha256"])
        source = sources[component["id"]]
        if component["archive"]:
            extract_zip(source, output / component["directory"], artifact, component["prefix"], component.get("windows_separators", False))
        else:
            # The open handle is rehashed before publishing raw executable bytes.
            data = source.read_bytes()
            if hashlib.sha256(data).hexdigest() != artifact.digest:
                raise BuildError("Raw binary changed after cache validation: " + artifact.id)
            write_new(output / component["entrypoint"], data)
        for index, license_source in enumerate(component["license_sources"]):
            data = sources[component["id"] + "-license-" + str(index)].read_bytes()
            if hashlib.sha256(data).hexdigest() != license_source["sha256"]:
                raise BuildError("License changed after cache validation: " + component["id"])
            write_new(output / license_source["destination"], data)
        if not (output / component["entrypoint"]).is_file():
            raise BuildError("Native entrypoint missing: " + component["entrypoint"])


def write_source_sidecar(output: Path, sidecar: dict, sources: dict[str, Path]) -> None:
    target = output / sidecar["destination"]
    reject_reparse(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    # Compressed upstream archives remain inert; no source/build hook is executed.
    with zipfile.ZipFile(target, "x", compression=zipfile.ZIP_STORED, allowZip64=True) as archive:
        def member(name: str, data: bytes) -> None:
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)
        for name, data in sorted((("README-SOURCE.txt", sidecar["readme"].encode("utf-8")),
                                  ("source-index.json", json_bytes(sidecar["index"])) )):
            member(name, data)
        for source in sorted(sidecar["inputs"], key=lambda item: item["member"]):
            data = sources[source["id"]].read_bytes()
            if hashlib.sha256(data).hexdigest() != source["sha256"]:
                raise BuildError("Source archive changed after validation: " + source["id"])
            member(source["member"], data)
    if hash_file(target) != sidecar["sha256"]:
        raise BuildError("Generated source sidecar differs from reviewed bytes")


def build(args: argparse.Namespace, receipt: dict, output: Path, cache: Path) -> None:
    collector = REPO / "scripts/collect_lsp_notices.py"
    if hash_file(collector) != NOTICE_COLLECTOR_SHA256:
        raise BuildError("Notice collector differs from the reviewed source SHA256")
    package_bytes, lock_bytes, lock = package_inputs(args.package_json, args.package_lock)
    receipt["inputs"] = {"package_json": {"path": str(args.package_json), "sha256": hashlib.sha256(package_bytes).hexdigest()},
                         "package_lock": {"path": str(args.package_lock), "sha256": hashlib.sha256(lock_bytes).hexdigest()}}
    native_bytes, native_components, native_artifacts = native_inputs(args.native_sources)
    receipt["inputs"]["native_sources"] = {"path": str(args.native_sources), "sha256": hashlib.sha256(native_bytes).hexdigest()}
    receipt["inputs"]["notice_collector"] = {"path": str(collector), "sha256": NOTICE_COLLECTOR_SHA256}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir()  # Fresh root is the transaction boundary for this external build.
    work = Path(tempfile.mkdtemp(prefix="odin-lsp-build-", dir=output.parent))
    receipt["work_directory"] = str(work)
    runner = Runner(work, receipt)
    all_artifacts = (*ARTIFACTS, CSHARP_LICENSE, *native_artifacts)
    sources = {artifact.id: cached_artifact(artifact, cache) for artifact in all_artifacts}
    receipt["sources"] = [{"id": a.id, "version": a.version, "url": a.url, "cache": str(sources[a.id]),
                           "algorithm": a.algorithm, "digest": a.digest} for a in all_artifacts]
    by_id = {artifact.id: artifact for artifact in ARTIFACTS}
    for component, directory, prefix in (("node", "node", "node-v24.21.0-win-x64/"),
                                          ("lua-language-server", "lua", ""), ("go", "go", "go/"),
                                          ("dotnet", "dotnet", ""), ("csharp-ls", "csharp", "tools/net10.0/any/")):
        extract_zip(sources[component], output / directory, by_id[component], prefix)
    extract_native(sources, native_components, output)
    sidecar = json.loads(native_bytes).get("source_sidecar")
    if sidecar:
        write_source_sidecar(output, sidecar, sources)
        receipt["source_sidecar"] = {key: sidecar[key] for key in ("destination", "sha256", "source_closure_status")}
    write_new(output / "licenses/native-sources.json", native_bytes)
    write_new(output / "package.json", package_bytes)
    write_new(output / "package-lock.json", lock_bytes)
    write_new(output / "licenses/csharp-ls/LICENSE", sources[CSHARP_LICENSE.id].read_bytes())
    (output / "gopls").mkdir()
    env = build_environment(output, work)
    node, go, dotnet = output / "node/node.exe", output / "go/bin/go.exe", output / "dotnet/dotnet.exe"
    runner.run([str(node), str(output / "node/node_modules/npm/bin/npm-cli.js"), "ci", "--ignore-scripts", "--no-audit", "--no-fund"], output, env)
    npm_inventory(output, lock)
    runner.run([sys.executable, str(collector), "--metadata", str(args.native_sources),
                "--expected-metadata-sha256", NATIVE_SOURCES_SHA256, "--cache-dir", str(cache),
                "--output", str(output / "licenses/native-dependencies")], work, env)
    receipt["notice_inventory"] = json.loads((output / "licenses/native-dependencies/inventory.json").read_bytes())
    runner.run([str(go), "install", "-trimpath", "golang.org/x/tools/gopls@" + GOPLS_VERSION], work, env)
    runner.run([str(go), "version", "-m", str(output / "gopls/gopls.exe")], work, env)
    copy_go_notices(work / "go-modules", output)
    required = ("node/node.exe", "node/node_modules/npm/bin/npm-cli.js", "go/bin/go.exe", "gopls/gopls.exe",
                "dotnet/dotnet.exe", "csharp/CSharpLanguageServer.dll", "lua/bin/lua-language-server.exe")
    for relative in required:
        if not (output / relative).is_file():
            raise BuildError(f"Required payload file is absent: {relative}")
    files = payload_files(output)
    if args.versions:
        commands = [[str(node), "--version"], [str(go), "version"], [str(output / "gopls/gopls.exe"), "version"],
                    [str(dotnet), "--list-sdks"], [str(dotnet), str(output / "csharp/CSharpLanguageServer.dll"), "--version"],
                    [str(output / "lua/bin/lua-language-server.exe"), "--version",
                     "--logpath=" + str(work / "lua-log"), "--metapath=" + str(work / "lua-meta")]]
        commands += [[str(node), str(npm_bin(output, package, binary)), "--version"] for package, binary in (
            ("basedpyright", "basedpyright"), ("typescript-language-server", "typescript-language-server"),
            ("typescript", "tsc"), ("bash-language-server", "bash-language-server"),
            ("yaml-language-server", "yaml-language-server"), ("@vtsls/language-server", "vtsls"))]
        commands += [[str(output / component["entrypoint"]), *component["health_args"]] for component in native_components]
        for command in commands:
            runner.run(command, work, env, timeout=120)
        if payload_files(output) != files:
            raise BuildError("A version probe modified the payload; output was preserved for investigation")
    components = []
    for artifact in ARTIFACTS:
        item = {"id": artifact.id, "version": artifact.version, "source": artifact.url,
                "sha256": artifact.digest if artifact.algorithm == "sha256" else hash_file(sources[artifact.id])}
        components.append(item)
    for name, version in NPM_VERSIONS.items():
        entry = lock["packages"]["node_modules/" + name]
        components.append({"id": name, "version": version, "source": entry["resolved"], "integrity": entry["integrity"]})
    components.append({"id": "gopls", "version": GOPLS_VERSION.removeprefix("v"),
                       "source": "https://proxy.golang.org/golang.org/x/tools/gopls/@v/v0.24.0.info",
                       "sha256": hash_file(output / "gopls/gopls.exe")})
    components.extend({"id": item["id"], "version": item["version"], "source": item["url"],
                       "sha256": item["sha256"], **({"release": item["release"]} if "release" in item else {})} for item in native_components)
    if sidecar:
        next(item for item in components if item["id"] == "slint-lsp")["source_sidecar"] = receipt["source_sidecar"]
    manifest = {"schema_version": 1, "target": TARGET, "components": sorted(components, key=lambda item: item["id"]),
                "files": files}
    write_new(output / "manifest.json", json_bytes(manifest))
    receipt["manifest_sha256"] = hash_file(output / "manifest.json")
    receipt["file_count"] = len(manifest["files"])
    receipt["components"] = manifest["components"]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="Fresh external backends directory; must not exist")
    parser.add_argument("--cache-dir", type=Path, required=True, help="External original archive cache; hashes checked on every use")
    parser.add_argument("--package-json", type=Path, default=REPO / "packaging/lsp-package.json")
    parser.add_argument("--package-lock", type=Path, default=REPO / "packaging/lsp-package-lock.json")
    parser.add_argument("--native-sources", type=Path, default=REPO / "packaging/lsp-native-sources.json")
    parser.add_argument("--receipt", type=Path, help="Create-only external JSON; default sibling <output>.build-receipt.json")
    parser.add_argument("--versions", action="store_true", help="Run actual version probes; does not certify IDE functionality")
    args = parser.parse_args(argv)
    if os.name != "nt" or platform.machine().upper() not in {"AMD64", "X86_64"}:
        parser.error("The actual payload build requires Windows x64")
    receipt_path = args.receipt or args.output.with_name(args.output.name + ".build-receipt.json")
    try:
        output, cache, receipt_path = validate_paths(args.output, args.cache_dir, receipt_path)
    except (BuildError, OSError) as error:
        print(f"Precondition failed: {error}", file=sys.stderr)
        return 1
    receipt = {"schema_version": 1, "scope": "WINDOWS_LSP_PAYLOAD_BUILD_ONLY", "status": "FAIL", "target": TARGET,
               "output": str(output), "builder_sha256": hash_file(Path(__file__)), "commands": [],
               "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "runtime": {"python": sys.version, "platform": platform.platform()}}
    code = 1
    try:
        build(args, receipt, output, cache)
        receipt["status"] = "PASS"
        code = 0
    except Exception as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
        print(receipt["error"], file=sys.stderr)
    finally:
        receipt["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        try:
            write_new(receipt_path, json_bytes(receipt))
        except OSError as error:
            print(f"Create-only receipt could not be written: {error}", file=sys.stderr)
            code = 1
    print(json.dumps({"status": receipt["status"], "exit_code": code, "receipt": str(receipt_path), "output": str(output)}))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
