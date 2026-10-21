# Independent review, phase 1

Task: language-sandbox-installers-v3. Reviewer instance: /root/independent_review_v3.
Base: 9017908f8759de74d5c3deb89e7bdb8aedcf83fd.
Reviewed source: 780c6adf7de71a6d96d3af12314dcbe9b526ad8b.
Observed: 2026-10-21 03:15:38 UTC.

Source review: PASS, no actionable P1/P2 findings in the assigned diff.
Release verification: NOT_RUN pending the final compiled candidates, language
matrix and installer lifecycle reports. This is not release approval or a claim
that the five additional sandbox languages have passed.

The reviewer used an independent worktree and did not author or edit product
sources, change OS policy/ACLs, execute cloud CI, replace a candidate binary,
or publish anything. Only the two phase-1 review files are owned outputs.

Reviewed paths cover trusted-host Ruff argv validation, bounded stdin/stdout,
Windows queued-write flush errors, cancellation lifetime guards, UTF-8 and
failure rejection, source snapshots and expected-hash Transactions. The native
formatter route is explicit; absent a profile, the LSP route retains its actual
capabilities. Range/nonempty caller options are refused; metadata omits host
program/argv. No fallback or Serena bridge was added.

Enumeration defaults empty and is separately typed, bounded and directory-only.
Volume roots are permitted only for metadata enumeration. Runtime validation
keeps executable/cwd coverage restricted to read-only/read-write grants, rejects
protected/denied storage and aliases, and requires a positive native probe.
Enumeration is emitted only under processContainer.filesystem.enumeratePaths.
This intentionally exposes directory names; it does not grant content reads.
The implementation agrees with the official [stable MXC schema](https://github.com/microsoft/mxc/blob/main/schemas/stable/mxc-config.schema.1.0.0.json)
and [OS capability contract](https://github.com/microsoft/mxc/blob/main/docs/process-container/os-version-support.md).
Kernel execution of this final implementation remains separately unverified.

Windows packaging inspects PE imports, rejects unknown/delay-load dependencies,
and selects Microsoft-signed app-local runtime from the VS redist directory.
The [Microsoft redistribution list](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution)
supports the referenced redist location subject to its license terms. The
installer remains per-user, does not modify PATH or grant host permissions, and
does not recursively delete operator-added files. The verifier checks digests,
loaded runtime paths, same-version reinstall and file retention. These are code
inspection conclusions; the new lifecycle has not yet been accepted.

Linux packaging requires target execution of the selected clean-source release
ELF before building the static-musl deb/tar. It labels guest execution separately
from native guest Cargo tests and Linux OS sandbox support. Inno is a Windows
installer builder, as its [official description](https://jrsoftware.org/isinfo.php)
states. New target build/install/stdio/HTTP acceptance is pending.

Actual reviewer commands include cargo fmt --all -- --check at both 418124e and
780c6ad, each exit 0. An independent read-only Python check at 418124e verified
byte-identical paired 85-tool catalogs; 34 Position fields with maximum
2147483647; the unchanged completion-count limit; Python AST syntax; unchanged
lockfile/toolchain manifests; and manual self-hosted-only workflow triggers.
A second check verified the historical failed language report's receipt,
transcript and log digests. Exact command bodies and outputs are in the JSON.

The v3 diagnostic language report remains FAIL, 68 PASS / 12 FAIL, on the old
f7e57a5a executable. Its normalization PASS is only byte-integrity evidence.
The earlier native stdin PASS predates later source changes. Neither establishes
final-candidate acceptance. New enumeration core evidence records 8 host-policy
tests passed, followed by an AppControl 4551 refusal to start stdin tests.
The final native-test executable was also refused with 4551. The actual commands
returned 101; no OS-policy bypass or retry under a renamed/copied executable was
performed by this reviewer.

Next owner: root. Provide the final source SHA and candidate-bound
Windows/Linux build, protocol, language and packaging receipts; request
revalidation by this same independent instance. Preserve FAIL, BLOCKED and
NOT_RUN until those specific commands execute.
