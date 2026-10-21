"""Verify both catalog mirrors and reproducible generated API documentation."""
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
paths = [root / 'crates/odin-engine/tools.json', root / 'schemas/tools.json', root / 'docs/API.md']
before = [path.read_bytes() for path in paths]
for _ in range(2):
    for script, arguments in [('generate_catalog.py', []), ('generate_docs.py', ['--api-only'])]:
        subprocess.run([sys.executable, str(root / 'scripts' / script), *arguments], cwd=root, check=True)
    assert before == [path.read_bytes() for path in paths], 'Generated artifacts are stale or nondeterministic'
assert before[0] == before[1], 'Catalog copies differ'
catalog = json.loads(before[0])
names = [entry['name'] for entry in catalog]
assert len(names) == len(set(names))
for name in ['completion', 'completion_resolve', 'hover', 'signature_help', 'workspace_symbols', 'call_hierarchy',
             'format_document', 'inlay_hints', 'inlay_hint_resolve', 'semantic_tokens',
             'list_inspection_providers', 'run_inspections', 'prepare_inspection_fix']:
    assert name in names, f'Missing IDE contract: {name}'
print(json.dumps({'tool_count': len(names), 'catalog_mirrors': 'PASS', 'generator_idempotence': 'PASS'}))
