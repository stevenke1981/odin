# JDTLS WorkspaceEdit empty-alternative compatibility

The real Java matrix failed rename with `WorkspaceEdit must use either changes
or documentChanges`. An independent direct LspClient probe reused the explicit
Java fixture factory and backend profile from the platform owner. It opened
Greeter.java and Consumer.java, waited for both cross-file references, and
requested the real greet-to-salute rename. The raw JDTLS 1.62.0 response was
`changes: {}` together with two populated `documentChanges`. Both document
versions were null. The old planner rejected it based solely on field presence.
The complete observational counterexample is in
`protocol-ide-uri-java-real-before.txt` (planned:false, no files applied).

The planner now validates both optional field types. Absent/null/empty unused
alternatives are accepted, while two populated alternatives still fail before
interpreting edits. Empty strings, wrong container types, and entirely empty
proposals remain errors. A map containing a URI key is populated even if that
URI has an empty edit array. There is no merging of competing representations.
All source snapshots, expected hashes, version checks, annotations, resource
operation ordering, no-overwrite and no-follow rules retain their existing code.

The direct probe now asserts two guarded changes, applies the real two-file
rename through core Transactions, and restores both original files through
another expected-hash transaction. The source-bound log captures the external
fixture module, JVM, JDTLS launcher/core, configuration and fixture source hashes.
Fixtures separately exercise all four null/empty combinations; negatives cover
two populated alternatives, malformed alternatives, absent snapshots, stale
versions and overwrite attempts. Planner-only checks assert no source mutation.

The first probe compile incorrectly called `locations` instead of the actual
`at` API. Its exit-101 log is preserved as `protocol-ide-uri-java-first-compile.txt`.
After correcting the test call, raw capture, the fixed real rename/CAS rollback,
all 66 package tests, Clippy and formatting executed successfully. Three real
tests remain ignored in the default package run; the Java and Python/TypeScript
probes were also executed explicitly on this final source. Real RA was NOT_RUN.

Root/platform own the rebuilt integrated binary matrix and independent review.
These direct probes do not prove all Java refactors, every language capability,
OS sandbox enforcement, or Linux/macOS support. No OS policy or permission was
changed. Source hashes and exact commands are in
`protocol-ide-uri-java-verification.json`.
