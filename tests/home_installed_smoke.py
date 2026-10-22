"""Real installed synthetic-home default and Go/C#/Lua LSP smoke.

Never target the real OS home. Production host files and permissions are read
only; initial fixture creation uses fresh paths and every Odin write is denied.
"""
from __future__ import annotations
import argparse
import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tomllib

from protocol_harness import Peer
from real_installed_languages import Driver as BaseDriver, digest, user_path_snapshot


def real_os_homes():
    homes = {Path.home().resolve()}
    if os.name == 'nt':
        buffer = ctypes.create_unicode_buffer(32768)
        result = ctypes.windll.shell32.SHGetFolderPathW(None, 0x0028, None, 0, buffer)
        if result != 0:
            raise RuntimeError('Cannot identify real OS profile; synthetic-home test is refused')
        homes.add(Path(buffer.value).resolve())
    return homes


class Driver(BaseDriver):
    def __init__(self, args):
        super().__init__(args)
        self.report['scope'] = 'ACTUAL_INSTALLED_SYNTHETIC_HOME_DEFAULT_AND_FIXED_RUNNER_LSP'
        self.report['home_driver_sha256'] = digest(Path(__file__).read_bytes())
        self.report['real_os_home_rejection'] = 'PASS: checked before subprocess HOME/USERPROFILE overrides'
        self.current = {'language': 'synthetic-home', 'status': 'NOT_RUN', 'checks': [], 'stderr': []}
        self.report['languages'] = [self.current]

    def start_home(self, cwd, explicit_config=True, current_workspace=False):
        command = [str(self.args.binary)]
        if explicit_config:
            command += ['--config', str(self.args.host_config)]
        if current_workspace:
            command += ['--workspace', '.']
        command += ['serve', '--transport', 'stdio']
        self.report['client_entries'].append({'command': command, 'cwd': str(cwd)})
        peer = Peer(command, cwd=cwd, env=self.environment)
        result = peer.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'odin-synthetic-home-smoke', 'version': '1'}}, timeout=30)
        assert result.get('result', {}).get('serverInfo', {}).get('name') == 'odin', result
        peer.notify('notifications/initialized')
        return peer

    def make_fixture(self):
        name = 'odin-home-smoke-' + self.args.output.stem
        fixture = self.args.synthetic_home / name
        fixture.mkdir()
        for folder in ('go', 'lua', 'csharp', 'repo-a', 'repo-b'):
            (fixture / folder).mkdir()
        self.fixture = fixture
        self.sources = {
            'go': (name + '/go/home.go', 'package homesmoke\nfunc HomeGreet(name string) string { return "Hi " + name }\n'),
            'csharp': (name + '/csharp/Home.cs', 'namespace HomeSmoke;\npublic static class HomeGreeter { public static string HomeGreet(string name) { return "Hi " + name; } }\n'),
            'lua': (name + '/lua/home.lua', 'function HomeGreet(name)\n    return "Hi " .. name\nend\n'),
        }
        files = {
            name + '/go/go.mod': 'module odin.local/home-smoke\n\ngo 1.27.0\n',
            'go.work': 'go 1.27.0\n\nuse ./' + name + '/go\n',
            'OdinHomeSmoke.csproj': '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable></PropertyGroup><ItemGroup><Compile Include="' + name + '/csharp/Home.cs" /></ItemGroup></Project>\n',
            name + '/NuGet.Config': '<configuration><packageSources><clear/></packageSources></configuration>\n',
            **{relative: text for relative, text in self.sources.values()},
            name + '/repo-a/sentinel.txt': 'repo-a', name + '/repo-b/sentinel.txt': 'repo-b',
        }
        for relative in files:
            if (self.args.synthetic_home / relative).exists():
                raise FileExistsError('Refusing to replace fixture or user file: ' + relative)
        for relative, content in files.items():
            with (self.args.synthetic_home / relative).open('x', encoding='utf-8', newline='\n') as stream:
                stream.write(content)
        self.report['initial_fixture_sha256'] = {name: digest((self.args.synthetic_home / name).read_bytes()) for name in files}
        dotnet = self.args.install_root / 'backends/dotnet/dotnet.exe'
        environment = {**self.environment, 'DOTNET_ROOT': str(dotnet.parent), 'DOTNET_ROOT_X64': str(dotnet.parent), 'DOTNET_MULTILEVEL_LOOKUP': '0', 'DOTNET_CLI_HOME': str(fixture / 'dotnet-home'), 'NUGET_PACKAGES': str(fixture / 'nuget'), 'DOTNET_ADD_GLOBAL_TOOLS_TO_PATH': 'false', 'DOTNET_GENERATE_ASPNET_CERTIFICATE': 'false', 'DOTNET_CLI_TELEMETRY_OPTOUT': '1', 'DOTNET_SKIP_FIRST_TIME_EXPERIENCE': '1'}
        command = [str(dotnet), 'restore', 'OdinHomeSmoke.csproj', '--configfile', str(fixture / 'NuGet.Config'), '--ignore-failed-sources']
        restore = subprocess.run(command, cwd=self.args.synthetic_home, env=environment, capture_output=True, timeout=120)
        self.report['offline_csharp_restore'] = {'command': command, 'cwd': str(self.args.synthetic_home), 'exit_code': restore.returncode, 'stdout': restore.stdout.decode('utf-8', errors='replace'), 'stderr': restore.stderr.decode('utf-8', errors='replace')}
        self.record('explicit_compile_items_offline_restore', restore.returncode == 0, self.report['offline_csharp_restore'])

    def check_root(self, peer, expected):
        projects = self.call(peer, 'list_projects')['projects']
        self.record('one_expected_root', len(projects) == 1 and os.path.samefile(projects[0]['root'], expected), projects)
        self.record('readonly_root', not any(projects[0].get(key, False) for key in ('allow_write', 'allow_shell', 'allow_debug', 'allow_inspections')), projects)
        return projects[0]['id']

    def run(self):
        self.environment = dict(os.environ)
        self.environment.update(HOME=str(self.args.synthetic_home), USERPROFILE=str(self.args.synthetic_home))
        self.environment['PATH'] = str(Path(os.environ['SystemRoot']) / 'System32') + os.pathsep + os.environ['SystemRoot']
        for key in ('NODE_OPTIONS', 'NODE_PATH', 'GOROOT', 'GOPATH', 'GOCACHE', 'GOMODCACHE', 'GOTOOLCHAIN', 'DOTNET_ROOT', 'DOTNET_CLI_HOME', 'NUGET_PACKAGES'):
            self.environment.pop(key, None)
        self.report['persistent_user_path_before'] = user_path_snapshot()
        before_host = self.args.host_config.read_bytes()
        try:
            config = tomllib.loads(before_host.decode('utf-8'))
            workspace = config['workspace']
            self.record('fixed_host_default_root', Path(workspace['default_root']).resolve() == self.args.synthetic_home, workspace['default_root'])
            self.record('no_ancestor_registered_project', not any(Path(p['root']).resolve() == self.args.synthetic_home or self.args.synthetic_home.is_relative_to(Path(p['root']).resolve()) for p in config.get('projects', {}).values()))
            self.make_fixture()
            with self.start_home(self.args.synthetic_home) as peer:
                home_id = self.check_root(peer, self.args.synthetic_home)
                for language, (relative, _) in self.sources.items():
                    cap = self.call(peer, 'get_capabilities', {'probe': True, 'language': language})
                    row = next((p for p in cap['languages'] if p['id'] == language), {})
                    self.record(language + '_actual_initialize_at_home', row.get('ready') is True and bool(row.get('capabilities', {}).get('documentSymbolProvider')), row)
                    result = self.retry(lambda: self.call(peer, 'get_symbols_overview', {'relative_path': relative}), lambda v: 'HomeGreet' in json.dumps(v.get('symbols', [])), seconds=45)
                    self.record(language + '_actual_home_symbols', result.get('backend') == 'lsp' and 'HomeGreet' in json.dumps(result.get('symbols', [])), result)
                host_relative = self.args.host_config.relative_to(self.args.synthetic_home).as_posix()
                denied = self.call(peer, 'read_file', {'relative_path': host_relative}, error=True)
                self.record('private_host_is_denied', any(code in json.dumps(denied) for code in ('PERMISSION', 'SCOPE', 'PROTECTED')), denied)
                app_relative = self.args.binary.relative_to(self.args.synthetic_home).as_posix()
                denied = self.call(peer, 'read_file', {'relative_path': app_relative}, error=True)
                self.record('installed_app_is_denied', any(code in json.dumps(denied) for code in ('PERMISSION', 'SCOPE', 'PROTECTED')), denied)
                denied = self.call(peer, 'create_text_file', {'relative_path': 'home-write-denied.txt', 'expected_sha256': None, 'content': 'denied'}, error=True)
                self.record('home_write_is_denied', 'PERMISSION' in json.dumps(denied) and not (self.args.synthetic_home / 'home-write-denied.txt').exists(), denied)
                self.current['stderr'].extend(peer.errors)
            for cwd, explicit in ((self.fixture / 'repo-a', False), (self.args.output.parent, True)):
                with self.start_home(cwd, explicit_config=explicit) as peer:
                    self.record('default_home_independent_of_cwd', self.check_root(peer, self.args.synthetic_home) == home_id)
                    self.current['stderr'].extend(peer.errors)
            ids = []
            with self.start_home(self.fixture / 'repo-a', current_workspace=True) as first, self.start_home(self.fixture / 'repo-b', current_workspace=True) as second:
                for peer, tag in ((first, 'a'), (second, 'b')):
                    ids.append(self.check_root(peer, self.fixture / ('repo-' + tag)))
                    result = self.call(peer, 'read_file', {'relative_path': 'sentinel.txt'})
                    self.record('explicit_workspace_' + tag + '_binding', result.get('content') == 'repo-' + tag, result)
                self.record('explicit_workspace_ids_are_distinct', len(set(ids)) == 2 and home_id not in ids, ids)
                self.current['stderr'].extend(first.errors + second.errors)
            self.record('fixture_sources_unchanged', all(digest((self.args.synthetic_home / name).read_bytes()) == expected for name, expected in self.report['initial_fixture_sha256'].items()))
        except Exception as error:
            self.current['checks'].append({'name': 'execution', 'status': 'FAIL', 'detail': str(error)})
        self.record('production_host_unchanged', self.args.host_config.read_bytes() == before_host)
        self.report['persistent_user_path_after'] = user_path_snapshot()
        self.record('persistent_user_path_unchanged', self.report['persistent_user_path_before'] == self.report['persistent_user_path_after'])
        self.current['status'] = 'FAIL' if any(row['status'] != 'PASS' for row in self.current['checks']) else 'PASS'
        self.report['status'] = self.current['status']
        self.report['completed_unix'] = time.time()
        self.save()
        print(json.dumps({'status': self.report['status'], 'checks': len(self.current['checks'])}), flush=True)
        return 0 if self.report['status'] == 'PASS' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('synthetic-home', 'install-root', 'binary', 'output'):
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--timeout', type=int, default=120)
    args = parser.parse_args()
    homes = real_os_homes()  # Deliberately before subprocess HOME/USERPROFILE overrides.
    for key, value in vars(args).items():
        if isinstance(value, Path):
            setattr(args, key, value.resolve())
    if os.name != 'nt' or args.synthetic_home in homes:
        parser.error('Choose a synthetic Windows home distinct from the real OS profile')
    args.host_config = args.synthetic_home / '.config/odin/host.toml'
    if not args.synthetic_home.is_dir() or not args.host_config.is_file():
        parser.error('Synthetic home must already have the standard host configured by installed setup')
    if not args.install_root.is_relative_to(args.synthetic_home) or not args.binary.is_relative_to(args.install_root):
        parser.error('Install the candidate below the synthetic home for this acceptance')
    if args.output.exists() or (args.synthetic_home / ('odin-home-smoke-' + args.output.stem)).exists():
        parser.error('Choose a fresh output and fixture path; preserve previous attempts')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    return Driver(args).run()


if __name__ == '__main__':
    raise SystemExit(main())