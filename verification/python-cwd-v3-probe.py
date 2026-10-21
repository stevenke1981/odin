"""Three bounded hypotheses, fixed native executables; no product modifications."""
from contextlib import ExitStack
from datetime import datetime, timezone
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
EXECUTOR = Path('C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe')
EXECUTOR_SHA = '8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259'
TOOLS = Path('E:/odin/.odin/validation-tools')
DIST = TOOLS / 'python/Lib/site-packages/basedpyright/dist'
NODES = [TOOLS / 'python/Lib/site-packages/nodejs_wheel/node.exe', Path('C:/Program Files/nodejs/node.exe')]
PYTHON = Path('C:/Python314/python.exe')
CANDIDATE = Path('E:/odin/.odin/candidates/v3-780c6ad/odin.exe')
CANDIDATE_SHA = '371d4fbd36fffa334b0ef052d4d1157c1c799562a3dd8fd619eebfefe2b12451'


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--descendant-lower', action='store_true')
    parser.add_argument('--descendant-forward', action='store_true')
    args = parser.parse_args()
    if args.descendant_forward:
        args.descendant_lower = True
    report_path = HERE / f'python-cwd-v3-{datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")}.json'
    if report_path.exists():
        raise FileExistsError(report_path)
    assert digest(EXECUTOR) == EXECUTOR_SHA
    assert digest(CANDIDATE) == CANDIDATE_SHA
    for path in [*NODES, PYTHON, DIST]:
        assert path.exists()
    report = {'task_id': 'python-cwd-v3-native-diagnostic', 'status': 'NOT_RUN',
              'scope': 'Fixed direct native cwd diagnostics; no Odin/MCP or language support claims.',
              'hypotheses': ['H1: drive case/separator or initial-cwd normalization differs.',
                             'H2: bundled Node 24.19 vs host Node 24.21 libuv behavior differs.',
                             'H3: native SetCurrentDirectory/access-mask operation differs from read access.'],
              'files': {str(path): digest(path) for path in [Path(__file__), HERE / 'python-cwd-v3-node.cjs', HERE / 'python-cwd-v3-winapi.py', HERE / 'python-cwd-v3-bootstrap.cjs', EXECUTOR, *NODES, PYTHON, CANDIDATE]},
              'descendant_lower': args.descendant_lower,
              'descendant_forward': args.descendant_forward,
              'runs': []}
    probe = run([str(EXECUTOR), '--probe'], HERE)
    report['native_capability_probe'] = probe
    native = probe.get('result', {})
    assert probe['exit_code'] == 0 and native.get('tier') == 'base-container'
    assert native.get('needsDaclAugmentation') is False and native.get('warnings') == []
    assert native.get('probes', {}).get('baseContainerSupportsEnumeratePaths') is True
    assert native.get('probes', {}).get('baseContainerSupportsDenyPaths') is True
    with ExitStack() as stack:
        base = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='python-cwd-v3-')))
        outside_c = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='python-cwd-v3-outside-')))
        outside_e = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='python-cwd-v3-outside-', dir=TOOLS.parent)))
        source, scratch, policies = base / 'source', base / 'scratch', base / 'policies'
        for path in [source, scratch, policies]:
            path.mkdir()
        sample = source / 'fixed-source.py'
        sample.write_text('fixed_source = "read-only"\n', encoding='utf-8', newline='\n')
        outside = [outside_c / 'private.txt', outside_e / 'private.txt']
        for path in outside:
            path.write_text('host-private-negative-sentinel\n', encoding='utf-8', newline='\n')
        hashes = {str(path): digest(path) for path in [sample, *outside]}
        listener = stack.enter_context(socket.socket())
        listener.bind(('127.0.0.1', 0)); listener.listen(); listener.settimeout(.1)
        accepted, stop = [], threading.Event()
        stack.callback(stop.set)
        def listen():
            while not stop.is_set():
                try:
                    connection, _ = listener.accept(); accepted.append(True); connection.close()
                except socket.timeout:
                    continue
                except OSError:
                    break
        thread = threading.Thread(target=listen, daemon=True); thread.start()
        readonly = [source, TOOLS / 'python', PYTHON.parent, NODES[1].parent,
                    HERE / 'python-cwd-v3-node.cjs', HERE / 'python-cwd-v3-winapi.py', HERE / 'python-cwd-v3-bootstrap.cjs']
        targets = []
        for label, path in [('dist', DIST), ('venv', TOOLS / 'python'), ('source', source),
                            ('python_base', PYTHON.parent), ('node_host', NODES[1].parent), ('private_rw', scratch)]:
            native_path = str(path.resolve())
            for variant, value in [('upper_backslash', native_path[0].upper() + native_path[1:]),
                                   ('lower_backslash', native_path[0].lower() + native_path[1:]),
                                   ('upper_forward', (native_path[0].upper() + native_path[1:]).replace('\\', '/')),
                                   ('lower_forward', (native_path[0].lower() + native_path[1:]).replace('\\', '/'))]:
                targets.append({'name': label + '_' + variant, 'path': value})
        manifest = base / 'fixed-input.json'
        manifest.write_text(json.dumps({'targets': targets, 'python': str(PYTHON), 'winapi_helper': str(HERE / 'python-cwd-v3-winapi.py'),
            'bundled_node': str(NODES[0]), 'lower_dist': (str(DIST.resolve())[0].lower() + str(DIST.resolve())[1:]).replace('\\', '/') if args.descendant_forward else str(DIST.resolve())[0].lower() + str(DIST.resolve())[1:],
            'node_helper': str(HERE / 'python-cwd-v3-node.cjs'),
            'source': str(sample), 'source_sha256': digest(sample), 'positive': str(scratch / 'positive.txt'),
            'outside': [str(path) for path in outside], 'port': listener.getsockname()[1],
            'policy': str(policies / 'protected.json')}), encoding='utf-8', newline='\n')
        readonly.append(manifest)
        enumeration = sorted({str(parent) for path in readonly for parent in path.resolve().parents})
        environment = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT', 'OS', 'PROCESSOR_ARCHITECTURE'] if key in os.environ}
        environment['PATH'] = os.pathsep.join([str(NODES[1].parent), str(PYTHON.parent), str(Path(os.environ['SystemRoot']) / 'System32')])
        for key in ['HOME', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP']:
            environment[key] = str(scratch)
        policy = {'version': '1.0.0', 'containment': 'processcontainer',
                  'lifecycle': {'destroyOnExit': True, 'preservePolicy': False},
                  'process': {'cwd': str(source), 'env': [f'{key}={value}' for key, value in environment.items()],
                              'inheritDefaultEnv': False, 'timeout': 25000},
                  'filesystem': {'readonlyPaths': [str(path.resolve()) for path in readonly],
                                 'readwritePaths': [str(scratch)], 'deniedPaths': [str(policies)]},
                  'processContainer': {'filesystem': {'enumeratePaths': enumeration}},
                  'fallback': {'allowDaclMutation': False},
                  'network': {'egress': {'default': 'deny'}, 'ingress': {'default': 'deny', 'hostLoopback': 'deny'}},
                  'ui': {'disable': False, 'clipboard': 'none', 'injection': False}, 'telemetry': {'enabled': False}}
        protected = policies / 'protected.json'
        protected.write_text('host-private-policy-sentinel', encoding='utf-8')
        report['base_policy'] = policy
        for initial in ([source] if args.descendant_lower else [source, DIST]):
            policy['process']['cwd'] = str(initial)
            for node in NODES:
                policy_path = policies / f'node-{len(report["runs"])}.json'
                policy_path.write_text(json.dumps(policy), encoding='utf-8', newline='\n')
                helper = HERE / ('python-cwd-v3-bootstrap.cjs' if args.descendant_lower else 'python-cwd-v3-node.cjs')
                entry = run([str(EXECUTOR), '--config', str(policy_path), '--', str(node),
                             '--preserve-symlinks', '--preserve-symlinks-main', str(helper), str(manifest)], policies)
                entry.update(kind='node', initial_cwd=str(initial), policy_sha256=digest(policy_path))
                report['runs'].append(entry)
                report_path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
                parsed = entry.get('result', {})
                if args.descendant_lower:
                    print(json.dumps({'descendant_bootstrap': {key: value for key, value in parsed.items() if key != 'result'}}), flush=True)
                    parsed = parsed.get('result', {})
                print(json.dumps({'kind': 'node', 'node': parsed.get('node'), 'initial': str(initial), 'exit_code': entry['exit_code'],
                    'first_failure': next((target for target in parsed.get('targets', []) if 'error' in target), None)}), flush=True)
            if args.descendant_lower:
                continue
            policy_path = policies / f'winapi-{len(report["runs"])}.json'
            policy_path.write_text(json.dumps(policy), encoding='utf-8', newline='\n')
            entry = run([str(EXECUTOR), '--config', str(policy_path), '--', str(PYTHON), '-I', '-B',
                         str(HERE / 'python-cwd-v3-winapi.py'), str(manifest)], policies)
            entry.update(kind='winapi', initial_cwd=str(initial), policy_sha256=digest(policy_path))
            report['runs'].append(entry)
        stop.set(); thread.join(timeout=1)
        report['host_checks'] = {'source_and_outside_unchanged': all(digest(path) == expected for path, expected in hashes.items()),
            'readonly_create_absent': not Path(str(sample) + '.forbidden').exists(),
            'positive_scratch_verified': (scratch / 'positive.txt').read_text(encoding='utf-8') == 'fixed-positive' if (scratch / 'positive.txt').exists() else False,
            'host_loopback_accepts': len(accepted), 'hashes': hashes}
        report['status'] = 'OBSERVED' if all(entry['exit_code'] == 0 for entry in report['runs']) else 'PARTIAL_DIAGNOSTIC'
        report_path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
        print(json.dumps({'status': report['status'], 'report': str(report_path), 'sha256': digest(report_path), 'host_checks': report['host_checks']}), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
