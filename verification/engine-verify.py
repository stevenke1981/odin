#!/usr/bin/env python3
"""Execute native engine gates and preserve actual command outputs and hashes."""
import datetime
import hashlib
import json
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'verification/engine-verification.json'
COMMANDS = [
    ['cargo', 'fmt', '-p', 'odin-engine', '--', '--check'],
    ['cargo', 'test', '-p', 'odin-engine', '--locked'],
    ['cargo', 'clippy', '-p', 'odin-engine', '--all-targets', '--locked', '--no-deps', '--', '-D', 'warnings'],
    ['cargo', 'clippy', '-p', 'odin-engine', '--all-targets', '--locked', '--', '-D', 'warnings'],
]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(command):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding='utf-8', errors='replace')
    output = (result.stdout + result.stderr).replace(str(ROOT), '<WORKTREE>').replace(str(ROOT).replace('\\', '/'), '<WORKTREE>')
    return {'command': command, 'cwd': '.', 'exit_code': result.returncode,
            'status': 'PASS' if result.returncode == 0 else 'FAIL', 'output': output}

def main():
    files = [path for path in (ROOT / 'crates/odin-engine').rglob('*') if path.is_file() and '__pycache__' not in path.parts]
    files += [ROOT / path for path in ['Cargo.lock', 'scripts/generate_catalog.py', 'scripts/generate_docs.py', 'schemas/tools.json', 'docs/API.md', 'docs/ENGINE_IMPLEMENTATION.md']]
    before = {str(path.relative_to(ROOT)).replace('\\', '/'): digest(path) for path in files}
    report = {'platform': platform.platform(), 'native_target': run(['rustc', '-Vv']),
              'head_sha_before_uncommitted_change': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'source_sha256': before, 'source_snapshot_note': 'Hashes bind the actual working files; test evidence is separate from the later commit.',
              'commands': [run(command) for command in COMMANDS],
              'backend_evidence': {'lsp_dap_completion_tests': 'PROTOCOL_FIXTURES_ONLY', 'real_language_refactor': 'NOT_RUN', 'real_debuggee': 'NOT_RUN', 'linux_native': 'NOT_RUN', 'macos_native': 'NOT_RUN'}}
    generated = [run(['python', 'scripts/generate_catalog.py']), run(['python', 'scripts/generate_docs.py', '--api-only'])]
    report['generator_commands'] = generated
    report['generator_idempotence'] = 'PASS' if all(path.exists() and digest(path) == before[str(path.relative_to(ROOT)).replace('\\', '/')] for path in files) and all(item['exit_code'] == 0 for item in generated) else 'FAIL'
    report['executed_at_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    OUTPUT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    for command in report['commands']:
        print(command['status'], ' '.join(command['command']), 'exit', command['exit_code'])
    print('Generator idempotence', report['generator_idempotence'])
    return int(any(command['exit_code'] != 0 for command in report['commands']) or report['generator_idempotence'] != 'PASS')

if __name__ == '__main__':
    sys.exit(main())
