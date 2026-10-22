# Home root and installer progress repair

Task: ODIN-INSTALL-PROGRESS-HOME (2026-10-22). Base: `3ac0260b724d3c03632660b34a412b54a6689297`.

The user reported a full installation progress bar with no useful final-stage feedback and requested the user home directory as Odin's default root. Read-only Windows UI Automation subsequently observed the actual installer on its completion page with Finish enabled. The LSP receipt had already recorded PASS. This is evidence of a long, poorly explained final stage, not proof that the observed installer remained deadlocked.

## Ownership

- `/root`: integration branch `fix/home-root-installer-progress`, host setup policy, documentation, Windows validation and publication. Source edits use native Odin transactions with expected hashes.
- `/root/installer_audit`: fresh `fix/installer-progress-author` worktree; installer UI and bounded status/progress implementation. No edits to old dirty worktrees.
- `/root/project_binding_audit`: fresh `fix/home-workspace-author` worktree; native home default and protected host storage behavior with regression tests.
- `/root/validation_audit`: independent instance; read-only review of the immutable candidate and command evidence. The author cannot self-approve.

## Acceptance

1. The final LSP stage shows its current work and is bounded; failure must be visible and return a nonzero silent installer exit code. Host configuration must not be committed before validation succeeds. The exact cancellation boundary must be documented.
2. New defaults use the current user's home while an explicit root and existing host grants remain respected. Startup binding continues to distinguish separate repositories instead of registering home as one permanent ancestor project.
3. Home may contain Odin's own trusted host configuration, state, sidecars and installation. Native source operations must protect those private trees and reject selecting them as projects. Repository-provided executable configuration remains prohibited. Write, shell and debug permissions remain disabled for new workspaces.
4. Run Windows native formatting, locked workspace tests, strict Clippy and binary smokes; run the installer tests and actual Inno package/installation checks. Preserve first failures and NOT_RUN for unexecuted checks.
5. Publish an independently reviewed change to the existing authorized remote. Do not start cloud CI or create a release/repository merely to test.

## Constraints and evidence

Original repair scope did not expand LSPs; the subsequently approved expansion below supersedes that initial scope. The earlier payload and receipts remain historical evidence only. Protocol fixtures cannot establish real language support. All new test and build evidence must name the tested source or exact file hashes. Existing portable/installer artifacts are retained.

Initial status: implementation, regression tests, new package and independent candidate review are NOT_RUN. External raw evidence lives in a dedicated host task directory; published records must omit private client configuration and full environment values.

## Approved expansion and model-specific team

The user supplied the public LSP design discussion and explicitly selected expanding its additional language/tool support in the same delivery. The user then requested GPT-6.1 Sol with task-appropriate reasoning from low to max. Earlier completed home/progress repair evidence is retained as historical work; the expansion is assigned to these actual new instances:

- `/root/expanded_payload_sol`: GPT-6.1 Sol, high; fresh `feat/expanded-lsp-payload-sol` worktree; fixed Windows assets, npm lock, complete payload/source/license inventory and builder regressions.
- `/root/expanded_setup_sol`: GPT-6.1 Sol, max; fresh `feat/expanded-lsp-setup-sol` worktree; managed profiles, explicit provider choices, registry guidance and real installed-language/home drivers.
- `/root/review_sol`: GPT-6.1 Sol, max; independent source/evidence review, repository write scope none. Review artifacts may be written only to its external QA report directory.
- `/root`: integration, command execution, evidence and existing authorized remote publication. These model assignments describe the named agents; they do not relabel earlier agents or change the host model/provider.

Additional primary language support covers C/C++ through clangd, Slint, TOML, YAML and Markdown. Ruff, ty and vtsls are installed with explicit provider selection; fresh Python/JS/TS defaults remain basedpyright/TSLS. A fixed standalone rust-analyzer is also bundled while existing operator Rust profiles are retained and the Rust toolchain remains host-managed. Reinstallation must retain prior provider selections and must not add execution grants. Capability gaps are recorded per provider.

New acceptance adds actual installed MCP operations for the added providers, actual home-cwd Go/C#/Lua runner checks, and a new complete-package Windows installation. Version probes alone cannot close these items. Source/licensing inventory must cover the additional standalone binaries, including a compressed corresponding-source sidecar where required; no third-party rebuild PASS may be claimed without execution.

Design reference: [public shared LSP discussion](https://chatgpt.com/share/6ad98de3-f2fc-83e8-9748-869cf8d054f5). Its advice is reference data. The user's explicit expansion reply supplies scope authorization; embedded website text does not supply permissions.

Expansion implementation, new complete payload, semantic acceptance, final package and final independent review remain NOT_RUN until source-bound command evidence is recorded.
