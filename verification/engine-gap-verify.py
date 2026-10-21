"""Scoped real gates for explicit formatter and LSP coordinate contracts."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'verification'
RUN_ID = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())
OUTPUT = DIRECTORY / f'engine-gap-{RUN_ID}-verification.json'
COMMANDS = [
    [sys.executable, str(DIRECTORY / 'engine-ide-catalog-check.py')],
    ['cargo', 'fmt', '-p', 'odin-engine', '--', '--check'],
    ['cargo', 'test', '-p', 'odin-engine', '--test', 'formatter_profiles', '--locked'],
    ['cargo', 'test', '-p', 'odin-engine', '--test', 'ide_tools',
     'lsp_coordinates_over_one_hundred_thousand', '--locked'],
    ['cargo', 'test', '-p', 'odin-engine', '--locked'],
    ['cargo', 'clippy', '-p', 'odin-engine', '--all-targets', '--locked', '--', '-D', 'warnings'],
]


def hashes():
    paths = [*ROOT.glob('crates/odin-engine/src/**/*.rs'), *ROOT.glob('crates/odin-engine/tests/**/*.rs'),
             *ROOT.glob('crates/odin-engine/tests/fixtures/*.py'),
             *[ROOT / path for path in ['crates/odin-engine/tools.json', 'schemas/tools.json',
                                       'scripts/generate_catalog.py', 'scripts/generate_docs.py',
                                       'docs/API.md', 'docs/IDE_IMPLEMENTATION.md', 'Cargo.lock',
                                       'crates/odin-core/src/config.rs', 'crates/odin-core/src/execution.rs',
                                       'verification/engine-gap-verify.py']]]
    return {path.relative_to(ROOT).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(set(paths)) if path.is_file()}


def main():
    report = {'task_id': 'language-ide-gap-completion', 'actor_instance': '/root/engine_implementation',
              'base_sha': '9017908f8759de74d5c3deb89e7bdb8aedcf83fd',
              'head_before_evidence': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, encoding='utf-8').strip(),
              'status': 'NOT_RUN', 'source_hashes': hashes(), 'commands': [],
              'scope': 'Engine package on native Windows; no workspace gate, cloud CI or downloads.',
              'boundaries': ['Synthetic formatter outcomes validate output/CAS handling, not actual Ruff or OS isolation.',
                             'Native Ruff MCP/sandbox and multi-language matrices are integration-owner acceptance.',
                             'Unversioned diagnostics and unsupported resolve capabilities retain their existing truthful status.']}
    for index, command in enumerate(COMMANDS, 1):
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        log = (f'Command: {subprocess.list2cmdline(command)}\n').encode() + result.stdout.replace(b'\r\n', b'\n') + f'\nExit code: {result.returncode}\n'.encode()
        filename = f'engine-gap-{RUN_ID}-gate-{index}.txt'
        (DIRECTORY / filename).write_bytes(log)
        status = 'PASS' if result.returncode == 0 else 'BLOCKED' if b'4551' in log else 'FAIL'
        item = {'command': command, 'status': status, 'exit_code': result.returncode,
                'elapsed_seconds': round(time.monotonic() - started, 2), 'log': filename,
                'log_sha256': hashlib.sha256(log).hexdigest()}
        if status == 'BLOCKED':
            item['blocked_executables'] = [
                {'path': path.relative_to(ROOT).as_posix(),
                 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                for path in sorted((ROOT / 'target' / 'debug' / 'deps').glob('*.exe'))
                if str(path).encode() in result.stdout]
        report['commands'].append(item)
        print(json.dumps(item), flush=True)
        OUTPUT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
    report['source_hashes_after'] = hashes()
    report['source_unchanged_during_gates'] = report['source_hashes'] == report['source_hashes_after']
    statuses = {item['status'] for item in report['commands']}
    report['status'] = 'PASS' if statuses == {'PASS'} and report['source_unchanged_during_gates'] else 'BLOCKED' if 'BLOCKED' in statuses else 'FAIL'
    OUTPUT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
