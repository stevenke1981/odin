#!/usr/bin/env python3
"""Verify Odin's spec traceability; this is a documentation check, not a Rust test."""
from pathlib import Path
import json,sys
root=Path(__file__).resolve().parents[1]
f=json.loads((root/'requirements/features.json').read_text(encoding='utf-8'))
c=json.loads((root/'requirements/serena_coverage.json').read_text(encoding='utf-8'))
par=json.loads((root/'docs/serena-parity.json').read_text(encoding='utf-8'))
errors=[]
items=f['features'];ids={x['id'] for x in items};names=[x['id'] for x in items]
if len(ids)!=40 or len(ids)!=len(names):errors.append('expected 40 unique requirements')
for it in items:
    if not it['depends_on'] or len(it['acceptance'])<3 or not it['tests']:
        if len(it['acceptance'])<3 or not it['tests']:errors.append(it['id']+': missing acceptance/tests')
    for ref in it['depends_on']:
        if ref not in ids:errors.append(it['id']+': invalid dependency '+ref)
    if it['priority'] not in ('P0','P1','P2'):errors.append(it['id']+': invalid priority')
    if it['current'] not in ('SOURCE_PREVIEW','SOURCE_STATIC_ONLY','NATIVE_IMPLEMENTED','NATIVE_SOURCE_PARTIAL','NOT_VERIFIED','NOT_IMPLEMENTED'):errors.append(it['id']+': invalid status')
    evidence=it.get('evidence','')
    if ';' in evidence and evidence!='NOT_RUN; requires runtime verification':
        claimed,reference=(part.strip() for part in evidence.split(';',1))
        if claimed not in ('PASS','PARTIAL','FAIL','BLOCKED','NOT_RUN'):errors.append(it['id']+': invalid evidence status')
        location,_,anchor=reference.partition('#')
        path=(root/location).resolve()
        if not path.is_relative_to((root/'verification').resolve()) or not path.is_file():
            errors.append(it['id']+': missing or out-of-scope evidence report')
        else:
            try:
                document=json.loads(path.read_text(encoding='utf-8'))
                match=next((entry for entry in document.get('requirements',[]) if entry.get('id')==(anchor or it['id'])),None)
                if match is None or match.get('id')!=it['id'] or match.get('status')!=claimed:errors.append(it['id']+': evidence ID/status mismatch')
                elif claimed=='PASS':
                    checks=match.get('checks',[])
                    if not checks or match.get('unverified_acceptance') or any(check.get('status',check.get('result'))!='PASS' or check.get('exit_code')!=0 for check in checks):errors.append(it['id']+': PASS requires executed zero-exit checks and all acceptance verified')
            except (ValueError,TypeError,OSError) as error:errors.append(it['id']+': unreadable evidence: '+str(error))
    elif evidence.startswith('PASS'):errors.append(it['id']+': PASS requires a versioned verification report reference')
actual={x['serena_tool'] for x in par['rows']};got={x['serena_reference'] for x in c['rows']}
if actual!=got or len(c['rows'])!=53:errors.append('Serena reference coverage does not match 53 original tools')
for x in c['rows']:
    if not x['odin_spec_ids'] or set(x['odin_spec_ids'])-ids:errors.append('unmapped tool: '+x['serena_reference'])
    if x['done']:
        for spec_id in x['odin_spec_ids']:
            item=next((item for item in items if item['id']==spec_id),{})
            if not item.get('evidence','').startswith('PASS;'):errors.append('parity marked done without full acceptance evidence: '+x['serena_reference'])
for n in ['docs/FEATURE_SPEC.md','docs/SERENA_COVERAGE_PLAN.md','docs/IMPLEMENTATION_BACKLOG.md','docs/specs/CORE_PROTOCOL.md','docs/specs/REFACTOR_TRANSACTION.md','docs/specs/LANGUAGE_DEBUG_INSPECTIONS.md','docs/specs/OPERATIONS_RELEASE.md']:
    if not (root/n).is_file():errors.append('missing doc: '+n)
report={'kind':'DOCUMENTATION_TRACEABILITY_ONLY','features':len(items),'serena_reference_tools':len(c['rows']),'runtime_checks_executed_by_this_script':False,'errors':errors,'status':'FAIL' if errors else 'PASS'}
print(json.dumps(report,ensure_ascii=False,indent=2))
sys.exit(1 if errors else 0)
