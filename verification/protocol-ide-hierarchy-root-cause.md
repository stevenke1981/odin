# SIR-001 hierarchy query resource bounds

Source: `E:/odin/verification/sandbox-ide-review.json`, finding SIR-001, reviewed
integration head `5e4cf03527d5396fe56c558f43f0097c222c23ab`. Patch base in this
isolated worktree: `0e50a643df38c6bcb4d67ec5df36455a25bcff78`.

The prepare response previously drove an unrestricted number of sequential RPCs.
Each had its own transport timeout and response limit. There was no total query
deadline or quota for retained JSON, related items, or call ranges. Retaining
responses and cloning the final result into the `hierarchy` compatibility field
could multiply the per-frame memory limit. The review used static evidence;
an intentional GiB/OOM workload was NOT_RUN and was not necessary to fix this.

The fix reserves the encoded output cost of all prepared items before the first
related request. A streaming serializer measures exact escaping/key/separator
cost without allocating a serialized copy. Related responses must fit the
remaining quota before validation or insertion. Output values are moved until
the final bounded compatibility clone. Limits apply across the whole query:
128 prepared items, 4096 relations, 8192 call ranges, and 4 MiB encoded result
JSON (8 MiB across both result fields, excluding envelope/allocator overhead).

One deadline uses the configured startup timeout for snapshots, all RPCs,
validation and final freshness checks. Async expiry cancels the pending request
through the existing RPC future-drop path. Synchronous serialization and loops
also check the deadline. A blocking OS file read cannot be preempted by Tokio;
the next explicit check returns TIMEOUT. Transport frame size remains an
additional independent bound on the response that crosses the total quota.

Executed native Windows regressions use at most four approximately 1 MiB
related responses, not an OOM workload. A fixture request log proves that
129 prepared items cause zero related requests and that the cumulative byte
quota prevents the fifth related request. Both call and type hierarchy routes
are covered. An 800 ms whole-query timeout rejects five individually 225 ms
responses. Additional tests cover relation/range counts, exact escaped UTF-8
measurement, and synchronous deadline checks. These are protocol fixtures, not
real-language capability or native OS sandbox enforcement evidence.

Actual command logs and normalized source hashes are in
`protocol-ide-hierarchy-verification.json`. The historical Application Control
denial remains in `protocol-ide-appcontrol.json`; this new source candidate
executed at the same target directory without any policy changes. Root owns
integration build, independent rereview, and any release/publication decision.
