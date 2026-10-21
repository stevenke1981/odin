# Real language and IDE acceptance

`tests/real_ide_backends.py` tests the installed Odin binary through MCP stdio
against installed language servers. It creates two independent projects for
each selected language. These are real backend workloads with small trusted
source fixtures, not protocol mocks. Python runs the development harness;
Odin's Rust runtime does not require this harness or Python to start.

The default **baseline** explicitly uses host processes without OS isolation.
An independently selected **sandbox** run creates a host-owned MXC policy and
records the actual LSP transport route, executor hash and successful LSP results.
Neither profile silently falls back to another server, executable, sandbox tier,
or UI policy.

## Profiles and fixture projects

| Language | Installed backend | Two-project source layout |
|---|---|---|
| Rust | host `rust-analyzer` from the selected Rust toolchain | Cargo library, definition and `src/consumer.rs` |
| Python | isolated `basedpyright-langserver.exe` | typed function and importing `consumer.py` |
| TypeScript | host Node plus isolated `typescript-language-server` and explicit `tsserver.js` | strict tsconfig, exported function and importing consumer |
| Go | isolated `gopls.exe` and Go SDK | local module, function and second source file |
| Java | isolated JDK and JDT LS Equinox launcher | Eclipse Java project with two classes under `src/fixture` |
| C# | isolated .NET SDK and `csharp-ls.exe` | net10.0 project with two source files |

The host chooses all executables, complete argv and settings. The harness never
installs a tool, changes global PATH, or executes repository-supplied host
configuration. Its default tools root is `E:/odin/.odin/validation-tools`,
overridable with `--tools-root`. Rust and Node are resolved from the selected
host PATH; all other paths are under that tools root. Runtime program hashes
and actual version-command outputs are captured in the report.
The Node profile explicitly uses `--preserve-symlinks` and `--preserve-symlinks-main`, avoiding a main
entrypoint resolution walk that can require an ungranted drive root. Rust
sandbox profiles explicitly retain the selected host Rustup/Cargo homes while
the ordinary HOME directory remains private; the matching toolchain paths are
read-only grants.

Java uses a separate external JDT workspace cache for each project, not the
source directory as `-data`. The fixtures disable Maven/Gradle project import.
C# restores only its trusted fixture before starting Odin, using an explicit
NuGet configuration with no package sources and the installed framework packs.
Go uses explicit SDK/module/build-cache directories and `GOTOOLCHAIN=local`.
Rust fixtures have no third-party dependencies, build scripts or proc macros.
The per-process environment retains existing `ProgramFiles`, `ProgramFiles(x86)`,
`ProgramW6432`, `ProgramData` and `ALLUSERSPROFILE` values required by the .NET/NuGet
runtime. This adds no filesystem grants. The initial missing-environment restore
failure and successful isolated correction are preserved separately from LSP tests.
The Go profile explicitly enables
[gopls semantic tokens](https://go.dev/gopls/settings#semanticTokens), whose default
is disabled. The Java profile explicitly enables signature help, also disabled
by default in [JDT LS preferences](https://github.com/eclipse-jdtls/eclipse.jdt.ls/blob/main/org.eclipse.jdt.ls.core/src/org/eclipse/jdt/ls/core/internal/preferences/Preferences.java).
Signature queries test the call's opening argument boundary and the end of the
first argument: a backend may suppress signature UI inside a string token.
Actual query positions are retained in the transcript.

## Checks and result meanings

Each project attempts document symbols, a reference in the second source file,
and a real rename preview followed by transaction apply and rollback. The
preview must leave all fixture sources unchanged. Applying a rename must update
the definition and consumer, leave other projects unchanged, and rollback must
restore exact bytes.

Completion, hover, signature help, workspace symbols, incoming calls, inlay hints
and semantic tokens use the actual advertised capabilities. A supported query
must return meaningful fixture data and current source/UTF-16 metadata. Available
completion and hint resolution are tested with the originating hash; reads and
resolve calls must leave both fixture sources unchanged. Formatting must produce
a nonempty CAS preview, leave the source unchanged before apply, change it on
apply, and restore exact bytes on rollback. A no-op format is not evidence that
a formatting transaction happened.

Diagnostics cover valid source, deliberately broken source, and recovery. The
broken-source edit and restoration go through Odin Transactions. Only a current,
version-verified report with `summary.complete=true` can pass. Unversioned push
diagnostics remain `NOT_VERIFIED`, even when they contain the expected error or
an empty list. None of these results is represented as a successful build or
proof that every project file was analyzed.

| Per-check status | Meaning |
|---|---|
| `PASS` | The live binary/backend result met this fixture's explicit assertion |
| `FAIL` | Operation failed, data was invalid, a required result was missing, or a source/transaction assertion failed |
| `UNAVAILABLE` | The backend's actual initialize response did not advertise the optional capability |
| `NOT_VERIFIED` | A result was observed but freshness or a nonempty transaction was not proven |
| `NOT_RUN` | A dependent check could not run, for example resolve without an originating item |

Project/report `PASS` means the attempted supported checks passed, with
`UNAVAILABLE` entries still visible. It does not mean every IDE feature exists.
`PARTIAL` preserves `NOT_VERIFIED`/`NOT_RUN`; `FAIL` preserves actual failures.
Exit codes are respectively 0, 2 and 1. No failed optional operation is converted
to `UNAVAILABLE` merely because it returned an error. Backend initialization
and source freshness do not prove a complete semantic index.
A candidate that Windows refuses to launch is `BLOCKED` (exit 3), with backend
checks left `NOT_RUN`. The harness performs one ordinary binary version launch
preflight and does not retry a blocked candidate under another filename or policy.

## Reproducible commands

```powershell
python tests/real_ide_backends.py --binary E:/odin/target/release/odin.exe `
  --output verification/language-ide-baseline-01.json
```

To diagnose one backend, use `--languages java` (or another language). Every
selected language still receives two independent projects. `--timeout` controls
the bounded server/RPC deadline, while semantic retries preserve the last result
and actual backend errors. A report path and its companion `.requests.jsonl`
must not already exist: use a fresh output path to preserve every failed attempt.

An explicit native sandbox run, after the integrated binary is available:

```powershell
python tests/real_ide_backends.py --binary E:/odin/target/release/odin.exe `
  --languages typescript --mode sandbox `
  --sandbox-executor '<absolute trusted wxc-exec.exe>' `
  --sandbox-sha256 '<verified SHA-256>' --sandbox-allow-ui `
  --output verification/language-ide-sandbox-typescript-01.json
```

`--sandbox-allow-ui` is an explicit host choice for runtimes that require it;
omitting it keeps UI disabled. Clipboard and input-injection restrictions remain
fixed. Sandbox runs grant source and installed runtime directories read-only,
and only a per-launch private scratch directory writable. Odin itself applies
CAS transactions outside the untrusted language-server process. The harness
checks `status.transport.isolation` for the actual MXC route and expected
executor pin; it does not change `os_boundary_verified` to true. Actual OS
denial/lifecycle evidence belongs to the native tests described in
[OS_SANDBOX.md](OS_SANDBOX.md). A successful LSP handshake alone is not that proof.

## Evidence boundary

Reports under `verification/language-ide-*` include the binary and harness
SHA-256, server capabilities, per-project checks, version outputs and a hashed
request transcript. Temporary project/tool paths are redacted to stable labels;
fixture source text contains no user data. Baseline and sandbox reports are
separate. Installation records, a registry entry, or a test merely authored
does not establish language support. Until an actual report exists for a binary,
that binary's six-language IDE acceptance remains `NOT_RUN`.

The fixture matrix does not certify IDE equivalence, all language syntax,
third-party dependency resolution, build/launch configurations, unsupported
server capabilities, complete indexing, or JetBrains/other proprietary backends.
Real adapter versions and observed limitations must be read from the relevant
immutable report, not inferred from this list of profiles.
