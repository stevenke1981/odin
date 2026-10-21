"""Execute independent Windows checks after a fail-fast workspace gate stops.

This does not replace that failed gate. It preserves every nonzero result and
uses an already-built, hash-selected candidate without rebuilding it.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--expected-binary-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--logs', type=Path, required=True)
    parser.add_argument('--artifact-prefix', default='language-installers-v3-continued',
                        help='Fresh filename prefix under verification; previous reports are never overwritten')
    args = parser.parse_args()
    if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', args.artifact_prefix) or len(args.artifact_prefix) > 96:
        parser.error('--artifact-prefix must be a lowercase filename prefix of at most 96 characters')
    args.output = args.output.resolve()
    args.logs = args.logs.resolve()
    args.output.relative_to(ROOT)
    args.logs.relative_to(ROOT)
    binary = args.binary.resolve(strict=True)
    if os.name != 'nt' or digest(binary) != args.expected_binary_sha256.lower():
        raise ValueError('Select the actual Windows candidate by SHA-256')
    executor = Path('C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe')
    executor_sha = '8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259'
    prefix = 'verification/' + args.artifact_prefix + '-'
    artifact = lambda name: prefix + name + '.json'
    commands = [
        ('engine_protocol_units', ['cargo', 'test', '-p', 'odin-engine', '-p', 'odin-protocol', '--lib', '--locked']),
        ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']),
        ('cli', [sys.executable, 'scripts/cli_smoke.py', '--binary', str(binary), '--output', artifact('cli')]),
        ('mcp_stdio', [sys.executable, 'scripts/mcp_smoke.py', '--binary', str(binary), '--output', artifact('mcp-stdio')]),
        ('mcp_http', [sys.executable, 'scripts/http_smoke.py', '--binary', str(binary), '--output', artifact('http')]),
        ('mcp_cancel', [sys.executable, 'scripts/mcp_cancel_smoke.py', '--binary', str(binary), '--output', artifact('cancellation')]),
        ('real_backends', [sys.executable, 'tests/real_backends.py', '--binary', str(binary),
                           '--python', '.odin/validation-tools/python/Scripts/python.exe',
                           '--python-lsp', '.odin/validation-tools/python/Scripts/basedpyright-langserver.exe',
                           '--typescript-lsp', '.odin/validation-tools/typescript/node_modules/typescript-language-server/lib/cli.mjs',
                           '--node', 'C:/Program Files/nodejs/node.exe', '--output', artifact('real-backends')]),
        ('native_inspections', [sys.executable, 'tests/real_native_inspections.py', '--binary', str(binary),
                                '--executor', str(executor), '--executor-sha256', executor_sha,
                                '--tools-root', '.odin/validation-tools', '--node', 'C:/Program Files/nodejs/node.exe',
                                '--output', artifact('native-inspections')]),
        ('dependency_audit', ['cargo', 'audit', '--deny', 'warnings', '--json']),
        ('licenses', ['cargo', 'deny', '--format', 'json', 'check', 'licenses']),
        ('static_source', [sys.executable, 'scripts/check_source.py', '--output', artifact('source')]),
        ('requirements', [sys.executable, 'scripts/check_requirements.py']),
        ('publish_policy', [sys.executable, 'scripts/check_publish_policy.py']),
    ]
    artifact_paths = [ROOT/command[command.index('--output')+1]
                      for _, command in commands if '--output' in command]
    if args.output in artifact_paths or len(set(artifact_paths)) != len(artifact_paths):
        raise ValueError('Each evidence report must have a distinct destination')
    if any(path.exists() for path in [args.output, args.logs, *artifact_paths]):
        raise FileExistsError('Use fresh evidence paths and artifact prefix; previous attempts are immutable')
    if digest(executor) != executor_sha:
        raise ValueError('Native executor differs from the explicitly selected pin')
    # Preflight every child report before starting a native process or creating logs.
    metadata = json.loads(subprocess.check_output([str(binary), 'metadata'], cwd=ROOT))
    args.logs.mkdir(parents=True)
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    sources = [name for name in tracked if name.startswith(('crates/', 'scripts/', 'tests/', 'schemas/')) or name in ('Cargo.lock', 'Cargo.toml')]
    sources.append(Path(__file__).resolve().relative_to(ROOT).as_posix())
    source_hashes = {name: digest(ROOT/name) for name in sorted(set(sources))}
    report = {
        'scope': 'INDEPENDENT_WINDOWS_CONTINUATION_NOT_A_WORKSPACE_GATE_REPLACEMENT',
        'status': 'NOT_RUN', 'binary_sha256': digest(binary), 'binary_metadata': metadata,
        'artifact_prefix': args.artifact_prefix,
        'source_head_sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_sha256': source_hashes,
        'failed_workspace_gate': 'verification/language-installers-v3-validation-01.json',
        'native_boundary_attempt': 'verification/language-installers-v3-enumeration-native-01.json',
        'started_utc': datetime.datetime.now(datetime.UTC).isoformat(),
        'commands': [{'name': name, 'argv': command, 'status': 'NOT_RUN', 'exit_code': None} for name, command in commands],
    }

    def save():
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')

    save()
    environment = dict(os.environ, ODIN_PYTHON=sys.executable)
    for entry, (name, command) in zip(report['commands'], commands):
        started = time.monotonic()
        try:
            completed = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, timeout=1800)
            raw = completed.stdout + completed.stderr
            code = completed.returncode
        except subprocess.TimeoutExpired as error:
            raw = (error.stdout or b'')+(error.stderr or b'')+b'\nCommand deadline exceeded.\n'
            code = -1
        output = raw.decode('utf-8', errors='replace').replace('\r\n', '\n')
        for path, replacement in [(str(ROOT), '<WORKSPACE>'), (str(Path.home()), '<USERPROFILE>')]:
            output = output.replace(path, replacement).replace(path.replace('\\', '/'), replacement)
        log = args.logs/(name+'.txt')
        log.write_text(output.rstrip()+'\n', encoding='utf-8', newline='\n')
        entry.update(status='PASS' if code == 0 else 'FAIL', exit_code=code,
                     log=log.resolve().relative_to(ROOT).as_posix(), log_sha256=digest(log),
                     elapsed_seconds=round(time.monotonic()-started, 3))
        if '4551' in output and code != 0:
            entry['failure_class'] = 'HOST_APPLICATION_CONTROL_BLOCK'
        if '--output' in command:
            result = ROOT/command[command.index('--output')+1]
            if result.is_file():
                result.write_bytes(result.read_bytes().replace(b'\r\n', b'\n'))
                entry.update(report=result.relative_to(ROOT).as_posix(), report_sha256=digest(result))
        print(f'{name}: {entry["status"]} (exit {code})', flush=True)
        save()
    report['source_changed_during_validation'] = [name for name, sha in source_hashes.items() if digest(ROOT/name) != sha]
    report['status'] = 'PASS' if all(row['status'] == 'PASS' for row in report['commands']) and not report['source_changed_during_validation'] else 'FAIL'
    report['completed_utc'] = datetime.datetime.now(datetime.UTC).isoformat()
    save()
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
