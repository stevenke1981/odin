"""Explicit QA-only per-project basedpyright RW copies; never reuse as trusted SDK."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
TOOLS = Path('E:/odin/.odin/validation-tools')
ORIGIN = TOOLS / 'python/Lib/site-packages/basedpyright'
NODE = TOOLS / 'python/Lib/site-packages/nodejs_wheel/node.exe'
PYTHON = Path('C:/Python314/python.exe')
EXECUTOR = Path('C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe')
EXECUTOR_SHA = '8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259'


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8', newline='\n')


def run(command, cwd):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, capture_output=True, timeout=35)
    entry = {'command': [str(arg) for arg in command], 'exit_code': result.returncode,
             'elapsed_seconds': round(time.monotonic() - started, 3),
             'stdout': result.stdout.decode('utf-8', errors='replace'),
             'stderr': result.stderr.decode('utf-8', errors='replace'),
             'stdout_raw_sha256': hashlib.sha256(result.stdout).hexdigest(),
             'stderr_raw_sha256': hashlib.sha256(result.stderr).hexdigest()}
    try:
        entry['result'] = json.loads(entry['stdout'])
    except json.JSONDecodeError:
        pass
    return entry


def main():
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    report_path = HERE / f'python-cwd-v3-private-{stamp}.json'
    manifest_path = HERE / f'python-cwd-v3-private-{stamp}-files.json'
    assert not report_path.exists() and not manifest_path.exists()
    assert digest(EXECUTOR) == EXECUTOR_SHA
    metadata = ORIGIN.parent / 'basedpyright-1.40.2.dist-info/METADATA'
    assert '\nVersion: 1.40.2\n' in metadata.read_text(encoding='utf-8')
    package_paths = sorted(ORIGIN.rglob('*'))
    assert all(not path.is_symlink() and not path.is_junction() for path in package_paths)
    files = [path for path in package_paths if path.is_file()]
    assert 0 < len(files) <= 10000 and sum(path.stat().st_size for path in files) <= 256 * 1024 * 1024
    original = {path.relative_to(ORIGIN).as_posix(): digest(path) for path in files}
    # Deliberately preserve QA caches, including failures, for inspection. Only
    # this fresh, explicitly named subtree is created; no old cache is reused.
    base = Path(tempfile.mkdtemp(prefix='python-cwd-v3-private-', dir=TOOLS.parent)).resolve()
    caches, sources, scratches = {}, {}, {}
    for project in ['a', 'b']:
        cache = base / f'backend-{project}' / 'basedpyright'
        shutil.copytree(ORIGIN, cache)
        caches[project] = cache
        assert {name: digest(cache / name) for name in original} == original
        sources[project] = base / f'source-{project}'
        scratches[project] = base / f'scratch-{project}'
        sources[project].mkdir(); scratches[project].mkdir()
        (sources[project] / 'sample.py').write_text(f'project = "{project}"\n', encoding='utf-8', newline='\n')
    write(manifest_path, {'origin': str(ORIGIN), 'origin_metadata_sha256': digest(metadata),
        'origin_version': '1.40.2', 'files': original, 'count': len(original),
        'copies': {project: {'path': str(cache), 'per_file_sha256_equal_before_execution': True} for project, cache in caches.items()}})
    policies = base / 'denied-policies'; policies.mkdir()
    state = base / 'denied-host-state'; state.mkdir()
    (state / 'state.txt').write_text('host-state-negative-sentinel', encoding='utf-8')
    (policies / 'host-policy.txt').write_text('host-policy-negative-sentinel', encoding='utf-8')
    outside_e = base / 'outside-private.txt'
    outside_e.write_text('outside-e-negative-sentinel', encoding='utf-8')
    outside_c_dir = Path(tempfile.mkdtemp(prefix='python-cwd-v3-outside-'))
    outside_c = outside_c_dir / 'private.txt'
    outside_c.write_text('outside-c-negative-sentinel', encoding='utf-8')
    protected = [outside_c, outside_e, state / 'state.txt', policies / 'host-policy.txt',
                 sources['a'] / 'sample.py', sources['b'] / 'sample.py',
                 caches['a'] / 'langserver.index.js', caches['b'] / 'langserver.index.js']
    before_hashes = {str(path): digest(path) for path in protected}
    report = {'task_id': 'python-cwd-v3-private-copy-native-proof', 'status': 'NOT_RUN',
        'scope': 'Explicit QA host profile only. No product code/config changes, no automatic fallback.',
        'base': str(base), 'copy_manifest': manifest_path.name, 'copy_manifest_sha256': digest(manifest_path),
        'files': {str(path): digest(path) for path in [Path(__file__), HERE / 'python-cwd-v3-node.cjs', HERE / 'python-cwd-v3-winapi.py', EXECUTOR, NODE, PYTHON]},
        'risk': 'Backend can modify its own private RW copy. Never execute/reuse this copy outside the sandbox or publish it as a trusted SDK. Recreate each QA profile from verified pristine installed assets.',
        'runs': []}
    probe = run([str(EXECUTOR), '--probe'], HERE)
    report['native_capability_probe'] = probe
    native = probe.get('result', {})
    assert probe['exit_code'] == 0 and native.get('tier') == 'base-container'
    assert native.get('needsDaclAugmentation') is False and native.get('warnings') == []
    assert native.get('probes', {}).get('baseContainerSupportsEnumeratePaths') is True
    assert native.get('probes', {}).get('baseContainerSupportsDenyPaths') is True
    listener = socket.socket(); listener.bind(('127.0.0.1', 0)); listener.listen(); listener.settimeout(.1)
    accepted, stop = [], threading.Event()
    def listen():
        while not stop.is_set():
            try:
                connection, _ = listener.accept(); accepted.append(True); connection.close()
            except socket.timeout:
                continue
            except OSError:
                break
    thread = threading.Thread(target=listen, daemon=True); thread.start()
    try:
        for project in ['a', 'b']:
            other = 'b' if project == 'a' else 'a'
            targets = []
            for label, path in [('private_copy_dist', caches[project] / 'dist'),
                                ('installed_dist', ORIGIN / 'dist'), ('private_scratch', scratches[project])]:
                path = str(path)
                for variant, value in [('upper_backslash', path[0].upper() + path[1:]),
                                       ('lower_backslash', path[0].lower() + path[1:]),
                                       ('upper_forward', (path[0].upper() + path[1:]).replace('\\', '/')),
                                       ('lower_forward', (path[0].lower() + path[1:]).replace('\\', '/'))]:
                    targets.append({'name': label + '_' + variant, 'path': value})
            manifest = base / f'fixed-input-{project}.json'
            write(manifest, {'targets': targets, 'python': str(PYTHON), 'winapi_helper': str(HERE / 'python-cwd-v3-winapi.py'),
                'source': str(sources[project] / 'sample.py'), 'source_sha256': digest(sources[project] / 'sample.py'),
                'positive': str(scratches[project] / 'positive.txt'),
                'cache_positive': str(caches[project].parent / 'fixed-positive.txt'),
                'outside': [str(path) for path in [outside_c, outside_e, state / 'state.txt', sources[other] / 'sample.py', caches[other] / 'langserver.index.js']],
                'readonly_files': [str(ORIGIN / 'package.json')],
                'port': listener.getsockname()[1], 'policy': str(policies / 'host-policy.txt')})
            readonly = [sources[project], TOOLS / 'python', PYTHON.parent,
                        HERE / 'python-cwd-v3-node.cjs', HERE / 'python-cwd-v3-winapi.py', manifest]
            # Enumerate only ancestors outside each explicit grant. An enum rule
            # for the RW backend root itself overrides its RW access in PSEC.
            enumeration = sorted({str(parent) for path in readonly + [caches[project].parent, scratches[project]] for parent in path.resolve().parents})
            environment = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT', 'OS', 'PROCESSOR_ARCHITECTURE'] if key in os.environ}
            environment['PATH'] = os.pathsep.join([str(NODE.parent), str(PYTHON.parent), str(Path(os.environ['SystemRoot']) / 'System32')])
            for key in ['HOME', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP']:
                environment[key] = str(scratches[project])
            policy = {'version': '1.0.0', 'containment': 'processcontainer',
                'lifecycle': {'destroyOnExit': True, 'preservePolicy': False},
                'process': {'cwd': str(sources[project]), 'env': [f'{key}={value}' for key, value in environment.items()],
                            'inheritDefaultEnv': False, 'timeout': 25000},
                'filesystem': {'readonlyPaths': [str(path.resolve()) for path in readonly],
                    'readwritePaths': [str(caches[project].parent), str(scratches[project])],
                    'deniedPaths': [str(policies), str(state), str(sources[other]), str(caches[other].parent), str(scratches[other])]},
                'processContainer': {'filesystem': {'enumeratePaths': enumeration}},
                'fallback': {'allowDaclMutation': False},
                'network': {'egress': {'default': 'deny'}, 'ingress': {'default': 'deny', 'hostLoopback': 'deny'}},
                'ui': {'disable': False, 'clipboard': 'none', 'injection': False}, 'telemetry': {'enabled': False}}
            policy_path = policies / f'fixed-{project}.json'; write(policy_path, policy)
            entry = run([str(EXECUTOR), '--config', str(policy_path), '--', str(NODE), '--preserve-symlinks',
                         '--preserve-symlinks-main', str(HERE / 'python-cwd-v3-node.cjs'), str(manifest)], policies)
            entry.update(project=project, policy=policy, policy_sha256=digest(policy_path),
                fixed_ls_profile={'program': str(NODE), 'args': ['--preserve-symlinks', '--preserve-symlinks-main', str(caches[project] / 'langserver.index.js'), '--stdio'],
                                  'source_readonly': str(sources[project]), 'private_backend_rw': str(caches[project].parent),
                                  'original_runtime_readonly': str(TOOLS / 'python'), 'bootstrap': False, 'fallback': False})
            report['runs'].append(entry)
            write(report_path, report)
            parsed = entry.get('result', {})
            print(json.dumps({'project': project, 'exit_code': entry['exit_code'],
                'copy_chdir': [{'name': target['name'], 'success': target.get('returned', False)} for target in parsed.get('targets', []) if target['name'].startswith('private_copy')],
                'boundary_checks': parsed.get('checks')}), flush=True)
    finally:
        stop.set(); thread.join(timeout=1); listener.close()
    report['host_checks'] = {'protected_hashes_unchanged': all(digest(path) == expected for path, expected in before_hashes.items()),
        'readonly_creates_absent': all(not Path(str(source / 'sample.py') + '.forbidden').exists() for source in sources.values()),
        'positive_scratch_verified': all((path / 'positive.txt').read_text(encoding='utf-8') == 'fixed-positive' for path in scratches.values()),
        'positive_backend_write_verified': all((cache.parent / 'fixed-positive.txt').read_text(encoding='utf-8') == 'private-backend-positive' for cache in caches.values()),
        'original_assets_unchanged': all(digest(ORIGIN / name) == expected for name, expected in original.items()),
        'copies_still_match_original': {project: all(digest(cache / name) == expected for name, expected in original.items()) for project, cache in caches.items()},
        'host_loopback_accepts': len(accepted), 'protected_hashes_before': before_hashes}
    passed = (all(entry['exit_code'] == 0 and len(entry.get('result', {}).get('checks', [])) >= 17
        and all(target.get('returned') is True for target in entry['result']['targets'] if target['name'].startswith('private_copy')) for entry in report['runs'])
        and report['host_checks']['protected_hashes_unchanged'] and report['host_checks']['original_assets_unchanged'] and not accepted)
    report['status'] = 'PASS_NATIVE_PROFILE_ONLY' if passed else 'FAIL_NATIVE_PROFILE'
    report['semantic_and_mcp_acceptance'] = 'NOT_RUN: platform owner runs exact explicit profile against both projects next'
    write(report_path, report)
    print(json.dumps({'status': report['status'], 'report': str(report_path), 'sha256': digest(report_path), 'base_preserved': str(base)}))
    return 0 if report['status'] == 'PASS_NATIVE_PROFILE_ONLY' else 1


if __name__ == '__main__':
    raise SystemExit(main())
