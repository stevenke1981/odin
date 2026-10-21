# Language sandbox and installers v3 — phase4 independent review

Reviewed snapshot: `bf08a08bac1fef1a4364fa5b382164ae19668101`.
Serialized delivery snapshot: `e87da8a1741c90e540a82581c4b3c423ac43c54b`.
Reviewer: `/root/independent_review_v3`, separate from the implementation authors.

**Source review PASS. Documentation/evidence review PASS for the present receipts. Release approval BLOCKED.**
No open P1/P2 remains in the reviewed changes. R4-E01 is closed against the actual final pipeline. The source preview can carry these bounded results; the retained Windows release candidate and the original full-feature acceptance remain incomplete.

## Source and candidate identities

| Scope | Identity | Result |
| --- | --- | --- |
| Current application source | `0fe03ba30da9600c25588d506d7e094df89cf56f` | Source review PASS |
| Current Linux packaging/docs | `f5878bfa725db8c0dcab7e45ca3cf89159508975` | 868 Git blobs, 99 product files equivalent to the app source |
| Linux ELF | `e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42` | 14,550,736 bytes, ELF64 x86-64 static PIE; no PT_INTERP or DT_NEEDED |
| Retained Windows source | `69e769be9baa4e825139b72ce1a9b48bf8143114` | Build exit 0; actual runtime blocked |
| Retained Windows EXE | `83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44` | Independently rehashed, never executed/copied/rebuilt by reviewer |

Phase3 JSON/Markdown retain their original hashes `2d4bac2d…5bb480` / `5e7e6b10…1e24cb`. Neither file was changed. Earlier phase reviews cover the host formatter, typed enumeration grants, transaction/CAS routing, explicit backend failures, isolated LSP PID and Unix directory-sync changes; this phase reviews their subsequent source and evidence deltas.

## Source corrections

The hierarchy quota unit sets the counter at the actual 4 MiB limit minus 8. The separator leaves 7 bytes, so a small escaped item's JSON envelope reaches the real bounded serializer's LIMIT_EXCEEDED path. It retains the 1-second deadline and checks that failure does not change retained_bytes. Production LSP bytes, byte/time quotas and error propagation are unchanged; the separate expired-serialization test remains.

The original d3 assertion did not print its Fault. TIMEOUT is an inference from the deadline-first check order, not an observed error value.

The URI correction rejects a raw fourth slash with bounds-safe get(8), after case-insensitive file:/// validation and before URL parsing. Existing three-slash positives and encoded-separator/NUL/authority/query/fragment rules remain. Four/five-slash and uppercase-scheme negatives are covered. [RFC 8089 E.3.2](https://www.rfc-editor.org/rfc/rfc8089.html#appendix-E.3.2) describes these UNC forms; the tagged [url 2.5.7 file parser](https://raw.githubusercontent.com/servo/rust-url/v2.5.7/url/src/parser.rs) explains why the raw guard must precede normalization.

The actual combined Windows receipt has 35 protocol tests, fmt and strict workspace/all-target Clippy passing. Its 62 source fingerprints and log hashes match 0fe. These unit/static results do not establish execution of the blocked Windows release app. The Debian Python command alias is a development fixture dependency.

## Linux execution and final package

The original application execution is preserved in [the functional report](package-v3-linux-final.json), SHA-256 `c1a9f4a750660873c55be1294a431c048fa3f0094c6f1a314d46f448cb62af93`. Its actual locked/offline workspace run records **211 PASS / 0 FAIL / 3 ignored**, with fmt, strict Clippy and the regular optimized release build exiting 0. Focused core 39 and protocol 35 are separate runs and are not added to 211. CLI 64, MCP 24, HTTP 36 and cancellation 8 checks passed; fixture smokes do not prove real language-server parity.

The original a839 package omitted standard-library/native-runtime notices beyond the Cargo inventory. That package, its first Git-ownership preflight failure (outer 1 / Git 128, before dpkg), and its eventual functional lifecycle remain immutable historical evidence. The repair changed only two read-only Git preflights to run as the checkout owner; it did not change global Git trust.

The final [notices package report](package-v3-linux-notices-final.json), SHA-256 `3a25fd4b2f77556b844a648cec36b4b50218c004c8220639671461abbc1eed62`, binds the **same ELF** to the new docs source and a fresh install/reinstall/remove lifecycle. Its 19 application-command receipts explicitly say REUSED_ORIGINAL_APPLICATION_EXECUTION; three package/checksum/installer commands are newly executed. The reviewer verified all 53 copied raw files against the original transfer manifest, original transfer UTC values and original argv/exits/elapsed times. Missing absolute command timestamps remain NOT_RECORDED.

The reviewer independently verified 92 reference occurrences / 87 unique references, all 81 transferred files (25,820,697 bytes), all 688 app-source and 868 package-source Git blobs, and the complete 99-file product-equivalence set. Both archives were read without filesystem extraction or execution: all 48 inventory payloads match, tar has 49 regular files, Debian has 50 regular files including its copyright copy, and the binary in both equals the sealed ELF. Modes/paths are safe, no maintainer scripts are present, and the five shipped example permissions are false.

| Final artifact | SHA-256 |
| --- | --- |
| Portable tar.gz | `1a0afa762e48f9ee119e79629cc6fffb2ee997c74af129b824dfb3547f8c6d42` |
| Debian amd64 .deb | `a09930cfe415117e92757171c4502d0baac8669a93bee6bb76f31f1e7aa9e3a4` |

The new installer report has 20 PASS checks; actual dpkg install, reinstall and remove exits are 0. Native metadata from portable and installed binaries matches the original clean release metadata. Operator-file retention is tested. Cross-version upgrade, interrupted rollback and a physical Linux host remain unrun. Packages are unsigned.

## Runtime notice source integrity

The 420,502-byte notice has SHA-256 `3909eadeb6220801da62ebfe18b7335639dcc2ebb49c1f05d916cc91d575f246`; its source report is `3c3808fb09cd289f91c274c2b54a82080e03a190073b5d36318901043de6a2ed`.

The reviewer rehashed the two actual Rust archives, 23 archive members, the SQLite crate and three members, and 18 complete official text responses. The archive checksums match the exact Rust publisher target records and earlier setup receipt. Rust's [1.90.0 COPYRIGHT](https://raw.githubusercontent.com/rust-lang/rust/1.90.0/COPYRIGHT) points to the distributed standard-library notice document; the complete body of that actual archive member is included. HTML tags/entities and trailing layout padding are handled without deleting non-whitespace text. The other nine notice payloads are byte exact.

The pinned Rust recipe identifies musl 1.2.3; bootstrap source and its LLVM submodule pin identify compiler-rt CRT and libunwind. An independent HTTPS download of the [official musl archive](https://www.musl-libc.org/releases/musl-1.2.3.tar.gz) matched the recorded SHA-256 and pinned SHA-1, including the complete COPYRIGHT member. Compiler_builtins 0.1.160, Unicode 3.0 and SQLite 3.50.2's complete opening notice are also included. All 10 document offsets/payload hashes match. The author's 94 offline checks and exact stdin/exit 0 are verified; the first whitespace rendering's exit 3 remains recorded.

This is source and document integrity verification. No detached signature verification, exhaustive per-object link map or legal compliance certification is claimed.

## R4-E01: closed

The original exporter `26d37e…` could produce canonical PASS after an original runner FAIL/driver_error when all recorded commands had returned 0. The reviewer reproduced the boolean defect without executing the reviewed exporter. The author fixed the exporter/collector and made the exact v5 driver identity mandatory.

The final exporter is `150744b5682576421e0f26f4c6dbf608205e4ee4b70b6d490360c8964876114a`; final notices collector is `823376d8b69cf5ea7aaea45c38571895678cf2652e29d645afef63a6f7fceaf8`; v5 is `7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984`. PASS now requires the original runner PASS with no driver_error and matching actual commands. Fresh exact-source six-case inert checks, exit 0, reject failed/error runners and missing/legacy driver identities. Proof `76c45a3e…90e2d`, its nine references and actual final canonical usage were verified. Prior 02/03/04 checks and source versions remain historical rather than transferring to changed helper bytes.

## Delivery and cleanup evidence

The serialized summary preserves all 40 original requirement definitions and all 40 original FEATURE_SPEC acceptance blocks. Except ODIN-029's specifically evidenced criteria, original checks/criterion results/supporting reports remain unchanged. The result is **39 PARTIAL / 1 NOT_RUN**; ODIN-029 A1/A3 remain PARTIAL and A2 NOT_RUN. Thirteen implementation summaries agree with their evidence summaries. Five actual documentation checks and twelve exact-source inert classifier checks pass. Both before-v3 snapshots still match Git75c38f8 byte for byte.

The new shutdown/source-transfer receipt `527a3d6c…ad678` binds 23 referenced files. The first source-HTTP ownership guard refused its mismatched quoted command string: outer exit 1, child exit not separately serialized. Only the observed command quoting changed before the fresh exit 0; executable/PID/command guards remain. A single exact hrtimer kernel line interrupted the manifest's base64 stream; independent decoding verified gzip CRC, the preannounced whole manifest SHA and equality with the actual transfer manifest. All 81 artifacts were rehashed after shutdown.

Windows connect_ex returned 10035 in the reviewer's probe, which alone does not prove listener closure. A separate actual Get-NetTCPConnection listener inventory and Get-CimInstance task-PID inventory both returned zero. VM/helper shutdown is environment cleanup; it does not establish ODIN-029-A2.

Linux69 and d3 failures remain FAIL: respectively workspace 39/6 exit 101 and workspace 192/19/3 exit 101. The d3 owner-stopped release exit -15 is preserved; no d3 ELF PASS is inferred. Windows780's 214/5/6 workspace failure, independent continuations and 22 installer checks do not certify the retained Windows69 candidate.

## Remaining acceptance and handoff

Windows83 is still blocked by AppControl4551 with no administrator allowance confirmed. Its metadata, final six-language/twelve-project matrix and final installer remain NOT_RUN. Linux OS sandboxing is NOT_IMPLEMENTED; dedicated signal/descendant/journal acceptance, macOS, full IDE/language parity and the remaining original requirements are incomplete.

The reviewer ran only its own read-only source/blob/archive/hash/log and host-inventory checks. No Odin/backend/installer/assembler/classifier execution, VM serial access, policy/ACL change, product mutation, cloud CI or Git push was performed. Exact tool command captures, actual exits, corrected reviewer-only read/check attempts and source hashes are retained in the companion JSON.

Root owns the final review binding, public scan/manifest and authorized commit/push receipts. Windows/platform owners retain the blocked or unrun acceptance work. **Release approval remains BLOCKED.**
