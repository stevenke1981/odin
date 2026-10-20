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
POS=obj({"line":n(),"character":n()},["line","character"])
RANGE=obj({"start":POS,"end":POS},["start","end"])
CHANGE=obj({"path":s(),"expected_sha256":nullable_string(),"content":nullable_string()},["path","expected_sha256","content"])
TOOLS=[]
def add(name,description,policy,props=None,required=None):
    props={"project":s("Configured project ID; never an arbitrary root path"),**(props or {})}
    TOOLS.append({"name":name,"description":description,"policy":policy,"inputSchema":obj(props,["project"]+(required or [])),
                  "annotations":{"readOnlyHint":policy == "read","destructiveHint":policy in ("write","execute"),"idempotentHint":policy=="read","openWorldHint":policy in ("execute",)}})
add("status","Effective standalone project status and backend boundaries","read")
add("list_projects","List operator-registered projects without changing a global active project","read")
add("get_current_config","Redacted effective host configuration","read")
add("initial_instructions","Return Odin's project and transaction usage instructions","read")
add("onboarding","Inspect project files and return evidence and memory guidance; never invent build results","read")
add("list_dir","List project files respecting ignore rules and path policy","read",{"relative_path":s(),"recursive":b(True)})
add("find_file","Find paths using a glob, respecting ignore rules","read",{"file_mask":s()},["file_mask"])
add("read_file","Read bounded UTF-8 text; start/end lines are 1-based and inclusive","read",{"relative_path":s(),"start_line":n(1,1),"end_line":n(None,1)},["relative_path"])
add("search_for_pattern","Live regex search; returned line numbers are 1-based","read",{"pattern":s()},["pattern"])
add("index_build","Rebuild a local SQLite FTS5 lexical snapshot (not vector semantic search)","state")
add("index_query","Query FTS5; stale file hits are omitted; new files require reindexing","read",{"query":s(),"limit":n(20,1,200)},["query"])
add("get_symbols_overview","Hierarchical file symbols from a configured LSP","read",{"relative_path":s()},["relative_path"])
add("find_symbol","Find matching LSP name paths; never report regex results as semantic symbols","read",{"query":s(),"relative_path":s(),"include_body":b()},["query"])
for name,desc in [("find_referencing_symbols","Find LSP references and enclosing project symbols"),("find_implementations","Find implementations using server capability negotiation")]:
    add(name,desc,"read",{"relative_path":s(),"name_path":s()},["relative_path","name_path"])
add("find_declaration","Find definition/declaration at a zero-based UTF-16 position","read",{"relative_path":s(),"position":POS,"declaration_only":b()},["relative_path","position"])
add("get_type_hierarchy","Query LSP type hierarchy if the server advertises support","read",{"relative_path":s(),"name_path":s(),"direction":s(enum=["super","sub"])},["relative_path","name_path","direction"])
add("get_diagnostics_for_file","Get pull/push diagnostics; pending or unversioned is not a clean-build proof","read",{"relative_path":s()},["relative_path"])
add("get_diagnostics_for_symbol","Filter file diagnostics to a symbol range","read",{"relative_path":s(),"name_path":s()},["relative_path","name_path"])
add("code_actions","List LSP code actions only; never auto-execute a server command","read",{"relative_path":s(),"range":RANGE},["relative_path","range"])
add("restart_language_server","Drop project LSP processes; lazily start clean instances on the next query","state")
add("transaction_prepare","Stage full-file changes guarded by expected SHA-256; null means create-only","write",{"changes":arr(CHANGE),"request_id":s()},["changes"])
for name,policy,desc in [("transaction_preview","read","Read a durable transaction preview"),("transaction_apply","write","Apply a staged transaction with revalidation and recovery journal"),("transaction_rollback","write","Undo a committed transaction using a new conflict-checked transaction")]:
    add(name,desc,policy,{"transaction_id":s()},["transaction_id"])
add("transaction_list","List journal phases","read")
add("transaction_recover","Restore interrupted transactions without overwriting external modifications","write")
add("create_text_file","Prepare a file create/overwrite; explicit transaction_apply is required","write",{"relative_path":s(),"content":s(),"expected_sha256":nullable_string(),"request_id":s()},["relative_path","content","expected_sha256"])
replace={"relative_path":s(),"needle":s(),"replacement":s(),"regex":b(),"expected_sha256":s(),"expected_matches":n(1,1,10000)}
add("replace_content","Prepare guarded literal/regex replacement, preserving full-file hash precondition","write",replace,["relative_path","needle","replacement","expected_sha256"])
add("replace_in_files","Prepare a single guarded cross-file replacement journal; expected_total_matches is mandatory","write",{"files":arr(obj({"relative_path":s(),"expected_sha256":s()},["relative_path","expected_sha256"])),"needle":s(),"replacement":s(),"regex":b(),"expected_total_matches":n(None,1,10000)},["files","needle","replacement","expected_total_matches"])
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
add("check_memories","Validate explicit mem: reference integrity","read")
add("search_memories","Search names/body by regex without returning every memory body","read",{"pattern":s()},["pattern"])
add("recover_memories","Recover an interrupted memory journal; shared across configured projects","write")
add("query_project","Call a read-only native tool against another configured project","read",{"target_project":s(),"tool":s(),"arguments":obj(additional=True)},["target_project","tool","arguments"])
add("flow_run","Run a bounded JSON dataflow; no eval, imports, arbitrary Python, shell, or external tool forwarding","read",{"session":s(),"steps":arr(obj({"as":s(),"tool":s(),"arguments":obj(additional=True),"select":{},"take":n(None,0,10000),"pointer":s()},["as"]),128),"return":{}},["session","steps"])
add("flow_vars","List retained per-project flow variable names and sizes","read",{"session":s()},["session"])
add("flow_clear","Clear this per-project flow session","state",{"session":s()},["session"])
add("execute_shell_command","Run a fixed operator-defined argv profile; no caller-supplied shell string","execute",{"profile":s()},["profile"])
add("debug_start","Start a configured DAP adapter; requires allow_debug","execute",{"profile":s()},["profile"])
add("debug_request","Send an allowlisted DAP request; launch/attach return asynchronous tickets","execute",{"command":s(),"arguments":obj(additional=True)},["command","arguments"])
add("debug_events","Drain DAP events and completed launch tickets","execute")
add("debug_stop","Disconnect debuggee with termination requested and drop the adapter","execute")
add("result_read","Read a spilled large result by byte chunks; offsets remain UTF-8 aligned","read",{"result_id":s(),"offset":n(0,0,16777216),"max_bytes":n(16384,256,32768)},["result_id"])
add("audit_log","Read redacted tool audit records without input/output source contents","read",{"limit":n(50,1,1000)})
(ROOT/'crates/odin-engine/tools.json').write_text(json.dumps(TOOLS,ensure_ascii=False,indent=2)+'\n', encoding='utf-8')
(ROOT/'schemas/tools.json').write_text(json.dumps(TOOLS,ensure_ascii=False,indent=2)+'\n', encoding='utf-8')
print(f'Generated {len(TOOLS)} tool contracts')
