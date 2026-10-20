"""Execute both generators and require all derived artifacts to remain byte-identical."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = ['schemas/tools.json', 'crates/odin-engine/tools.json', 'docs/API.md',
         'docs/SERENA_PARITY.md', 'docs/serena-parity.csv', 'docs/serena-parity.json', 'docs/DEPENDENCIES.md']


def main():
    before = {name: hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in FILES}
    for script in ['scripts/generate_catalog.py', 'scripts/generate_docs.py']:
        subprocess.run([sys.executable, script], cwd=ROOT, check=True)
    after = {name: hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in FILES}
    changed = [name for name in FILES if before[name] != after[name]]
    print(json.dumps({'scope': 'EXECUTED_GENERATOR_IDEMPOTENCE', 'status': 'FAIL' if changed else 'PASS',
                      'changed': changed, 'sha256': after}, indent=2))
    return 1 if changed else 0


if __name__ == '__main__':
    raise SystemExit(main())
