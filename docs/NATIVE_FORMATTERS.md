# Host-owned native formatting

`format_document` normally asks the selected language server. A separate,
explicit `formatter_profile` selects a host-owned Ruff stdin formatter; this is
not an automatic LSP fallback. Python servers without formatting keep their
actual capability result. Backend-unadvertised resolve operations and
unversioned diagnostics are not upgraded by this route.

The trusted host must configure both `allow_write=true` and
`allow_formatters=true`, a native MXC policy, and an allowed formatter profile.
Both grants default to false. Context/mode policies can only reduce either
grant. Do not import this policy from a repository or permit an agent to edit
its own host configuration.

```toml
[projects.python.formatters.ruff]
provider = 'ruff'
extensions = ['py', 'pyi']
command = { program = 'C:/trusted-tools/ruff.exe', args = ['format', '--isolated', '--no-cache', '--stdin-filename', '{relative_path}', '-'], timeout_secs = 30 }
```

This fragment grants no permissions by itself. The host must separately choose
its source root, native executor/hash, read-only runtime/source grants and
private scratch policy. Sources remain read-only to the formatter. Shared or
source-overlapping writable caches are rejected. Ruff is a separately
installed host tool, not bundled or downloaded by the product.

The command grammar permits only `format`, mandatory `--isolated`, `--no-cache`
and `--stdin-filename {relative_path}`, optional fixed `--line-length` (1..320),
`--target-version` (a Ruff `py` version token), one preview mode, and final `-`.
Duplicate/unknown flags, config discovery, positional source files and output
files are rejected. The exact filename placeholder is replaced with the
absolute path resolved through the project's safe-root checks; caller flags,
range formatting and nonempty LSP formatting options are unsupported here.

The runner concurrently feeds the bounded UTF-8 source snapshot and drains
bounded stdout/stderr. A single deadline covers blocked stdin and process
completion. Cancellation/timeout terminate the managed process tree before
private sandbox resources are released. Invalid UTF-8, failed or truncated
output cannot become source text. A successful output still needs the
unchanged expected source hash and produces a normal transaction preview;
only explicit transaction apply writes the source. Conflicts are preserved.

Fixed Rust process fixtures test pipes and lifecycle only. Real Ruff native
acceptance, source/binary hashes and executed command logs are recorded
separately in the language sandbox v3 evidence. Those results do not establish
complete IDE parity, debugging support for every language or Linux sandbox
support. MXC ProcessContainer enforcement in this release targets Windows.
