#!/usr/bin/env python3
"""Generate API and parity indexes from the actual Odin catalog. Not a test runner."""
from pathlib import Path
import argparse, csv, io, json, tomllib
parser=argparse.ArgumentParser()
parser.add_argument('--api-only',action='store_true',help='Regenerate API.md without touching separately owned parity/release docs')
arguments=parser.parse_args()
ROOT=Path(__file__).resolve().parents[1]
T=json.loads((ROOT/'schemas/tools.json').read_text(encoding='utf-8'))
header='''# Odin 工具 API 0.2.0 Standalone\n\n此表由 `schemas/tools.json` 生成；實作與實際測試證據見 verification/local-validation.json；本表本身不是測試結果。\n\n所有工具以明確 `project` 範圍執行；CLI 在提供 `--project` 時可注入該欄位。`read` / `state` / `write` / `execute` 是內部策略分類，不是 OS 隔離保證。read 工具仍可能啟動語言伺服器、寫 audit/cache。\n\n`write`：來源 edits 先回 transaction preview，需另 apply；記憶 CRUD 直接經 journal commit，import/move 先回 preview。`execute`：要求相應 shell/debug/inspections 權限；native inspections 另強制使用 host 設定的 OS sandbox。本版本沒有外部工具橋接。\n\nLSP position 為 0-based UTF-16；檔案行數為 1-based inclusive。參數細節、範圍與 additionalProperties 規則請讀 JSON schema。此版 schema validator 實作契約需要的有限 JSON Schema 子集，非通用 JSON Schema 驗證器。\n\n| Tool | Policy | 必填參數 | 作用 |\n|---|---|---|---|\n'''
rows=[]
for t in T:
 req=', '.join('`'+k+'`' for k in t['inputSchema'].get('required',[]))
 rows.append(f"| `{t['name']}` | {t['policy']} | {req} | {t['description'].replace('|','/')} |")
(ROOT/'docs/API.md').write_text(header+'\n'.join(rows)+'''\n\n## 結果／錯誤\n\nMCP 回 content + structuredContent。業務錯誤使用 isError 與 `{error:{code,message}}`。常見 code：INVALID_ARGUMENT、PERMISSION_DENIED、CONFLICT、CAPABILITY_UNAVAILABLE、MISSING_BACKEND、BACKEND_ERROR、RECOVERY_REQUIRED、LIMIT_EXCEEDED、RESULT_EXPIRED、RESULT_NOT_FOUND、SESSION_CLOSED。\n\n成功結果包含 `trace.id`、`trace.result_sha256`、`trace.result_bytes`、`trace.elapsed_ms`；digest 是加上 trace 前的 JSON bytes。audit 記錄同一 trace ID 與錯誤 code，不保存工具輸入輸出內容。未經真實後端驗證的功能仍需分別驗收。\n\n大型結果回 result_id、bytes、SHA-256、expires_ms；以 result_read 分塊讀取，整份 digest 和 chunk_sha256 可校驗。輸出 chunk 為 UTF-8 byte offsets，不是字數。結果依 project 隔離，保留 24 小時，最多 128 份／128 MiB；達配額拒絕新 spill，不提前驅逐未到期結果。清理於 spill/read 到期時進行，沒有背景排程。\n\n`replacement_preview` 列出 source hash 與搜尋選項綁定的 occurrence IDs；`replace_in_files.selected_occurrence_ids` 可只選部分命中。expected_total_matches 是所有匹配數，selected_matches 是實際選取數；空匹配、重複檔案或 ID、錯誤 hash 均拒絕。Regex flags 為 case_sensitive、multiline、dot_matches_new_line；行為由 Rust regex 決定，這是文字編輯。\n\n`prepare_code_action`／`move_symbol`／`inline_symbol`／`apply_quick_fix` 只接受實際 LSP 提供的 edit-only CodeAction，必須以唯一 title 選擇，先回交易預覽。需要 resolve 的 action 只呼叫 codeAction/resolve；含 server command、disabled、kind 不符、缺少 edit 均拒絕。Move 必須真的修改指定 source 和 target。沒有後端相應語意能力就回明確錯誤。\n\n`memory_export` 回 `bundle`；`memory_import` 與 `memory_move_scope` 先回 shared memory transaction preview，以 memory_transaction_apply 套用。所有 affected scope 與既有 hash 重新核對；現存目標不覆寫。舊 write/edit/delete/rename_memory 保持直接 journal commit。\n\nflow 的 namespace 依 (project, session) 各自序列化；不同 session 可進行獨立流程，每次 native tool 仍經 project gate 和 policy。pointer/take/map/filter/reduce 是有界資料操作，sum 使用有限 f64，無任意程式執行。整條 flow 使用 host RPC timeout，失敗／取消不提交 namespace；busy session 的 flow_clear 回 CONFLICT。\n\n`debug_start` 回 session_id；debug_batch 必填該 ID，每步最多 32 個 allowlisted DAP request，回每步 request_id 和真實 outcome。launch/attach 是 pending ticket，需配置完成並 poll 才能判斷啟動完成。allow_debug 由 host 明確授權 debugger target execution；它不是 OS 沙箱，也沒有宣稱限制 launch payload 的所有除錯器特有欄位。\n\nFTS5 查詢先增量 refresh 當前檔案快照；這是詞彙搜尋，不是語意分析，也沒有 watcher 或 embeddings。詳見 [引擎實作與驗證界線](ENGINE_IMPLEMENTATION.md)。\n''',encoding='utf-8', newline='\n')
with (ROOT/'docs/API.md').open('a',encoding='utf-8',newline='\n') as api:
 api.write('''\n## 原生 IDE 與檢查器\n\n`completion`、`hover`、`signature_help`、`call_hierarchy`、`inlay_hints`、`semantic_tokens` 回傳 provider、來源 sha256/version、UTF-16 encoding 與 freshness。`workspace_symbols` 要求明確 configured language，限制本專案相符語言檔案與 scope；回傳的 current 不等於語言伺服器索引完整。completion/inlay resolve 需要 expected_sha256，只解析 metadata，不執行 command 或套用 edits。semantic tokens 附 tokenTypes/tokenModifiers legend。缺少 server capability 回明確錯誤。\n\n`format_document` 要求 allow_write 與 expected_sha256，將真正 LSP formatter edits 轉為 transaction preview；文件格式化與 range formatting 都使用相同 CAS 路徑。空 edit 回 changed=false。\n\n`run_inspections` 需要 allow_inspections、host 固定 profile 及 configured native sandbox；不能接受 caller argv。Clippy/Ruff/ESLint 解析各自真正 JSON，不以 regex 冒充分析器；來源以執行前後快照核對。Ruff 要求 --no-fix --no-cache，Clippy 要求 --locked，ESLint 禁止 repository executable config lookup，explicit executable config 只能來自所有專案外的 trusted host path。可寫／暫存 sandbox 路徑不得與任一專案或 Odin state 重疊。\n\n`prepare_inspection_fix` 另要求 allow_write，重新執行固定檢查器，以 current finding_id 選擇 safe Ruff、ESLint fix 或 Clippy MachineApplicable 的單檔 edits；驗證 expected_sha256 後只回交易預覽。未知版本、未完成分析、過期來源、跨檔或不安全建議不能套用。檢查結果的 summary.complete 僅表示固定 profile 的 report 完整；whole_project_verified 始終為 false，不是完整 IDE 或安全性認證。細節與證據見 [IDE_IMPLEMENTATION](IDE_IMPLEMENTATION.md)。\n''')
 api.write('''\n`debug_launches[profile]` 可由 host 固定 launch/attach/restart payload；空 arguments 使用該 payload，非空必須完全相同，未配置的 attach/restart 拒絕。single request 與 batch 都在送給 adapter 前核對。未配置此限制時 allow_debug 保持 privileged host debugging，回報 unrestricted_host_debug=true。LSP/DAP 與 shell 的 host sandbox 設定會傳入共用 native launch；配置失敗不降級。\n''')
if arguments.api_only:
 print(f'Generated API for {len(T)} Odin tools.')
 raise SystemExit(0)
N={t['name'] for t in T}
serena_groups={
'symbol_tools': 'find_declaration find_implementations find_referencing_symbols find_symbol get_diagnostics_for_file get_diagnostics_for_symbol get_symbols_overview insert_after_symbol insert_before_symbol rename_symbol replace_symbol_body restart_language_server safe_delete_symbol',
'jetbrains_tools': 'jet_brains_debug jet_brains_find_declaration jet_brains_find_implementations jet_brains_find_referencing_symbols jet_brains_find_symbol jet_brains_get_symbols_overview jet_brains_inline_symbol jet_brains_list_inspections jet_brains_move jet_brains_rename jet_brains_run_inspections jet_brains_safe_delete jet_brains_type_hierarchy',
'cmd_tools':'execute_shell_command',
'config_tools':'activate_project get_current_config open_dashboard remove_project',
'file_tools':'create_text_file delete_lines find_file insert_at_line list_dir read_file replace_content replace_in_files replace_lines search_for_pattern',
'memory_tools':'delete_memory edit_memory list_memories read_memory rename_memory write_memory',
'query_project_tools':'list_queryable_projects query_project',
'repl_tools':'serena_repl',
'workflow_tools':'initial_instructions onboarding serena_info',
}
notes={
 'jet_brains_run_inspections':'原生 Clippy/Ruff/ESLint providers、強制唯讀來源 OS 沙箱與 CAS safe-fix preview；不重現 JetBrains 私有 inspection 規則全集。',
'find_declaration':'原生接受 LSP position；沒有上游 regex-occurrence 完全同形 API。',
'find_symbol':'LSP name paths 的 exact/substring/glob/regex 篩選、kind/depth/scope/grouping/page；不保證上游輸出同形。',
'find_referencing_symbols':'原生位置＋可取得的 enclosing symbol；結果受 LSP / workspace 影響。',
'get_diagnostics_for_file':'原生 pull/push report；不等同 IDE inspection 或已完成 build。',
'get_diagnostics_for_symbol':'原生 range 篩選；未遞迴檢查全部 referencing symbols。',
'get_symbols_overview':'原生 DocumentSymbol flatten；不重現全部上游輸出格式。',
'rename_symbol':'有序 text/resource WorkspaceEdit → CAS journal；限regular UTF-8 files，拒絕overwrite/directory/recursive。',
'safe_delete_symbol':'原生只檢查 LSP 靜態引用；不含動態使用或 cascading delete。',
'execute_shell_command':'原生僅固定 host argv profile；不接受上游任意 shell 字串。',
'activate_project':'原生每次指定 project ID；不允許任意 root 動態註冊。',
'get_current_config':'Host TOML、project flags與只收緊權限的contexts/modes；不解析上游YAML/Jinja。',
'open_dashboard':'原生手動開 loopback / 唯讀頁；沒有完整上游 UI 或自動 browser-open API。',
'remove_project':'原生編輯 host config 後重啟；沒有 Agent 動態刪配置。',
'list_dir':'原生 files_only；不列獨立空目錄。',
'find_file':'原生 glob 對專案相對路徑；參數與上游不完全相同。',
'replace_in_files':'原生全檔 hash＋match count＋preview，可依 hash 綁定 occurrence IDs 選取；有展開預算。',
'edit_memory':'原生 literal replace＋CAS；非上游全部 regex 選項。',
'rename_memory':'原生同 scope 改名及引用更新；另提供 scope move/import/export 交易與 CAS。',
'list_queryable_projects':'原生 list_projects；只有 host 已配置的範圍。',
'query_project':'原生只接受 nonrecursive native read tools；不啟動 Serena Project Server。',
'serena_repl':'原生 flow_run 是受限 JSON，不是 Python REPL；沒有任意 Python REPL；此差異不會以橋接補足。',
'onboarding':'原生收集結構與證據；讓 Agent 決定記憶，不宣稱自動理解／測試。',
'serena_info':'原生文件及 initial_instructions；提供 Odin 自己的工具說明，不提供上游主題 API。',
}
parity=[]
for group,names in serena_groups.items():
 for name in names.split():
  native=name if name in N else {'jet_brains_debug':'debug_start / debug_request / debug_batch','jet_brains_find_declaration':'find_declaration','jet_brains_find_implementations':'find_implementations','jet_brains_find_referencing_symbols':'find_referencing_symbols','jet_brains_find_symbol':'find_symbol','jet_brains_get_symbols_overview':'get_symbols_overview','jet_brains_inline_symbol':'inline_symbol','jet_brains_move':'move_symbol','jet_brains_rename':'rename_symbol','jet_brains_run_inspections':'run_inspections / prepare_inspection_fix','jet_brains_safe_delete':'safe_delete_symbol','jet_brains_type_hierarchy':'type_hierarchy','list_queryable_projects':'list_projects','serena_repl':'flow_run (not Python)','activate_project':'explicit project argument','open_dashboard':'HTTP /','remove_project':'host configuration only','serena_info':'initial_instructions + docs'}.get(name,'—')
  status='NATIVE_SOURCE_PARTIAL' if name in N or native!='—' else 'NOT_IMPLEMENTED'
  note=notes.get(name,'原生 LSP／DAP 取決於 backend capabilities；無 JetBrains 私有 inspection provider 或 persistent Python REPL，未宣稱等價。' if name.startswith('jet_brains_') else '有原生對應原始碼；參數／輸出不保證與 Serena 同形。')
  if name in ['create_text_file','delete_lines','insert_at_line','replace_content','replace_lines','insert_after_symbol','insert_before_symbol','replace_symbol_body']:
   note+=' 原生來源修改先 preview 再 apply。'
  if group=='memory_tools':note+=' 原生使用 central Markdown storage。'
  parity.append({'serena_tool':name,'group':group,'odin_native':native,'status':status,'runtime_dependency':'NONE_ON_SERENA','verification':'FULL_UPSTREAM_EQUIVALENCE_NOT_VERIFIED','notes':note})
with (ROOT/'docs/serena-parity.csv').open('w',encoding='utf-8-sig',newline='') as f:
 w=csv.DictWriter(f,fieldnames=list(parity[0]),lineterminator='\n');w.writeheader();w.writerows(parity)
(ROOT/'docs/serena-parity.json').write_text(json.dumps({'baseline_commit':'abb0a0f01b94bc8050086f0fb860077d17730324','rows':parity},ensure_ascii=False,indent=2)+'\n', encoding='utf-8', newline='\n')
preamble='''# Serena → Odin 功能對照\n\n基準：Serena tools catalog / v2 時期 main commit `abb0a0f01b94bc8050086f0fb860077d17730324`。本表是設計與原始碼映射，不是實測相容認證。\n\n- NATIVE_SOURCE_PARTIAL：有原生對應或刻意改變的工作流；不表示逐參數等價。\n- NOT_IMPLEMENTED：本版缺少相對應實作，沒有橋接補足。\n- NONE_ON_SERENA：程式不啟動、載入或呼叫 Serena；不是全部功能等價的宣告。\n- Windows 建置／測試見 [local-validation](../verification/local-validation.json)；Rust/Python/TypeScript LSP 與 debugpy 實測見 [real-backends](../verification/real-backends.json)。這些不代表本表全部語言、參數與 IDE 等價。\n\n| Serena tool | Odin 原生對應 | 實作分類 | 差異／依賴 |\n|---|---|---|---|\n'''
(ROOT/'docs/SERENA_PARITY.md').write_text(preamble+'\n'.join(f"| `{r['serena_tool']}` | `{r['odin_native']}` | {r['status']} | {r['notes']} |" for r in parity)+'''\n\n## 不只工具名稱的功能\n\n| 領域 | 原生現況 | 尚未對齊／依賴界線 |\n|---|---|---|\n| 40+ 語言與專用適配 | 直接 LSP transport、28 個設定指引 profiles 與檔案 languageId；逐語驗收另列 | 必須逐語言原生適配與驗收；不包含上游 adapters |\n| REPL facade `lsp / jb / ide / fs / edit / mem / cfg / ext / shell` | 同領域部分 native tools；不模擬 Python objects/facades | 沒有 Python / JetBrains REPL；待獨立設計，不回退上游 |\n| YAML、Jinja、contexts、modes | host TOML、project flags、只收緊權限的 contexts/modes | 不執行 repository 配置或 Jinja；不保證相同設定語意 |\n| 記憶 topic / ref / rename | Markdown + mem: + CAS、readonly、scope import/export/move | 不承諾完整上游 repository-local 行為 |\n| index | 增量 SQLite FTS5 + LSP 自有語意索引 | 沒有背景 watcher；FTS 是詞彙搜尋 |\n| 多專案 | explicit scope，禁止重疊 roots | 直接按專案管理 LSP / DAP process，沒有上游 active_project |\n| Dashboard / logs / analytics | 唯讀專案與 metadata audit、trace ID／digest | 不含上游完整設定 UI、日誌串流與 token analytics |\n| DAP vs IDE debug | session、pending tickets、batch、cancel，已實測 debugpy | 不提供 JetBrains persistent debug REPL 語法 |\n| stdio / Streamable HTTP | 原生 official rmcp server + loopback Bearer | Odin 自己提供 tools；沒有外部工具轉發 |\n| 安裝／更新／clients hooks | Windows portable/Inno recipe、Codex/Claude/OpenCode setup dry-run/CAS apply | 清潔機、跨版本與其他平台驗證仍待完成 |\n\n**整體結論：Odin 0.2.0 不依賴 Serena；功能缺口仍存在。獨立運作與全功能等價是不同的驗收條件。**\n''',encoding='utf-8', newline='\n')
manifest=tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))
deps=manifest['workspace']['dependencies']
text='# 依賴與建置狀態\n\n本次已用 Rust/Cargo 1.90.0 在 Windows MSVC 實際建置，提交真實 Cargo.lock；後續使用 --locked。直接 dependencies 固定精確版本。完整套件／checksums 見 Cargo.lock、[SBOM](../verification/sbom.cdx.json)、[授權清單](../verification/license-inventory.json)及 [授權全文](THIRD_PARTY_LICENSES.txt)。漏洞／授權檢查的實際 exit code 見 [local-validation](../verification/local-validation.json)，不代表未來持續無漏洞或可重現位元組建置。\n\n| Dependency | Manifest requirement |\n|---|---|\n'
for name,value in deps.items():
 text+=f"| `{name}` | `{value if isinstance(value,str) else value.get('version','workspace')}` |\n"
text+='\nCore 另外固定 cap-std/cap-fs-ext 4.0.3、Windows win32job 2.0.3、Unix nix 0.30.1（signal/process）。capability handles、Job objects 與 process groups 不是完整 OS 沙箱。\n\n原生核心的語意後端不是內嵌 compiler：language server / debug adapter 是另外安裝的程式。SQLite 使用 bundled feature，建置仍需原生 C toolchain。Odin 不連接 Serena 或其 JetBrains 外掛；語言伺服器與除錯器有各自執行環境。\n'
(ROOT/'docs/DEPENDENCIES.md').write_text(text,encoding='utf-8', newline='\n')
print(f'Generated API for {len(T)} Odin tools and parity for {len(parity)} Serena tools.')
