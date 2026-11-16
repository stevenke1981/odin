#!/usr/bin/env python3
"""Client setup and MCP auto-load smoke against a real compiled Odin binary.

Covers `odin init`, `odin project add` (CAS dry-run/apply/conflict/duplicate), `odin setup --client X
--project-from-cwd` for every supported client (dry-run, apply into a pre-existing file with an unrelated
server, idempotent re-apply), and launches each written stdio entry from a cwd inside the project.
Does NOT run the real Claude, Codex, OpenCode, agy, Gemini, Cursor or VS Code applications, so the report
always carries actual_client_applications_tested=false. Missing binary is NOT_RUN + failure exit.
"""
from pathlib import Path
import argparse, hashlib, json, os, platform, subprocess, sys, tempfile, time, tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tests'))
from protocol_harness import Peer

TIMEOUT = 30
PROTOCOL = '2025-11-25'
SESSION_LINE = 'Session default project: demo (selected by cwd)'
# client -> (container key of the odin entry, file extension of the client config)
CLIENTS = {
    'codex': ('mcp_servers', 'toml'),
    'claude': ('mcpServers', 'json'),
    'opencode': ('mcp', 'json'),
    'agy': ('mcpServers', 'json'),
    'gemini': ('mcpServers', 'json'),
    'cursor': ('mcpServers', 'json'),
    'vscode': ('servers', 'json'),
}
REGISTRATION = {'codex': 'codex', 'claude': 'claude', 'agy': 'agy'}  # clients with a native registration CLI
TYPED = {'claude', 'vscode'}  # entries carry "type": "stdio"


def sha256_file(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def same_file(left, right):
    try:
        return os.path.samefile(left, right)
    except (OSError, TypeError):
        return False


def odin(binary, config, *command, cwd):
    return subprocess.run([binary, '--config', str(config), *command], cwd=str(cwd), capture_output=True,
                          text=True, encoding='utf-8', errors='replace', timeout=TIMEOUT)


def unrelated_entry(client):
    if client == 'opencode':
        return {'type': 'local', 'command': ['echo', 'keep'], 'enabled': True}
    entry = {'command': 'echo', 'args': ['keep']}
    if client in TYPED:
        entry['type'] = 'stdio'
    return entry


def seed_text(client):
    """Pre-existing client config holding an unrelated server that the apply must preserve."""
    if client == 'codex':
        return 'model = "unchanged"\n\n[mcp_servers.unrelated]\ncommand = "echo"\nargs = ["keep"]\n'
    container, _ = CLIENTS[client]
    document = {'unrelated': {'keep': True}, container: {'unrelated-server': unrelated_entry(client)}}
    return json.dumps(document, indent=2) + '\n'


def unrelated_preserved(client, document):
    if client == 'codex':
        return document.get('model') == 'unchanged' and document.get('mcp_servers', {}).get('unrelated') == {'command': 'echo', 'args': ['keep']}
    container, _ = CLIENTS[client]
    return document.get('unrelated') == {'keep': True} and document.get(container, {}).get('unrelated-server') == unrelated_entry(client)


def launch_argv(client, entry):
    """The argv the client would spawn, taken only from the written entry."""
    if client == 'opencode':
        command = entry.get('command')
        return command if isinstance(command, list) and command else []
    if isinstance(entry.get('command'), str) and isinstance(entry.get('args'), list):
        return [entry['command'], *entry['args']]
    return []


def entry_shape_ok(client, entry, argv, binary):
    if not argv or not same_file(argv[0], binary):
        return False
    if client == 'opencode' and (entry.get('type') != 'local' or entry.get('enabled') is not True):
        return False
    if client in TYPED and entry.get('type') != 'stdio':
        return False
    transport = any(argv[i:i+2] == ['--transport', 'stdio'] for i in range(len(argv) - 1))
    return '--config' in argv and 'serve' in argv and transport and '--project-from-cwd' in argv


def registration_ok(client, command, binary):
    if client not in REGISTRATION:
        return command is None
    if not isinstance(command, list) or '--' not in command:
        return False
    index = command.index('--')
    return (command[0] == REGISTRATION[client] and index + 1 < len(command)
            and same_file(command[index + 1], binary) and '--project-from-cwd' in command[index + 2:])


def initialize(peer):
    response = peer.request('initialize', {'protocolVersion': PROTOCOL, 'capabilities': {},
                                           'clientInfo': {'name': 'client-setup-smoke', 'version': '1'}}, timeout=TIMEOUT)
    peer.notify('notifications/initialized')
    return response.get('result', {})


def finish(peer):
    """Close stdin (EOF) and return the exit code; kill the server if it does not exit in time."""
    peer.process.stdin.close()
    try:
        return peer.process.wait(timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        peer.process.kill()
        return None


class Fixture:
    def __init__(self, binary, base):
        self.binary = binary
        self.base = base
        self.ws = base/'ws'
        self.ws2 = base/'ws2'
        self.config = base/'host.toml'
        self.clients = base/'clients'


class Recorder:
    def __init__(self):
        self.checks = []
        self.messages = []

    def check(self, name, condition):
        self.checks.append({'name': name, 'status': 'PASS' if condition else 'FAIL'})
        return bool(condition)

    def require(self, name, condition):
        if not self.check(name, condition):
            raise AssertionError(name)

    def guard(self, name, step, *args):
        try:
            step(self, *args)
        except AssertionError as exc:
            self.messages.append(f'{name}: check {exc} failed')
        except Exception as exc:
            self.checks.append({'name': f'{name}:exception', 'status': 'FAIL'})
            self.messages.append(f'{name}: {type(exc).__name__}: {exc}')


def host_config(rec, fx):
    for root in (fx.ws, fx.ws2):
        (root/'src').mkdir(parents=True)
        (root/'Cargo.toml').write_text('[package]\nname = "fixture"\nversion = "0.1.0"\nedition = "2021"\n', encoding='utf-8')
        (root/'src'/'lib.rs').write_text('pub fn fixture() {}\n', encoding='utf-8')
    init = odin(fx.binary, fx.config, 'init', '--root', str(fx.ws), '--name', 'demo', cwd=fx.base)
    rec.require('init_exit', init.returncode == 0)
    rec.require('init_creates_config', fx.config.is_file())

    before = fx.config.read_bytes()
    add = ['project', 'add', '--root', str(fx.ws2), '--name', 'second', '--detect-languages']
    dry = odin(fx.binary, fx.config, *add, cwd=fx.base)
    rec.require('project_add_dry_run_exit', dry.returncode == 0)
    preview = json.loads(dry.stdout)
    rec.check('project_add_dry_run_unchanged', fx.config.read_bytes() == before and preview.get('operation') == 'DRY_RUN')
    rec.check('project_add_languages_not_probed', preview.get('languages_installed') == 'NOT_PROBED')
    expected = preview.get('expected_sha256')
    rec.check('project_add_dry_run_expected_sha256', isinstance(expected, str) and len(expected) == 64)
    expected = expected if isinstance(expected, str) else sha256_file(fx.config)

    conflict = odin(fx.binary, fx.config, *add, '--apply', '--expected-sha256', '0'*64, cwd=fx.base)
    rec.check('project_add_conflict_refused', conflict.returncode != 0 and fx.config.read_bytes() == before)

    applied = odin(fx.binary, fx.config, *add, '--apply', '--expected-sha256', expected, cwd=fx.base)
    rec.require('project_add_apply_exit', applied.returncode == 0)
    backup = json.loads(applied.stdout).get('backup')
    rec.check('project_add_backup', bool(backup) and Path(backup).read_bytes() == before)
    after = fx.config.read_bytes()
    document = tomllib.loads(after.decode('utf-8'))
    projects = document.get('projects', {})
    project = projects.get('second', {})
    rec.check('project_add_present', same_file(project.get('root'), fx.ws2))
    rec.check('project_add_demo_kept', 'demo' in projects)
    rec.check('project_add_default_permissions', not any(project.get(k) for k in ('allow_write', 'allow_shell', 'allow_debug')) and not project.get('commands'))
    rec.check('project_add_detects_rust', any(lang.get('id') == 'rust' for lang in project.get('languages', [])))

    duplicate = odin(fx.binary, fx.config, *add, cwd=fx.base)
    rec.check('project_add_duplicate_dry_run_refused', duplicate.returncode != 0 and fx.config.read_bytes() == after)
    duplicate = odin(fx.binary, fx.config, *add, '--apply', '--expected-sha256', sha256_file(fx.config), cwd=fx.base)
    rec.check('project_add_duplicate_apply_refused', duplicate.returncode != 0 and fx.config.read_bytes() == after)


def client_setup(rec, fx, client):
    container, extension = CLIENTS[client]
    output = fx.clients/f'{client}.{extension}'
    output.write_text(seed_text(client), encoding='utf-8')
    original = output.read_bytes()
    setup = ['setup', '--client', client, '--project-from-cwd', '--output', str(output)]

    preview = odin(fx.binary, fx.config, *setup, cwd=fx.base)
    rec.require(f'setup_dry_run_exit:{client}', preview.returncode == 0)
    data = json.loads(preview.stdout)
    rec.check(f'setup_dry_run:{client}', data.get('operation') == 'DRY_RUN' and output.read_bytes() == original)
    rec.check(f'setup_metadata:{client}', data.get('client') == client and isinstance(data.get('default_outputs'), list) and len(data['default_outputs']) > 0)
    rec.check(f'setup_registration_command:{client}', registration_ok(client, data.get('registration_command'), fx.binary))
    rec.check(f'setup_auto_project:{client}', (data.get('auto_project') or {}).get('project_from_cwd') is True)

    applied = odin(fx.binary, fx.config, *setup, '--apply', '--expected-sha256', sha256_file(output), cwd=fx.base)
    rec.require(f'setup_apply_exit:{client}', applied.returncode == 0)
    backup = json.loads(applied.stdout).get('backup')
    rec.check(f'setup_backup:{client}', bool(backup) and Path(backup).read_bytes() == original)

    text = output.read_text(encoding='utf-8')
    document = tomllib.loads(text) if extension == 'toml' else json.loads(text)
    rec.check(f'unrelated_preserved:{client}', unrelated_preserved(client, document))
    entry = (document.get(container) or {}).get('odin') or {}
    argv = launch_argv(client, entry)
    rec.check(f'odin_entry_shape:{client}', entry_shape_ok(client, entry, argv, fx.binary))

    again = odin(fx.binary, fx.config, *setup, '--apply', '--expected-sha256', sha256_file(output), cwd=fx.base)
    rec.check(f'setup_idempotent:{client}', again.returncode == 0 and json.loads(again.stdout).get('operation') == 'UNCHANGED')

    with Peer(argv, cwd=str(fx.ws/'src')) as peer:
        result = initialize(peer)
        rec.check(f'launch_initialize:{client}', result.get('serverInfo', {}).get('name') == 'odin')
        rec.check(f'launch_session_default:{client}', SESSION_LINE in result.get('instructions', ''))
        tools = peer.request('tools/list', timeout=TIMEOUT).get('result', {}).get('tools', [])
        rec.check(f'launch_tools_list:{client}', len(tools) > 0)
        called = peer.request('tools/call', {'name': 'status', 'arguments': {'project': 'demo'}}, timeout=TIMEOUT)
        rec.check(f'launch_status_call:{client}', 'result' in called and not called['result'].get('isError', False))
        rec.check(f'launch_exit_after_eof:{client}', finish(peer) == 0)


def unscoped_launch(rec, fx):
    """Launched outside every root: must initialize without selecting a session default."""
    argv = [fx.binary, '--config', str(fx.config), 'serve', '--transport', 'stdio', '--project-from-cwd']
    with Peer(argv, cwd=str(fx.base)) as peer:
        result = initialize(peer)
        rec.check('unscoped_initialize', result.get('serverInfo', {}).get('name') == 'odin')
        rec.check('unscoped_no_session_default', 'Session default project' not in result.get('instructions', ''))
        rec.check('unscoped_exit_after_eof', finish(peer) == 0)


def serve_defaults(rec, fx):
    refused = subprocess.run([fx.binary, '--config', str(fx.config), 'serve', '--transport', 'stdio', '--default-project', 'nope'],
                             cwd=str(fx.base), stdin=subprocess.DEVNULL, capture_output=True, timeout=TIMEOUT)
    rec.check('serve_unknown_default_project_refused', refused.returncode != 0)
    setup_refused = odin(fx.binary, fx.config, 'setup', '--client', 'claude', '--default-project', 'nope',
                         '--output', str(fx.clients/'claude.json'), cwd=fx.base)
    rec.check('setup_unknown_default_project_refused', setup_refused.returncode != 0)
    argv = [fx.binary, '--config', str(fx.config), 'serve', '--transport', 'stdio', '--default-project', 'second']
    with Peer(argv, cwd=str(fx.base)) as peer:
        result = initialize(peer)
        rec.check('serve_default_project_selected', 'Session default project: second (selected by default-project)' in result.get('instructions', ''))
        rec.check('serve_default_project_exit_after_eof', finish(peer) == 0)


def main():
    parser = argparse.ArgumentParser(description='Client setup and MCP auto-load smoke for a real Odin binary (stdlib only).')
    parser.add_argument('--binary', type=Path, required=True, help='compiled odin executable')
    parser.add_argument('--output', type=Path, default=Path(tempfile.gettempdir())/'odin-client-setup-smoke.json',
                        help='JSON report path (default: system temp dir); a relative path resolves from the repository root, '
                             'e.g. verification/client-setup-smoke.json')
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else ROOT/args.output
    rec = Recorder()
    status = 'NOT_RUN'
    try:
        if not args.binary.is_file():
            raise FileNotFoundError('Build the real Odin binary before running client setup acceptance')
        status = 'FAIL'
        binary = str(args.binary.resolve())
        with tempfile.TemporaryDirectory(prefix='odin-client-setup-中文 space-', ignore_cleanup_errors=True) as directory:
            fx = Fixture(binary, Path(directory))
            fx.clients.mkdir()
            rec.guard('host_config', host_config, fx)
            for client in CLIENTS:
                rec.guard(f'client:{client}', client_setup, fx, client)
            rec.guard('unscoped_launch', unscoped_launch, fx)
            rec.guard('serve_defaults', serve_defaults, fx)
        status = 'PASS' if rec.checks and all(c['status'] == 'PASS' for c in rec.checks) else 'FAIL'
    except Exception as exc:
        rec.messages.append(f'{type(exc).__name__}: {exc}')
    report = {'scope': 'REAL_ODIN_BINARY_CLIENT_SETUP_AND_MCP_LAUNCH', 'status': status,
              'binary': str(args.binary.resolve()), 'binary_sha256': sha256_file(args.binary) if args.binary.is_file() else None,
              'platform': platform.platform(), 'checks': rec.checks, 'error': '; '.join(rec.messages) or None,
              'actual_client_applications_tested': False, 'timestamp_unix': time.time()}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report))
    return 0 if status == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
