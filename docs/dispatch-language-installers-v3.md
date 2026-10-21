# Five additional sandbox languages and native installers

Base: `9017908f8759de74d5c3deb89e7bdb8aedcf83fd`.
The user authorizes implementation, verification, local Windows/Linux installer
creation, commits and pushing the existing public repository. Cloud CI is not
part of this change. Odin remains the existing CLI/MCP/local-dashboard app.

| Owner | Isolated branch/worktree | Scope |
|---|---|---|
| root | E:/odin / feat/language-sandbox-installers-v3 | core host formatter policy and bounded stdin execution, integration, final builds and delivery evidence |
| platform_implementation | odin-platform / new branch from base | explicit six-language native sandbox profiles, real two-project LSP/IDE/CAS acceptance harness and reports |
| engine_implementation | odin-engine / feat/language-ide-gap-completion | explicit Ruff stdin formatting transaction route, LSP position schemas, paired catalogs, tests and IDE docs |
| packaging_implementation | odin-packaging / feat/native-installers-v3 | Inno Windows installer, Linux deb/portable packaging, target VM and installer lifecycle evidence |
| independent review | separately assigned after integration | independent source/contract/evidence review; NOT_RUN until the actual instance is assigned |

Root owns shared Cargo manifests, lockfile and core configuration; other changes
are integrated by serialized commits. Authors must not overwrite another
worker's edits. Existing historical reports remain immutable evidence of their
own source/binary snapshots.

## Acceptance

1. Each of Rust, Python, Go, Java and C# runs real LSP workloads in explicitly
   configured MXC, with two independent projects and source read-only grants.
   Verify symbols, cross-file references, applicable IDE capabilities, rename
   and formatting transaction preview/apply/exact rollback. TypeScript is
   rerun on the same final binary. No mock or handshake-only substitution.
2. Python formatting is an explicit host-owned Ruff profile, uses bounded stdin
   in native sandbox, never writes sources itself and returns a hash-bound
   transaction. No automatic backend fallback or repository executable config.
3. Backend-unadvertised resolve capabilities remain unavailable; unversioned
   diagnostics remain unverified. Larger legal LSP positions must be accepted
   by the schema and still checked against real UTF-16 source boundaries.
4. Execute the locked Windows Rust gates, native boundary tests, actual binary
   smokes and scoped real-language/analyzer acceptance. Preserve first failures
   with actual commands, exit codes and digest-bound logs.
5. Compile the Windows app and build/verify its Inno installer and portable
   contents, including isolated install/reinstall/uninstall. Linux uses an ELF
   binary with native deb/portable installers; Inno is Windows-only.
6. Existing WSL is unavailable. Use a disposable userspace QEMU/TCG guest for
   real Linux execution and installer tests without changing Windows features.
   Label cross compilation separately from target VM execution; guest Cargo
   tests remain NOT_RUN unless actually executed. Downloads require publisher
   checksums, local management ports only and no persistent host credentials.
7. Final independent review, source/command/binary/package hashes, public-content
   scan and complete manifest precede push. Verify remote/public state after
   push. Installer artifacts do not imply all 40 requirements or full IDE parity.

Initial state: implementation and new acceptance NOT_RUN. This plan is not
execution evidence. Reports are written only after the corresponding commands.
