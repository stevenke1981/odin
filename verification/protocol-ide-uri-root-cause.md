# Real push-diagnostic URI mismatch

The integration matrix observed repeated pending diagnostic reports for both
basedpyright and TypeScript even though other IDE requests worked. The cache
used the unmodified publish URI as its key and looked up the unmodified didOpen
URI. A direct RpcProcess probe captured actual notifications from basedpyright
1.40.2 and typescript-language-server 6.0.2: didOpen used `file:///C:/...`, while
both servers published `file:///c%3A/...`. Both notifications contained genuine
type errors, but lookup using the didOpen URI returned None. The raw records are
in `protocol-ide-uri-real-before.txt`; that observational probe did not assert
that the defective lookup was successful.

The fix uses one lexical local-file URI key for insert, lookup, and invalidation.
It validates URI shape and escapes, decodes via the existing URL library without
filesystem I/O, and folds only Windows drive letters. Remaining path-component
case is preserved, including on Windows case-sensitive directories. UNC and
authority URIs, query/fragment suffixes, malformed escapes, encoded separators
or NUL, and non-UTF-8 paths are errors. Cache identity does not authorize paths.

The same direct probe now asserts that both actual notification reports are
retrievable using the original didOpen URI. Fixtures verify symmetric alias
lookup/invalidation and versioned clean/error/CAS recovery transitions. Stale
versions and unversioned reports retain their prior freshness rules. A malformed
notification closes the transport with an explicit protocol error, never a clean
report. Package, Clippy, formatting, and explicit real-probe commands all ran on
Windows with exit 0; exact sources and logs are in `protocol-ide-uri-verification.json`.

The actual TypeScript notification did not include a version. It therefore
remains unverified after URI matching and cannot prove current source or clean
status. basedpyright supplied version 1. Final integrated tool-level acceptance
belongs to root/platform; Linux/macOS execution and broad backend parity were
NOT_RUN by this slice. No OS policy, target directory, or timeout was changed.
