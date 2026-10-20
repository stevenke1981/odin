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
    if it['current'] not in ('SOURCE_PREVIEW','SOURCE_STATIC_ONLY','NATIVE_SOURCE_PARTIAL','NOT_VERIFIED','NOT_IMPLEMENTED'):errors.append(it['id']+': invalid status')
    if it['evidence'].startswith('PASS'):errors.append(it['id']+': unsupported evidence')
actual={x['serena_tool'] for x in par['rows']};got={x['serena_reference'] for x in c['rows']}
if actual!=got or len(c['rows'])!=53:errors.append('Serena reference coverage does not match 53 original tools')
for x in c['rows']:
    if not x['odin_spec_ids'] or set(x['odin_spec_ids'])-ids:errors.append('unmapped tool: '+x['serena_reference'])
    if x['done']:errors.append('unverified parity claim: '+x['serena_reference'])
for n in ['docs/FEATURE_SPEC.md','docs/SERENA_COVERAGE_PLAN.md','docs/IMPLEMENTATION_BACKLOG.md','docs/specs/CORE_PROTOCOL.md','docs/specs/REFACTOR_TRANSACTION.md','docs/specs/LANGUAGE_DEBUG_INSPECTIONS.md','docs/specs/OPERATIONS_RELEASE.md']:
    if not (root/n).is_file():errors.append('missing doc: '+n)
report={'kind':'DOCUMENTATION_TRACEABILITY_ONLY','features':len(items),'serena_reference_tools':len(c['rows']),'verified_runtime':False,'errors':errors,'status':'FAIL' if errors else 'PASS'}
print(json.dumps(report,ensure_ascii=False,indent=2))
sys.exit(1 if errors else 0)
