#!/usr/bin/env python3
"""Generate the deliberately small JSON-Schema vocabulary used by the core.
No tool implementation is generated. The contract checker verifies dispatcher parity.
"""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def s(description="", **kw): return {"type":"string", **({"description":description} if description else {}), **kw}
def n(default=None, minimum=0, maximum=100000):
    d={"type":"integer","minimum":minimum,"maximum":maximum}
    if default is not None: d["default"]=default
    return d
def b(default=False): return {"type":"boolean","default":default}
def obj(props=None, required=None, additional=False):
    d={"type":"object","properties":props or {},"additionalProperties":additional}
    if required: d["required"]=required
    return d
def arr(items, maximum=256):return {"type":"array","items":items,"maxItems":maximum}
def nullable_string():return {"type":["string","null"]}
POS=obj({"line":n(None,0,2147483647),"character":n(None,0,2147483647)},["line","character"])
RANGE=obj({"start":POS,"end":POS},["start","end"])
CHANGE=obj({"path":s(),"expected_sha256":nullable_string(),"content":nullable_string()},["path","expected_sha256","content"])
TOOLS=[]
def add(name,description,policy,props=None,required=None):
    props={"project":s("Configured project ID; never an arbitrary root path"),**(props or {})}
    TOOLS.append({"name":name,"description":description,"policy":policy,"inputSchema":obj(props,["project"]+(required or [])),
                  "annotations":{"readOnlyHint":policy == "read","destructiveHint":policy in ("write","execute"),"idempotentHint":policy=="read","openWorldHint":policy in ("execute",)}})
add("status","Effective standalone project status and backend boundaries","read")
add("get_version","Return Odin version and native schema format version","read")
add("get_schemas","Return tool contracts or one named tool schema","read",{"tool":s()})
add("get_capabilities","Report effective tool policy and optionally initialize configured language servers to probe capabilities","read",{"language":s(),"probe":b(),"include_tools":b(True)})
add("list_projects","List operator-registered projects without changing a global active project","read")
add("get_current_config","Redacted effective host configuration","read")
add("initial_instructions","Return Odin's project and transaction usage instructions","read")
add("onboarding","Inspect project files and return evidence and memory guidance; never invent build results","read")
add("refactor_rules","Report host-defined refactor rules enforced by semantic refactor tools","read")
add("project_analysis","Analyze live project structure, language hints and host backend gaps without executing repository commands or starting backends","read",{"max_evidence":n(20,1,64),"sample_files":n(20,0,100)})
add("list_dir","List project files respecting ignore rules and path policy","read",{"relative_path":s(),"recursive":b(True)})
add("find_file","Find paths using a glob, respecting ignore rules","read",{"file_mask":s()},["file_mask"])
add("read_file","Read bounded UTF-8 text; start/end lines are 1-based and inclusive","read",{"relative_path":s(),"start_line":n(1,1),"end_line":n(None,1)},["relative_path"])
add("search_for_pattern","Live regex search; returned line numbers are 1-based","read",{"pattern":s()},["pattern"])
add("index_build","Rebuild a local SQLite FTS5 lexical snapshot (not vector semantic search)","state")
add("index_query","Refresh changed/new/deleted files then query local FTS5 lexical search","read",{"query":s(),"limit":n(20,1,200)},["query"])
SYMBOL_FILTERS={"depth":n(None,0,64),"include_kinds":arr(n(None,1,26),26),"exclude_kinds":arr(n(None,1,26),26),"scope":s(),"offset":n(0,0,100000),"limit":n(None,1,10000),"group_by_file":b()}
add("get_symbols_overview","File symbols from a configured LSP with kind, scope and hierarchy depth filters","read",{"relative_path":s(),**SYMBOL_FILTERS},["relative_path"])
add("find_symbol","Find real LSP name paths with exact/substring/glob/regex matching, filters and pagination","read",{"query":s(),"relative_path":s(),"include_body":b(),"match_mode":s(enum=["substring","exact","glob","regex"]),"case_sensitive":b(True),**SYMBOL_FILTERS},["query"])
for name,desc in [("find_referencing_symbols","Find LSP references and enclosing project symbols"),("find_implementations","Find implementations using server capability negotiation")]:
    add(name,desc,"read",{"relative_path":s(),"name_path":s()},["relative_path","name_path"])
add("find_declaration","Find definition/declaration at a zero-based UTF-16 position","read",{"relative_path":s(),"position":POS,"declaration_only":b()},["relative_path","position"])
add("get_type_hierarchy","Query LSP type hierarchy if the server advertises support","read",{"relative_path":s(),"name_path":s(),"direction":s(enum=["super","sub"])},["relative_path","name_path","direction"])
add("get_diagnostics_for_file","Get pull/push diagnostics; pending or unversioned is not a clean-build proof","read",{"relative_path":s()},["relative_path"])
add("get_diagnostics_for_symbol","Filter file diagnostics to a symbol range","read",{"relative_path":s(),"name_path":s()},["relative_path","name_path"])
add("code_actions","List LSP code actions only; never auto-execute a server command","read",{"relative_path":s(),"range":RANGE},["relative_path","range"])
for name,description in [("completion","Request native LSP completions with document freshness metadata"),("hover","Get native LSP hover information with UTF-16 coordinates"),("signature_help","Get native LSP signature help for the current document")]:
    props={"relative_path":s(),"position":POS}
    if name=="completion":props["limit"]=n(50,1,200)
    add(name,description,"read",props,["relative_path","position"])
add("completion_resolve","Resolve completion metadata against an exact source hash; never execute item commands or apply text edits","read",{"relative_path":s(),"item":obj(additional=True),"expected_sha256":s()},["relative_path","item","expected_sha256"])
add("workspace_symbols","Search real LSP workspace symbols for an explicit configured language; bounded local URI results","read",{"language":s(),"query":s(),"relative_path":s(),"limit":n(100,1,1000)},["language","query"])
add("call_hierarchy","Prepare and query real LSP incoming or outgoing calls; missing backend capabilities are errors","read",{"relative_path":s(),"position":POS,"direction":s(enum=["incoming","outgoing"])},["relative_path","position","direction"])
FORMAT_OPTIONS=obj({"tab_size":n(4,1,16),"insert_spaces":b(True),"trim_trailing_whitespace":b(),"insert_final_newline":b(),"trim_final_newlines":b()})
add("format_document","Prepare LSP formatting or an explicitly selected host Ruff stdin profile as a SHA-256 guarded transaction; no fallback","write",{"relative_path":s(),"expected_sha256":s(),"formatter_profile":s("Explicit host native formatter profile; requires formatter permission and read-only source sandbox; whole document and fixed host options only"),"options":FORMAT_OPTIONS,"range":RANGE,"request_id":s()},["relative_path","expected_sha256"])
add("inlay_hints","Get bounded native LSP inlay hints with document metadata; never execute label commands","read",{"relative_path":s(),"range":RANGE,"limit":n(200,1,1000)},["relative_path"])
add("inlay_hint_resolve","Resolve an LSP inlay hint against an exact source hash without executing commands or edits","read",{"relative_path":s(),"hint":obj(additional=True),"expected_sha256":s()},["relative_path","hint","expected_sha256"])
add("semantic_tokens","Get real LSP semantic tokens and legend/encoding metadata, with current document freshness","read",{"relative_path":s(),"range":RANGE},["relative_path"])
add("list_inspection_providers","List native provider types and configured host profile metadata without commands or secrets","read")
add("run_inspections","Run one host-fixed Clippy/Ruff/ESLint profile inside a mandatory configured OS sandbox; return bounded source-verified findings","execute",{"profile":s(),"limit":n(100,1,2000)},["profile"])
add("prepare_inspection_fix","Rerun an authorized native inspection and preview one current safe single-file finding fix as a hash-guarded transaction","write",{"profile":s(),"finding_id":s(),"expected_sha256":s(),"request_id":s(),"limit":n(100,1,2000)},["profile","finding_id","expected_sha256"])
ACTION={"relative_path":s(),"range":RANGE,"action_title":s(),"expected_sha256":s(),"request_id":s()}
add("prepare_code_action","Prepare exactly one edit-only LSP code action as a CAS transaction; reject commands and ambiguous titles","write",{**ACTION,"action_kind":s()},["relative_path","range","action_title","expected_sha256"])
add("apply_quick_fix","Preview one real LSP quickfix WorkspaceEdit; transaction_apply is still required","write",ACTION,["relative_path","range","action_title","expected_sha256"])
for name,kind in [("move_symbol","move"),("inline_symbol","inline")]:
    props={"relative_path":s(),"name_path":s(),"action_title":s(),"expected_sha256":s(),"request_id":s()};required=["relative_path","name_path","action_title","expected_sha256"]
    if name=="move_symbol":props["target_path"]=s();required.append("target_path")
    add(name,f"Preview backend-provided refactor.{kind} edit-only code action; no text fallback or server command execution","write",props,required)
add("restart_language_server","Drop project LSP processes; lazily start clean instances on the next query","state")
add("transaction_prepare","Stage full-file changes guarded by expected SHA-256; null means create-only","write",{"changes":arr(CHANGE),"request_id":s()},["changes"])
for name,policy,desc in [("transaction_preview","read","Read a durable transaction preview"),("transaction_apply","write","Apply a staged transaction with revalidation and recovery journal"),("transaction_rollback","write","Undo a committed transaction using a new conflict-checked transaction")]:
    add(name,desc,policy,{"transaction_id":s()},["transaction_id"])
add("transaction_list","List journal phases","read")
add("transaction_recover","Restore interrupted transactions without overwriting external modifications","write")
add("create_text_file","Prepare a file create/overwrite; explicit transaction_apply is required","write",{"relative_path":s(),"content":s(),"expected_sha256":nullable_string(),"request_id":s()},["relative_path","content","expected_sha256"])
replace={"relative_path":s(),"needle":s(),"replacement":s(),"regex":b(),"expected_sha256":s(),"expected_matches":n(1,1,10000)}
add("replace_content","Prepare guarded literal/regex replacement, preserving full-file hash precondition","write",replace,["relative_path","needle","replacement","expected_sha256"])
OCCURRENCES={"files":arr(obj({"relative_path":s(),"expected_sha256":s()},["relative_path","expected_sha256"])),"needle":s(),"replacement":s(),"regex":b(),"case_sensitive":b(True),"multiline":b(),"dot_matches_new_line":b(),"allow_multiple":b(True),"expected_total_matches":n(None,1,10000),"selected_occurrence_ids":arr(s(),10000),"request_id":s()}
add("replacement_preview","List source-hash-bound occurrence IDs and before/after text without changing source","read",OCCURRENCES,["files","needle","replacement"])
add("replace_in_files","Prepare selected immutable occurrence IDs or all matches with exact source hashes and total count","write",OCCURRENCES,["files","needle","replacement","expected_total_matches"])
for name in ["replace_lines","delete_lines","insert_at_line"]:
    props={"relative_path":s(),"start_line":n(None,1),"expected_sha256":s()};req=list(props)
    if name!="insert_at_line":props["end_line"]=n(None,1);req.append("end_line")
    if name!="delete_lines":props["content"]=s();req.append("content")
    add(name,"Prepare line edit; line indices are 1-based and end is inclusive","write",props,req)
for name in ["replace_symbol_body","insert_before_symbol","insert_after_symbol"]:
    add(name,"Prepare a symbol-bound edit using UTF-16 ranges and an exact content hash","write",{"relative_path":s(),"name_path":s(),"body":s(),"expected_sha256":s()},["relative_path","name_path","body","expected_sha256"])
add("rename_symbol","Ask LSP for a workspace rename, validate all text edits, return a transaction preview","write",{"relative_path":s(),"name_path":s(),"new_name":s()},["relative_path","name_path","new_name"])
add("safe_delete_symbol","Prepare deletion only when LSP reports no static references; dynamic uses remain outside the guarantee","write",{"relative_path":s(),"name_path":s(),"expected_sha256":s()},["relative_path","name_path","expected_sha256"])
add("list_memories","List Markdown memory names without loading content","read")
add("read_memory","Read Markdown memory and its current hash","read",{"name":s()},["name"])
add("write_memory","Write a Markdown memory with compare-and-swap; null hash is create-only","write",{"name":s(),"content":s(),"expected_sha256":nullable_string()},["name","content","expected_sha256"])
add("edit_memory","Guarded literal replacement in an existing Markdown memory","write",{"name":s(),"needle":s(),"replacement":s(),"expected_sha256":s(),"expected_matches":n(1,1,10000)},["name","needle","replacement","expected_sha256"])
add("delete_memory","Delete a Markdown memory guarded by hash; use check_memories to inspect dangling references","write",{"name":s(),"expected_sha256":s()},["name","expected_sha256"])
add("rename_memory","Rename Markdown and exact mem: references with a shared recovery journal","write",{"old_name":s(),"new_name":s(),"expected_sha256":s()},["old_name","new_name","expected_sha256"])
MEMORY_BUNDLE=obj({"format":s(enum=["odin-memory-bundle"]),"schema_version":n(None,1,1),"project":s(),"notes":arr(obj({"name":s(),"content":s(),"sha256":s()},["name","content","sha256"]),1024)},["format","schema_version","project","notes"])
add("memory_export","Export versioned Markdown notes with SHA-256 without writing repository files","read")
add("memory_import","Prepare create-only import of a checked versioned memory bundle; apply separately","write",{"bundle":MEMORY_BUNDLE},["bundle"])
add("memory_move_scope","Prepare permission-checked memory move to a configured project or global scope; reject dangling references","write",{"name":s(),"destination_project":s(),"destination_name":s(),"expected_sha256":s()},["name","destination_project","destination_name","expected_sha256"])
add("memory_transaction_preview","Preview a memory transaction owned by this initiating project","read",{"transaction_id":s()},["transaction_id"])
add("memory_transaction_apply","Apply a memory transaction after rechecking initiating project, affected scopes and source hashes","write",{"transaction_id":s()},["transaction_id"])
add("check_memories","Validate explicit mem: reference integrity","read")
add("search_memories","Search names/body by regex without returning every memory body","read",{"pattern":s()},["pattern"])
add("recover_memories","Recover an interrupted memory journal; shared across configured projects","write")
add("query_project","Call a read-only native tool against another configured project","read",{"target_project":s(),"tool":s(),"arguments":obj(additional=True)},["target_project","tool","arguments"])
add("flow_run","Run session-isolated bounded JSON dataflow with pointer/map/filter/count/sum; no eval, imports or arbitrary Python","read",{"session":s(),"steps":arr(obj({"as":s(),"tool":s(),"arguments":obj(additional=True),"select":{},"take":n(None,0,10000),"pointer":s(),"map":s(),"filter":obj({"pointer":s(),"equals":{}},["pointer","equals"]),"reduce":s(enum=["count","sum"])},["as"]),128),"return":{}},["session","steps"])
add("flow_vars","List retained per-project flow variable names and sizes","read",{"session":s()},["session"])
add("flow_clear","Clear this per-project flow session","state",{"session":s()},["session"])
add("execute_shell_command","Run a fixed operator-defined argv profile; no caller-supplied shell string","execute",{"profile":s()},["profile"])
add("debug_start","Start a configured DAP adapter; requires allow_debug","execute",{"profile":s()},["profile"])
add("debug_request","Send an allowlisted DAP request; launch/attach/restart return tickets; native cancel_ticket requests best-effort cancellation","execute",{"command":s(),"arguments":obj(additional=True),"session_id":s()},["command","arguments"])
add("debug_events","Drain DAP events and completed launch tickets","execute",{"session_id":s()})
add("debug_stop","Disconnect debuggee with termination requested and drop the adapter","execute",{"session_id":s()})
add("debug_batch","Run 1..32 allowlisted DAP requests in an explicit project session with per-step request IDs","execute",{"session_id":s(),"commands":arr(obj({"command":s(),"arguments":obj(additional=True)},["command","arguments"]),32),"stop_on_error":b(True)},["session_id","commands"])
add("result_read","Read a spilled large result by byte chunks; offsets remain UTF-8 aligned","read",{"result_id":s(),"offset":n(0,0,16777216),"max_bytes":n(16384,256,32768)},["result_id"])
add("audit_log","Read redacted tool audit records without input/output source contents","read",{"limit":n(50,1,1000)})
# Agent-facing descriptions (Claude first): the first sentence says WHEN to use the
# tool; constraints follow. These supersede the terse internal summaries above.
DESCRIPTIONS={
 "refactor_rules":"Host-defined refactor rules enforced by rename_symbol, safe_delete_symbol, move_symbol, inline_symbol, prepare_code_action and apply_quick_fix (violations fail with REFACTOR_RULE_VIOLATION). Call before refactoring. Read-only.",
 "move_symbol":"Preview a backend-provided refactor.move code action (needs target_path); no text fallback. Host refactor_rules may reject it.",
 "inline_symbol":"Preview a backend-provided refactor.inline code action; no text fallback. Host refactor_rules may reject it.",
 "project_analysis":"Start here in an unfamiliar workspace: live layout, detected languages and which semantic backends are missing. Read-only; runs no repository commands.",
 "get_capabilities":"Report effective policy and configured language server status. The tools list is included by default. For a compact language check, pass {\"project\":\"demo\",\"language\":\"rust\",\"include_tools\":false}. By default, only report current status. Set probe=true to attempt the LSP initialize handshake.",
 "list_projects":"List configured project IDs and their permissions. Only needed when more than one project is configured.",
 "list_dir":"List project files (respects .gitignore and host deny rules); recursive by default.",
 "find_file":"Find project files by glob, e.g. **/lib.rs (respects .gitignore and host deny rules).",
 "read_file":"Read a UTF-8 file (1-based inclusive start_line/end_line) and get its sha256. Call this before any Odin edit: every edit tool requires expected_sha256.",
 "search_for_pattern":"Regex search over project files; returns 1-based lines and each file's sha256. Prefer find_symbol for definitions and find_referencing_symbols for usages.",
 "index_build":"Rebuild the local SQLite FTS5 full-text index. Lexical only, not semantic; index_query refreshes it incrementally anyway.",
 "index_query":"Ranked full-text (FTS5) word search with snippets; refreshes changed files first. Use for fuzzy word lookup in large repos.",
 "get_symbols_overview":"Use to understand a file without reading it all: its symbols (functions, types, fields) with kinds and nesting, from the language server.",
 "find_symbol":"Use to locate definitions by name across the project via the language server (substring/exact/glob/regex). Returns name_path for other symbol tools; include_body=true returns source.",
 "find_referencing_symbols":"Use before changing a symbol: all semantic references to it (relative_path + name_path from find_symbol), each with its enclosing symbol. Unlike text search, ignores comments and same-named items.",
 "find_implementations":"Find implementations of a trait/interface or method (relative_path + name_path) via the language server.",
 "find_declaration":"Go to definition/declaration of the identifier at a position (0-based line, UTF-16 character).",
 "get_type_hierarchy":"Supertypes or subtypes of a type (relative_path + name_path), when the language server supports it.",
 "get_diagnostics_for_file":"Compiler/linter diagnostics the language server reports for one file. Pending or empty results do not prove a clean build; run the test profile for that.",
 "get_diagnostics_for_symbol":"Diagnostics limited to one symbol's range.",
 "code_actions":"List language-server code actions (quick fixes, refactors) for a range. Never executes them; use prepare_code_action to preview one.",
 "hover":"Type and documentation of the identifier at a position (0-based line, UTF-16 character).",
 "signature_help":"Function signature and active parameter at a call position.",
 "workspace_symbols":"Fast fuzzy symbol search via the language server for one configured language (e.g. language=rust).",
 "call_hierarchy":"Callers (incoming) or callees (outgoing) of the function at a position.",
 "format_document":"Format a file with the language server (or an explicit host Ruff profile). Returns a transaction preview; transaction_apply writes it.",
 "prepare_code_action":"Preview one edit-only code action, selected by its exact title from code_actions, as a transaction. Host refactor_rules may reject it.",
 "apply_quick_fix":"Preview one quickfix code action (exact title) as a transaction; transaction_apply writes it. Host refactor_rules may reject it.",
 "restart_language_server":"Restart this project's language servers, e.g. after a crash or a toolchain/config change. They start lazily on the next query.",
 "transaction_prepare":"Stage full new contents for one or more files at once (expected_sha256=null creates). Returns a transaction preview; transaction_apply writes it.",
 "transaction_preview":"Show the diff and state of a prepared transaction. Review it before transaction_apply.",
 "transaction_apply":"Write a prepared transaction after re-checking every file hash; refuses (CONFLICT) if a file changed since preview. Applying is not a passing build.",
 "transaction_rollback":"Undo an applied transaction with a new conflict-checked transaction.",
 "transaction_list":"List transactions and their journal phases.",
 "create_text_file":"Create a file (expected_sha256=null) or overwrite one (expected_sha256=current hash). Returns a transaction preview; transaction_apply writes it.",
 "replace_content":"Use for a targeted text edit in one file: literal (or regex=true) needle to replacement, guarded by expected_sha256 and expected_matches. Returns a transaction preview; transaction_apply writes it.",
 "replacement_preview":"Preview a multi-file search/replace and get occurrence IDs to select for replace_in_files. Changes nothing.",
 "replace_in_files":"Multi-file replace of all or selected occurrence IDs, guarded by per-file sha256 and the total match count. Returns a transaction preview.",
 "replace_lines":"Replace lines start_line..end_line (1-based, inclusive) with content. Returns a transaction preview.",
 "delete_lines":"Delete lines start_line..end_line (1-based, inclusive). Returns a transaction preview.",
 "insert_at_line":"Insert content before start_line (1-based). Returns a transaction preview.",
 "replace_symbol_body":"Use to rewrite a whole function/type: replaces the symbol's full definition (name_path) with body. Returns a transaction preview; transaction_apply writes it.",
 "insert_before_symbol":"Insert source just before a symbol definition (name_path), e.g. a new function or import. Returns a transaction preview.",
 "insert_after_symbol":"Insert source just after a symbol definition (name_path), e.g. a sibling function. Returns a transaction preview.",
 "rename_symbol":"Use to rename across files: semantic rename via the language server (relative_path + name_path). Returns a transaction preview; transaction_apply writes it. Host refactor_rules may reject it.",
 "safe_delete_symbol":"Delete a symbol only when the language server reports no static references. Returns a transaction preview. Host refactor_rules may reject it.",
 "execute_shell_command":"Run a host-defined command profile by name (e.g. test, check); no free-form shell. Judge the result by success/exit_code, not by the call succeeding.",
 "result_read":"Read a large spilled result (result_id) in UTF-8-aligned byte chunks.",
}
# MCP exposure tiers for --tool-profile: core (default) is the small everyday set,
# ide adds navigation/refactoring/editing helpers, everything else is full only.
CORE="""project_analysis get_capabilities refactor_rules read_file search_for_pattern get_symbols_overview find_symbol
find_referencing_symbols find_implementations find_declaration get_diagnostics_for_file rename_symbol
replace_symbol_body replace_content transaction_preview transaction_apply transaction_rollback
execute_shell_command result_read""".split()
IDE="""list_projects list_dir find_file index_build index_query workspace_symbols hover signature_help
call_hierarchy get_type_hierarchy get_diagnostics_for_symbol code_actions prepare_code_action apply_quick_fix
format_document insert_before_symbol insert_after_symbol safe_delete_symbol move_symbol inline_symbol
create_text_file replace_lines delete_lines insert_at_line replacement_preview replace_in_files
transaction_prepare transaction_list transaction_recover restart_language_server status""".split()
_names={t['name'] for t in TOOLS}
assert not (set(DESCRIPTIONS)|set(CORE)|set(IDE))-_names, 'unknown tool in DESCRIPTIONS/CORE/IDE'
assert not set(CORE)&set(IDE)
for tool in TOOLS:
    tool['description']=DESCRIPTIONS.get(tool['name'],tool['description'])
    tool['tier']='core' if tool['name'] in CORE else 'ide' if tool['name'] in IDE else 'full'
(ROOT/'crates/odin-engine/tools.json').write_text(json.dumps(TOOLS,ensure_ascii=False,indent=2)+'\n', encoding='utf-8', newline='\n')
(ROOT/'schemas/tools.json').write_text(json.dumps(TOOLS,ensure_ascii=False,indent=2)+'\n', encoding='utf-8', newline='\n')
print(f'Generated {len(TOOLS)} tool contracts')
