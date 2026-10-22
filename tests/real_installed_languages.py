"""Real installed LSP acceptance through Odin MCP, using independent source projects.

This is a development driver. Repository fixtures are never host configuration.
Every output path must be fresh; optional backend features are reported separately.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tomllib

from protocol_harness import Peer
from real_ide_backends import toml, position

LANGUAGES = ('python', 'go', 'csharp', 'lua', 'javascript', 'typescript', 'bash')


def local_location(value, relative_path, text, token, first=False):
    index = text.index(token) if first else text.rindex(token)
    prefix = text[:index]
    at = (prefix.count('\n'), len(prefix.rsplit('\n', 1)[-1].encode('utf-16-le')) // 2)
    for item in value.get('locations', []):
        if item.get('external') is not False or item.get('relative_path') != relative_path:
            continue
        location = item.get('location', {})
        span = location.get('targetSelectionRange') or location.get('range') or {}
        start, end = span.get('start', {}), span.get('end', {})
        if all(k in start and k in end for k in ('line', 'character')):
            if (start['line'], start['character']) <= at < (end['line'], end['character']):
                return True
    return False


def digest(data):
    return hashlib.sha256(data).hexdigest()


def user_path_snapshot():
    if os.name != 'nt':
        return {'available': False}
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as key:
        try:
            value, kind = winreg.QueryValueEx(key, 'Path')
        except FileNotFoundError:
            value, kind = None, None
    return {'available': True, 'value': value, 'registry_type': kind}


def fixtures(base, language, tag):
    root = base / (('\u5c08\u6848 ' if tag == 'a' else '') + language + '_' + tag)
    root.mkdir(parents=True)
    extra = {}
    if language == 'python':
        source, consumer = 'sample.py', 'consumer.py'
        text = 'def greet(name: str) -> str:\n    return f"Hi {name}"\n\ndef run() -> str:\n    message = greet("Odin")\n    return message\n'
        caller = 'from sample import greet\nmessage = greet("Caller")\n'
        broken = '\ndef odin_broken(:\n'
    elif language == 'go':
        source, consumer = 'sample.go', 'consumer.go'
        text = 'package fixture\nfunc greet(name string) string{return "Hi "+name}\nfunc run() string{message:=greet("Odin");return message}\n'
        caller = 'package fixture\nfunc consume() string{return greet("Caller")}\n'
        broken = '\nfunc odin_broken( {\n'
        extra['go.mod'] = 'module odin.local/installed-' + tag + '\n\ngo 1.27.0\n'
    elif language == 'csharp':
        source, consumer = 'Greeter.cs', 'Consumer.cs'
        text = 'namespace Fixture;\npublic static class Greeter {\npublic static string greet(string name){return "Hi "+name;}\npublic static string run(){var message=greet("Odin");return message;}\n}\n'
        caller = 'namespace Fixture;\npublic static class Consumer {public static string consume(){return Greeter.greet("Caller");}}\n'
        broken = '\nclass Broken { void odin_broken( }\n'
        extra['Fixture.csproj'] = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable></PropertyGroup></Project>\n'
        extra['NuGet.Config'] = '<configuration><packageSources><clear/></packageSources></configuration>\n'
    elif language == 'lua':
        source, consumer = 'sample.lua', 'consumer.lua'
        text = 'function greet(name)\n    return "Hi " .. name\nend\n\nfunction run()\n    local message = greet("Odin")\n    return message\nend\n'
        caller = 'local message = greet("Caller")\n'
        broken = '\nfunction odin_broken(\n'
    elif language in ('javascript', 'typescript'):
        ts = language == 'typescript'
        source, consumer = ('sample.ts', 'consumer.ts') if ts else ('sample.js', 'consumer.js')
        text = ('export function greet(name: string): string{return `Hi ${name}`;}\nexport function run(): string{const message=greet("Odin");return message;}\n' if ts else '/** @param {string} name @returns {string} */\nexport function greet(name){return `Hi ${name}`;}\nexport function run(){const message=greet("Odin");return message;}\n')
        caller = 'import { greet } from "./sample' + ('' if ts else '.js') + '";\nexport const message=greet("Caller");\n'
        broken = '\nexport function odin_broken( {\n'
        extra['tsconfig.json' if ts else 'jsconfig.json'] = json.dumps({'compilerOptions': {'strict': True, 'noEmit': True, **({} if ts else {'allowJs': True, 'checkJs': True})}, 'include': ['*.ts' if ts else '*.js']})
    else:
        source, consumer = 'sample.sh', 'consumer.sh'
        text = 'greet() {\n    printf "Hi %s\\n" "$1"\n}\n\nrun() {\n    greet "Odin"\n}\n'
        caller = 'source ./sample.sh\ngreet "Caller"\n'
        broken = '\nodin_broken() {\n'
    for path, content in {source: text, consumer: caller, **extra}.items():
        (root / path).write_text(content, encoding='utf-8', newline='\n')
    (root / 'sentinel.txt').write_text(language + '_' + tag, encoding='utf-8')
    return {'root': root, 'language': language, 'tag': tag, 'source': source, 'consumer': consumer, 'text': text, 'broken': broken, 'call_needle': 'greet "Odin"' if language == 'bash' else 'greet("Odin")'}


def hashes(fixture):
    return {name: digest((fixture['root'] / name).read_bytes()) for name in (fixture['source'], fixture['consumer'])}


class Driver:
    def __init__(self, args):
        self.args = args
        self.report = {'scope': 'INSTALLED_REAL_LSP_MCP_TWO_PROJECTS_PER_LANGUAGE', 'status': 'NOT_RUN', 'binary': str(args.binary), 'binary_sha256': digest(args.binary.read_bytes()), 'host_config_sha256': digest(args.host_config.read_bytes()), 'languages': [], 'transcript': []}
        root = Path(__file__).resolve().parents[1]
        sources = sorted((root / 'crates').rglob('*.rs')) + sorted((root / 'crates').rglob('Cargo.toml')) + [root / 'Cargo.toml', root / 'Cargo.lock']
        self.report['runtime_source_sha256'] = {p.relative_to(root).as_posix(): digest(p.read_bytes()) for p in sources}
        self.report['driver_sha256'] = digest(Path(__file__).read_bytes())
        self.report['setup_script_sha256'] = digest((args.install_root / 'setup-lsps.mjs').read_bytes())
        self.report['bundle_manifest_sha256'] = digest((args.install_root / 'backends/manifest.json').read_bytes())
        self.report['client_entries'] = []
        self.current = None

    def record(self, name, ok, detail=None, optional=False):
        status = ok if isinstance(ok, str) else ('PASS' if ok else 'FAIL')
        self.current['checks'].append({'name': name, 'status': status, 'optional': optional, 'detail': detail})
        if status == 'FAIL' and not optional:
            raise AssertionError(name + ': ' + str(detail))

    def call(self, peer, name, arguments=None, error=False):
        response = peer.request('tools/call', {'name': name, 'arguments': arguments or {}}, timeout=self.args.timeout)
        result = response.get('result', {})
        value = result.get('structuredContent')
        if value is None and result.get('content'):
            value = json.loads(result['content'][0]['text'])
        self.report['transcript'].append({'language': self.current['language'], 'tool': name, 'arguments': arguments or {}, 'response': response})
        if 'error' in response or bool(result.get('isError')) != error:
            raise RuntimeError(json.dumps(response))
        if isinstance(value, dict) and value.get('result_id') and value.get('truncated'):
            chunks, offset = [], 0
            while True:
                chunk = self.call(peer, 'result_read', {'result_id': value['result_id'], 'offset': offset, 'max_bytes': 32768})
                assert digest(chunk['text'].encode('utf-8')) == chunk['chunk_sha256']
                chunks.append(chunk['text']); offset = chunk['next_offset']
                if offset is None:
                    break
            raw = ''.join(chunks).encode('utf-8')
            assert digest(raw) == value['sha256']
            value = json.loads(raw)
        return value

    def start(self, config, root):
        command = [str(self.args.binary), '--config', str(config), '--workspace', '.', 'serve', '--transport', 'stdio']
        self.report['client_entries'].append({'command': command, 'cwd': str(root)})
        peer = Peer(command, cwd=root, env=self.environment)
        result = peer.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'odin-installed-lsp', 'version': '1'}}, timeout=30)
        assert result.get('result', {}).get('serverInfo', {}).get('name') == 'odin', result
        peer.notify('notifications/initialized')
        return peer

    def retry(self, action, predicate, seconds=45):
        end, last = time.monotonic() + seconds, None
        while True:
            last = action()
            if predicate(last) or time.monotonic() >= end:
                return last
            time.sleep(0.25)

    def overview(self, peer, fixture):
        value = self.retry(lambda: self.call(peer, 'get_symbols_overview', {'relative_path': fixture['source']}), lambda v: any(s['name'] == 'greet' or s['name'].startswith('greet(') for s in v.get('symbols', [])))
        self.record(fixture['tag'] + '_real_symbols', value.get('backend') == 'lsp' and any(s['name'] == 'greet' or s['name'].startswith('greet(') for s in value.get('symbols', [])), value)
        return next(s['name_path'] for s in value['symbols'] if s['name'] == 'greet' or s['name'].startswith('greet('))

    def diagnostic(self, peer, fixture, name, expect_error):
        bash = next((p for p in self.profiles if p['id'] == 'bash'), {})
        if (fixture['language'] == 'bash'
                and bash.get('settings', {}).get('bashIde', {}).get('shellcheckPath') == ''
                and not (self.args.install_root / 'backends/shellcheck/shellcheck.exe').exists()):
            observed = self.call(peer, 'get_diagnostics_for_file', {'relative_path': fixture['source']})
            self.record(name, 'UNAVAILABLE', {'reason': 'ShellCheck is explicitly disabled and absent from this bundle. BashLS source-resolution diagnostics do not detect the missing-brace fixture; an empty push is not syntax validation.', 'observed': observed}, optional=True)
            return
        value = self.retry(lambda: self.call(peer, 'get_diagnostics_for_file', {'relative_path': fixture['source']}), lambda v: v.get('available') is True and v.get('version_verified') is True and v.get('summary', {}).get('complete') is True and (v.get('summary', {}).get('errors', 0) > 0) == expect_error, seconds=12)
        complete = value.get('available') is True and value.get('version_verified') is True and value.get('summary', {}).get('complete') is True
        self.record(name, ('PASS' if (value.get('summary', {}).get('errors', 0) > 0) == expect_error else 'FAIL') if complete else 'NOT_VERIFIED', value, optional=not complete)

    def edit(self, peer, fixture, text):
        prepared = self.call(peer, 'transaction_prepare', {'changes': [{'path': fixture['source'], 'expected_sha256': digest((fixture['root'] / fixture['source']).read_bytes()), 'content': text}]})
        self.call(peer, 'transaction_preview', {'transaction_id': prepared['transaction_id']})
        self.call(peer, 'transaction_apply', {'transaction_id': prepared['transaction_id']})
        return prepared['transaction_id']

    def test(self, language, profiles, base):
        self.current = {'language': language, 'status': 'NOT_RUN', 'checks': [], 'stderr': []}
        self.report['languages'].append(self.current)
        a, b = fixtures(base, language, 'a'), fixtures(base, language, 'b')
        all_before = [hashes(a), hashes(b)]
        state = base / (language + '_state')
        config = base / (language + '-readonly.toml')
        config.write_text('state_dir = ' + toml(str(state)) + '\n[limits]\nrpc_timeout_secs = 120\n[workspace]\nallowed_roots = ' + toml([str(a['root']), str(b['root'])]) + '\n' + ''.join('[[workspace.languages]]\n' + '\n'.join(k + ' = ' + toml(v) for k, v in profile.items()) + '\n' for profile in profiles), encoding='utf-8')
        if language == 'csharp':
            dotnet = self.args.install_root / 'backends/dotnet/dotnet.exe'
            env = {**self.environment, 'DOTNET_ROOT': str(dotnet.parent), 'DOTNET_CLI_HOME': str(state / 'dotnet'), 'NUGET_PACKAGES': str(state / 'nuget'), 'DOTNET_ADD_GLOBAL_TOOLS_TO_PATH': 'false', 'DOTNET_GENERATE_ASPNET_CERTIFICATE': 'false', 'DOTNET_CLI_TELEMETRY_OPTOUT': '1'}
            self.current['dotnet_first_use_controls'] = {key: env[key] for key in ('DOTNET_ADD_GLOBAL_TOOLS_TO_PATH', 'DOTNET_GENERATE_ASPNET_CERTIFICATE', 'DOTNET_CLI_TELEMETRY_OPTOUT')}
            for fixture in (a, b):
                result = subprocess.run([str(dotnet), 'restore', 'Fixture.csproj', '--configfile', 'NuGet.Config', '--ignore-failed-sources'], cwd=fixture['root'], env=env, capture_output=True, timeout=120)
                self.current.setdefault('restore', []).append({'exit_code': result.returncode, 'stdout': result.stdout.decode('utf-8', errors='replace'), 'stderr': result.stderr.decode('utf-8', errors='replace')})
                self.record('offline_restore_' + fixture['tag'], result.returncode == 0)
        try:
            ids = []
            # Identical client argv, distinct cwd. Both remain live to detect process-global binding.
            with self.start(config, a['root']) as first, self.start(config, b['root']) as second:
                for peer, fixture in ((first, a), (second, b)):
                    projects = self.call(peer, 'list_projects')['projects']
                    self.record(fixture['tag'] + '_one_root', len(projects) == 1 and os.path.samefile(projects[0]['root'], fixture['root']), projects)
                    sentinel = self.call(peer, 'read_file', {'relative_path': 'sentinel.txt'})
                    self.record(fixture['tag'] + '_unique_source_root', sentinel.get('content') == language + '_' + fixture['tag'], sentinel)
                    ids.append(projects[0]['id'])
                    analysis = self.call(peer, 'project_analysis')
                    self.record(fixture['tag'] + '_readonly_bound', not any(analysis['permissions'].values()) and language not in analysis['unconfigured_language_hints'], analysis)
                    cap = self.call(peer, 'get_capabilities', {'probe': True, 'language': language})
                    self.record(fixture['tag'] + '_actual_initialize', any(p['id'] == language and p.get('ready') is True for p in cap['languages']), cap)
                    self.overview(peer, fixture)
                    denied = self.call(peer, 'create_text_file', {'relative_path': 'forbidden.txt', 'content': 'denied', 'expected_sha256': None}, error=True)
                    self.record(fixture['tag'] + '_write_denied', 'PERMISSION' in json.dumps(denied) and not (fixture['root'] / 'forbidden.txt').exists(), denied)
                self.record('distinct_workspace_ids', ids[0] != ids[1], ids)
                wrong = self.call(second, 'read_file', {'project': ids[0], 'relative_path': a['source']}, error=True)
                self.record('cross_project_id_rejected', bool(wrong), wrong)
                self.current['stderr'].extend(first.errors + second.errors)
            self.record('readonly_sources_unchanged', [hashes(a), hashes(b)] == all_before)
            # Explicit test-only writable host. No production config/grants are changed.
            writable = base / (language + '-writable.toml')
            writable.write_text(config.read_text(encoding='utf-8') + '\n[projects.' + language + '_test]\nroot = ' + toml(str(a['root'])) + '\nallow_write = true\nallow_shell = false\nallow_debug = false\n' + ''.join('[[projects.' + language + '_test.languages]]\n' + '\n'.join(k + ' = ' + toml(v) for k, v in p.items()) + '\n' for p in profiles), encoding='utf-8')
            with self.start(writable, a['root']) as peer:
                name_path = self.overview(peer, a)
                self.call(peer, 'get_symbols_overview', {'relative_path': a['consumer']})
                consumer_text = (a['root'] / a['consumer']).read_text(encoding='utf-8')
                refs = self.retry(lambda: self.call(peer, 'find_referencing_symbols', {'relative_path': a['source'], 'name_path': name_path}), lambda v: local_location(v, a['consumer'], consumer_text, 'greet'))
                self.record('cross_file_references', local_location(refs, a['consumer'], consumer_text, 'greet'), refs)
                definition = self.call(peer, 'find_declaration', {'relative_path': a['source'], 'position': position(a['text'], a['call_needle'], 1)})
                self.record('definition_navigation', local_location(definition, a['source'], a['text'], 'greet', first=True), definition)
                completion = self.call(peer, 'completion', {'relative_path': a['source'], 'position': position(a['text'], a['call_needle'], 3), 'limit': 200})
                completed = completion.get('result')
                items = completed.get('items', []) if isinstance(completed, dict) else completed or []
                self.record('function_completion', completion.get('source') == 'lsp' and completion.get('freshness') == 'current' and completion.get('document', {}).get('sha256') == hashes(a)[a['source']] and any('greet' in item.get('label', '').lower() for item in items), completion)
                hover = self.call(peer, 'hover', {'relative_path': a['source'], 'position': position(a['text'], a['call_needle'], 1)})
                self.record('custom_function_hover', 'greet' in json.dumps(hover).lower(), hover)
                self.diagnostic(peer, a, 'diagnostics_valid', False)
                preview = self.call(peer, 'rename_symbol', {'relative_path': a['source'], 'name_path': name_path, 'new_name': 'salute'})
                tid = preview['transaction_id']
                self.call(peer, 'transaction_preview', {'transaction_id': tid})
                self.record('rename_preview_readonly', [hashes(a), hashes(b)] == all_before and bool(preview.get('files')), preview)
                # A distinct valid edit makes the immutable rename preview stale.
                concurrent = a['text'] + ('\n# independent edit\n' if language in ('python', 'bash') else '\n-- independent edit\n' if language == 'lua' else '\n// independent edit\n')
                changed = self.edit(peer, a, concurrent)
                conflict = self.call(peer, 'transaction_apply', {'transaction_id': tid}, error=True)
                self.record('rename_cas_conflict', 'CONFLICT' in json.dumps(conflict) and (a['root'] / a['source']).read_text(encoding='utf-8') == concurrent, conflict)
                self.call(peer, 'transaction_rollback', {'transaction_id': changed})
                self.record('concurrent_edit_rollback', [hashes(a), hashes(b)] == all_before)
                # After restoring the source, prepare a new user edit; never replay a failed apply.
                self.call(peer, 'get_symbols_overview', {'relative_path': a['source']})
                fresh = self.call(peer, 'rename_symbol', {'relative_path': a['source'], 'name_path': name_path, 'new_name': 'salute'})
                self.call(peer, 'transaction_preview', {'transaction_id': fresh['transaction_id']})
                self.call(peer, 'transaction_apply', {'transaction_id': fresh['transaction_id']})
                try:
                    self.record('rename_definition_and_consumer', all('salute' in (a['root'] / path).read_text(encoding='utf-8') and 'greet' not in (a['root'] / path).read_text(encoding='utf-8') for path in (a['source'], a['consumer'])))
                    self.record('other_project_unchanged', hashes(b) == all_before[1])
                finally:
                    self.call(peer, 'transaction_rollback', {'transaction_id': fresh['transaction_id']})
                self.record('rename_exact_rollback', [hashes(a), hashes(b)] == all_before)
                broken = self.edit(peer, a, a['text'] + a['broken'])
                try:
                    self.diagnostic(peer, a, 'diagnostics_syntax_error', True)
                finally:
                    self.call(peer, 'transaction_rollback', {'transaction_id': broken})
                self.diagnostic(peer, a, 'diagnostics_recovered', False)
                self.record('all_source_hashes_restored', [hashes(a), hashes(b)] == all_before)
                if language == 'bash':
                    self.record('shellcheck_shfmt_man', 'NOT_RUN', 'ShellCheck syntax/lint, shfmt formatting and man/help tools are not bundled; navigation acceptance only', optional=True)
                self.current['stderr'].extend(peer.errors)
        except Exception as exc:
            self.current['checks'].append({'name': 'execution', 'status': 'FAIL', 'optional': False, 'detail': str(exc)})
        required = [v for v in self.current['checks'] if not v.get('optional')]
        self.current['status'] = 'FAIL' if any(v['status'] == 'FAIL' for v in required) else 'PASS'
        self.current['optional_status'] = 'PARTIAL' if any(v['status'] != 'PASS' for v in self.current['checks'] if v.get('optional')) else 'PASS'
        print(json.dumps({'language': language, 'status': self.current['status'], 'optional_status': self.current['optional_status']}), flush=True)
        self.save()

    def save(self):
        self.args.output.write_text(json.dumps(self.report, indent=2, ensure_ascii=True) + '\n', encoding='utf-8')

    def run(self):
        self.report['persistent_user_path_before'] = user_path_snapshot()
        config = tomllib.loads(self.args.host_config.read_text(encoding='utf-8'))
        profiles = config['workspace']['languages']
        self.profiles = profiles
        base = self.args.output.with_suffix('.fixtures')
        base.mkdir()
        self.environment = dict(os.environ)
        self.environment['PATH'] = str(Path(os.environ['SystemRoot']) / 'System32') + os.pathsep + os.environ['SystemRoot']
        for key in ('GOROOT', 'GOPATH', 'GOCACHE', 'GOMODCACHE', 'GOTOOLCHAIN', 'DOTNET_ROOT', 'DOTNET_CLI_HOME', 'NUGET_PACKAGES', 'NODE_OPTIONS', 'NODE_PATH'):
            self.environment.pop(key, None)
        self.report['external_runtime_PATH'] = self.environment['PATH']
        for language in self.args.languages:
            try:
                self.test(language, profiles, base)
            except Exception as exc:
                self.current['checks'].append({'name': 'fixture_setup', 'status': 'FAIL', 'optional': False, 'detail': str(exc)})
                self.current['status'] = 'FAIL'
                self.current['optional_status'] = 'NOT_RUN'
                self.save()
        self.report['persistent_user_path_after'] = user_path_snapshot()
        path_unchanged = self.report['persistent_user_path_before'] == self.report['persistent_user_path_after']
        self.report['persistent_user_path_unchanged'] = 'PASS' if path_unchanged else 'FAIL'
        self.report['status'] = 'PASS' if path_unchanged and all(v['status'] == 'PASS' for v in self.report['languages']) else 'FAIL'
        self.report['optional_status'] = 'PARTIAL' if any(v['optional_status'] != 'PASS' for v in self.report['languages']) else 'PASS'
        self.report['completed_unix'] = time.time()
        self.save()
        return 0 if self.report['status'] == 'PASS' else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--host-config', type=Path, required=True)
    parser.add_argument('--install-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--languages', nargs='+', choices=LANGUAGES, default=LANGUAGES)
    args = parser.parse_args()
    for name in ('binary', 'host_config', 'install_root', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    if args.output.exists() or args.output.with_suffix('.fixtures').exists():
        parser.error('Choose a fresh output and fixture path; previous attempts are evidence')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    return Driver(args).run()


if __name__ == '__main__':
    raise SystemExit(main())
