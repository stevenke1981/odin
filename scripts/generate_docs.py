#!/usr/bin/env python3
"""Generate API and parity indexes from the actual Odin catalog. Not a test runner."""
from pathlib import Path
import csv, io, json, tomllib
ROOT=Path(__file__).resolve().parents[1]
T=json.loads((ROOT/'schemas/tools.json').read_text(encoding='utf-8'))
header='''# Odin 工具 API 0.2.0 Standalone\n\n此表由 `schemas/tools.json` 生成；來源包含實作分派，不代表已編譯或通過測試。\n\n所有工具以明確 `project` 範圍執行；CLI 在提供 `--project` 時可注入該欄位。`read` / `state` / `write` / `execute` 是內部策略分類，不是 OS 隔離保證。read 工具仍可能啟動語言伺服器、寫 audit/cache。\n\n`write`：來源 edits 先回 transaction preview，需另 apply；記憶變更則直接經 journal commit。`execute`：要求相應 shell/debug 權限。本版本沒有外部工具橋接。\n\nLSP position 為 0-based UTF-16；檔案行數為 1-based inclusive。參數細節、範圍與 additionalProperties 規則請讀 JSON schema。此版 schema validator 實作契約需要的有限 JSON Schema 子集，非通用 JSON Schema 驗證器。\n\n| Tool | Policy | 必填參數 | 作用 |\n|---|---|---|---|\n'''
rows=[]
for t in T:
 req=', '.join('`'+k+'`' for k in t['inputSchema'].get('required',[]))
 rows.append(f"| `{t['name']}` | {t['policy']} | {req} | {t['description'].replace('|','/')} |")
(ROOT/'docs/API.md').write_text(header+'\n'.join(rows)+'''\n\n## 結果／錯誤\n\n成功不強制單一業務 envelope；MCP 回 content + structuredContent。業務錯誤使用 isError 與 `{error:{code,message}}`。常見 code：INVALID_ARGUMENT、PERMISSION_DENIED、CONFLICT、CAPABILITY_UNAVAILABLE、BACKEND_ERROR、RECOVERY_REQUIRED、LIMIT_EXCEEDED。\n\n大型結果回 result_id、bytes、SHA-256；以 result_read 分塊讀取。輸出 chunk 為 UTF-8 byte offsets，不是字數。原生限制避免把巨大完整結果直接塞給模型，但沒有實測 token 節省比例。\n\n索引與 source edits 是不同資料流；原生 edits 後 LSP 會 refresh，但 FTS5 需要明確重建快照。\n''',encoding='utf-8')
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
'find_declaration':'原生接受 LSP position；沒有上游 regex-occurrence 完全同形 API。',
'find_symbol':'原生 name_path substring；未重現全部 kind/depth/grouping 選項。',
'find_referencing_symbols':'原生位置＋可取得的 enclosing symbol；結果受 LSP / workspace 影響。',
'get_diagnostics_for_file':'原生 pull/push report；不等同 IDE inspection 或已完成 build。',
'get_diagnostics_for_symbol':'原生 range 篩選；未遞迴檢查全部 referencing symbols。',
'get_symbols_overview':'原生 DocumentSymbol flatten；不重現全部上游輸出格式。',
'rename_symbol':'原生文字 WorkspaceEdit → journal；resource operations 明確拒絕。',
'safe_delete_symbol':'原生只檢查 LSP 靜態引用；不含動態使用或 cascading delete。',
'execute_shell_command':'原生僅固定 host argv profile；不接受上游任意 shell 字串。',
'activate_project':'原生每次指定 project ID；不允許任意 root 動態註冊。',
'get_current_config':'原生 host TOML 與 flags；不含上游 YAML/Jinja contexts/modes。',
'open_dashboard':'原生手動開 loopback / 唯讀頁；沒有完整上游 UI 或自動 browser-open API。',
'remove_project':'原生編輯 host config 後重啟；沒有 Agent 動態刪配置。',
'list_dir':'原生 files_only；不列獨立空目錄。',
'find_file':'原生 glob 對專案相對路徑；參數與上游不完全相同。',
'replace_in_files':'原生全檔 hash＋match count＋preview；無逐 occurrence selection。',
'edit_memory':'原生 literal replace＋CAS；非上游全部 regex 選項。',
'rename_memory':'原生同 scope 改名及引用更新；無跨 scope move。',
'list_queryable_projects':'原生 list_projects；只有 host 已配置的範圍。',
'query_project':'原生只接受 nonrecursive native read tools；不啟動 Serena Project Server。',
'serena_repl':'原生 flow_run 是受限 JSON，不是 Python REPL；沒有任意 Python REPL；此差異不會以橋接補足。',
'onboarding':'原生收集結構與證據；讓 Agent 決定記憶，不宣稱自動理解／測試。',
'serena_info':'原生文件及 initial_instructions；提供 Odin 自己的工具說明，不提供上游主題 API。',
}
parity=[]
for group,names in serena_groups.items():
 for name in names.split():
  native=name if name in N else {'list_queryable_projects':'list_projects','serena_repl':'flow_run (not Python)','activate_project':'explicit project argument','open_dashboard':'HTTP /','remove_project':'host configuration only','serena_info':'initial_instructions + docs'}.get(name,'—')
  status='NATIVE_SOURCE_PARTIAL' if name in N or native!='—' else 'NOT_IMPLEMENTED'
  note=notes.get(name,'未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。' if name.startswith('jet_brains_') else '有原生對應原始碼；參數／輸出不保證與 Serena 同形。')
  if name in ['create_text_file','delete_lines','insert_at_line','replace_content','replace_lines','insert_after_symbol','insert_before_symbol','replace_symbol_body']:
   note+=' 原生來源修改先 preview 再 apply。'
  if group=='memory_tools':note+=' 原生使用 central Markdown storage。'
  parity.append({'serena_tool':name,'group':group,'odin_native':native,'status':status,'runtime_dependency':'NONE_ON_SERENA','verification':'NOT_RUN','notes':note})
with (ROOT/'docs/serena-parity.csv').open('w',encoding='utf-8-sig',newline='') as f:
 w=csv.DictWriter(f,fieldnames=list(parity[0]));w.writeheader();w.writerows(parity)
(ROOT/'docs/serena-parity.json').write_text(json.dumps({'baseline_commit':'abb0a0f01b94bc8050086f0fb860077d17730324','rows':parity},ensure_ascii=False,indent=2)+'\n', encoding='utf-8')
preamble='''# Serena → Odin 功能對照\n\n基準：Serena tools catalog / v2 時期 main commit `abb0a0f01b94bc8050086f0fb860077d17730324`。本表是設計與原始碼映射，不是實測相容認證。\n\n- NATIVE_SOURCE_PARTIAL：有原生對應或刻意改變的工作流；不表示逐參數等價、已編譯或已測試。\n- NOT_IMPLEMENTED：本版缺少相對應實作，沒有橋接補足。\n- NONE_ON_SERENA：程式不啟動、載入或呼叫 Serena；不是全部功能等價的宣告。\n- 所有本表真實後端驗證均為 **NOT_RUN**。\n\n| Serena tool | Odin 原生對應 | 實作分類 | 差異／依賴 |\n|---|---|---|---|\n'''
(ROOT/'docs/SERENA_PARITY.md').write_text(preamble+'\n'.join(f"| `{r['serena_tool']}` | `{r['odin_native']}` | {r['status']} | {r['notes']} |" for r in parity)+'''\n\n## 不只工具名稱的功能\n\n| 領域 | 原生現況 | 尚未對齊／依賴界線 |\n|---|---|---|\n| 40+ 語言與專用適配 | Generic LSP transport / 可配置伺服器；未驗證所有語言 | 必須逐語言原生適配與驗收；不包含上游 adapters |\n| REPL facade `lsp / jb / ide / fs / edit / mem / cfg / ext / shell` | 同領域部分 native tools；不模擬 Python objects/facades | 沒有 Python / JetBrains REPL；待獨立設計，不回退上游 |\n| YAML、Jinja、contexts、modes | host TOML、每 project flags / excluded_tools | 原生無完全相同設定語意，沒有上游配置入口 |\n| 記憶 topic / ref / rename | Markdown + mem: + CAS + read-only patterns | 原生沒有上游全部 ignore / cross-scope / repo-local 行為 |\n| index | SQLite snapshot lexical + LSP 自有語意索引 | 不把兩者說成 Serena 相同的 symbol cache |\n| 多專案 | explicit scope，禁止重疊 roots | 直接按專案管理 LSP / DAP process，沒有上游 active_project |\n| Dashboard / logs / analytics | 唯讀專案與 metadata audit | 不含上游完整設定 UI、日誌串流與 token analytics |\n| DAP vs IDE debug | 原生 generic DAP primitives | 不提供 JetBrains persistent debug REPL 語法 |\n| stdio / Streamable HTTP | 原生 official rmcp server + loopback Bearer | Odin 自己提供 tools；沒有外部工具轉發 |\n| 安裝／更新／clients hooks | 本地建置腳本、手動 MCP config | 沒有原生重寫 Serena setup/hook/marketplace 流程 |\n\n**整體結論：Odin 0.2.0 不依賴 Serena；功能缺口仍存在。獨立運作與全功能等價是不同的驗收條件。**\n''',encoding='utf-8')
manifest=tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))
deps=manifest['workspace']['dependencies']
text='# 依賴與建置狀態\n\n直接 dependencies 已指定精確版本。Cargo 未能在交付環境執行，故 **沒有 Cargo.lock、完整 transitive inventory、dependency audit 或 reproducible build 證據**。首次 bootstrap 會解析後產生真實 lock；審查並提交後再用 --locked。\n\n| Dependency | Manifest requirement |\n|---|---|\n'
for name,value in deps.items():
 text+=f"| `{name}` | `{value if isinstance(value,str) else value.get('version','workspace')}` |\n"
text+='\n原生核心的語意後端不是內嵌 compiler：language server / debug adapter 是另外安裝的程式。SQLite 使用 bundled feature，建置仍需原生 C toolchain。Odin 不連接 Serena 或其 JetBrains 外掛；語言伺服器與除錯器有各自執行環境。\n'
(ROOT/'docs/DEPENDENCIES.md').write_text(text,encoding='utf-8')
print(f'Generated API for {len(T)} Odin tools and parity for {len(parity)} Serena tools.')
