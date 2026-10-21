# Python cwd native diagnosis and explicit QA profile

This change contains fixed diagnostic scripts and evidence only. It changes no Odin source, language-server code, installed SDK, existing host policy, ACL or OS setting. `cargo` workspace gates are N/A for this evidence-only slice; root owns final product validation.

The installed pinned executor is `wxc-exec.exe` SHA-256 `8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259`. Its actual probe selected `base-container`, reported no DACL augmentation and no warnings, and advertised deny/enumerate support. Diagnostics used explicit C/E ancestor enumeration, narrowly granted runtime/source reads, private scratch writes, denied policy/state fixtures and network deny. No audit/learning mode was used. Host Windows reports `10.0.26300.0`; installed `bfs.sys` file version was `10.0.26100.8875`.

## Three bounded hypotheses

1. **Path spelling / initial cwd:** ordinary RO directory changes fail for either drive case and either separator. Only the exact initial cwd spelling succeeds. Native readonly descendants can start with lowercase drive names, but Windows changes a forward-slash initial cwd into backslashes. Their subsequent lower-forward `chdir`, which matches basedpyright's URI-derived path shape, still fails. The readonly bootstrap therefore does not solve this issue.
2. **Runtime version:** bundled Node `24.19.0` and host Node `24.21.0` behave identically in the observed matrix. A newer external Node is not a demonstrated fix.
3. **Native API / access mask:** direct `SetCurrentDirectoryW` returns false with `GetLastError=5` for RO transitions. `GetCurrentDirectoryW` remains unchanged. `CreateFileW(FILE_FLAG_BACKUP_SEMANTICS)` with traversal, synchronization and read-attribute masks succeeds on the same paths. Node's public cwd can be cached, so observations also capture native `.` entries and a child inheriting the actual cwd queried with WinAPI.

The [upstream MXC issue 1467](https://github.com/microsoft/mxc/issues/1467) describes the same class of RO directory failure: `FILE_DIRECTORY_FILE` plus traversal is rejected, while the corresponding non-directory open succeeds. This corroborates the diagnosis; this agent did not reproduce the issue author's kernel trace or test a released OS fix. [Node's pinned chdir implementation](https://github.com/nodejs/node/blob/v24.19.0/src/node_process_methods.cc) delegates to libuv; [pinned libuv](https://github.com/nodejs/node/blob/v24.19.0/deps/uv/src/win/util.c) invokes `SetCurrentDirectoryW` before updating its hidden drive environment value. Thus the observed error is not evidence of a successful chdir followed by an environment update error. [Microsoft's API contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setcurrentdirectory) defines a zero return plus `GetLastError` as failure. The observed upstream OS/driver versions differ from this host, so no exact kernel-patch attribution is claimed.

## Explicit QA-only private backend profile

Root explicitly authorized a fresh, per-project writable copy of installed basedpyright `1.40.2` after the readonly bootstrap failed. Both copies' 5,427 files were independently hashed against the original, including JS and typeshed. The successful report is `python-cwd-v3-private-20261021T033321Z.json`; its per-file source manifest is `python-cwd-v3-private-20261021T033321Z-files.json`.

For each project, fixed host argv is:

```text
<original pinned bundled node.exe> --preserve-symlinks --preserve-symlinks-main <fresh-private-backend>/basedpyright/langserver.index.js --stdio
```

Only that project's backend directory and private scratch are RW. The original Python/Node runtime and project source remain RO. Other project source/backend/scratch, policy and state fixtures are denied. Enumerate rules contain grant-root ancestors only: listing the RW backend root itself in `enumeratePaths` overrides its RW access. That initial failed profile and its original caches are retained in `python-cwd-v3-private-20261021T033218Z.json`.

The corrected profile passed eight copy-directory chdir variants and 36 boundary checks across two distinct projects. Those checks include permitted reads and private writes; denied C/E outside content/overwrite; denied other project source/backend content/overwrite; denied state/policy reads; source overwrite/create rejection; non-mutating write-open rejection on original SDK metadata; and loopback rejection. Host-side protected bytes and original assets remained unchanged, forbidden source files were absent, both copies still matched the source manifest, and the loopback listener accepted zero connections.

This is **PASS_NATIVE_PROFILE_ONLY**. Full MCP semantic operations, source transaction apply/rollback, language diagnostics and backend cleanup are **NOT_RUN by this agent** and handed to `/root/platform_implementation`. The fixed test does not execute the copied language server. It proves the narrow native cwd and isolation preconditions only.

The backend can modify its own private copy. Never execute such a copy outside the native sandbox, reuse it as trusted installed tooling, or include it as a published SDK. Each QA fixture must recreate and hash a new copy from the pristine pinned source. Production performs no automatic copy, permission enablement or fallback as part of this change.

## Commands and immutable reports

- `python verification/python-cwd-v3-probe.py`: exit 0, six native observations; RO failure and 40 boundary checks captured in `python-cwd-v3-20261021T032507Z.json`.
- `python verification/python-cwd-v3-probe.py --descendant-lower`: exit 0; lowercase-backslash descendant observations and 20 boundary checks in `python-cwd-v3-20261021T032837Z.json`.
- `python verification/python-cwd-v3-probe.py --descendant-forward`: exit 0; forward initial spelling normalizes to backslashes; 20 boundary checks in `python-cwd-v3-20261021T032926Z.json`.
- Initial `python verification/python-cwd-v3-private-copy.py`: syntax error before execution was corrected; the subsequent first native profile ran and returned exit 1 (**FAIL_NATIVE_PROFILE**) due to the overlapping enumerate rule. Its report is `python-cwd-v3-private-20261021T033218Z.json`.
- Corrected `python verification/python-cwd-v3-private-copy.py`: exit 0, **PASS_NATIVE_PROFILE_ONLY**, in `python-cwd-v3-private-20261021T033321Z.json`.

Reports preserve the script hashes used at each attempt. Earlier probe source evolved to add the fixed descendant variants and the non-mutating original-SDK write-open check; the successful private-profile report binds the final executed private-copy script/helper hashes. Failed caches and successful QA caches remain at their explicitly recorded `python-cwd-v3-private-*` paths for inspection; no existing cache was modified or removed.
