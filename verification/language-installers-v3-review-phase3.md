# Language sandbox and installers v3: independent review, phase 3 interim

Reviewed snapshot: `0b7b77476a64828c90c2b5d94e622f7fda59cbae`.
Actor: `/root/independent_review_v3`; branch:
`feat/review-language-installers-v3-phase3`.

**Source review: PASS. Present documentation/evidence classification: PASS.
Release approval: BLOCKED. Fixed-source Linux native/package evidence:
NOT_RUN (pending delivery).**

This review extends phase 2 at `75c38f86f97e4ec14c840a8cb80afc80fa098674`
and began from `bb1042c6f153ad4f3597743b3a3980aac4968ef2`. The reviewer
did not author the reviewed source, change product files or host policy,
execute/copy/rebuild the blocked candidate, access the VM serial console, push
Git commits or dispatch cloud CI. The only owned outputs are this document and
its JSON companion.

## Finding and recheck

**R3-E01, P2: report presence promoted unexecuted Linux criteria.**
At `2145fb8`, `verification/language-installers-v3-summarize.py:179`
updated every ODIN-029 acceptance result to PARTIAL whenever `--linux-report`
was supplied. Generic hash binding did not validate source or target. A build
or metadata report could consequently promote dedicated signal/journal
acceptance or archive consistency without corresponding execution evidence.

The author fixed this in `908ff9b6918a88622f53cf7673af46bb7c746f78`.
Static source/AST inspection confirms canonical report scope, selected source,
target, clean release metadata, ELF identity and raw file/command-log digests.
A1 and A3 now have independent evidence conditions; A2 remains NOT_RUN.
General MCP cancellation does not establish dedicated SIGTERM/SIGINT LSP/DAP
descendant cleanup and journal acceptance. The actual author receipt records
12 inert classifier regressions, exit 0 and zero native calls. Its stdout/result
and source hashes matched the source at908; these are historical QA checks,
not native Linux acceptance or tests of the later source constants.

At `0b7b774`, selected Linux source is d3e5505 and its replacement ELF hash is
unset. The classifier rejects a canonical Linux report before reading it until
a real replacement binary is selected. The Windows source remains69. The
reviewer's new read-only target-provenance assertions exited 0. Finding state:
**SOURCE_FIX_VERIFIED_PENDING_ACTUAL_REPORT**. No further P1/P2 was found in the
reviewed changes. Final report integration still requires a later recheck.

## Unix durability fix

`d3e550594783ee9881c6aba251403ff1add0af86` changes two Unix-only branches
in `crates/odin-core/src/paths.rs`: after rename/unlink,
`parent.open(".")?.sync_all()?` replaces fsync of a cloned/consumed directory
capability that could be O_PATH. Both errors still propagate. Parent-handle
acquisition, no-follow traversal, rename, Transactions and expected-hash checks
are unchanged. The architecture document states this boundary.

The reviewer independently inspected locally cached primary source for locked
cap-std/cap-primitives4.0.3. `Dir::open` supplies read(true) to the held file;
Linux openat2 uses that fd with BENEATH/NO_MAGICLINKS. Its manual resolution
also reopens CurDir relative to the held base using the read options. Those
options select RDONLY rather than O_PATH; `File::sync_all` delegates to std.
This does not reopen an ambient absolute path, clone O_PATH again or suppress
fsync errors. Exact primary-source file hashes are in the JSON companion.

The reviewer's static diff/route assertions exited 0. The parent reports that
the prior Linux69 workspace run exited101 with core33PASS/6FAIL from real
fsync EBADF/RecoveryRequired. Raw logs are pending in this snapshot, so this
review does not claim independent execution verification of that run.
The fixed source's native core/full gates, release binary and packages remain
pending. No Windows binary was rebuilt as part of this review.

## Evidence boundaries verified

The prior delivery/feature JSON snapshots exactly match Git75c38f8 bytes:
`fc7c8440b7501e255890d99ce8539f6e92cdec9a4be788b41da112b68ae677a0`
and `5d7a8e1e4e2706356f0b252ee06bdb40d89dddaa95661940ddc45b1c699b9838`.
All 40 original requirement status/check/acceptance objects remain unchanged:
38 PARTIAL and 2 NOT_RUN. Registry/backlog state the remaining work.

Current source fingerprints cover62 tracked product files and match d3e5505
byte for byte. The only Rust/Cargo/schema delta from Windows69 is paths.rs.
Delivery metadata distinguishes current Linux-selected source from the
retained Windows compiled source; it does not claim that all current bytes
were compiled into Windows83.

The final Windows source is `69e769be9baa4e825139b72ce1a9b48bf8143114`;
its binary SHA-256 is
`83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44`.
Build exit0 is compilation evidence. Corrected metadata records no execution
and null exit after actual CreateProcess was denied with WinError4551.
Administrator allowance remains unconfirmed; six-language matrix/final
installer remain NOT_RUN. Actual Windows69 fmt and strict all-target Clippy
receipts/log/source hashes were verified, exit0, with the blocked binary hash
unchanged before/after. Those checks do not establish execution of Windows83
or testing of the subsequent Unix source change.

Historical Windows780 SHA-256 remains
`371d4fbd36fffa334b0ef052d4d1157c1c799562a3dd8fd619eebfefe2b12451`.
The workspace gate retains exit101, 214 passed /5 failed /6 ignored.
The13 continuation commands and22 installer checks do not replace that gate
or certify Windows69/83. Direct Csharp still records driver cleanup exit1
despite real symbols/server exit0.

Prior Linux69 ELF5b1f does not certify d3e5505. No canonical
`verification/package-v3-linux-final.json` is present and no replacement ELF
has been selected. Build/test/package/lifecycle results remain NOT_RUN here.

## Reviewer verification and handoff

The JSON companion preserves exact executed read-only commands, source SHA,
output and exit code for summary integrity, candidate classification, Unicode
inspection, classifier source/author-receipt verification, Unix durability
diff/routing, target provenance and file indexing. All these shell checks
exited0. One index-output JSON parsing attempt was truncated by the tool's
token budget; re-reading with a sufficient output budget corrected that
reviewer transport issue. No product failure or test result was inferred.

The reviewer did not execute the assembler, classifier or native product,
repeat author fmt/Clippy checks, or access VM serial. The12 inert regression
checks are author-run QA evidence, not native support proof.

Root/tech lead owns Linux raw failure logs and fixed-source canonical report,
gates, manifests, archives and final documentation integration. Reinvoke this
reviewer on that immutable SHA to finish evidence verification in the same
owned phase3 files. The Windows release owner must retain the blocked
candidate until actual authorized allowance and same-candidate tests.
Dedicated signal/journal acceptance, Linux OS sandbox, macOS, full language/IDE
parity, clean-machine installation, signatures and all40 requirements remain
incomplete or unverified. No current-extension publication or binary release
is claimed.
