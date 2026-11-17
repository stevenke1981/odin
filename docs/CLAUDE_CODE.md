# Odin with Claude Code (quick start)

Odin is a local MCP server. Claude Code already has Read/Edit/Grep/Glob/Bash;
Odin adds what those lack: **language-server symbols and references**, semantic
rename, symbol-level edits, and hash-guarded edit transactions with rollback.
Use Claude Code's built-in tools for plain file reading and text search; use
Odin when you need semantics. Nothing here runs `claude` or edits Claude Code
settings for you: every command below is printed by Odin and run by you.

## 1. Build and create a trusted host config

```powershell
cargo build --workspace --release --locked          # Rust 1.90.0 (rust-toolchain.toml)
# Host config lives OUTSIDE any repository; new projects are read-only.
.\target\release\odin.exe --config C:\Odin\host.toml init --root C:\src\myproj --name myproj
.\target\release\odin.exe --config C:\Odin\host.toml doctor --probe-lsp   # needs rust-analyzer etc. on PATH
```

`init --root` registers one fixed project. For "whatever directory Claude Code
was started in", omit `--root` (home workspace template, see
[CURRENT_WORKSPACE.md](CURRENT_WORKSPACE.md)) and add `--workspace .` below.

## 2. Register with Claude Code

```powershell
odin --config C:\Odin\host.toml setup --client claude            # dry run, writes nothing
odin --config C:\Odin\host.toml --workspace . setup --client claude
```

The JSON output contains:

- `claude_code.user_scope_command` - e.g.
  `claude mcp add --scope user odin -- C:\Odin\odin.exe --config C:\Odin\host.toml serve --transport stdio`.
  Run it once; `--scope local` limits it to the current project instead.
- `content` - the same entry in `.mcp.json` shape (`mcpServers.odin`). Only commit
  it as a project `.mcp.json` if every user has the same absolute paths.

Check with `claude mcp list` or `/mcp` inside Claude Code. Restart Claude Code
(or reconnect in `/mcp`) after changing the host config or the binary.
`setup --client claude-desktop` keeps the previous Claude Desktop output.

## 3. Tool profiles (context budget)

Every advertised tool schema costs context in every session. Odin therefore
advertises a small set by default (`--tool-profile` is a global flag; pass it to
`setup` so it is copied into the client entry):

| Profile | Tools advertised (read-only / writable) | tools/list bytes (read-only / writable) |
|---|---|---|
| `core` (default) | 13 / 19: analysis, refactor_rules, read_file, search, symbols, references, declaration, implementations, diagnostics, rename, symbol/text edits, transactions, command profiles | 8,534 / 11,661 |
| `ide` | 29 / 50: core + list/find files, FTS index, hover, call/type hierarchy, code actions, formatting, line edits, multi-file replace | 18,043 / 33,609 |
| `full` | 87: everything (memories, flows, DAP, completion, inlay hints...) | 55,895 |

Bytes were measured on 2026-11-17 with the release binary against this repository
as a single project without `allow_shell`, so the writable counts measured were
18 / 49 (the command-profile tool is hidden). The same setup measured
7,996 / 11,088, 17,505 / 32,911 and 55,167 before `refactor_rules` was added.
Earlier figures on a real 12-crate workspace with shell enabled were
8,008 / 11,625, 17,533 / 33,479 and 55,167 (12 / 18, 28 / 49, 86 tools). In `core`/`ide`, tools that host policy
disables for every project (edits on a read-only project, commands without
`allow_shell`) are not advertised, and with a single project the `project`
argument becomes optional. `--include-tools hover,index_query` adds individual
tools. Profiles are an exposure choice, not a security boundary: the engine
enforces host policy on every call regardless.

## 4. Permissions and writes

- Claude Code asks before each `mcp__odin__*` call. To pre-approve read-only
  tools, add e.g. `"mcp__odin__find_symbol"`, `"mcp__odin__find_referencing_symbols"`,
  `"mcp__odin__get_symbols_overview"` to `permissions.allow` in
  `.claude/settings.json` (or `~/.claude/settings.json`). Keep
  `mcp__odin__transaction_apply` on ask if you want to review every write.
- Projects are read-only until the operator sets `allow_write = true` under
  `[projects.<id>]` in the host config (or uses `init --writable --root ...`).
  `allow_shell = true` enables the fixed `[projects.<id>.commands]` profiles
  (e.g. `test`, `check`). Restart the server afterwards.
- Edits never write directly: an edit tool returns a transaction preview, then
  `transaction_apply` writes it after re-checking file hashes. If Claude Code's
  own Edit tool changed the file meanwhile, apply fails with CONFLICT: re-read
  with `read_file` and prepare again.

## 5. Suggested workflow for the agent

1. `project_analysis` (once), `get_capabilities` if a semantic call fails.
2. `find_symbol` / `get_symbols_overview` to locate code; `find_referencing_symbols`
   before changing a signature.
3. Before a refactor (`rename_symbol`, `safe_delete_symbol`, `move_symbol`,
   `inline_symbol`, `prepare_code_action`, `apply_quick_fix`) call `refactor_rules`;
   a host rule violation fails with `REFACTOR_RULE_VIOLATION` and prepares nothing.
   `rename_symbol` for renames; `replace_symbol_body` / `replace_content` for edits;
   review the preview; `transaction_apply`.
4. Run the `test` command profile (or Bash) and judge by `exit_code`.

Other agents: `setup --client codex` (`~/.codex/config.toml`),
`--client opencode` (`opencode.json`). Cursor and other `mcpServers` JSON clients
can use the `content` printed for `--client claude`.
