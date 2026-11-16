# 交付狀態索引

此索引用於辨識各證據的適用對象；不代表重新執行測試。原始紀錄及其 hash、命令和限制以 [delivery-status.json](../verification/delivery-status.json) 與各收據為準。

| 對象 | 來源與產物 | 範圍與狀態 | 證據 |
|---|---|---|---|
| Windows Home／擴充 LSP | native build `190c308e9ab7bbbf2917f879f58fd2d8a948cee0`；binary SHA-256 `2e1c11b040ab1c13e4843a0de7b912b0f8b9452d2284a5c353077df32e021792`；packaging freeze `ca190e867981b1351c74c0946b0cd45c427613ce` | `PASS_WITH_DOCUMENTED_GAPS`；原生 gate 與指定安裝生命週期通過，已安裝 MCP 有診斷版本缺口；現有 client 重啟 NOT_RUN | [Home 與安裝收據](../verification/home-root-installer-validation.json)、[交付狀態](../verification/delivery-status.json) 的 `current_extension` 和 `home_and_expanded_lsp` |
| Linux x86-64 歷史候選 | application source `0fe03ba30da9600c25588d506d7e094df89cf56f`；ELF SHA-256 `e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42` | 當時 VM 執行的 workspace、smoke 與 deb／tar 安裝生命週期通過；不代表目前 source 在 Linux 重新驗證，亦不代表 Linux sandbox 或 macOS 通過 | [Linux 收據](../verification/package-v3-linux-notices-final.json)、[歷史看板](IMPLEMENTATION_BACKLOG.md) |
| 先前 Windows 候選 | source `69e769be9baa4e825139b72ce1a9b48bf8143114`；binary SHA-256 `83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44` | 當時編譯通過，執行遭 AppControl 阻擋；這個 BLOCKED 不適用於後續 Home 候選 | [歷史阻擋收據](../verification/language-installers-v3-final-host-block.json) |
| macOS | 無對應已驗收產物 | native build、完整平台矩陣與 sandbox 驗收 NOT_RUN／未完成 | [已知限制](KNOWN_LIMITATIONS.md) |

所有 40 項需求尚未全部驗收。乾淨 Windows、跨版升級、崩潰復原、完整語言及 DAP 矩陣、簽章、benchmark 和 formal security audit 的缺口見 [已知限制](KNOWN_LIMITATIONS.md)。版本探測、fixture、來源或清單完整性檢查，不等同真實後端、安裝或產品執行驗收。
