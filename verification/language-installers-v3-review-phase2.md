# Independent review, phase 2

Task: language-sandbox-installers-v3. Reviewer: /root/independent_review_v3.
Incremental base: 780c6adf7de71a6d96d3af12314dcbe9b526ad8b.
Reviewed source: 75c38f86f97e4ec14c840a8cb80afc80fa098674.
Product Rust/Cargo/schema contents remain identical to compiled source 69e769b.
Prior independent assessment: verification/language-installers-v3-review-phase1.json.

Source review: PASS, no actionable P1/P2 code findings in the assigned delta.
Final release approval: BLOCKED. This document does not certify that the
remaining five sandbox languages or the final installation packages are done.

The isolated LSP now sends a null processId while the ordinary host LSP retains
the current client PID. Native guard/job lifetime management is unchanged.
The nullable field agrees with the official [LSP 3.17 initialize contract](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.17/general/initialize.md).
The direct OmniSharp null-PID diagnostic supports this correction, but its
driver exited 1 during cleanup. Its successful symbols/shutdown are not full
product or cleanup acceptance. A current-source protocol receipt records
31 passed / 3 ignored; fixtures are not native C# language proof.

Python's private-backend option is explicit and limited to sandbox QA with
ancestor enumeration. It creates a fresh per-project copy from the pinned
installed metadata and 5,427-file manifest, verifies the copied bytes and checks
them again after acceptance. Node/Python, original SDK assets and project sources
remain read-only. The writable cache is separate from every source/state root.
Other project sources/caches/scratch and state are denied. Enumeration excludes
writable roots and their descendants, avoiding an enum rule overriding RW.
The host version probe uses the original Python metadata reader; it does not
execute the writable JS copy on the host. There is no product copying policy,
language-server patch or automatic fallback.

The writable backend can modify itself and must not be trusted after use.
Never host-execute, reuse as installed tooling, or publish such a copy as an SDK.
The fixed QA setup and its recorded risk reflect an explicit host decision.
The [upstream readonly-cwd report](https://github.com/microsoft/mxc/issues/1467)
corroborates the observed behavior; its OS/driver versions do not establish an
exact kernel-patch attribution for this host.

An executed read-only reviewer check verified 13 diagnostic artifact digests,
the 5,427-file manifest binding, two native exits, all 36 recorded boundary
checks, original/private byte preservation and zero loopback connections.
A second check verified the old-binary Python MCP report, request/log digests,
current harness hash and both complete private manifests. Its 60 PASS /
4 UNAVAILABLE results belong only to binary 371d4fbd...2b12451, source 780c6ad.
Unavailable LSP formatting/hint resolution remain unavailable; Ruff is separate.

Windows recipes now import the standard modules from PSHOME and preserve
JSON-array shape on PowerShell 5. The shipped host template is inert, grants
write/shell/debug/inspections/formatters=false and is not auto-loaded.
The 22-check installer lifecycle PASS applies to the old 780c6ad binary and
earlier template. It does not certify the final PID-corrected candidate/template.

The Linux recipe keeps musl-gcc for bundled C compilation and uses cc by default
for Rust's self-contained static CRT. No product cfg or security gate changed;
PT_INTERP still causes packaging refusal. The preserved 9017908 Odin build
succeeded but faulted at startup. Two tiny driver probes isolate the linker
difference; they do not certify final Odin execution, protocols or installation.

The final Windows build produced SHA-256
83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44.
Reviewer read-only PE inspection confirmed AMD64 PE64, 14,023,168 bytes, known
Windows/API-set imports plus vcruntime140.dll, and no delay-load imports.
The reviewer did not start, copy or rebuild this candidate. Its native execution
remains blocked by AppControl 4551; administrator policy allowance is pending.

The first build receipt's metadata_exit_code=0 was stale PowerShell state after
an invocation error, with empty stdout. The original receipt remains preserved.
The corrected receipt uses null plus BLOCKED_BEFORE_EXECUTION and binds the
actual Python CreateProcess denial and CodeIntegrity report. Reviewer digest
checks verified this correction. These four receipts were subsequently committed
in 39d782c and are included in the reviewed snapshot. Their exact hashes are
recorded in the companion JSON.

The QA continuation runner validates a bounded filename prefix and checks all
child report destinations before native execution or log creation. Five inert,
mocked regression checks recorded zero native process calls. The offline index
checks historical digests without executing a backend; reviewer assertions
verified all 64 indexed artifacts. These are evidence-integrity checks only.

The actual workspace receipt remains exit 101, 214 passed / 5 failed /
6 ignored. Five stdin workloads were refused by AppControl. Neither the
31 fixture passes, old-binary language successes, prior installer, compiled PE,
nor successful Hello program substitutes for these failed or missing gates.
The reviewer also executed fmt exit 0 at the reviewed snapshot, AST/template
checks, and evidence-integrity checks; command bodies/results are in the JSON.

Next owner: root. Preserve the frozen Windows candidate and host policy.
After the administrator confirms its own state change, perform the final
candidate-bound Windows protocol/language/installer runs. Independently finish
the final Linux ELF/stdio/HTTP/deb lifecycle. Request review of that evidence
before describing this extension as fully delivered.
