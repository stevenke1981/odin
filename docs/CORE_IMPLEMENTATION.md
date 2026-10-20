# Core implementation and boundaries

The Windows build now exercises the real Rust core. Core changes include
handle-relative no-follow source reads, replacement and deletion; Windows
reparse-point rejection; hard-link rejection; scoped immutable memory journals;
versioned create-only memory import/export; reference-safe memory scope moves;
demand-driven incremental FTS updates; and metadata-only trace audit records.

`memory_import` and `memory_move_scope` return a preview. Their journal records
the initiating project; a different project cannot read or apply it. A local
memory name cannot represent a reference into another project, so moves with
such references return `NEED_REVIEW`. Import never overwrites existing notes.

File operations hold directory capabilities and open each component without
following links. Source changes still pass through Transactions, with hashes
checked before preparation, before the changeset starts, and before each write.
Uncooperative outside processes can race the final check and rename; this is
not an OS-wide compare-and-swap primitive or multi-file atomic visibility.
Directory capabilities bind to opened directories, not a filesystem-wide lock.

Subprocesses receive a small explicit runtime environment. Windows Job Objects
with kill-on-close manage child lifetime; Unix groups are the corresponding
cleanup primitive. These are lifecycle controls, **not** restricted-token,
AppContainer, seccomp, filesystem or network isolation. Windows job assignment
occurs after spawn and has a documented launch window. Configured executable
profiles must therefore be trusted. Write, shell and debug permissions remain
disabled unless the host explicitly enables them.

The index reconciles additions, changes and deletions by live hash scan before
queries. It updates only changed FTS rows, and is lexical search. There is no
background filesystem watcher or vector search claim.

Implementation dependencies use safe public APIs and keep `forbid(unsafe_code)`
in Odin. Sources: [cap-std directory capabilities](https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html),
[no-follow operations](https://docs.rs/cap-fs-ext/4.0.3/cap_fs_ext/trait.DirExt.html),
[Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[win32job limits](https://docs.rs/win32job/2.0.3/win32job/struct.ExtendedLimitInfo.html).
