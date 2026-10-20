# 依賴與建置狀態

本次已用 Rust/Cargo 1.90.0 在 Windows MSVC 實際建置，提交真實 Cargo.lock；後續使用 --locked。直接 dependencies 固定精確版本。完整套件／checksums 見 Cargo.lock、[SBOM](../verification/sbom.cdx.json)、[授權清單](../verification/license-inventory.json)及 [授權全文](THIRD_PARTY_LICENSES.txt)。漏洞／授權檢查的實際 exit code 見 [local-validation](../verification/local-validation.json)，不代表未來持續無漏洞或可重現位元組建置。

| Dependency | Manifest requirement |
|---|---|
| `anyhow` | `=1.0.104` |
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

Core 另外固定 cap-std/cap-fs-ext 4.0.3、Windows win32job 2.0.3、Unix nix 0.30.1（signal/process）。capability handles、Job objects 與 process groups 不是完整 OS 沙箱。

原生核心的語意後端不是內嵌 compiler：language server / debug adapter 是另外安裝的程式。SQLite 使用 bundled feature，建置仍需原生 C toolchain。Odin 不連接 Serena 或其 JetBrains 外掛；語言伺服器與除錯器有各自執行環境。
