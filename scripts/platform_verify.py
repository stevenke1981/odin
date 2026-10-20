#!/usr/bin/env python3
"""Record actual platform-scoped commands, outputs and source hashes. Root runs full workspace gates separately."""
import argparse, datetime, hashlib, json, platform, subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=ROOT/'verification/platform-verification.json')
    args = parser.parse_args()
    binary = 'target/debug/odin.exe' if platform.system() == 'Windows' else 'target/debug/odin'
    commands = [
        ['cargo', 'fmt', '-p', 'odin-agent', '--', '--check'],
        ['cargo', 'check', '-p', 'odin-agent', '--locked'],
        ['cargo', 'test', '-p', 'odin-agent', '--locked'],
        ['cargo', 'clippy', '-p', 'odin-agent', '--all-targets', '--locked', '--no-deps', '--', '-D', 'warnings'],
        ['cargo', 'build', '-p', 'odin-agent', '--locked'],
        ['python', 'scripts/check_source.py', '--output', 'verification/platform-source-check.json'],
        ['python', 'scripts/check_requirements.py'],
        ['python', 'scripts/check_publish_policy.py'],
        ['python', 'scripts/cli_smoke.py', '--binary', binary, '--output', 'verification/platform-cli-smoke.json'],
        ['python', 'scripts/mcp_smoke.py', '--binary', binary, '--output', 'verification/platform-mcp-smoke.json'],
        ['python', 'scripts/http_smoke.py', '--binary', binary, '--output', 'verification/platform-http-smoke.json'],
    ]
    sources = sorted(path for folder in ['crates/odin', 'dashboard', 'scripts', 'packaging'] for path in (ROOT/folder).rglob('*') if path.is_file() and '__pycache__' not in path.parts)
    sources += [ROOT/'Cargo.lock', ROOT/'schemas/tools.json', ROOT/'docs/PLATFORM_IMPLEMENTATION.md']
    hashes = {path.relative_to(ROOT).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
    report = {'task_id':'ODIN-PLATFORM-COMPLETION', 'actor_instance':'platform_implementation', 'base_sha':'2ed95d6',
              'source_head_sha':subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip(),
              'source_sha256':hashes, 'source_snapshot_note':'Working source hashes, not later commit SHA, bind this execution.',
              'platform':platform.platform(), 'executed_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'commands':[],
              'full_workspace_dependency_clippy':'NOT_RUN_BY_THIS_SCOPED_SCRIPT', 'linux_macos_runtime':'NOT_RUN', 'actual_client_applications':'NOT_RUN'}
    for command in commands:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, encoding='utf-8', errors='replace')
        output = (result.stdout+result.stderr).replace(str(ROOT), '<WORKTREE>').replace(str(ROOT).replace('\\','/'), '<WORKTREE>')
        check = {'command':command, 'cwd':'.', 'exit_code':result.returncode, 'status':'PASS' if result.returncode == 0 else 'FAIL', 'output':output}
        report['commands'].append(check)
        print(check['status'], ' '.join(command), 'exit', result.returncode, flush=True)
    report['source_unchanged_during_commands'] = all(path.exists() and hashlib.sha256(path.read_bytes()).hexdigest() == hashes[path.relative_to(ROOT).as_posix()] for path in sources)
    report['status'] = 'PASS' if report['source_unchanged_during_commands'] and all(item['exit_code'] == 0 for item in report['commands']) else 'FAIL'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
