# 研究來源與基準

研究以公開官方資料與程式介面為依據；來源不是本交付的執行證據。查閱日期 2026-10-21（UTC+08 日期跨日；上游 commit 日期依 Git）。

- Serena: https://github.com/oraios/serena
- Reviewed main commit: `abb0a0f01b94bc8050086f0fb860077d17730324` (2026-10-20)
- Tools / REPL / facade catalog: https://oraios.github.io/serena/01-about/035_tools.html
- Workflow: https://oraios.github.io/serena/02-usage/040_workflow.html
- Memories: https://oraios.github.io/serena/02-usage/045_memories.html
- Configuration: https://oraios.github.io/serena/02-usage/050_configuration.html
- Security: https://oraios.github.io/serena/02-usage/070_security.html
- Serena license: https://github.com/oraios/serena/blob/abb0a0f01b94bc8050086f0fb860077d17730324/LICENSE
- MCP specification: https://modelcontextprotocol.io/specification/2025-11-25
- Official MCP Rust SDK: https://github.com/modelcontextprotocol/rust-sdk
- SDK source baseline: `c960b4b841dddca570e00a6f583986dbf78621dc`; reviewed workspace version `3.6.0`
- SDK API example: https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/sampling_stdio.rs
- SDK HTTP example: https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/counter_streamhttp.rs
- LSP 3.17: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- DAP: https://microsoft.github.io/debug-adapter-protocol/specification

Serena 支援的是整個後端、語言適配、工具與工作流生態；Odin 0.2.0 只用獨立原生實作及直接 LSP／DAP，移除工具橋接，不能僅憑工具名稱相同宣稱完全相容。SDK 支援更新版本 MCP 也不表示本應用已實作全部更新特性。
