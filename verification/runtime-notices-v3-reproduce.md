# Runtime notice source and text verification

Task: `runtime-notices-v3`. Author: `/root/engine_implementation`.
Base: `a688ec59bd33aa864e55b66a4ca6fd058569e2bc`.
Write scope: the new runtime notice and fresh `runtime-notices-v3-*` evidence only.

The notice supplements the unchanged Cargo inventory (260 resolved packages).
It is bound to Linux application revision
`0fe03ba30da9600c25588d506d7e094df89cf56f`, ELF
`e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42`.
This is document/source-integrity verification, not legal certification, an
exhaustive per-object link map, new application testing, or package acceptance.

## Source decisions

- The pre-existing official Rust 1.90.0 compiler and musl target archives were
  read from the packaging owner's host cache. Their SHA-256 values match both
  the exact publisher manifest target records and the earlier setup report.
  The newly retrieved manifest matches the publisher's `.sha256` file and the
  earlier setup report's manifest hash. This is checksum verification, not a
  new detached-signature verification.
- Rust's release tag resolves to compiler commit
  `1159e78c4747b02ef996e55082b704c09b970588`, also present in both archives.
  The three root license/copyright files are identical in those two archives.
- The complete standard-library copyright document is taken from that actual
  compiler archive. Its HTML body becomes plain text: tags are removed,
  character references decoded and line-end presentation padding removed.
  No non-whitespace text or license paragraphs are omitted. The other nine
  payloads retain their original bytes, including the SQLite comment boundary.
- The [Rust 1.90.0 musl recipe](https://github.com/rust-lang/rust/blob/1.90.0/src/ci/docker/scripts/musl-toolchain.sh)
  fixes musl 1.2.3 and musl-cross-make commit
  `fe915821b652a7fa37b34a596f47d8e20bc72338`. The official musl release archive
  matches the SHA-1 published by that pinned recipe's source. SHA-1 is recorded
  as such; the observed SHA-256 is not mislabelled a publisher-signed checksum.
  The exact archive's complete `COPYRIGHT` is included.
- Rust bootstrap `compile.rs:378-405` copies musl libc/startup objects and
  separately obtains `llvm::CrtBeginEnd`. `llvm.rs:1317-1383` builds the latter
  from LLVM `compiler-rt/lib/builtins/crtbegin.c` and `crtend.c`.
  `llvm.rs:1395-1418` identifies LLVM libunwind. Their precise LLVM submodule
  commit is `e8a2ffcf322f45b8dce82c65ab27a3e2430a6b51`.
  The corresponding two full LLVM runtime licenses and Rust's
  `compiler_builtins` 0.1.160 notice are included. Object names and a GCC
  producer string are not treated as evidence of GCC runtime linkage.
- Cargo's existing libsqlite3-sys entry contains the wrapper's MIT license.
  It does not contain SQLite's own copyright disclaimer/blessing. The complete
  opening comment in bundled SQLite 3.50.2 `sqlite3.h` is reproduced, from a
  local crate archive whose SHA-256 matches `Cargo.lock`.
- No clang-sys, compiler_builtins, compiler-rt or libunwind package appears in
  the 260-package Cargo inventory. The LLVM runtime notices above therefore
  supplement that inventory. The Cargo `cc` license covers the Rust build
  helper, not all native code its selected compiler may compile.
- Debian musl/musl-dev/musl-tools 1.2.3-1 and GCC 12.2.0-14+deb12u1 are only
  build-host observations. No host compiler, Debian package, VM or external
  language server is added to the bundle by this document.

## Checks and limits

Actual command outcomes and input/output hashes are in
`runtime-notices-v3-checks.json`; raw official text responses, archive/member
hashes and notice byte offsets are in `runtime-notices-v3-sources.json`.
The raw source receipt intentionally retains full publisher responses, including
the Rust manifest and attribution texts, for offline comparison.

The first HTML rendering retained 498 layout-padding whitespace locations.
That `git diff --no-index --check` exit 3 is retained in the source receipt;
only that rendering's line-end spaces/tabs were removed. Separate original
license-file payloads were not reformatted. The final whitespace command and
byte checks have their own actual result.

Product tests, Windows execution, VM access and a new build are **N/A** for
this docs-only slice. Independent review and new bundle inclusion/lifecycle
acceptance are **NOT_RUN** by this author. Root/reviewer and the packaging
owner perform those next, preserving the same application ELF bytes and the
existing source-bound application test results.

## Reproduce the read-only checks

From the repository root, save the following block outside the repository or
pipe it to `python -`. It reads local archives/receipts and the notice; it does
not execute Odin, access the VM, use the network, modify files or rebuild code.
The packaging owner's original cache paths are intentionally explicit.

```python
from pathlib import Path
from html.parser import HTMLParser
import hashlib
import json
import subprocess
import tarfile
import tomllib

root = Path.cwd()
j = json.loads((root / "verification/runtime-notices-v3-sources.json").read_text(encoding="utf-8"))
sources = {x["id"]: x for x in j["sources"]}
checks = []

def sha(data):
    return hashlib.sha256(data).hexdigest()

def check(name, condition):
    assert condition, name
    checks.append(name)

for name, source in sources.items():
    if "text_utf8" in source:
        raw = source["text_utf8"].encode("utf-8")
        check("raw source " + name, len(raw) == source["bytes"] and sha(raw) == source["sha256"])
manifest = sources["rust_manifest"]["text_utf8"].encode("utf-8")
check("publisher manifest checksum", sha(manifest) == sources["rust_manifest_checksum"]["text_utf8"].split()[0])
setup = json.loads(Path("E:/odin/.odin/validation-tools/packaging-v3/native-rust/setup-report.json").read_text(encoding="utf-8"))
check("pre-existing publisher manifest binding", sha(manifest) == setup["manifest_sha256"])
check("Rust release commit", json.loads(sources["rust_release_commit"]["text_utf8"])["sha"] == "1159e78c4747b02ef996e55082b704c09b970588")
check("musl pinned source archive SHA-1", sources["musl_release"]["sha1"] == sources["musl_cross_hash"]["text_utf8"].split()[0])
check("musl recipe version", "MUSL_VER=1.2.3" in sources["rust_musl_recipe"]["text_utf8"])
check("LLVM source pin", json.loads(sources["rust_llvm_ref"]["text_utf8"])["sha"] == "e8a2ffcf322f45b8dce82c65ab27a3e2430a6b51")
check("CRT source provenance", 'compiler-rt/lib/builtins/crtbegin.c' in sources["rust_bootstrap_llvm"]["text_utf8"])
check("compiler builtins version", 'version = "0.1.160"' in sources["compiler_builtins_cargo"]["text_utf8"])
members = {}
manifest_data = tomllib.loads(manifest.decode("utf-8"))
for archive in j["local_archives"]:
    path = Path(archive["path"])
    raw = path.read_bytes()
    digest = sha(raw)
    check("archive " + path.name, len(raw) == archive["bytes"] and digest == archive["sha256"])
    check("earlier download " + path.name, any(x["sha256"] == digest and Path(x["path"]).name == path.name for x in setup["downloads"]))
    check("publisher target entry " + path.name, any(v.get("xz_hash") == digest and v.get("xz_url") == archive["publisher_source"] for pkg in manifest_data["pkg"].values() for v in pkg.get("target", {}).values()))
    with tarfile.open(path) as t:
        for member in archive["members"]:
            data = t.extractfile(member["path"]).read()
            check("member " + member["path"], len(data) == member["bytes"] and sha(data) == member["sha256"])
            members[member["path"]] = data
for name in ["LICENSE-MIT", "LICENSE-APACHE", "COPYRIGHT", "version", "git-commit-hash"]:
    pair = [data for path, data in members.items() if path.rsplit("/", 1)[-1] == name]
    check("same compiler/std " + name, len(pair) == 2 and pair[0] == pair[1])

class BodyText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.body = False
        self.parts = []
    def handle_starttag(self, tag, attrs):
        if tag == "body":
            self.body = True
    def handle_endtag(self, tag):
        if tag == "body":
            self.body = False
    def handle_data(self, data):
        if self.body:
            self.parts.append(data)

html = next(data for path, data in members.items() if path.endswith("COPYRIGHT-library.html"))
parser = BodyText()
parser.feed(html.decode("utf-8"))
plain_original = "".join(parser.parts).encode("utf-8")
plain = b"\n".join(line.rstrip(b" \t") for line in plain_original.split(b"\n"))
check("HTML non-whitespace text preserved", plain.decode().split() == plain_original.decode().split())
sqlite = j["sqlite_archive"]
raw = Path(sqlite["path"]).read_bytes()
check("SQLite crate bytes", len(raw) == sqlite["bytes"] and sha(raw) == sqlite["sha256"])
lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
check("SQLite Cargo checksum", any(x["name"] == "libsqlite3-sys" and x["version"] == "0.35.0" and x["checksum"] == sha(raw) for x in lock["package"]))
with tarfile.open(Path(sqlite["path"])) as t:
    for member in sqlite["members"]:
        data = t.extractfile(member["path"]).read()
        check("SQLite member " + member["path"], len(data) == member["bytes"] and sha(data) == member["sha256"])
        members[member["path"]] = data
sqliteh = members["libsqlite3-sys-0.35.0/sqlite3/sqlite3.h"]
check("SQLite version", b'#define SQLITE_VERSION        "3.50.2"' in sqliteh)
blessing = sqliteh[sqliteh.index(b"/*"):sqliteh.index(b"*/") + 2]
raw_payloads = {
    "RUST-LIBRARY-NOTICES": plain,
    "RUST-UNICODE-3.0": sources["rust_unicode_license"]["text_utf8"].encode("utf-8"),
    "MUSL-COPYRIGHT": sources["musl_release"]["member"]["text_utf8"].encode("utf-8"),
    "RUST-COMPILER-BUILTINS": sources["compiler_builtins_license"]["text_utf8"].encode("utf-8"),
    "LLVM-LIBUNWIND": sources["llvm_libunwind_license"]["text_utf8"].encode("utf-8"),
    "LLVM-COMPILER-RT": sources["llvm_compiler_rt_license"]["text_utf8"].encode("utf-8"),
    "SQLITE-SOURCE-NOTICE": blessing,
}
for name in ["LICENSE-MIT", "LICENSE-APACHE", "COPYRIGHT"]:
    raw_payloads["RUST-" + name] = members["rust-std-1.90.0-x86_64-unknown-linux-musl/" + name]
doc = (root / j["document"]["path"]).read_bytes()
check("whole notice", len(doc) == j["document"]["bytes"] and sha(doc) == j["document"]["sha256"])
for section in j["document"]["sections"]:
    start = section["payload_offset_bytes"]
    payload = doc[start:start + section["payload_bytes"]]
    check("notice payload " + section["id"], payload == raw_payloads[section["id"]] and sha(payload) == section["payload_sha256"])
    check("notice markers " + section["id"], doc.count(("--- BEGIN " + section["id"] + " ---").encode()) == 1 and doc.count(("--- END " + section["id"] + " ---").encode()) == 1)
inventory = json.loads((root / "verification/license-inventory.json").read_text(encoding="utf-8"))
check("Cargo inventory count", len(inventory["packages"]) == 260)
entry = next(x for x in inventory["packages"] if x["name"] == "libsqlite3-sys")
check("Cargo SQLite wrapper scope", entry["license_files"] == ["LICENSE"])
check("native runtime absent from Cargo inventory", not any(x["name"] in {"clang-sys", "compiler_builtins", "compiler-rt", "libunwind"} for x in inventory["packages"]))
check("original Cargo notice untouched", (root / "docs/THIRD_PARTY_LICENSES.txt").read_bytes() == subprocess.check_output(["git", "show", "a688ec5:docs/THIRD_PARTY_LICENSES.txt"]))
elf = Path("E:/odin/dist/package-v3-linux-0fe03ba/odin").read_bytes()
check("same ELF read-only hash", len(elf) == 14550736 and sha(elf) == "e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42")
check("ELF magic without execution", elf[:4] == b"\x7fELF")
print(json.dumps({"status": "PASS", "scope": "DOCUMENT_SOURCE_INTEGRITY_ONLY", "checks": checks, "count": len(checks)}, indent=2))
```
