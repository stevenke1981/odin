# Odin 改寫與重構規格 (RFC-EDIT-001)

**地位：設計目標，非已驗收功能。** 參閱 ODIN-005/006/009/014/015/016/038。

## 寫入前必須的四道關卡

1. **解析計畫**：需要解析 symbol semantic meaning 時只能由 LSP/capable language adapter 提供；regex 為純文字替換，不能當 semantic rename/move/inline。
2. **預覽與舊版核對**：每個目標檔案取得 expected SHA-256、版本、文字 edit offset / 資源 operation；建立 diff，不修改原檔。
3. **提交前核對**：全 project lock + 所有受影響檔案的 CAS preflight，包含 source+destination+parent directory、安全開檔 handle。
4. **持久日誌與套用**：fsync journal + prepared state；逐檔 temp+atomic rename；成功以 committed state 記錄，失敗根據 snapshot 恢復，遇外部變動中止並回 NEED_REVIEW。

不宣稱跨多檔瞬間原子性；Windows sharing violation、junction、case-only rename、rename across filesystem 都必須有平台專用測試。

## 完整 WorkspaceEdit

支援 `changes` 與 `documentChanges`（`TextDocumentEdit`、`CreateFile`、`RenameFile`、`DeleteFile`）；操作有序、含 annotations / doc versions。URI → 專案相對路徑需防 traversal。若 backend 回 unsupported operation，**拒絕整個 changeset**，不能只取文字編輯就回成功。

### Semantic Move

最小 API 目標：`move_symbol(project, source, symbol_name_path, target, options)`；可回 `PREVIEW`、`UNSUPPORTED`、`NEED_REVIEW`、`CONFLICT`。LSP 沒有可用 CodeAction 或語言 adapter 時不能假裝完成。

### Semantic Inline

最小 API 目標：`inline_symbol(project, relative_path, name_path, occurrence_policy)`；需要評估 side effects、shadowing、macro hygiene、type inference。缺語意證據先拒絕，不退化成全域字串替換。

### Safe Delete

最小 API 目標：`safe_delete_symbol(project, symbol, cascade=false, risk_policy=conservative)`；找到靜態 references 必須阻擋或 preview cascade；macro/reflection/exported API 區域需要使用者核准。`no static refs` 不代表已證明安全。

### Multi-file 精確修改

`replace_in_files` 需提供每個 occurrence immutable ID，容許只選定指定 occurrences、expected match count、multiple/regex flags。預覽需要包含 line/column、read hash、before/after；若匹配數變動，不得使用舊 occurrence ID 套用。

## 回復策略

- `transaction_prepare`：只產生 durable 計畫和 diff。
- `transaction_apply`：以 idempotency key / immutable snapshot / per-project lock 防重複及衝突。
- `transaction_recover`：在 journal 可信且現值吻合 before/after 時自動回復；不吻合須明確人工介入，禁止強制覆寫。
- `transaction_rollback`：創建反向交易，永不修改原有 journal。

### 每一項必備測試

任何 edit action 不得標為 DONE，除非至少有 happy path、conflict、malformed payload、permission failure、cancel/crash/recovery、Windows NTFS 與 Linux FS 的可再現證據。
