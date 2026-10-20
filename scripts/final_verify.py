"""Run actual local gates and bind evidence to the current source snapshot."""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--real-backends', action='store_true')
    args = parser.parse_args()
    report_path = ROOT/'verification/local-validation.json'
    logs = ROOT/'verification/logs'
    logs.mkdir(parents=True, exist_ok=True)
    native_binary = 'target/release/odin.exe' if os.name == 'nt' else 'target/release/odin'
    commands = [
        ('generated_artifacts', [sys.executable,'scripts/check_generated.py']),
        ('fmt', ['cargo','fmt','--all','--','--check']),
        ('workspace_tests', ['cargo','test','--workspace','--locked']),
        ('clippy', ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
        ('release_build', ['cargo','build','--workspace','--release','--locked']),
        ('cli', [sys.executable,'scripts/cli_smoke.py','--binary',native_binary,'--output','verification/local-cli-smoke.json']),
        ('mcp_stdio', [sys.executable,'scripts/mcp_smoke.py','--binary',native_binary,'--output','verification/local-mcp-smoke.json']),
        ('mcp_http', [sys.executable,'scripts/http_smoke.py','--binary',native_binary,'--output','verification/local-http-smoke.json']),
        ('mcp_cancellation', [sys.executable,'scripts/mcp_cancel_smoke.py','--binary',native_binary,'--output','verification/local-mcp-cancellation.json']),
        ('dependency_audit', ['cargo','audit','--deny','warnings','--json']),
        ('licenses', ['cargo','deny','--format','json','check','licenses']),
        ('supply_chain', [sys.executable,'scripts/supply_chain.py']),
        ('static_source', [sys.executable,'scripts/check_source.py','--output','verification/local-source-check.json']),
        ('requirements', [sys.executable,'scripts/check_requirements.py']),
        ('publish_policy', [sys.executable,'scripts/check_publish_policy.py']),
    ]
    if args.real_backends:
        commands.insert(9, ('real_backends', [sys.executable,'tests/real_backends.py','--binary',native_binary,
            '--python','.odin/validation-tools/python/Scripts/python.exe',
            '--python-lsp','.odin/validation-tools/python/Scripts/basedpyright-langserver.exe',
            '--typescript-lsp','.odin/validation-tools/typescript/node_modules/typescript-language-server/lib/cli.mjs',
            '--node','C:/Program Files/nodejs/node.exe','--output','verification/real-backends.json']))
    tracked = subprocess.check_output(['git','ls-files','-z'], cwd=ROOT).decode().split('\0')
    source_paths = [name for name in tracked if name and (name.startswith(('crates/','scripts/','tests/','schemas/','.github/','dashboard/','examples/')) or name in {'Cargo.toml','Cargo.lock','rust-toolchain.toml','deny.toml','.gitattributes'})]
    hashes = {name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in source_paths if (ROOT/name).is_file() and '__pycache__' not in name}
    report = {'status':'NOT_RUN', 'platform':platform.platform(), 'target':subprocess.check_output(['rustc','-Vv'], cwd=ROOT, text=True),
              'source_head_sha':subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip(),
              'source_sha256':hashes, 'source_snapshot_note':'Hashes bind actual tested working files; later documentation/evidence commits may have a different Git SHA.',
              'started_at_utc':datetime.datetime.now(datetime.UTC).isoformat(),
              'commands':[{'name':name,'argv':[arg if arg != sys.executable else 'python' for arg in command], 'cwd':'.', 'status':'NOT_RUN','exit_code':None} for name,command in commands]}
    def save():
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')
    save()
    environment = dict(os.environ, ODIN_PYTHON=sys.executable)
    for entry, (name, command) in zip(report['commands'], commands):
        start = time.monotonic()
        try:
            process = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, timeout=1800)
            output = (process.stdout+process.stderr).decode('utf-8', errors='replace')
            code = process.returncode
        except subprocess.TimeoutExpired as error:
            output = ((error.stdout or b'')+(error.stderr or b'')).decode('utf-8', errors='replace')+'\nVerifier command deadline exceeded.'
            code = -1
        for path, label in [(str(ROOT), '<WORKSPACE>'), (str(Path.home()), '<USERPROFILE>')]:
            output = output.replace(path, label).replace(path.replace('\\','/'), label)
        log = f'verification/logs/{name}.txt'
        (ROOT/log).write_text(output, encoding='utf-8', newline='\n')
        entry.update(status='PASS' if code == 0 else 'FAIL', exit_code=code, elapsed_seconds=round(time.monotonic()-start,3), log=log,
                     log_sha256=hashlib.sha256((ROOT/log).read_bytes()).hexdigest())
        print(f'{name}: {entry["status"]} (exit {code})', flush=True)
        if name == 'workspace_tests' and code == 0:
            report['rust_tests_passed'] = sum(int(number) for number in re.findall(r'test result: ok\. (\d+) passed', output))
            report['rust_tests_ignored'] = sum(int(number) for number in re.findall(r'test result: ok\. \d+ passed; \d+ failed; (\d+) ignored', output))
        if name == 'release_build' and code == 0:
            report['binary_sha256'] = hashlib.sha256((ROOT/native_binary).read_bytes()).hexdigest()
            report['binary_metadata'] = json.loads(subprocess.check_output([str(ROOT/native_binary),'metadata'], cwd=ROOT))
        if name == 'dependency_audit' and code == 0:
            (ROOT/'verification/cargo-audit.json').write_bytes(process.stdout)
        save()
        if code:
            report['status'] = 'FAIL'
            save()
            print(output[-4500:])
            return 1
    changed = [name for name, digest in hashes.items() if hashlib.sha256((ROOT/name).read_bytes()).hexdigest() != digest]
    report['source_changed_during_validation'] = changed
    report['status'] = 'FAIL' if changed else 'PASS'
    report['completed_at_utc'] = datetime.datetime.now(datetime.UTC).isoformat()
    save()
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
