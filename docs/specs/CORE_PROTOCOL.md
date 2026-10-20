# Odin 核心協定規格 (RFC-CORE-001)

**地位：設計目標，非現行 API 承諾。** 參閱 ODIN-002/003/004/007/008/040。

## MCP / CLI 請求與回應

- 所有會接觸專案檔案的工具要求 `project`：host config 中的穩定 project ID，**不接受用檔案路徑取代 project ID**。
- 建議統一回應欄位：`schema_version`、`trace_id`、`project`、`success`、`data`、`error`、`result_id`、`truncated`、`capabilities`。此為目標版本 `odin.tool.v1`，不能在 0.2 API 尚未變更前當現有契約。
- 嚴格 JSON Schema `additionalProperties:false`；不接受未知 key、未知 enum、過大字串與不合理 glob。
- 所有 tool error 使用 `error.code` 與 `error.message`，且保留 MCP 層 `isError=true`；未知方法使用正確 JSON-RPC protocol error。

## 目標 ErrorCode 分類

| 分類 | 範例 | 觸發時機 |
|---|---|---|
| `INVALID_ARGUMENT` | 錯 type/schema | 輸入驗證 |
| `PROJECT_NOT_FOUND` | 無效 project ID | 專案解析 |
| `OUT_OF_SCOPE` | escape root/symlink | 開檔前 |
| `PERMISSION_DENIED` | policy 禁止寫入 | 政策驗證 |
| `CAPABILITY_UNAVAILABLE` | LSP 無 declaration | 後端協商 |
| `BACKEND_NOT_READY` | 正在初始化 | readiness |
| `BACKEND_UNAVAILABLE` | 尚未安裝 LSP/DAP | 後端啟動 |
| `CONFLICT` | SHA/version 不符 | apply |
| `NEED_REVIEW` | 動態引用不確定 | 安全重構 |
| `TIMEOUT` | backend 沒回應 | timeout |
| `INTEGRITY_ERROR` | journal 內容/格式異常 | recovery |
| `RESULT_EXPIRED` | large result TTL 過期 | chunk read |

## Client protocol

- `stdio`: client 啟動 Odin 子程序；stdout 只能輸出協定 frames，所有除錯輸出進 stderr，關閉時終止程序樹。
- `streamable-http`: 預設 `127.0.0.1`；Bearer；Host/Origin 驗證；最大 request bytes、連線上限、graceful shutdown。**不提供公網未授權模式**。
- `tools/list`: 回傳確實具備的契約；透過 `get_capabilities` 標示 LSP 尚未 ready 或個別語言不支援能力。對不支援的功能直接報錯，而不是靜默降級。
- `tools/call`: project → auth/policy → lock/scope → execution → audit + trace → size cap/render；data 與 server errors 不得混淆。

## LSP 位置與引用資料

```json
{
  "symbol": {
    "name": "Workspace::run",
    "name_path": "Workspace/run",
    "kind": "method",
    "project": "my-rust-repo",
    "relative_path": "src/lib.rs",
    "range": {"start": {"line": 12,"character": 2},"end": {"line": 21,"character": 3}},
    "position_encoding": "utf-16",
    "source": "rust-analyzer",
    "document_version": 7
  }
}
```

此例為 **目標 JSON 形狀**，不是現版程式碼執行輸出。所有 LSP range 依 negotiated position encoding 解讀；若支援 UTF-8/UTF-32，必須明示並轉換，不可假設所有 server 都一定是 UTF-16。

## 多專案及並行

每次工具呼叫包含 project ID；cache、connection、result namespace、journal 以 `(project_id, language_id)` 隔離。多個 Agent 可共享同一 Rust 服務，但不得存在可被一個 agent 靜默改寫的全域 active project。跨專案只讀查詢需要明確指定 source/target，確認權限後執行。

## 向後相容

0.2 → v1 response schema 需以 `schema_version` 轉接且有 migration tests。新增欄位在未變更版號時不可改寫既有工具的必填語意。
