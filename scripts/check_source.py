#!/usr/bin/env python3
"""Static artifact/contract checks; explicitly NOT a Rust parser or compiler."""
from pathlib import Path
import argparse, ast, hashlib, json, re, shutil, subprocess, sys, tempfile, tomllib
ROOT=Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,default=ROOT/'verification/source-check.json');args=parser.parse_args()
    checks=[]
    def record(name,ok,detail=''):
        checks.append({'name':name,'status':'PASS' if ok else 'FAIL','detail':detail})
    files=[p for p in ROOT.rglob('*') if p.is_file() and not any(v in p.relative_to(ROOT).parts for v in ['target','dist','.git','.odin','__pycache__'])]
    for p in files:
        if p.suffix in ['.rs','.py','.json','.toml','.md','.html','.sh','.ps1','.yml']:
            try:p.read_text(encoding='utf-8');record('utf8:'+str(p.relative_to(ROOT)),True)
            except UnicodeError as error:record('utf8:'+str(p.relative_to(ROOT)),False,str(error))
    for p in (p for p in files if p.suffix=='.toml'):
        try:tomllib.loads(p.read_text(encoding='utf-8'));record('toml:'+str(p.relative_to(ROOT)),True)
        except Exception as error:record('toml:'+str(p.relative_to(ROOT)),False,str(error))
    for p in (p for p in files if p.suffix=='.json'):
        if 'verification' in p.parts:continue
        try:json.loads(p.read_text(encoding='utf-8'));record('json:'+str(p.relative_to(ROOT)),True)
        except Exception as error:record('json:'+str(p.relative_to(ROOT)),False,str(error))
    for p in (p for p in files if p.suffix=='.py'):
        try:ast.parse(p.read_text(encoding='utf-8'),filename=str(p));record('python_ast:'+str(p.relative_to(ROOT)),True)
        except Exception as error:record('python_ast:'+str(p.relative_to(ROOT)),False,str(error))
    workspace=tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))
    for member in workspace['workspace']['members']:
        p=ROOT/member/'Cargo.toml';record('workspace_member:'+member,p.exists())
        if not p.exists():continue
        data=tomllib.loads(p.read_text(encoding='utf-8'))
        for group in ['dependencies','dev-dependencies','build-dependencies']:
            for name,dep in data.get(group,{}).items():
                if isinstance(dep,dict) and 'path' in dep:
                    record('path_dependency:'+member+':'+name,(p.parent/dep['path']/'Cargo.toml').exists())
    tools=json.loads((ROOT/'schemas/tools.json').read_text(encoding='utf-8'))
    mirror=json.loads((ROOT/'crates/odin-engine/tools.json').read_text(encoding='utf-8'))
    record('catalog_mirror',tools==mirror)
    names=[t['name'] for t in tools];record('unique_nonempty_contracts',bool(names) and len(names)==len(set(names)))
    engine=(ROOT/'crates/odin-engine/src/lib.rs').read_text(encoding='utf-8')
    record('no_bridge_tools',not any(name.startswith('serena_') for name in names))
    record('no_bridge_policy',all(t['policy'] in ['read','state','write','execute'] for t in tools))
    record('no_bridge_module',not (ROOT/'crates/odin-protocol/src/bridge.rs').exists())
    record('no_bridge_examples',not (ROOT/'examples/odin.bridge.toml').exists())
    for source in (ROOT/'crates').glob('*/src/*.rs'):
        # Config tests using retired identifiers live under tests/, not src/.
        text=source.read_text(encoding='utf-8')
        record('standalone_production_source:'+str(source.relative_to(ROOT)),
               not re.search(r'serena|BridgeConfig|SerenaBridge',text,re.I))
    for source in (ROOT/'crates').glob('*/Cargo.toml'):
        record('no_upstream_cargo_dependency:'+str(source.relative_to(ROOT)),
               not re.search(r'serena|solidlsp',source.read_text(encoding='utf-8'),re.I))

    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        Draft202012Validator=None
        checks.append({'name':'json_schema_validation','status':'NOT_RUN','detail':'Install jsonschema to run optional contract-schema validation'})
    for tool in tools:
        name=tool['name'];schema=tool['inputSchema']
        record('scoped_contract:'+name,'project' in schema.get('required',[]))
        record('dispatcher_marker:'+name,bool(re.search(r'"'+re.escape(name)+r'"\s*(?:\||=>)',engine)))
        if Draft202012Validator:
            try:Draft202012Validator.check_schema(schema);record('schema_valid:'+name,True)
            except Exception as error:record('schema_valid:'+name,False,str(error))
    if Draft202012Validator:
        cases={'flow-files.json':'flow_run','flow-symbols.json':'flow_run','prepare-create.json':'transaction_prepare','write-memory.json':'write_memory'}
        catalog={t['name']:t for t in tools}
        for filename,name in cases.items():
            try:Draft202012Validator(catalog[name]['inputSchema']).validate(json.loads((ROOT/'examples'/filename).read_text(encoding='utf-8')));record('example_contract:'+filename,True)
            except Exception as error:record('example_contract:'+filename,False,str(error))
    rust=[p for p in files if p.suffix=='.rs' and 'crates' in p.relative_to(ROOT).parts]
    for path in rust:
        text=path.read_text(encoding='utf-8')
        record('no_stub_macro:'+str(path.relative_to(ROOT)),not re.search(r'\b(?:todo|unimplemented)!\s*\(',text))
        for rel in re.findall(r'include_str!\("([^"\n]+)"\)',text):
            record('included_file:'+str(path.relative_to(ROOT))+':'+rel,(path.parent/rel).is_file())
    html=(ROOT/'dashboard/index.html').read_text(encoding='utf-8')
    record('dashboard_no_remote_scripts',not re.search(r'<script[^>]+src\s*=\s*[\'\"]https?://',html,re.I))
    record('dashboard_no_localstorage',not re.search(r'\b(?:localStorage|sessionStorage)\s*(?:\.|\[)',html))
    node=shutil.which('node')
    if node:
        with tempfile.TemporaryDirectory() as directory:
            for index,code in enumerate(re.findall(r'<script\b[^>]*>(.*?)</script>',html,re.S|re.I)):
                path=Path(directory)/f'inline-{index}.js';path.write_text(code, encoding='utf-8')
                run=subprocess.run([node,'--check',str(path)],capture_output=True,text=True,timeout=15)
                record('dashboard_js_syntax:'+str(index),run.returncode==0,run.stderr.strip())
    else:checks.append({'name':'dashboard_js_syntax','status':'NOT_RUN','detail':'Node unavailable'})
    # Source-only inventory: counting declarations is not executing those tests.
    tests=sum(len(re.findall(r'#\[(?:tokio::)?test\]',p.read_text(encoding='utf-8'))) for p in rust)
    report={'scope':'ARTIFACT_AND_CONTRACT_STATIC_CHECKS_ONLY','status':'FAIL' if any(c['status']=='FAIL' for c in checks) else 'PASS',
            'checks':checks,'rust_files':len(rust),'rust_lines':sum(len(p.read_text(encoding='utf-8').splitlines()) for p in rust),
            'rust_test_declarations':tests,'rust_tests_executed_by_this_check':0,'rust_compilation':'NOT_PERFORMED_BY_STATIC_CHECK','tool_contracts':len(tools),
            'compiler_presence':{name:shutil.which(name) for name in ['rustc','cargo','rustfmt']},
            'not_proven':['Rust syntax/types/linking','real backend compatibility','security correctness','runtime performance']}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n', encoding='utf-8')
    print(json.dumps({k:report[k] for k in ['scope','status','rust_files','rust_lines','rust_test_declarations','rust_compilation']}))
    print('Checks:',len(checks),'Fail:',sum(c['status']=='FAIL' for c in checks))
    for c in checks:
        if c['status']=='FAIL':print(c)
    return 0 if report['status']=='PASS' else 1
if __name__=='__main__':raise SystemExit(main())
