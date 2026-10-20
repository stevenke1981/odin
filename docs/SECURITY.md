# 安全模型與信任邊界

此文件描述程式設計，不代表已完成滲透測試或平台安全認證。

## 可信任的範圍

主機、使用者、MCP client、host config、工具相依及被分析的程式碼庫應受信任。Odin 不是多租戶服務。不同 project 的識別碼是範圍控制，不是不同使用者之間的認證或保密隔離。

CLI 載入設定時拒絕重疊 root。作為 library 使用 `Engine::new(Config)` 時，呼叫方必須提供已驗證的 trusted Config；不要把未經檢查的 API payload 當作設定傳入。

## 原生防護

原生來源寫入必須 allow_write；所有來源 edits 走 journal / expected SHA-256。原生 paths 拒絕 parent traversal、絕對路徑、符號連結、.git/.odin、可疑 Windows device names 與 deny_globs。CLI 載入時把設定檔本身及 workspace 內的 state directory 加入原生 deny 規則，避免修改自己的設定。命令及 debugger 使用 host 設定的命名 profile。大型輸入、輸出、檔案、RPC frame、flow 步驟與狀態設上限。

`read_only` 是 API 層的寫入限制，不是 OS 保證：讀取操作可能啟動 LSP；LSP 可執行 build scripts、plugins、package hooks。即使沒有原生 shell 工具，子程序仍可能有主機帳號權限。

## 不提供的保證

沒有 Linux namespace / seccomp、macOS sandbox profile、Windows AppContainer / Job Objects。沒有完整後代程序終止、檔案 descriptor-relative 無競態操作，或惡意本機使用者防護。Windows junction、UNC、所有 reparse-point 型態的 native 驗證尚未執行。

對不可信任 repository，請在獨立 VM / container / 低權限帳號中執行，僅掛載必要路徑，限制網路與 credentials。設定 deny_globs 不能取代 OS 隔離。

## 直接後端

Serena 橋接已移除，也沒有由工具名稱觸發的上游 fallback。這縮小了一條任意上游工具執行路徑，**不代表 Odin 已被沙箱化**。

LSP／DAP 與允許執行的命令仍是外部程式；它們的內部檔案操作不經 Odin journal。可信任操作者也可能把任意程式填進 host profile，Odin 不對任意子程序提供 OS 權限隔離。不得因移除 Serena 就把外部後端視為安全。

## 網路

HTTP 僅 loopback，至少 32-byte-visible-character token，拒絕非預期 Host / Origin。token 使用環境變數注入，不寫 log、URL、前端 localStorage。實際熵由操作者生成方法決定，長度本身不代表密碼強度。

此版沒有 TLS、OAuth、遠端身分、session 專用權限；同一 token 代表同一可信任操作者。不要使用轉發／反向代理把 listener 直接暴露公網；需要遠端部署先完成獨立安全設計。

## 資料與稽核

audit 記錄 project、tool、ok、elapsed、輸入 SHA-256、error_code；不記 raw arguments。`ok` 只表示工具 handler 回傳成功。執行測試時仍需看 result.success / exit_code。

journal 包含完整程式碼 before/after；FTS 含程式碼內容；spooled results 與 Markdown memories 也可能敏感。它們未加密。Unix state dir 設 0700；Windows ACL 未自動設置或驗證。沒有自動 retention / 清除策略；自行規劃磁碟與備份。

## 失敗與恢復

timeout 可能代表「外部操作已完成但回覆遺失」，不能對修改型工具自動重試。原生寫入使用 journal 與 idempotency key；外部工具不共享此保證。恢復前若發現第三方編輯，停止並保留 RecoveryRequired，交由操作者處理。

詳見 `KNOWN_LIMITATIONS.md` 及 `RELEASE_GATES.md`。任何安全主張都須在實際目標系統驗證。
