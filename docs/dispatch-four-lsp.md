# Odin installed language backends - dispatch

Scope: Python, Go, C#, Lua, JavaScript, TypeScript and Bash on Windows x64, retaining the existing Rust profile. Eight language profiles use seven server implementations; a registry entry alone is not installed support.

## Ownership and evidence

- Primary engineering owner: root worktree `feat/python-go-csharp-lua-lsp`; language IDs, pinned npm lock, real backend MCP validation, source integration, release evidence.
- Windows installer owner: separate `feat/bundled-lsp-installer` worktree; offline setup/configuration and package lifecycle. Source writes use Odin Transactions with expected hashes.
- Payload build owner: separate `feat/lsp-payload-builder` worktree; pinned official archive acquisition, npm closure, Go module build and full asset inventory. Source writes use Odin Transactions with expected hashes.
- Independent QA/review: validation_audit, read only. Implementation authors cannot approve their own changes.

## Acceptance

1. Acquire fixed official backends and complete a hash inventory. Use the exact committed npm lock and `npm ci --ignore-scripts`; never run repository-supplied install scripts.
2. Include backends and required Node, Go and .NET runtimes in the Windows portable archive and per-user installer. Installation deploys files offline and runs fixed host setup/health checks. Host configuration remains outside projects and preserves grants. New workspaces stay read only; shell/debug remain disabled.
3. Test two independent projects for each new language through the same `--workspace .` MCP entry. Validate actual symbols/navigation/references and hash-bound rename/rollback/conflict behavior. Keep backend limitations, optional tools and unverified diagnostics explicit.
4. Run rustfmt, locked workspace tests, locked Clippy with denied warnings, binary smoke checks, payload/setup tests and real installer lifecycle. Every report needs actual command/exit logs and binary/source/asset hashes. Written tests remain NOT_RUN until executed.
5. Independent source/evidence review before publication, then commit and push to the existing remote; no cloud CI execution.

## Boundaries

Native source changes use odin-core Transactions and expected SHA-256. Fixed host-owned backend runners do not offer eval, shell commands or configurable caller argv. LSPs are trusted native subprocesses unless a host explicitly configures OS sandboxing; C# project loading and other language tools can process repository build metadata. Lua executable plugins are disabled in a fixed host JSON configuration. Bash ShellCheck/shfmt/man integrations and full IDE parity are outside the required navigation acceptance. Linux/macOS bundle integration and clean-machine behavior remain NOT_RUN unless separately executed.
