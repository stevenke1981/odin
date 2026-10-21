"""Run scoped engine IDE gates; log real commands and immutable source hashes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'verification'
OUTPUT = DIRECTORY / 'engine-ide-verification.json'
COMMANDS = [
    [sys.executable, str(DIRECTORY / 'engine-ide-catalog-check.py')],
    ['cargo', 'fmt', '-p', 'odin-engine', '--', '--check'],
    ['cargo', 'test', '-p', 'odin-engine', '--locked'],
    ['cargo', 'clippy', '-p', 'odin-engine', '--all-targets', '--locked', '--', '-D', 'warnings'],
]


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, encoding='utf-8').strip()


def hashes():
    paths = [*ROOT.glob('crates/odin-engine/src/**/*.rs'),
             *ROOT.glob('crates/odin-engine/tests/**/*.rs'),
             *ROOT.glob('crates/odin-engine/tests/fixtures/*.py'),
             ROOT / 'crates/odin-engine/tools.json', ROOT / 'schemas/tools.json',
             ROOT / 'scripts/generate_catalog.py', ROOT / 'scripts/generate_docs.py',
             DIRECTORY / 'engine-ide-verify.py', DIRECTORY / 'engine-ide-catalog-check.py',
             ROOT / 'docs/API.md', ROOT / 'docs/IDE_IMPLEMENTATION.md',
             ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
             ROOT / 'crates/odin-core/src/execution.rs', ROOT / 'crates/odin-core/src/config.rs',
             ROOT / 'crates/odin-protocol/src/lsp.rs', ROOT / 'crates/odin-protocol/src/dap.rs']
    return {str(path.relative_to(ROOT)).replace('\\', '/'): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(set(paths)) if path.is_file()}


def save(report):
    OUTPUT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')


def main():
    report = {'task_id': 'native-ide-engine', 'status': 'NOT_RUN', 'base_sha': '9b71abc98b89f72cce08d2795dfb139281f6d613',
              'head_before_evidence': git('rev-parse', 'HEAD'), 'branch': git('branch', '--show-current'),
              'platform': os.name, 'source_hashes': hashes(), 'commands': [],
              'boundaries': ['Protocol fixtures are not real language-support evidence.',
                             'Real OS sandbox/analyzer acceptance is owned by integration.',
                             'No cloud CI or remote push is performed by this runner.']}
    save(report)
    for index, command in enumerate(COMMANDS, 1):
        log = DIRECTORY / f'engine-ide-gate-{index}.txt'
        started = time.monotonic()
        with log.open('w', encoding='utf-8', newline='\n') as stream:
            stream.write('Command: ' + subprocess.list2cmdline(command) + '\n')
            stream.flush()
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                    encoding='utf-8', errors='replace', check=False)
            stream.write(f'\nExit code: {result.returncode}\n')
        item = {'command': command, 'exit_code': result.returncode,
                'status': 'PASS' if result.returncode == 0 else 'FAIL',
                'elapsed_seconds': round(time.monotonic() - started, 2),
                'log': log.name, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
        report['commands'].append(item)
        print(json.dumps(item), flush=True)
        save(report)
    report['source_hashes_after'] = hashes()
    report['source_unchanged_during_gates'] = report['source_hashes'] == report['source_hashes_after']
    report['status'] = 'PASS' if all(item['exit_code'] == 0 for item in report['commands']) and report['source_unchanged_during_gates'] else 'FAIL'
    save(report)
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
