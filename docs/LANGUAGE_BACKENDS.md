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
initial valid-source query runs before the first source mutation. The
broken-source edit and restoration go through Odin Transactions. Only a current,
version-verified report with `summary.complete=true` can pass. Unversioned push
diagnostics remain `NOT_VERIFIED`, even when they contain the expected error or
an empty list. None of these results is represented as a successful build or
proof that every project file was analyzed.

The installed TypeScript language server 6.0.2 suppresses consecutive empty
diagnostic notifications and publishes diagnostics without a document version.
An apply/rollback can therefore invalidate a cached clean report without a new
empty notification. The preserved `language-ide-final-baseline-02` attempt
returned `FAIL` for both initial-clean checks after rename/format mutations.
The subsequent driver observes initial diagnostics before those mutations;
broken-source and recovery checks retain the same strict assertions. A pending
report is still a failure, and an observed unversioned report remains
`NOT_VERIFIED`. Changing the order does not establish current clean diagnostics
after every transaction.

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
  --sandbox-typescript-init-temp --sandbox-typescript-child-path-flags `
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

The explicit `--sandbox-typescript-init-temp` profile generates a fixed module
under the external trusted `host-support/<project>` directory. It creates
`os.tmpdir()` inside MXC's existing private scratch grant, verifies the selected
TSLS module SHA-256, sets argv to that module and `--stdio`, and imports it. The
server path and expected hash are constants generated from the host-selected
installation; the bootstrap accepts no caller or repository target. The report
retains its complete source/hash, server path/hash and profile argv. Only that
bootstrap file receives an additional read-only grant; source/runtime grants,
private scratch writes and network denial retain their original scopes.

Without this explicit profile, `language-ide-final-sandbox-typescript-01`
preserves the actual startup failure: TSLS calls `realpath(os.tmpdir())` before
MXC's package-specific `AC/Temp` exists. The fixed bootstrap is test host setup,
separate from Odin's native runtime. It uses ordinary module loading, keeps stdio
unchanged and performs no fallback or permission retry. Its generated source is
a reproducible example in the report's `host_support` entries.

The separate `--sandbox-typescript-child-path-flags` opt-in sets the constant
`NODE_OPTIONS='--preserve-symlinks --preserve-symlinks-main'` inside that bootstrap
for its descendants. It accepts no inherited option text. Installed TSLS 6.0.2
explicitly constructs child `execArgv` without its parent's path flags; the
temp-only attempts 02 and 03 preserve actual tsserver-child exit failures.
Their captured stderr does not establish a specific child filesystem error.
The fixed-options profile tests this source-based compatibility hypothesis,
without changing Odin's environment allowlist, OS grants or server semantics.

## Executed Windows candidate

The final runs on 2026-10-21 used the same release executable, SHA-256
`f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`.
The [six-language baseline report](../verification/language-ide-final-baseline-03.json)
records 12 real LSP initializations and **260 PASS, 14 UNAVAILABLE,
12 NOT_VERIFIED, zero FAIL**. Its [actual command receipt](../verification/language-ide-final-baseline-03.command.json)
records exit **2**, consistent with the report's `PARTIAL` status.

| Baseline language | Projects | PASS | UNAVAILABLE | NOT_VERIFIED | Project results |
|---|---:|---:|---:|---:|---|
| Rust | 2 | 48 | 0 | 0 | PASS / PASS |
| Python | 2 | 40 | 4 | 0 | PASS / PASS |
| TypeScript | 2 | 40 | 2 | 6 | PARTIAL / PARTIAL |
| Go | 2 | 44 | 4 | 0 | PASS / PASS |
| Java | 2 | 40 | 2 | 6 | PARTIAL / PARTIAL |
| C# | 2 | 48 | 2 | 0 | PASS / PASS |

All twelve projects passed cross-file references and real rename transactions
with exact rollback. All six backends returned meaningful completion, hover,
signature help, workspace symbols, incoming calls, inlay hints and semantic
tokens. Formatting transactions passed for the five backends that advertised
formatting. Python's backend did not advertise formatting; Go did not advertise
completion resolution. Only Rust advertised inlay-hint resolution. Missing
capabilities retain the table's `UNAVAILABLE` counts.

Rust, Python, Go and C# passed current/version-verified valid, error and recovery
diagnostics. TypeScript and Java returned all six expected diagnostic observations
per language without document versions, so all twelve remain `NOT_VERIFIED`.
These results do not establish complete indexing or diagnostics for every file.

The [TypeScript native sandbox report](../verification/language-ide-final-sandbox-typescript-04.json)
records two projects, **42 PASS, 2 UNAVAILABLE, 6 NOT_VERIFIED, zero FAIL**;
its [actual command receipt](../verification/language-ide-final-sandbox-typescript-04.command.json)
also records exit **2**. Both projects passed the pinned MXC transport assertion,
symbols, cross-file references, the available IDE operations, rename apply and
rollback, and formatting apply and rollback. Diagnostic freshness remains
unverified for the same backend limitation as the baseline.

That successful profile explicitly selected both fixed-bootstrap options and
`allow_ui=true`. Its source and runtime grants remain read-only, the bootstrap
has one exact read-only file grant, and only private scratch is writable.
Constant descendant path flags resolved the observed child-exit problem; the
captured stderr did not directly identify an `EPERM` cause. These are actual
Odin/MCP/LSP results under the native MXC route. OS denial and child-lifecycle
tests provide separate boundary evidence; handshake/semantic success and the
route metadata do not set `os_boundary_verified=true` or prove full UI isolation.
At the time of that report, actual sandboxed LSP workloads for the other five
languages were `NOT_RUN`. The later v3 attempts below preserve their own scope.

The baseline used harness SHA-256
`d47fa6daa7c96aa8d5bc7b7c2603755b4047c4e3ed0c3d740b358f9e931a6e09`;
the sandbox run used
`1160258157d2790782e9060be0556af874397daf741196ef32297435c6c0dbf6`.
Later harness changes added only explicit sandbox compatibility profiles and
fail-fast handling of permanent backend exits. The completed baseline was not
reclassified or rerun. All failed attempts remain preserved alongside the
final reports. Their companion raw stdout, command receipts, transcripts and LF
normalization receipts bind the actual exit codes and artifact hashes.

## Windows native profiles: v3

The explicit sandbox profiles now select the actual installed toolchain/runtime
and give each project its own external backend cache. Source directories remain
read-only. All profiles retain MXC's network denial, no-fallback policy and
private scratch. `--sandbox-allow-ui` remains an explicit host choice, not a retry
after an isolation failure or proof of full UI isolation.

| Profile | Read-only runtime grants | Explicit writable cache |
|---|---|---|
| Rust | actual selected Rust toolchain, avoiding the Rustup proxy | private Cargo home and analyzer target directory |
| Python | installed venv, bundled Node and base Python installation | none beyond private scratch |
| Go | gopls and Go SDK | per-project GOPATH, GOCACHE and GOMODCACHE |
| Java | JDK and JDT LS installation | fresh Equinox configuration, workspace, home and JVM temporary directory |
| C# | csharp-ls and .NET SDK | separate CLI home, NuGet cache and MSBuild obj/bin directories |

Rust's host-authored settings request `--locked --offline` for Cargo, disable
build scripts/proc macros and select the external target directory, using the
[documented analyzer configuration](https://rust-analyzer.github.io/book/configuration).
The C# fixture's trusted `Directory.Build.props` sets the
[MSBuild output and intermediate paths](https://learn.microsoft.com/en-us/visualstudio/msbuild/common-msbuild-project-properties).
The fixture contains no third-party package sources. Java uses a fresh copy of
the installed `config.ini`, external `-configuration`/`-data`, headless JVM
settings and disabled automatic builds. These are host-selected test profiles;
they are not executable configuration discovered from an untrusted repository.

The [first v3 diagnostic attempt](../verification/language-sandbox-v3-diagnostic-01.json)
used the previous frozen executable, SHA-256 `f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`.
It actually returned exit **1**, **68 PASS / 12 FAIL**. Both Rust projects passed
all 25 checks each, including live semantic operations, cross-file CAS rename,
format transactions and versioned diagnostics. Python failed during the
installed server's `chdir` with `EPERM`. Go initialized but reported `no views`;
Java failed while initializing `sun.security.jca.Providers`. C# initialized and
identified the installed SDK, but Roslyn's BuildHost named-pipe connection threw
`UnauthorizedAccessException`. Those four languages did not pass semantic
acceptance in this attempt. Later fixes and candidate runs require new reports;
these failures remain unchanged.

`verification/language-sandbox-v3-runner.ps1` records the exact driver argv,
binary/harness/runner hashes, actual exit code, raw stdout and transcript. It
requires fresh output names, normalizes text to LF before final hash binding,
and verifies that normalization preserves every logical check result.

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
