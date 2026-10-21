# OS sandbox and native language/IDE implementation

Baseline: `9b71abc98b89f72cce08d2795dfb139281f6d613`.
The user authorized implementation, local verification, commits and pushing the
existing public repository. No cloud CI dispatch is part of this change.

| Owner | Isolated worktree / branch | Owned implementation |
|---|---|---|
| root | E:/odin / feat/os-sandbox-language-ide | host configuration, shared types, language adapters, process integration, real backend acceptance and final evidence |
| platform_implementation | odin-platform / feat/os-sandbox | native MXC preparation/guard, filesystem/network/stdio/process negative tests, then real six-language/two-project acceptance harness and runtime reports |
| protocol_implementation | odin-protocol / feat/language-ide-protocol | capability-checked IDE LSP requests, document freshness and protocol regressions |
| engine_implementation | odin-engine / feat/ide-native-tools | IDE tools, native inspection normalization/fix previews, schemas, paired catalogs and API documentation |
| independent_review | independent instance, after integration and fixes | read-only correctness/security review, source hashes, findings and re-review |
| requirements_audit | odin-requirements / docs/os-sandbox-language-ide-evidence | requirement summaries, unchanged historical evidence bindings, explicit remaining acceptance |

Shared configuration and Cargo manifests/lockfile have one owner: root.
Changes are integrated by serialized cherry-picks, never by replacing another
author's working tree. Existing worktrees are reused only after checking clean
state and preserving their old branches.

Acceptance requires a real Windows kernel boundary with denied out-of-policy
filesystem and network access, transparent LSP/DAP stdio, no insecure fallback,
and termination/cancellation coverage. Sandbox enforcement is explicitly opted
into trusted host TOML; existing permissions do not become enabled implicitly.
Policy files are generated under protected host state, not loaded as executable
repository configuration. Telemetry and DACL-mutation fallback remain disabled.

IDE requests require true backend capabilities and source freshness; unsupported
features return explicit errors. Every source edit remains a hash-bound
Transactions preview/apply operation. Inspection output is provider-labelled;
neither regex nor protocol peers are language-support evidence.

Run the required locked workspace gates, real language/provider and binary smoke
tests, then independent review and applicable regressions. Record command exit
codes, source/binary digests and remaining unsupported cases. Only verified
language/capability cells may be labelled PASS; a registry entry is not proof.
