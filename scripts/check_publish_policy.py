#!/usr/bin/env python3
"""Conservative checks for the small workflow syntax shipped in this project, not a YAML parser."""
from pathlib import Path
import json, re

root = Path(__file__).resolve().parents[1]
failures = []
for path in sorted((root/'.github/workflows').glob('*')):
    if path.suffix not in ('.yml', '.yaml'):
        continue
    text = path.read_text(encoding='utf-8')
    # Block automatic/public triggers even when a valid manual trigger also exists.
    trigger=re.search(r'^(?:on|[\'"]on[\'"]):\s*\n((?:[ \t]+.*(?:\n|$))*)',text,re.M)
    trigger_lines=[line for line in (trigger.group(1).splitlines() if trigger else []) if line.strip() and not line.lstrip().startswith('#')]
    if not trigger or not trigger_lines or any(line.strip()!='workflow_dispatch:' for line in trigger_lines):
        failures.append(f'{path.name}: only workflow_dispatch is allowed')
    runners = re.findall(r'^\s*runs-on:\s*(.+)$', text, re.M)
    if not runners or any('self-hosted' not in runner or '${{' in runner for runner in runners):
        failures.append(f'{path.name}: each explicit runner must include self-hosted')
    if re.search(r'\buses:\s*[^\s]+/.github/workflows/', text):
        failures.append(f'{path.name}: reusable workflows need separate manual review')
print(json.dumps({'scope':'WORKFLOW_SOURCE_POLICY_ONLY','status':'FAIL' if failures else 'PASS','errors':failures}))
raise SystemExit(1 if failures else 0)
