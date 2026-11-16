@AGENTS.md

## Claude Code notes

- Prefer the fast gate in AGENTS.md while iterating; run the full `scripts/verify.ps1` before claiming release readiness.
- Never report a test as passing unless you ran it in this session; keep NOT_RUN otherwise.
- When changing MCP exposure (`crates/odin/src/mcp.rs`) or tool text, re-measure `tools/list` size and update docs/CLAUDE_CODE.md.
- Do not edit the user's Claude Code settings; `odin setup --client claude` only prints commands.
