# 依賴與建置狀態

直接 dependencies 已指定精確版本。Cargo 未能在交付環境執行，故 **沒有 Cargo.lock、完整 transitive inventory、dependency audit 或 reproducible build 證據**。首次 bootstrap 會解析後產生真實 lock；審查並提交後再用 --locked。

| Dependency | Manifest requirement |
|---|---|
| `anyhow` | `=1.0.100` |
| `thiserror` | `=2.0.17` |
| `serde` | `=1.0.228` |
| `serde_json` | `=1.0.145` |
| `toml` | `=0.9.8` |
| `tokio` | `=1.48.0` |
| `tokio-util` | `=0.7.16` |
| `clap` | `=4.5.50` |
| `sha2` | `=0.10.9` |
| `hex` | `=0.4.3` |
| `uuid` | `=1.18.1` |
| `regex` | `=1.12.2` |
| `globset` | `=0.4.18` |
| `ignore` | `=0.4.24` |
| `url` | `=2.5.7` |
| `fs2` | `=0.4.3` |
| `tempfile` | `=3.23.0` |
| `similar` | `=2.7.0` |
| `rusqlite` | `=0.37.0` |
| `rmcp` | `=3.6.0` |
| `axum` | `=0.8.6` |
| `tracing` | `=0.1.41` |
| `tracing-subscriber` | `=0.3.20` |

原生核心的語意後端不是內嵌 compiler：language server / debug adapter 是另外安裝的程式。SQLite 使用 bundled feature，建置仍需原生 C toolchain。Odin 不連接 Serena 或其 JetBrains 外掛；語言伺服器與除錯器有各自執行環境。
