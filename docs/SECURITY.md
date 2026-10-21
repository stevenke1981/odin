# 安全模型與信任邊界

此文件描述程式設計，不代表已完成滲透測試或平台安全認證。

## 可信任的範圍

主機、使用者、MCP client、host config 與工具相依屬於信任邊界。未選用 OS 沙箱時，被分析程式碼庫也必須受信任。Odin 不是多租戶服務。不同 project 的識別碼是範圍控制，不是不同使用者之間的認證或保密隔離。

CLI 載入設定時拒絕重疊 root。作為 library 使用 `Engine::new(Config)` 時，呼叫方必須提供已驗證的 trusted Config；不要把未經檢查的 API payload 當作設定傳入。

## 原生防護

原生來源寫入必須 allow_write；所有來源 edits 走 journal / expected SHA-256。Paths 以 directory capabilities/no-follow 存取，拒絕 traversal、絕對路徑、symlink／reparse points、hardlinks、.git/.odin、device names 與 deny_globs。Windows deny case-insensitive，並重新核對 canonical 長路徑；existing parent／8.3 與 transaction targets 使用相同 identity。CLI 將 host config 與 canonical state 加入 deny，拒絕 case-colliding project IDs。命令／debugger 僅 host profile；context/mode 只能收緊權限。

`read_only` 是 API 層的寫入限制，不是 OS 保證：讀取操作可能啟動 LSP；LSP 可執行 build scripts、plugins、package hooks。即使沒有原生 shell 工具，子程序仍可能有主機帳號權限。

## 不提供的保證

沒有 Linux namespace / seccomp 或 macOS sandbox profile。Windows 可明確選用 MXC ProcessContainer：executor SHA-256、當前 probe、無 DACL mutation fallback、network deny、明確 filesystem grants 與每次執行私有 scratch。配置失敗不回退到未隔離執行。Job Objects／Unix process groups 另管生命週期；未選沙箱的路徑仍有 spawn→job 指派窗口。Junction、hardlink、大小寫與真實 NTFS 8.3 有原生回歸；不宣稱全部 reparse／外部 inode 移動競態都已涵蓋。見 [具體沙箱邊界及未驗收資源](OS_SANDBOX.md)。

對不可信任 repository，請在獨立 VM / container / 低權限帳號中執行，僅掛載必要路徑，限制網路與 credentials。設定 deny_globs 不能取代 OS 隔離。

## 直接後端

Serena 橋接已移除，也沒有由工具名稱觸發的上游 fallback。這縮小了一條任意上游工具執行路徑，**不代表 Odin 已被沙箱化**。

LSP／DAP 與允許執行的命令仍是外部程式；它們的內部檔案操作不經 Odin journal。配置 MXC 後限制在 host 授權的 OS grants；寫入 source grant 仍可能產生外部副作用。Native inspections 強制要求沙箱與所有配置 project 唯讀，保留外部 cache grants，修正全部轉成原生 Transactions。不得因移除 Serena 就把外部後端視為安全。

`debug_launches.<profile>` 可由 host 固定 launch、attach、restart JSON payload；空 arguments 使用固定值，非空須完全相等。缺少指定 command payload 時拒絕。有意保留舊 host allow_debug 的 privileged 模式時，metadata 明列 unrestricted_host_debug。evaluate 仍可在 debuggee 的權限內執行語言運算；不得把它當任意不可信運算的純資料工具。

## 網路

HTTP 僅 loopback，至少 32-byte-visible-character token，拒絕非預期 Host / Origin。token 使用環境變數注入，不寫 log、URL、前端 localStorage。實際熵由操作者生成方法決定，長度本身不代表密碼強度。

此版沒有 TLS、OAuth、遠端身分、session 專用權限；同一 token 代表同一可信任操作者。不要使用轉發／反向代理把 listener 直接暴露公網；需要遠端部署先完成獨立安全設計。

## 資料與稽核

audit 記錄 project、tool、ok、elapsed、輸入 SHA-256、error_code、trace_id，不記 raw arguments。Dashboard/export 另用 metadata allowlist，不輸出 code、args、backend error message。`ok` 只表示 handler 成功；命令仍需看 result.success / exit_code。

journal 包含完整 before/after；FTS、spool、Markdown memories 也可能敏感，未加密。Unix state dir 0700；Windows ACL 未自動強化。Spool 有 TTL/quota 與 lazy cleanup，其他儲存尚無完整 retention。Memory import/move journal 綁定 project，重驗所有 affected scopes；Windows note 政策與 canonical 名稱防別名繞過。交易 snapshots 總量限制 32 MiB，flow／replacement 在展開前檢查配額。

## 失敗與恢復

timeout／取消可能代表「操作已完成但回覆遺失」，不能對修改型工具自動重試。MCP 取消會丟棄仍在等待 gate／backend 的 future；已進入同步 apply 時維持 unknown-outcome，不假裝 rollback。原生寫入使用 journal/idempotency；外部工具不共享此保證。恢復發現第三方修改時保留 RecoveryRequired，不覆寫衝突。

詳見 `KNOWN_LIMITATIONS.md` 及 `RELEASE_GATES.md`。任何安全主張都須在實際目標系統驗證。
