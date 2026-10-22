"""External operator runner. --check-runner is inert; --execute installs for real.

No repository imports, client edits, configuration rewrites, grant changes, shell
commands, or broad process cleanup. Root must separately authorize --execute.
"""
from __future__ import annotations

import argparse
import copy
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shutil
import subprocess
import sys
import threading
import time
import tomllib
import uuid

BASE = Path(__file__).resolve().parent
PROBES = ('node', 'basedpyright', 'typescript-language-server', 'typescript',
          'bash-language-server', 'go', 'gopls', 'dotnet', 'csharp-ls',
          'lua-language-server', 'slint-lsp', 'clangd', 'taplo',
          'yaml-language-server', 'marksman', 'ruff', 'ty', 'rust-analyzer',
          '@vtsls/language-server')
LANGUAGES = ('python', 'go', 'csharp', 'lua', 'javascript', 'typescript', 'bash',
             'slint', 'c', 'cpp', 'toml', 'yaml', 'markdown', 'rust')
DEFAULTS = {'python': 'basedpyright', 'typescript': 'typescript-language-server'}
PINS = {
    'installer': 'ebf7f94acac5c0d850b4c5636684e775689df84dbc44a0b97324b35e17421af2',
    'binary': '2e1c11b040ab1c13e4843a0de7b912b0f8b9452d2284a5c353077df32e021792',
    'setup': 'be6ead44f0a2cd68915dfc1144c2f7b608fb54bfb943a8d09bf3b015c8b0abec',
    'payload_manifest': 'c12a4062e256a0cb6da19c5c2a383d83f9324fb892eb8609ac8bd9754c821680',
}
INNO_DOC = 'https://jrsoftware.org/ishelp/topic_setupcmdline.htm'


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(',', ':'), allow_nan=False).encode('utf-8')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def object_sha(value):
    return sha(canonical(value))


def file_sha(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            result.update(chunk)
    return result.hexdigest()


def write_new(path, value):
    data = value if isinstance(value, bytes) else json.dumps(value, ensure_ascii=True, indent=2).encode() + b'\n'
    with Path(path).open('xb') as stream:
        stream.write(data)


def local_drive_spelling(path):
    value = os.fspath(path)
    require(not value.startswith(('\\\\.\\', '//./', '//?/', '\\??\\')),
            'Device namespace paths are not supported for upgrade identity checks')
    if value.startswith('\\\\?\\'):
        # Equate only the reviewed absolute DOS drive spelling. Extended UNC,
        # GLOBALROOT, volume/device and drive-relative paths are never stripped.
        require(re.match(r'^[A-Za-z]:\\', value[4:]) is not None,
                'Only an extended absolute drive path can use the local drive alias')
        plain = value[4:]
        require('/' not in plain, 'Extended drive paths must use canonical backslashes')
        components = plain[3:].split('\\')
        for index, part in enumerate(components):
            require(part or index == len(components) - 1,
                    'Extended drive alias contains an empty interior component')
            require(part not in ('.', '..') and not part.endswith((' ', '.')) and ':' not in part,
                    'Extended drive alias contains a noncanonical component')
            stem = part.split('.')[0].upper()
            require(stem not in {'CON', 'PRN', 'AUX', 'NUL', 'CONIN$', 'CONOUT$'}
                    and re.fullmatch(r'(COM|LPT)[1-9]', stem) is None,
                    'Extended drive alias contains a DOS device component')
        return plain
    return value


def path_identity(path):
    # Resolve the validated local spelling, then handle Windows' resolved
    # extended-drive presentation too. Original no_redirect checks are unchanged.
    resolved = str(Path(local_drive_spelling(path)).resolve())
    return os.path.normcase(local_drive_spelling(resolved))


def no_redirect(path):
    path = Path(os.path.abspath(path))
    for item in [*reversed(path.parents), path]:
        if item.exists() or item.is_symlink():
            attributes = getattr(item.lstat(), 'st_file_attributes', 0)
            require(not item.is_symlink() and not attributes & 0x400,
                    'Refusing a redirected path or ancestor')
    return path


def scopes(config):
    result = []
    if isinstance(config.get('workspace'), dict):
        result.append(('workspace', config['workspace']))
    result += [('projects.' + key, value) for key, value in config.get('projects', {}).items()]
    return result


def config_projection(config):
    value = copy.deepcopy(config)
    for _, scope in scopes(value):
        scope.pop('languages', None)
    for key in ('allowed_roots', 'default_root', 'protected_roots'):
        value.get('workspace', {}).pop(key, None)
    return value


def grant_view(config):
    result = []
    for name, scope in scopes(config):
        result.append({'scope': name if name == 'workspace' else 'project:' + object_sha(name),
                       'grants': {key: val for key, val in scope.items() if key.startswith('allow_')}})
    return result


def rust_view(config):
    result = []
    for name, scope in scopes(config):
        for profile in scope.get('languages', []):
            if profile.get('id') == 'rust' or any(str(ext).lower() == 'rs' for ext in profile.get('extensions', [])):
                result.append({'scope_sha256': object_sha(name), 'id': profile.get('id'),
                               'profile_sha256': object_sha(profile)})
    return result


def profile_summary(profiles):
    require(isinstance(profiles, list), 'Expected actual language profiles')
    ids = [p.get('id') for p in profiles]
    require(len(ids) == len(LANGUAGES) and set(ids) == set(LANGUAGES),
            'Actual host must have exactly the 14 expected primary profiles')
    require(all(isinstance(p.get('program'), str) and p['program'] for p in profiles),
            'A profile is missing its actual executable')
    implementations = {object_sha({'program': p['program'], 'args': p.get('args', [])}) for p in profiles}
    require(len(implementations) == 12, 'Expected exactly 12 primary backend invocations')
    return {'primary_profile_count': len(ids), 'primary_profile_ids': ids,
            'backend_invocation_count': len(implementations),
            'profile_hashes': {p['id']: object_sha(p) for p in profiles}}


def native_profile_ids(value, profiles):
    actual = value.get('languages', [])
    expected = {row['id']: row.get('extensions', []) for row in profiles}
    require(len(actual) == 14 and {row.get('id'): row.get('extensions', []) for row in actual} == expected,
            'Native current-config language IDs/extensions differ from committed host profiles')
    require(all(value.get(key) is False for key in ('allow_write', 'allow_shell', 'allow_debug', 'allow_inspections', 'allow_formatters')),
            'Native default Home unexpectedly has an elevated grant')
    return {'primary_profile_ids': [row['id'] for row in actual], 'primary_profile_count': len(actual),
            'profile_invocation_evidence': 'Separate native languages --project result and committed host profiles'}


def verify_host_change(before, after, home, host_parent):
    require(config_projection(before) == config_projection(after),
            'A setting outside explicit workspace migration/language profiles changed')
    require(grant_view(before) == grant_view(after), 'Prior permissions changed')
    require(rust_view(before) == rust_view(after), 'An existing Rust profile changed')
    workspace = after.get('workspace', {})
    require(path_identity(workspace.get('default_root', '')) == path_identity(home),
            'Host default_root must be the actual OS Home')
    resolve_root = lambda v: path_identity(Path(v) if Path(v).is_absolute() else host_parent / v)
    old_roots = {resolve_root(root) for root in before.get('workspace', {}).get('allowed_roots', [])}
    new_roots = {resolve_root(root) for root in workspace.get('allowed_roots', [])}
    require(new_roots == old_roots | {path_identity(home)}, 'Unexpected allowed workspace root change')
    old_protected = {path_identity(v) for v in before.get('workspace', {}).get('protected_roots', [])}
    new_protected = {path_identity(v) for v in workspace.get('protected_roots', [])}
    require(old_protected <= new_protected, 'A prior protected-root denial was removed')
    return [{'scope_sha256': object_sha(name), **profile_summary(scope.get('languages', []))}
            for name, scope in scopes(after)]


def verify_receipt(receipt, host, before_sha, after_sha, manifest_sha):
    require(receipt.get('result', {}).get('status') == 'PASS', 'Installer worker did not commit successfully')
    result, plan, health = receipt['result'], receipt.get('plan', {}), receipt.get('health', {})
    require(plan.get('status') == 'READY', 'Worker plan was not READY')
    require(result.get('before_sha256') == before_sha == plan.get('before_sha256'),
            'Worker did not start from the reviewed host snapshot')
    require(result.get('after_sha256') == after_sha == plan.get('after_sha256'),
            'Committed host hash does not match the worker receipt')
    require(path_identity(result.get('host_config', '')) == path_identity(host) == path_identity(plan.get('host_config', '')),
            'Worker receipt is for a different host')
    require(result.get('bundle_manifest_sha256') == manifest_sha == plan.get('bundle_manifest_sha256'),
            'Worker receipt payload identity mismatch')
    require(result.get('selected_backends') == DEFAULTS == plan.get('selected_backends'),
            'Existing provider selection is not the expected default selection')
    require(len(result.get('languages', [])) == 14 and set(result['languages']) == set(LANGUAGES),
            'Worker did not report the 14 primary language IDs')
    require(health.get('status') == 'PASS' and health.get('post_health_integrity', {}).get('status') == 'PASS',
            'Actual installed worker health or post-health closure failed')
    checks = health.get('checks', [])
    require(len(checks) == 19 and {row.get('component') for row in checks} == set(PROBES),
            'Actual version probes must be the 19 executable components, excluding parser data')
    require(all(row.get('status') == 'PASS' and row.get('exit_code') == 0 for row in checks),
            'An actual installed version probe failed')
    return {'version_probe_count': len(checks), 'component_ids': [r['component'] for r in checks],
            'post_health_integrity': health['post_health_integrity'], 'semantic_acceptance': 'NOT_RUN'}


def installer_command(installer, install_root, home, host, receipt, log):
    return [str(installer), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
            '/CLOSEAPPLICATIONS', '/NOFORCECLOSEAPPLICATIONS', '/NORESTARTAPPLICATIONS',
            '/DIR=' + str(install_root), '/ODIN_HOST_CONFIG=' + str(host),
            '/ODIN_WORKSPACE_ROOT=' + str(home), '/ODIN_SETUP_RECEIPT=' + str(receipt),
            '/LOG=' + str(log)]


def verify_client(config, binary, host):
    entry = config.get('mcp_servers', {}).get('odin')
    require(isinstance(entry, dict), 'Codex has no existing Odin entry')
    require(path_identity(entry.get('command', '')) == path_identity(binary), 'Codex command is not the expected per-user Odin')
    args = entry.get('args')
    require(isinstance(args, list), 'Codex Odin arguments are not a list')
    require(args == ['--config', args[1] if len(args) > 1 else '', '--workspace', '.', 'serve', '--transport', 'stdio'],
            'Codex entry must retain the reviewed --workspace . stdio invocation')
    require(path_identity(args[1]) == path_identity(host), 'Codex references a different host')
    return entry


def project_binding(value, root, registered_ids):
    projects = value.get('projects', [])
    require(len(projects) == 1, 'Binding must expose exactly one selected project')
    row = projects[0]
    require(path_identity(row.get('root', '')) == path_identity(root), 'Selected project root is incorrect')
    require(isinstance(row.get('id'), str) and row['id'].startswith('ws_') and row['id'] not in registered_ids,
            'Expected a transient workspace ID, not a registered project')
    require(all(row.get(key) is False for key in ('allow_write', 'allow_shell', 'allow_debug', 'allow_inspections')),
            'A transient workspace unexpectedly has an elevated grant')
    return row['id']


def decode_tool(response):
    require('error' not in response, 'MCP returned a JSON-RPC error')
    result = response.get('result', {})
    require(result.get('isError') is not True, 'MCP returned a tool error')
    value = result.get('structuredContent')
    if value is None:
        values = [json.loads(c['text']) for c in result.get('content', []) if c.get('type') == 'text']
        require(len(values) == 1, 'Expected exactly one MCP result object')
        value = values[0]
    require(isinstance(value, dict), 'MCP tool result is not an object')
    return value


class Peer:
    """Only manages the child started here; never enumerates/kills other PIDs."""
    def __init__(self, command, cwd):
        self.process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        creationflags=subprocess.CREATE_NO_WINDOW)
        self.messages, self.stderr, self.events = queue.Queue(), bytearray(), []
        self.sequence = 0
        threading.Thread(target=self.read, daemon=True).start()
        threading.Thread(target=self.read_errors, daemon=True).start()

    def read(self):
        try:
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
            self.messages.put(EOFError('Owned MCP child exited'))
        except Exception as error:
            self.messages.put(error)

    def read_errors(self):
        for block in iter(lambda: self.process.stderr.read(4096), b''):
            self.stderr.extend(block)

    def send(self, value):
        self.process.stdin.write(json.dumps(value, separators=(',', ':')).encode() + b'\n')
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.send({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params})
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            message = self.messages.get(timeout=max(0.01, deadline - time.monotonic()))
            if isinstance(message, Exception):
                raise RuntimeError('Owned MCP child failed before the requested result') from message
            if message.get('id') == self.sequence:
                return message
            self.events.append(message)
        raise TimeoutError('Owned MCP request exceeded 30 seconds')

    def close(self):
        try:
            self.process.stdin.close()
        except (BrokenPipeError, OSError):
            pass
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        for stream in (self.process.stdout, self.process.stderr):
            stream.close()


def os_home():
    require(os.name == 'nt', 'Actual user upgrade is Windows-only')
    buffer = ctypes.create_unicode_buffer(32768)
    require(ctypes.windll.shell32.SHGetFolderPathW(None, 0x28, None, 0, buffer) == 0,
            'Cannot identify the actual Windows user profile')
    home = no_redirect(buffer.value)
    require(path_identity(os.environ.get('USERPROFILE', '')) == path_identity(home),
            'USERPROFILE does not match the OS-derived actual Home')
    return home


def operator_state():
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as key:
            try:
                value, kind = winreg.QueryValueEx(key, 'Path')
                path_value = {'present': True, 'kind': kind, 'value': value}
            except FileNotFoundError:
                path_value = {'present': False}
    except FileNotFoundError:
        path_value = {'present': False}
    command = [str(Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe'),
               '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', str(BASE / 'read_operator_cert_state.ps1')]
    observed = subprocess.run(command, capture_output=True, timeout=45,
                              creationflags=subprocess.CREATE_NO_WINDOW)
    require(observed.returncode == 0, 'Read-only certificate snapshot failed')
    certificates = json.loads(observed.stdout.decode('utf-8-sig'))
    return {'user_path_sha256': object_sha(path_value),
            'process_path_sha256': sha(os.environ.get('PATH', '').encode()),
            'certificates': certificates, 'certificate_state_sha256': object_sha(certificates)}


class Runner:
    def __init__(self, args):
        self.args = args
        no_redirect(BASE)
        self.output = BASE / ('run-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8])
        self.output.mkdir()
        self.report = {'task_id': 'ODIN-ACTUAL-USER-UPGRADE', 'status': 'NOT_RUN',
                       'runner_sha256': file_sha(__file__), 'checks': [], 'commands': [],
                       'certificate_reader_sha256': file_sha(BASE / 'read_operator_cert_state.ps1'),
                       'final_pkg7_expected_hashes': PINS,
                       'started_at_utc': datetime.now(timezone.utc).isoformat(),
                       'notes': ['No client restart is claimed; the client must reconnect/restart after upgrade.',
                                 'No installer expected-host override exists. An immediate prelaunch hash guard and the worker plan CAS are separate; they do not atomically bind the wrapper snapshot through the file-copy interval.',
                                 'Read-only native calls may create normal private audit/state records; no workspace file content is requested or changed.',
                                 'A failed run is preserved. No automatic retry, rollback, config rewrite, or conflict override.'],
                       'inno_standard_option_source': INNO_DOC,
                       'semantic_acceptance': 'NOT_RUN'}

    def save(self):
        # This report is owned by this fresh external run directory.
        (self.output / 'report.json').write_text(json.dumps(self.report, indent=2, ensure_ascii=True) + '\n', encoding='utf-8')

    def check(self, name, passed):
        self.report['checks'].append({'name': name, 'status': 'PASS' if passed else 'FAIL'})
        self.save()
        require(passed, name)

    def command(self, name, argv, cwd, timeout=90, private_output=False):
        started = time.monotonic()
        result = subprocess.run([str(v) for v in argv], cwd=cwd, capture_output=True,
                                timeout=timeout, creationflags=subprocess.CREATE_NO_WINDOW)
        stdout_path, stderr_path = self.output / (name + '.stdout.log'), self.output / (name + '.stderr.log')
        if not private_output:
            write_new(stdout_path, result.stdout)
            write_new(stderr_path, result.stderr)
        row = {'name': name, 'argv': [str(v) for v in argv], 'cwd': str(cwd),
               'exit_code': result.returncode, 'elapsed_seconds': round(time.monotonic() - started, 3),
               'stdout_sha256': sha(result.stdout), 'stderr_sha256': sha(result.stderr),
               'private_output_not_saved': private_output}
        self.report['commands'].append(row)
        self.save()
        self.check(name + '_exit_0', result.returncode == 0)
        return result.stdout

    def install(self, argv):
        # Unlike subprocess.run(timeout=...), never kill a live installer/worker
        # at the observer deadline. Its own bounded worker owns cancellation/CAS.
        stdout_path = self.output / 'actual_user_installer.stdout.log'
        stderr_path = self.output / 'actual_user_installer.stderr.log'
        started = time.monotonic()
        with stdout_path.open('xb') as stdout, stderr_path.open('xb') as stderr:
            process = subprocess.Popen(argv, cwd=self.output, stdout=stdout, stderr=stderr,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            row = {'name': 'actual_user_installer', 'argv': argv, 'cwd': str(self.output),
                   'owned_pid': process.pid, 'exit_code': None, 'observer_deadline_seconds': 1800}
            self.report['commands'].append(row)
            self.save()
            try:
                row['exit_code'] = process.wait(timeout=1800)
            except subprocess.TimeoutExpired:
                row['observer_status'] = 'TIMEOUT_INSTALLER_LEFT_RUNNING_NO_RETRY'
                row['elapsed_seconds'] = round(time.monotonic() - started, 3)
                self.save()
                raise TimeoutError('Installer observer exceeded 1800 seconds; installer was not killed. Inspect its PID/log before any retry.')
        row.update(elapsed_seconds=round(time.monotonic() - started, 3),
                   stdout_sha256=file_sha(stdout_path), stderr_sha256=file_sha(stderr_path))
        self.save()
        self.check('actual_user_installer_exit_0', row['exit_code'] == 0)

    def mcp_binding(self, name, command, cwd, expected, registered_ids):
        peer = Peer(command, cwd)
        started = time.monotonic()
        try:
            initialized = peer.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                      'clientInfo': {'name': 'odin-user-upgrade-readonly', 'version': '1'}})
            require(initialized.get('result', {}).get('serverInfo', {}).get('name') == 'odin', 'Actual MCP initialization failed')
            peer.send({'jsonrpc': '2.0', 'method': 'notifications/initialized', 'params': {}})
            response = peer.request('tools/call', {'name': 'list_projects', 'arguments': {}})
            value = decode_tool(response)
            identifier = project_binding(value, expected, registered_ids)
            write_new(self.output / (name + '.json'), {'initialize': initialized, 'list_projects': response})
            self.check(name + '_readonly_transient_binding', True)
            return identifier
        finally:
            active_error = sys.exc_info()[0] is not None
            peer.close()
            self.report['commands'].append({'name': name, 'argv': command, 'cwd': str(cwd),
                                           'owned_pid': peer.process.pid, 'child_exit_code': peer.process.returncode,
                                           'stderr_sha256': sha(bytes(peer.stderr)),
                                           'elapsed_seconds': round(time.monotonic() - started, 3)})
            self.save()
            if not active_error:
                self.check(name + '_owned_child_exit_0', peer.process.returncode == 0)

    def execute(self):
        args = self.args
        home = os_home()
        host = no_redirect(home / '.config/odin/host.toml')
        client = no_redirect(home / '.codex/config.toml')
        install_root = no_redirect(home / 'AppData/Local/Programs/Odin')
        require(path_identity(os.environ.get('LOCALAPPDATA', '')) == path_identity(home / 'AppData/Local'),
                'LOCALAPPDATA is not the reviewed per-user application root')
        binary = install_root / 'odin.exe'
        installer = no_redirect(args.installer)
        require(Path(args.installer).is_absolute() and Path(args.workspace_a).is_absolute() and Path(args.workspace_b).is_absolute(),
                'Installer and both explicit roots must use absolute paths')
        roots = [no_redirect(Path(args.workspace_a)), no_redirect(Path(args.workspace_b))]
        require(all(root.is_dir() for root in roots), 'Two explicit existing workspace directories are required')
        require(path_identity(roots[0]) != path_identity(roots[1]), 'Explicit workspace roots must differ')
        for value in (args.expected_host_sha256, args.expected_client_sha256, *PINS.values()):
            require(re.fullmatch('[a-f0-9]{64}', value or '') is not None, 'Expected SHA-256 values are required')
        self.check('final_pkg7_installer_hash', file_sha(installer) == PINS['installer'])
        before_bytes = host.read_bytes()
        before_host = tomllib.loads(before_bytes.decode('utf-8-sig'))
        before_client = client.read_bytes()
        self.check('reviewed_host_hash_before_snapshot', sha(before_bytes) == args.expected_host_sha256)
        self.check('reviewed_client_hash_before_snapshot', sha(before_client) == args.expected_client_sha256)
        entry = verify_client(tomllib.loads(before_client.decode('utf-8-sig')), binary, host)
        registered_ids = set(before_host.get('projects', {}))
        registered_roots = [Path(p['root']) if Path(p['root']).is_absolute() else host.parent / p['root']
                            for p in before_host.get('projects', {}).values()]
        protected = [Path(v) for v in before_host.get('workspace', {}).get('protected_roots', [])]
        for root in roots:
            require(not any(root.resolve().is_relative_to(p.resolve()) for p in registered_roots),
                    'An explicit workspace resolves to a registered project rather than a transient project')
            require(not any(root.resolve().is_relative_to(p.resolve()) for p in [home / '.config', home / 'AppData', home / '.codex', *protected]),
                    'An explicit workspace is in private/protected host storage')
            allowed = [Path(v) if Path(v).is_absolute() else host.parent / v for v in before_host.get('workspace', {}).get('allowed_roots', [])] + [home]
            require(any(root.resolve().is_relative_to(p.resolve()) for p in allowed), 'An explicit workspace is outside existing roots plus the authorized Home')
        marker = host.with_name(host.name + '.odin-lsps.json')
        marker_value = json.loads(marker.read_text(encoding='utf-8-sig')) if marker.is_file() else {}
        self.check('existing_provider_choices_are_defaults', {**DEFAULTS, **marker_value.get('selected_backends', {})} == DEFAULTS)
        write_new(self.output / 'host-before.backup.toml', before_bytes)
        self.report['before'] = {'host_sha256': sha(before_bytes), 'client_sha256': sha(before_client),
                                 'client_odin_entry_sha256': object_sha(entry),
                                 'grants': grant_view(before_host), 'rust_profiles': rust_view(before_host),
                                 'non_language_policy_sha256': object_sha(config_projection(before_host)),
                                 'managed_marker_sha256': file_sha(marker) if marker.is_file() else 'absent',
                                 'installed_file_hashes': {name: file_sha(install_root / name) if (install_root / name).is_file() else 'absent'
                                                           for name in ('odin.exe', 'setup-lsps.mjs', 'backends/manifest.json', 'SHA256SUMS.json')},
                                 'operator_state': operator_state()}
        self.report['identities'] = {'home': str(home), 'host': str(host), 'client': str(client),
                                     'install_root': str(install_root), 'workspace_roots': [str(r) for r in roots]}
        self.save()
        receipt = self.output / 'installer-worker-receipt.json'
        command = installer_command(installer, install_root, home, host, receipt, self.output / 'installer.log')
        # Last checks before invoking the installer: no setup/CLI call or config write intervenes.
        self.check('expected_client_sha256_immediately_before_installer', file_sha(client) == args.expected_client_sha256)
        self.check('expected_host_sha256_immediately_before_installer', file_sha(host) == args.expected_host_sha256)
        self.install(command)
        expected_files = {'odin.exe': PINS['binary'], 'setup-lsps.mjs': PINS['setup'],
                          'backends/manifest.json': PINS['payload_manifest']}
        self.report['installed_file_hashes'] = {name: file_sha(install_root / name) for name in expected_files}
        for name, expected in expected_files.items():
            self.check('installed_hash:' + name, self.report['installed_file_hashes'][name] == expected)
        self.report['installed_file_hashes'].update({name: file_sha(install_root / name)
                                                    for name in ('setup-progress.mjs', 'SHA256SUMS.json')})
        worker = json.loads(receipt.read_text(encoding='utf-8-sig'))
        after_bytes = host.read_bytes()
        after_host = tomllib.loads(after_bytes.decode('utf-8-sig'))
        self.report['actual_worker_health'] = verify_receipt(worker, host, args.expected_host_sha256, sha(after_bytes), PINS['payload_manifest'])
        self.check('actual_worker_19_health_probes_and_cas_receipt', True)
        self.report['actual_host_profiles'] = verify_host_change(before_host, after_host, home, host.parent)
        self.check('home_migration_all_prior_permissions_and_rust_preserved', True)
        self.check('codex_config_exact_bytes_unchanged', client.read_bytes() == before_client)
        self.report['worker_receipt_sha256'] = file_sha(receipt)
        self.report['host_backup_sha256'] = file_sha(self.output / 'host-before.backup.toml')
        for backup in worker['result'].get('backups', []):
            self.check('worker_backup_hash:' + object_sha(backup.get('path')), file_sha(backup['path']) == backup['sha256'])
        payload = json.loads(self.command('installed_payload_closure', [install_root / 'backends/node/node.exe', install_root / 'setup-lsps.mjs', '--verify-payload'], self.output, timeout=300))
        self.check('installed_payload_closure_identity', payload.get('status') == 'PASS' and payload.get('manifest_sha256') == PINS['payload_manifest'])
        metadata = json.loads(self.command('installed_metadata', [binary, 'metadata'], self.output))
        self.check('installed_windows_target', metadata.get('target') == 'x86_64-pc-windows-msvc')
        current = json.loads(self.command('default_host_current_config', [binary, 'call', 'get_current_config'], roots[1], private_output=True))
        self.report['native_default_profile_ids'] = native_profile_ids(current, after_host['workspace']['languages'])
        self.check('native_default_host_14_ids_extensions_readonly_grants', True)
        cli_ids = []
        for index, root in enumerate(roots):
            value = json.loads(self.command('cli_root_' + str(index + 1), [binary, '--config', host, '--workspace', root, 'call', 'list_projects'], root))
            cli_ids.append(project_binding(value, root, registered_ids))
        default_value = json.loads(self.command('cli_omitted_config_and_workspace', [binary, 'call', 'list_projects'], roots[1]))
        home_id = project_binding(default_value, home, registered_ids)
        configured = json.loads(self.command('native_configured_home_profiles', [binary, 'languages', '--project', home_id], roots[1], private_output=True))
        require(configured.get('project') == home_id, 'Native profile evidence is for a different project')
        native_profiles = configured.get('configured')
        self.report['native_default_profiles'] = profile_summary(native_profiles)
        require(native_profiles == after_host['workspace']['languages'], 'Native configured profiles differ from committed host')
        self.check('native_default_host_14_profiles_12_invocations', True)
        rust = next(p for p in after_host['workspace']['languages'] if p['id'] == 'rust')
        configured_rust = rust['program']
        resolved_rust = Path(configured_rust) if Path(configured_rust).is_absolute() else Path(shutil.which(configured_rust) or '')
        require(resolved_rust.is_absolute() and resolved_rust.is_file(), 'Preserved configured Rust executable is unavailable')
        resolved_rust = no_redirect(resolved_rust)
        require(not any(resolved_rust.resolve().is_relative_to(p.resolve()) for p in [*registered_roots, *roots]),
                'Refusing to run a repository/workspace-supplied Rust executable')
        rust_version = self.command('preserved_operator_rust_version', [resolved_rust, '--version'], self.output, timeout=60)
        self.check('preserved_operator_rust_version_identity', b'rust-analyzer' in rust_version.lower())
        self.report['preserved_operator_rust_health'] = {'status': 'PASS', 'scope': 'VERSION_ONLY_NOT_BUNDLED_RA_OR_SEMANTIC_ACCEPTANCE',
                                                       'configured_profile_sha256': object_sha(rust), 'resolved_program': str(resolved_rust),
                                                       'program_sha256': file_sha(resolved_rust), 'version_stdout_sha256': sha(rust_version)}
        mcp_ids = [self.mcp_binding('mcp_client_root_' + str(i + 1), [entry['command'], *entry['args']], root, root, registered_ids)
                   for i, root in enumerate(roots)]
        mcp_home_id = self.mcp_binding('mcp_omitted_config_and_workspace', [str(binary), 'serve', '--transport', 'stdio'], roots[0], home, registered_ids)
        self.check('two_explicit_transient_ids_and_default_home_match_cli_mcp',
                   len(set(cli_ids + [home_id])) == 3 and cli_ids == mcp_ids and home_id == mcp_home_id)
        self.check('host_unchanged_by_readonly_verification', host.read_bytes() == after_bytes)
        self.check('client_unchanged_by_readonly_verification', client.read_bytes() == before_client)
        self.report['after_host_sha256'] = sha(after_bytes)
        self.report['client_odin_entry_unchanged'] = True
        self.report['binding_ids'] = {'explicit': cli_ids, 'home_default': home_id}

    def run(self):
        first_failure = None
        try:
            self.execute()
        except Exception as error:
            first_failure = {'type': type(error).__name__, 'message': str(error)}
            self.report['first_failure'] = first_failure
        finally:
            if 'before' in self.report:
                try:
                    after = operator_state()
                    self.report['operator_state_after'] = after
                    before = self.report['before']['operator_state']
                    for key in ('user_path_sha256', 'process_path_sha256', 'certificate_state_sha256'):
                        same = before[key] == after[key]
                        self.report['checks'].append({'name': 'operator_state_preserved:' + key, 'status': 'PASS' if same else 'FAIL'})
                        if not same and first_failure is None:
                            first_failure = {'type': 'AssertionError', 'message': 'Operator state changed: ' + key}
                    for label, relative in (('host', '.config/odin/host.toml'), ('client', '.codex/config.toml')):
                        self.report['final_' + label + '_sha256'] = file_sha(os_home() / relative)
                    final_host = tomllib.loads((os_home() / '.config/odin/host.toml').read_text(encoding='utf-8-sig'))
                    self.report['after_host_policy'] = {'grants': grant_view(final_host), 'rust_profiles': rust_view(final_host),
                                                        'non_language_policy_sha256': object_sha(config_projection(final_host))}
                    for key, equal in (
                        ('prior_grants', self.report['after_host_policy']['grants'] == self.report['before']['grants']),
                        ('existing_rust_profiles', self.report['after_host_policy']['rust_profiles'] == self.report['before']['rust_profiles']),
                        ('prior_non_language_policy', self.report['after_host_policy']['non_language_policy_sha256'] == self.report['before']['non_language_policy_sha256']),
                        ('client_config_bytes', self.report['final_client_sha256'] == self.report['before']['client_sha256'])):
                        self.report['checks'].append({'name': 'final_snapshot_preserved:' + key, 'status': 'PASS' if equal else 'FAIL'})
                        if not equal and first_failure is None:
                            first_failure = {'type': 'AssertionError', 'message': 'Final snapshot changed: ' + key}
                except Exception as error:
                    self.report['after_snapshot_failure'] = {'type': type(error).__name__, 'message': str(error)}
                    first_failure = first_failure or self.report['after_snapshot_failure']
            self.report['status'] = 'FAIL' if first_failure else 'PASS'
            if first_failure is not None:
                self.report.setdefault('first_failure', first_failure)
            self.report['completed_at_utc'] = datetime.now(timezone.utc).isoformat()
            self.report['actual_user_installation'] = 'NOT_RUN' if not any(row['name'] == 'actual_user_installer' for row in self.report['commands']) else 'EXECUTED'
            self.save()
        print(json.dumps({'status': self.report['status'], 'report': str(self.output / 'report.json'),
                          'actual_user_installation': self.report['actual_user_installation'],
                          'semantic_acceptance': 'NOT_RUN'}))
        return 1 if first_failure else 0


def inert_checks():
    checked = []
    def test(name, condition):
        require(condition, name)
        checked.append(name)
    def reject(name, operation):
        try:
            operation()
        except (AssertionError, KeyError, ValueError, IndexError):
            checked.append(name)
        else:
            raise AssertionError('Expected guard rejection: ' + name)
    profiles = [{'id': name, 'program': name + '.exe', 'args': []} for name in LANGUAGES]
    for left, right in (('javascript', 'typescript'), ('c', 'cpp')):
        next(p for p in profiles if p['id'] == right)['program'] = left + '.exe'
    before = {'state_dir': 'state', 'workspace': {'allowed_roots': [str(BASE)], 'languages': profiles},
              'projects': {'fixture': {'root': str(BASE / 'registered'), 'allow_write': False, 'allow_shell': False, 'allow_debug': False,
                                       'allow_inspections': False, 'allow_formatters': False, 'languages': copy.deepcopy(profiles)}}}
    after = copy.deepcopy(before)
    home = BASE / 'inert-home'
    after['workspace'].update(default_root=str(home), allowed_roots=[str(BASE), str(home)], protected_roots=[str(BASE / 'private')])
    test('actual_14_profiles_12_invocations', profile_summary(profiles)['backend_invocation_count'] == 12)
    native_current = {'languages': [{'id': p['id'], 'extensions': p.get('extensions', [])} for p in profiles],
                      **{key: False for key in ('allow_write', 'allow_shell', 'allow_debug', 'allow_inspections', 'allow_formatters')}}
    test('actual_native_id_extension_shape_has_no_program_field', native_profile_ids(native_current, profiles)['primary_profile_count'] == 14)
    bad = copy.deepcopy(native_current); bad['allow_formatters'] = True
    reject('native_formatter_grant_change_rejected', lambda: native_profile_ids(bad, profiles))
    test('explicit_home_migration_preserves_prior_policy', len(verify_host_change(before, after, home, BASE)) == 2)
    bad = copy.deepcopy(after); bad['projects']['fixture']['allow_write'] = True
    reject('grant_change_rejected', lambda: verify_host_change(before, bad, home, BASE))
    bad = copy.deepcopy(after); bad['workspace']['languages'][-1]['program'] = 'new-rust.exe'
    reject('custom_rust_change_rejected', lambda: verify_host_change(before, bad, home, BASE))
    bad = copy.deepcopy(after); bad['workspace']['allowed_roots'].append(str(BASE / 'extra'))
    reject('unreviewed_allowed_root_rejected', lambda: verify_host_change(before, bad, home, BASE))
    bad = copy.deepcopy(after); bad['workspace']['default_root'] = str(BASE / 'wrong')
    reject('wrong_default_root_rejected', lambda: verify_host_change(before, bad, home, BASE))
    reject('registry_or_extra_profiles_not_counted_as_primary', lambda: profile_summary(profiles + [{'id': 'extra', 'program': 'extra.exe'}]))
    bad = copy.deepcopy(profiles); bad[5]['args'] = ['extra']
    reject('thirteenth_backend_invocation_rejected', lambda: profile_summary(bad))
    command = installer_command(BASE / 'inert.exe', BASE / 'app', home, home / 'host.toml', BASE / 'receipt.json', BASE / 'log.txt')
    test('regular_per_user_install_no_qa_replace_or_force', not any(v.startswith('/ODIN_QA') or v.startswith('/ODIN_REPLACE') or v == '/FORCECLOSEAPPLICATIONS' for v in command))
    test('restart_manager_close_without_stdio_autorestart', '/CLOSEAPPLICATIONS' in command and '/NORESTARTAPPLICATIONS' in command and '/NOFORCECLOSEAPPLICATIONS' in command)
    host = BASE / 'inert-host.toml'
    receipt = {'result': {'status': 'PASS', 'before_sha256': 'a' * 64, 'after_sha256': 'b' * 64,
                          'host_config': str(host), 'bundle_manifest_sha256': 'c' * 64,
                          'selected_backends': DEFAULTS, 'languages': list(LANGUAGES)},
               'plan': {'status': 'READY', 'before_sha256': 'a' * 64, 'after_sha256': 'b' * 64,
                        'host_config': str(host), 'bundle_manifest_sha256': 'c' * 64, 'selected_backends': DEFAULTS},
               'health': {'status': 'PASS', 'post_health_integrity': {'status': 'PASS'},
                          'checks': [{'component': name, 'status': 'PASS', 'exit_code': 0} for name in PROBES]}}
    validate = lambda value: verify_receipt(value, host, 'a' * 64, 'b' * 64, 'c' * 64)
    test('canonical_19_actual_probe_receipt', validate(receipt)['version_probe_count'] == 19)
    bad = copy.deepcopy(receipt); bad['health']['checks'][-1]['component'] = 'vtsls'
    reject('noncanonical_vtsls_probe_rejected', lambda: validate(bad))
    bad = copy.deepcopy(receipt); bad['health']['checks'].append({'component': 'smol-toml', 'status': 'PASS', 'exit_code': 0})
    reject('parser_data_cannot_inflate_executable_probes', lambda: validate(bad))
    bad = copy.deepcopy(receipt); bad['health']['checks'][0]['exit_code'] = 1
    reject('probe_exit_failure_rejected', lambda: validate(bad))
    bad = copy.deepcopy(receipt); bad['result']['before_sha256'] = 'd' * 64
    reject('worker_snapshot_drift_rejected', lambda: validate(bad))
    bad = copy.deepcopy(receipt); bad['plan']['status'] = 'CONFLICT'
    reject('worker_conflict_never_overridden', lambda: validate(bad))
    value = {'projects': [{'id': 'ws_inert', 'root': str(home), 'allow_write': False, 'allow_shell': False, 'allow_debug': False, 'allow_inspections': False}]}
    test('real_tool_shape_transient_readonly_binding', project_binding(value, home, set()) == 'ws_inert')
    reject('registered_id_cannot_satisfy_transient_claim', lambda: project_binding(value, home, {'ws_inert'}))
    reject('wrong_root_cannot_satisfy_binding', lambda: project_binding(value, BASE, set()))
    reject('mcp_tool_error_never_passes', lambda: decode_tool({'result': {'isError': True, 'structuredContent': value}}))
    test('mcp_structured_result_mapping', decode_tool({'result': {'structuredContent': value}}) == value)
    plain = 'C:\\Users\\steven\\AppData\\Local\\Programs\\Odin\\odin.exe'
    extended = '\\\\?\\' + plain
    test('actual_extended_drive_client_program_matches_plain', path_identity(extended) == path_identity(plain))
    test('extended_drive_root_matches_plain', path_identity('\\\\?\\C:\\') == path_identity('C:\\'))
    test('different_drive_stays_different', path_identity('\\\\?\\D:\\Odin\\odin.exe') != path_identity('C:\\Odin\\odin.exe'))
    test('different_program_stays_different', path_identity(extended) != path_identity(plain + '.other'))
    actual_host = 'C:\\Users\\steven\\.config\\odin\\host.toml'
    client_fixture = {'mcp_servers': {'odin': {'command': extended,
                       'args': ['--config', '\\\\?\\' + actual_host, '--workspace', '.', 'serve', '--transport', 'stdio'],
                       'startup_timeout_sec': 120}}}
    test('actual_extended_drive_codex_entry_passes_without_edit', verify_client(client_fixture, plain, actual_host) is client_fixture['mcp_servers']['odin'])
    for name, malformed in (
            ('extended_drive_relative_rejected', '\\\\?\\C:relative'),
            ('extended_unc_namespace_not_stripped', '\\\\?\\UNC\\server\\share\\odin.exe'),
            ('device_namespace_not_stripped', '\\\\.\\C:\\Odin\\odin.exe'),
            ('globalroot_namespace_not_stripped', '\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1\\odin.exe'),
            ('extended_dotdot_not_aliased', '\\\\?\\C:\\Odin\\..\\odin.exe'),
            ('extended_trailing_dot_not_aliased', '\\\\?\\C:\\Odin.\\odin.exe'),
            ('extended_dos_device_not_aliased', '\\\\?\\C:\\Odin\\NUL')):
        reject(name, lambda malformed=malformed: path_identity(malformed))
    print(json.dumps({'status': 'PASS', 'scope': 'INERT_EXTERNAL_RUNNER_GUARDS_ONLY', 'checks': checked,
                      'actual_user_installation': 'NOT_RUN', 'native_cli_mcp_execution': 'NOT_RUN'}, indent=2))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--check-runner', action='store_true')
    modes.add_argument('--execute', action='store_true', help='REAL installation. Only after separate root authorization.')
    parser.add_argument('--installer', default='C:/OdinQA/pkg7/odin-0.2.0-windows-x64-setup.exe')
    parser.add_argument('--expected-host-sha256')
    parser.add_argument('--expected-client-sha256')
    parser.add_argument('--workspace-a')
    parser.add_argument('--workspace-b')
    args = parser.parse_args()
    if args.check_runner:
        return inert_checks()
    for name in ('expected_host_sha256', 'expected_client_sha256', 'workspace_a', 'workspace_b'):
        if not getattr(args, name):
            parser.error('--execute requires --' + name.replace('_', '-'))
    return Runner(args).run()


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    raise SystemExit(main())
