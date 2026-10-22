"""Installed expanded Windows LSP acceptance through actual Odin MCP.

Claims are fixed by case before initialization. Missing required capabilities
fail; unversioned diagnostics stay NOT_VERIFIED. All source edits use Odin CAS.
"""
from __future__ import annotations
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import sys
import time
import tomllib

from real_installed_languages import Driver as BaseDriver, digest, local_location, user_path_snapshot
from real_ide_backends import position, toml

CASES = ('slint', 'c', 'cpp', 'toml', 'yaml', 'markdown', 'rust', 'ty', 'ruff', 'vtsls-javascript', 'vtsls-typescript')
PRIMARY = CASES[:7]
SEMANTIC = {'c', 'cpp', 'rust', 'ty', 'vtsls-javascript', 'vtsls-typescript'}
REQUIRED = {
    'slint': ('documentSymbolProvider',),
    'c': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
    'cpp': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
    'toml': ('documentSymbolProvider', 'documentFormattingProvider'),
    'yaml': ('documentSymbolProvider', 'documentFormattingProvider'),
    'markdown': ('documentSymbolProvider',),
    'rust': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
    'ty': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
    'ruff': ('diagnosticProvider', 'documentFormattingProvider', 'codeActionProvider'),
    'vtsls-javascript': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
    'vtsls-typescript': ('documentSymbolProvider', 'definitionProvider', 'referencesProvider', 'hoverProvider', 'renameProvider'),
}
FORMAT_REQUIRED = {'toml', 'yaml', 'ruff'}


def fixture(base, case, tag):
    root = base / case / (('project \u4e2d\u6587 ' if tag == 'a' else 'project ') + tag)
    root.mkdir(parents=True)
    extra, symbol = {}, 'greet'
    if case in ('c', 'cpp'):
        cpp = case == 'cpp'
        suffix, header = ('cpp', 'shared.h') if cpp else ('c', 'shared.h')
        source, consumer = 'sample.' + suffix, 'consumer.' + suffix
        text = '#include "' + header + '"\nint greet(int value){return value+1;}\nint run(void){return greet(1);}\n'
        caller = '#include "' + header + '"\nint consume(void){return greet(2);}\n'
        extra[header] = ('class Greeter {\npublic:\n    static int greet_member(int value);\n};\n' if cpp else '') + 'int greet(int value);\n'
        if cpp:
            text += 'int Greeter::greet_member(int value){return value+1;}\n'
            caller = '#include "shared.h"\nint consume(void){return greet(2)+Greeter::greet_member(2); }\n'
        extra['compile_commands.json'] = json.dumps([{'directory': str(root), 'file': str(root / name), 'arguments': ['clang++' if cpp else 'clang', '-std=c++17' if cpp else '-std=c11', '-c', str(root / name)]} for name in (source, consumer)])
        broken, call = '\nint odin_broken( {\n', 'greet(1)'
    elif case == 'rust':
        source, consumer = 'sample.rs', 'consumer.rs'
        text = 'mod consumer;\npub fn greet(name:&str)->String{format!("Hi {}",name)}\npub fn run()->String{greet("Odin")}\n'
        caller = 'pub fn consume()->String{crate::greet("Caller")}\n'
        extra['Cargo.toml'] = '[package]\nname="odin-expanded-' + tag + '"\nversion="0.1.0"\nedition="2021"\n[lib]\npath="sample.rs"\n'
        broken, call = '\nfn odin_broken( {\n', 'greet("Odin")'
    elif case in ('ty', 'ruff'):
        source, consumer = 'sample.py', 'consumer.py'
        text = 'def greet(name: str) -> str:\n    return "Hi " + name\n\ndef run() -> str:\n    return greet("Odin")\n'
        caller = 'from sample import greet\nmessage = greet("Caller")\n'
        broken = '\nimport os\nmessage=unknown_odin_name\n' if case == 'ruff' else '\ndef odin_broken(:\n'
        call = 'greet("Odin")'
    elif case.startswith('vtsls-'):
        ts = case.endswith('-typescript')
        source, consumer = ('sample.ts', 'consumer.ts') if ts else ('sample.js', 'consumer.js')
        text = 'export function greet(name' + (': string): string' if ts else ')') + '{return "Hi "+name;}\nexport function run(){return greet("Odin");}\n'
        caller = 'import { greet } from "./sample' + ('' if ts else '.js') + '";\nexport const message=greet("Caller");\n'
        extra['tsconfig.json' if ts else 'jsconfig.json'] = json.dumps({'compilerOptions': {'strict': ts, 'noEmit': True, **({} if ts else {'allowJs': True, 'checkJs': False})}, 'include': ['*.ts' if ts else '*.js']})
        broken, call = '\nexport function odin_broken( {\n', 'greet("Odin")'
    elif case == 'slint':
        source, consumer, symbol = 'sample.slint', 'consumer.slint', 'App'
        text = 'export component App inherits Window {\n    in-out property <string> title-text: "Hello";\n    Text { text: root.title-text; }\n}\n'
        caller = 'import { App } from "sample.slint";\nexport component Consumer inherits App {}\n'
        broken, call = '\nexport component Broken inherits Window { invalid-property: ???; }\n', None
    elif case == 'toml':
        source, consumer, symbol = 'sample.toml', 'consumer.toml', 'project'
        text, caller = '[project]\nname="odin"\nversion="0.1.0"\n', '[consumer]\nname="other"\n'
        broken, call = '\ninvalid = [\n', None
    elif case == 'yaml':
        source, consumer, symbol = 'sample.yaml', 'consumer.yaml', 'project'
        text, caller = 'project:\n  name: Odin\n  version: "1"\n', 'consumer:\n  name: Other\n'
        broken, call = '\ninvalid: [\n', None
    else:
        source, consumer, symbol = 'sample.md', 'consumer.md', 'Main'
        text, caller = '# Main\n\n[Next](consumer.md#consumer)\n', '# Consumer\n\nOther document.\n'
        broken, call = '\n[Broken](missing-odin-file.md#missing)\n', None
    for name, content in {source: text, consumer: caller, **extra}.items():
        (root / name).write_text(content, encoding='utf-8', newline='\n')
    (root / 'sentinel.txt').write_text(case + '_' + tag, encoding='utf-8')
    return dict(root=root, language=case, tag=tag, source=source, consumer=consumer, text=text, broken=broken, call_needle=call, symbol=symbol, source_files=[source, consumer, *extra.keys()])



def expected_fixture_rename(text):
    # Expected text for controlled fixtures only. Actual symbol discovery and
    # rename edits still come exclusively from the configured language server.
    return re.sub(r'\bgreet\b', 'salute', text)


def check_fixture_expectations():
    cases = [
        ('python_import', 'from sample import greet\nmessage = greet("Caller")\n', 'from sample import salute\nmessage = salute("Caller")\n'),
        ('javascript_import', 'import { greet } from "./sample.js";\nconst result=greet("Caller");\n', 'import { salute } from "./sample.js";\nconst result=salute("Caller");\n'),
        ('typescript_import', 'import { greet } from "./sample";\nconst result=greet("Caller");\n', 'import { salute } from "./sample";\nconst result=salute("Caller");\n'),
        ('cpp_member_and_include', '#include "shared.h"\nint consume(){return greet(2)+Greeter::greet_member(2);}\n', '#include "shared.h"\nint consume(){return salute(2)+Greeter::greet_member(2);}\n'),
    ]
    for name, before, expected in cases:
        assert expected_fixture_rename(before) == expected, name
    return {'status': 'PASS', 'checks': [name for name, _, _ in cases], 'scope': 'INERT_FIXTURE_EXPECTATION_GENERATION_NOT_BACKEND_ACCEPTANCE'}



def diagnostic_status(value, expect_error):
    summary = value.get('summary', {})
    errors = summary.get('errors')
    current = (value.get('available') is True and value.get('version_verified') is True
               and summary.get('complete') is True and type(errors) is int and errors >= 0)
    if not current:
        return 'NOT_VERIFIED'
    return 'PASS' if (errors > 0) == expect_error else 'FAIL'


def current_ruff_f821(value):
    return diagnostic_status(value, True) == 'PASS' and any(
        isinstance(row, dict) and row.get('rule_id') == 'F821'
        for row in value.get('findings', []))


def check_diagnostic_mapping():
    # Explicit normalized Odin responses, not raw LSP diagnostics arrays.
    clean = {'available': True, 'version_verified': True, 'summary': {'complete': True, 'errors': 0, 'warnings': 0}, 'findings': []}
    errors = {'available': True, 'version_verified': True, 'summary': {'complete': True, 'errors': 1, 'warnings': 0}, 'findings': [{'severity': 'error', 'rule_id': 'syntax-error'}]}
    warnings = {'available': True, 'version_verified': True, 'summary': {'complete': True, 'errors': 0, 'warnings': 1}, 'findings': [{'severity': 'warning', 'rule_id': 'warning-rule'}]}
    pending = {'available': False, 'version_verified': False, 'summary': {'complete': False, 'errors': 0}, 'findings': []}
    unversioned = {'available': True, 'version_verified': False, 'summary': {'complete': False, 'errors': 1}, 'findings': [{'rule_id': 'F821'}]}
    incomplete = {'available': True, 'version_verified': True, 'summary': {'complete': False, 'errors': 1}, 'findings': [{'rule_id': 'F821'}]}
    ruff = {'available': True, 'version_verified': True, 'summary': {'complete': True, 'errors': 1}, 'findings': [{'severity': 'error', 'rule_id': 'F821'}]}
    legacy_code = {'available': True, 'version_verified': True, 'summary': {'complete': True, 'errors': 1}, 'findings': [{'code': 'F821'}]}
    cases = [
        ('current_errors_are_errors', errors, True, 'PASS'),
        ('current_errors_are_not_clean', errors, False, 'FAIL'),
        ('current_clean_is_clean', clean, False, 'PASS'),
        ('current_clean_is_not_errors', clean, True, 'FAIL'),
        ('warnings_do_not_satisfy_syntax_errors', warnings, True, 'FAIL'),
        ('warnings_are_not_errors_for_valid_source', warnings, False, 'PASS'),
        ('pending_never_passes_clean', pending, False, 'NOT_VERIFIED'),
        ('pending_never_passes_errors', pending, True, 'NOT_VERIFIED'),
        ('unversioned_errors_never_pass', unversioned, True, 'NOT_VERIFIED'),
        ('incomplete_errors_never_pass', incomplete, True, 'NOT_VERIFIED'),
    ]
    for name, value, expect_error, expected in cases:
        assert diagnostic_status(value, expect_error) == expected, name
    ruff_cases = [
        ('ruff_rule_id_f821', ruff, True),
        ('ruff_legacy_code_is_not_rule_id', legacy_code, False),
        ('ruff_unversioned_f821_never_passes', unversioned, False),
        ('ruff_incomplete_f821_never_passes', incomplete, False),
    ]
    for name, value, expected in ruff_cases:
        assert current_ruff_f821(value) is expected, name
    return {'status': 'PASS', 'checks': [name for name, *_ in cases + ruff_cases], 'scope': 'INERT_NORMALIZED_DIAGNOSTIC_MAPPING_NOT_BACKEND_ACCEPTANCE'}



def check_reference_fixture_points():
    cpp = '#include "shared.h"\nint consume(void){return greet(2)+Greeter::greet_member(2); }\n'
    normal = 'from sample import greet\nmessage = greet("Caller")\n'
    def at(line, start, end):
        return {'locations': [{'external': False, 'relative_path': 'consumer', 'location': {
            'range': {'start': {'line': line, 'character': start}, 'end': {'line': line, 'character': end}}}}]}
    cases = [
        ('cpp_free_function_call', at(1, 25, 30), cpp, 'greet(', True),
        ('cpp_member_cannot_satisfy_free_function_reference', at(1, 43, 55), cpp, 'greet(', False),
        ('cpp_member_call', at(1, 43, 55), cpp, 'greet_member(', True),
        ('cpp_free_function_cannot_satisfy_member_reference', at(1, 25, 30), cpp, 'greet_member(', False),
        ('normal_consumer_call', at(1, 10, 15), normal, 'greet(', True),
        ('normal_import_is_not_the_consumer_call', at(0, 19, 24), normal, 'greet(', False),
    ]
    for name, value, text, token, expected in cases:
        assert local_location(value, 'consumer', text, token) is expected, name
    return {'status': 'PASS', 'checks': [name for name, *_ in cases], 'scope': 'INERT_REFERENCE_FIXTURE_POINTS_NOT_BACKEND_ACCEPTANCE'}



def rust_readiness_value(response, allow_zero_symbol=False):
    result = response.get('result', {})
    if 'error' in response or not isinstance(result, dict):
        raise RuntimeError(json.dumps(response))
    value = result.get('structuredContent')
    if value is None and result.get('content'):
        value = json.loads(result['content'][0]['text'])
    if not isinstance(value, dict) or value.get('truncated'):
        raise RuntimeError('Unexpected Rust readiness response: ' + json.dumps(response))
    if result.get('isError'):
        fault = value.get('error', {})
        if (allow_zero_symbol and fault.get('code') == 'BACKEND_ERROR'
                and fault.get('message') == 'Expected exactly one symbol named greet; found 0'):
            return None, True
        raise RuntimeError(json.dumps(response))
    return value, False


def wait_rust_references(request, arguments, positive, observations, seconds=45,
                         clock=time.monotonic, pause=time.sleep, record=None):
    if not 0 < seconds <= 45:
        raise ValueError('Rust startup readiness budget must be in (0, 45] seconds')
    started = clock()
    deadline = started + seconds
    def remaining():
        left = deadline - clock()
        if left <= 0:
            raise TimeoutError('Rust startup readiness deadline without ready status and required cross-file references')
        return left
    while True:
        remaining()
        row = {'attempt': len(observations) + 1, 'elapsed_before_secs': clock() - started}
        observations.append(row)
        try:
            row['status_response'] = request('get_capabilities', {'probe': True, 'language': 'rust'}, remaining())
            status_value, _ = rust_readiness_value(row['status_response'])
            entries = [entry for entry in status_value.get('languages', []) if entry.get('id') == 'rust']
            if len(entries) != 1:
                raise AssertionError('Rust readiness requires exactly one actual Rust capability entry')
            entry = entries[0]
            status = entry.get('status')
            if not isinstance(status, dict) or not isinstance(status.get('transport'), dict):
                raise AssertionError('Rust startup status and transport must be objects')
            server_status = status['transport'].get('server_status')
            if server_status is not None and not isinstance(server_status, dict):
                raise AssertionError('Rust startup server_status must be an object or null')
            health = server_status.get('health') if server_status is not None else None
            row['readiness'] = status.get('readiness', 'unknown')
            row['server_status_received'] = server_status is not None
            row['server_health'] = health
            if row['readiness'] in ('error', 'closed') or health == 'error' or entry.get('ready') is not True:
                raise RuntimeError('Rust startup status is error or closed: ' + json.dumps(row['status_response']))
            if server_status is None or row['readiness'] != 'ready' or status.get('initialized') is not True:
                row['outcome'] = 'waiting_for_ready_status'
            else:
                row['references_response'] = request('find_referencing_symbols', arguments, remaining())
                value, transient = rust_readiness_value(row['references_response'], allow_zero_symbol=True)
                if transient:
                    row['outcome'] = 'exact_zero_symbol_transient'
                else:
                    locations = value.get('locations')
                    if not isinstance(locations, list):
                        raise AssertionError('Rust readiness response lacks a locations array')
                    if positive(value):
                        remaining()  # A response arriving after the deadline cannot pass.
                        row['outcome'] = 'ready_and_required_reference_observed'
                        row['elapsed_secs'] = clock() - started
                        if record:
                            record()
                        remaining()  # Evidence I/O cannot turn a late result into PASS.
                        return value
                    if locations:
                        raise AssertionError('Nonempty Rust references do not contain the required consumer call')
                    row['outcome'] = 'empty_references'
        except Exception as error:
            row['outcome'] = 'failed'
            row['error'] = {'type': type(error).__name__, 'message': str(error)}
            row['elapsed_secs'] = clock() - started
            if record:
                record()
            raise
        row['elapsed_secs'] = clock() - started
        if record:
            record()
        pause(min(0.25, remaining()))


def check_rust_readiness():
    def response(value, error=False):
        return {'result': {'isError': error, 'structuredContent': value}}
    def status(readiness='ready', health='ok'):
        return response({'languages': [{'id': 'rust', 'ready': True, 'status': {
            'initialized': True, 'readiness': readiness,
            'transport': {'server_status': {'health': health, 'quiescent': readiness == 'ready'}}}}]})
    empty = response({'locations': []})
    positive = response({'locations': [{'fixture_required_consumer_call': True}]})
    transient = response({'error': {'code': 'BACKEND_ERROR', 'message': 'Expected exactly one symbol named greet; found 0'}}, True)
    other = response({'error': {'code': 'BACKEND_ERROR', 'message': 'OTHER backend error'}}, True)
    wrong_code = response({'error': {'code': 'INVALID_ARGUMENT', 'message': 'Expected exactly one symbol named greet; found 0'}}, True)
    class Fixture:
        def __init__(self, references, statuses=None):
            self.now = 0
            self.references = list(references)
            self.statuses = list(statuses or [status()])
            self.calls = []
        def clock(self):
            return self.now
        def pause(self, seconds):
            self.now += seconds
        def request(self, tool, arguments, remaining):
            assert 0 < remaining <= 45
            self.calls.append(tool)
            self.now += min(0.01, remaining)
            values = self.statuses if tool == 'get_capabilities' else self.references
            return values.pop(0) if len(values) > 1 else values[0]
    def run(fixture, observations, seconds=1):
        return wait_rust_references(fixture.request, {'relative_path': 'sample.rs', 'name_path': 'greet'},
                                    lambda value: bool(value['locations']) and value['locations'][0].get('fixture_required_consumer_call') is True,
                                    observations, seconds=seconds, clock=fixture.clock, pause=fixture.pause)
    checks = []
    fixture, observed = Fixture([empty, transient, positive]), []
    assert run(fixture, observed) == positive['result']['structuredContent']
    assert [row['outcome'] for row in observed] == ['empty_references', 'exact_zero_symbol_transient', 'ready_and_required_reference_observed']
    assert observed[1]['references_response'] == transient and observed[-1]['elapsed_secs'] < 1
    checks.append('empty_and_exact_transient_wait_for_actual_positive')
    for name, error in [('other_error_immediately_raises', other), ('same_message_wrong_code_immediately_raises', wrong_code)]:
        fixture, observed = Fixture([error, positive]), []
        try:
            run(fixture, observed)
            raise AssertionError('Unexpected error was retried')
        except RuntimeError:
            assert len(observed) == 1 and fixture.calls == ['get_capabilities', 'find_referencing_symbols']
            assert observed[0]['references_response'] == error
        checks.append(name)
    for name, fixture in [('empty_references_deadline_fails', Fixture([empty])), ('unknown_status_never_passes', Fixture([positive], [status('unknown')]))]:
        observed = []
        try:
            run(fixture, observed, seconds=0.5)
            raise AssertionError('Deadline must fail without ready and required references')
        except TimeoutError:
            assert fixture.now <= 0.500001 and observed
        checks.append(name)
    fixture, observed = Fixture([positive], [status('pending'), status()]), []
    assert run(fixture, observed) == positive['result']['structuredContent']
    assert observed[0]['outcome'] == 'waiting_for_ready_status' and fixture.calls.count('find_referencing_symbols') == 1
    checks.append('pending_status_requires_ready_and_actual_reference')
    fixture, observed = Fixture([positive], [status('error', 'error')]), []
    try:
        run(fixture, observed)
        raise AssertionError('Error status must not pass')
    except RuntimeError:
        assert fixture.calls == ['get_capabilities']
    checks.append('error_status_fails_without_reference_retry')
    null_status = response({'languages': [{'id': 'rust', 'ready': True, 'status': {
        'initialized': True, 'readiness': 'unknown', 'transport': {'server_status': None, 'active_progress': {}}}}]})
    fixture, observed = Fixture([positive], [null_status, status('pending'), status()]), []
    assert run(fixture, observed) == positive['result']['structuredContent']
    assert [row['outcome'] for row in observed] == ['waiting_for_ready_status', 'waiting_for_ready_status', 'ready_and_required_reference_observed']
    assert observed[0]['status_response'] == null_status and observed[0]['server_status_received'] is False
    assert fixture.calls.count('find_referencing_symbols') == 1
    checks.append('real_null_status_then_pending_then_ready_requires_positive')
    fixture, observed = Fixture([positive], [null_status]), []
    try:
        run(fixture, observed, seconds=0.5)
        raise AssertionError('Null status must not pass or query references before ready')
    except TimeoutError:
        assert fixture.now <= 0.500001 and observed and 'find_referencing_symbols' not in fixture.calls
        assert all(row['server_status_received'] is False for row in observed)
    checks.append('always_null_status_deadline_fails')
    for name, malformed in [('string_server_status_fails', 'starting'), ('array_server_status_fails', [])]:
        malformed_status = response({'languages': [{'id': 'rust', 'ready': True, 'status': {
            'initialized': True, 'readiness': 'unknown', 'transport': {'server_status': malformed}}}]})
        fixture, observed = Fixture([positive], [malformed_status]), []
        try:
            run(fixture, observed)
            raise AssertionError('Malformed server status must fail immediately')
        except AssertionError as error:
            assert str(error) == 'Rust startup server_status must be an object or null'
            assert fixture.calls == ['get_capabilities'] and observed[0]['outcome'] == 'failed'
        checks.append(name)
    return {'status': 'PASS', 'checks': checks, 'scope': 'INERT_RUST_STARTUP_READINESS_NOT_BACKEND_ACCEPTANCE'}


def source_hashes(item):
    return {name: digest((item['root'] / name).read_bytes()) for name in item['source_files']}


class Driver(BaseDriver):
    def __init__(self, args):
        super().__init__(args)
        self.report['scope'] = 'ACTUAL_INSTALLED_EXPANDED_LSP_MCP_FIXED_CLAIMS_TWO_PROJECTS'
        self.report['expanded_driver_sha256'] = digest(Path(__file__).read_bytes())
        self.report['predetermined_required_capabilities'] = REQUIRED
        self.report['native_inspection_formatter_execution'] = 'NOT_RUN: LSP path only; no inspection/formatter host grants are added'

    def overview(self, peer, item):
        value = self.retry(lambda: self.call(peer, 'get_symbols_overview', {'relative_path': item['source']}), lambda v: any(s['name'] == item['symbol'] or s['name'].startswith(item['symbol'] + '(') for s in v.get('symbols', [])))
        names = [s for s in value.get('symbols', []) if s['name'] == item['symbol'] or s['name'].startswith(item['symbol'] + '(')]
        self.record(item['tag'] + '_actual_backend_symbols', value.get('backend') == 'lsp' and bool(names), value)
        return names[0]['name_path']

    def diagnostic(self, peer, item, name, finding):
        value = self.retry(lambda: self.call(peer, 'get_diagnostics_for_file', {'relative_path': item['source']}), lambda v: diagnostic_status(v, finding) == 'PASS', seconds=15)
        status = diagnostic_status(value, finding)
        self.record(name, status, value)
        if item['language'] == 'ruff' and finding:
            self.record(name + '_ruff_undefined_name', 'NOT_VERIFIED' if status == 'NOT_VERIFIED' else current_ruff_f821(value), value)

    def config(self, path, roots, profile, write=False):
        state = path.with_suffix('.state')
        content = 'state_dir = ' + toml(str(state)) + '\n[limits]\nrpc_timeout_secs = 120\n[workspace]\nallowed_roots = ' + toml([str(p) for p in roots]) + '\n'
        profiles = [profile] + ([self.header_profile] if profile['id'] == 'cpp' else [])
        content += ''.join('[[workspace.languages]]\n' + '\n'.join(k + ' = ' + toml(v) for k, v in item.items()) + '\n' for item in profiles)
        if write:
            # Newly bound workspace templates are always read only. Only this
            # explicit external test project grants transactional source writes.
            content += '\n[projects.expanded_test]\nroot = ' + toml(str(roots[0])) + '\nallow_write = true\nallow_shell = false\nallow_debug = false\nallow_inspections = false\nallow_formatters = false\n'
            content += ''.join('[[projects.expanded_test.languages]]\n' + '\n'.join(k + ' = ' + toml(v) for k, v in item.items()) + '\n' for item in profiles)
        path.write_text(content, encoding='utf-8', newline='\n')
        return path

    def test_case(self, case, profile, base):
        self.current = {'language': case, 'status': 'NOT_RUN', 'checks': [], 'stderr': [], 'profile': profile}
        self.report['languages'].append(self.current)
        a, b = fixture(base, case, 'a'), fixture(base, case, 'b')
        before = [source_hashes(a), source_hashes(b)]
        config = self.config(base / (case + '-readonly.toml'), [a['root'], b['root']], profile)
        capabilities = {}
        try:
            ids = []
            with self.start(config, a['root']) as first, self.start(config, b['root']) as second:
                for peer, item in ((first, a), (second, b)):
                    projects = self.call(peer, 'list_projects')['projects']
                    self.record(item['tag'] + '_current_workspace', len(projects) == 1 and os.path.samefile(projects[0]['root'], item['root']), projects)
                    ids.append(projects[0]['id'])
                    sentinel = self.call(peer, 'read_file', {'relative_path': 'sentinel.txt'})
                    self.record(item['tag'] + '_source_binding', sentinel['content'] == case + '_' + item['tag'])
                    cap = self.call(peer, 'get_capabilities', {'probe': True, 'language': profile['id']})
                    entry = next((p for p in cap['languages'] if p['id'] == profile['id']), {})
                    self.record(item['tag'] + '_actual_lsp_initialize', entry.get('ready') is True, entry)
                    capabilities = entry.get('capabilities', {})
                    for required in REQUIRED[case]:
                        self.record(item['tag'] + '_required_' + required, bool(capabilities.get(required)), capabilities.get(required))
                    if case != 'ruff':
                        self.overview(peer, item)
                        if case == 'cpp':
                            header = self.call(peer, 'get_symbols_overview', {'relative_path': 'shared.h'})
                            self.record(item['tag'] + '_cpp_class_in_dot_h_through_c_profile', header.get('backend') == 'lsp' and 'Greeter' in json.dumps(header.get('symbols', [])) and 'greet_member' in json.dumps(header.get('symbols', [])), header)
                    else:
                        unavailable = self.call(peer, 'get_symbols_overview', {'relative_path': item['source']}, error=True)
                        self.record(item['tag'] + '_semantic_symbols_unavailable', 'CAPABILITY_UNAVAILABLE' in json.dumps(unavailable), unavailable)
                    self.diagnostic(peer, item, item['tag'] + '_valid_diagnostics', False)
                    denied = self.call(peer, 'create_text_file', {'relative_path': 'denied.txt', 'expected_sha256': None, 'content': 'denied'}, error=True)
                    self.record(item['tag'] + '_readonly_write_denied', 'PERMISSION' in json.dumps(denied) and not (item['root'] / 'denied.txt').exists(), denied)
                self.record('distinct_project_ids', len(set(ids)) == 2, ids)
                self.record('readonly_source_hashes', [source_hashes(a), source_hashes(b)] == before)
                self.current['stderr'].extend(first.errors + second.errors)
            writable = self.config(base / (case + '-writable.toml'), [a['root']], profile, write=True)
            with self.start(writable, a['root']) as peer:
                if case in SEMANTIC:
                    self.semantic(peer, a, b, before)
                if case in FORMAT_REQUIRED:
                    self.format(peer, a, b, before)
                changed = self.edit(peer, a, a['text'] + a['broken'])
                try:
                    self.diagnostic(peer, a, 'broken_source_diagnostics', True)
                    if case == 'ruff':
                        actions = self.call(peer, 'code_actions', {'relative_path': a['source'], 'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': (a['text'] + a['broken']).count('\n'), 'character': 0}}})
                        self.record('ruff_actual_code_actions_not_executed', bool(actions.get('actions')) and actions.get('executed') is False, actions)
                finally:
                    self.call(peer, 'transaction_rollback', {'transaction_id': changed})
                self.diagnostic(peer, a, 'recovered_source_diagnostics', False)
                self.record('all_sources_restored', [source_hashes(a), source_hashes(b)] == before)
                self.current['stderr'].extend(peer.errors)
        except Exception as error:
            self.current['checks'].append({'name': 'execution', 'status': 'FAIL', 'optional': False, 'detail': str(error)})
        outcomes = [check['status'] for check in self.current['checks']]
        self.current['status'] = 'FAIL' if 'FAIL' in outcomes else 'PARTIAL' if any(s != 'PASS' for s in outcomes) else 'PASS'
        self.current['capabilities'] = capabilities
        print(json.dumps({'case': case, 'status': self.current['status'], 'counts': dict(Counter(outcomes))}), flush=True)
        self.save()

    def rust_readiness_request(self, peer, tool, arguments, remaining):
        response = peer.request('tools/call', {'name': tool, 'arguments': arguments}, timeout=min(self.args.timeout, remaining))
        self.report['transcript'].append({'language': self.current['language'], 'tool': tool, 'arguments': arguments,
                                          'phase': 'rust_startup_readiness', 'response': response})
        return response

    def semantic(self, peer, a, b, before):
        name_path = self.overview(peer, a)
        self.call(peer, 'get_symbols_overview', {'relative_path': a['consumer']})
        consumer = (a['root'] / a['consumer']).read_text(encoding='utf-8')
        if a['language'] == 'rust':
            observations = []
            self.current['rust_startup_readiness'] = {'budget_seconds': 45, 'observations': observations}
            refs = wait_rust_references(lambda tool, arguments, remaining: self.rust_readiness_request(peer, tool, arguments, remaining),
                                        {'relative_path': a['source'], 'name_path': name_path},
                                        lambda value: local_location(value, a['consumer'], consumer, 'greet('), observations, record=self.save)
        else:
            refs = self.retry(lambda: self.call(peer, 'find_referencing_symbols', {'relative_path': a['source'], 'name_path': name_path}), lambda v: local_location(v, a['consumer'], consumer, 'greet('))
        self.record('cross_file_references', local_location(refs, a['consumer'], consumer, 'greet('), refs)
        if a['language'] == 'cpp':
            header = self.call(peer, 'get_symbols_overview', {'relative_path': 'shared.h'})
            member = next(row for row in header.get('symbols', []) if row.get('name', '').startswith('greet_member'))
            references = self.retry(lambda: self.call(peer, 'find_referencing_symbols', {'relative_path': 'shared.h', 'name_path': member['name_path']}), lambda value: local_location(value, a['consumer'], consumer, 'greet_member('))
            self.record('cpp_dot_h_class_member_references', local_location(references, a['consumer'], consumer, 'greet_member('), references)
        definition = self.call(peer, 'find_declaration', {'relative_path': a['source'], 'position': position(a['text'], a['call_needle'], 1)})
        self.record('definition_navigation', any(p.get('relative_path') in (a['source'], 'shared.h', 'shared.hpp') and p.get('external') is False for p in definition.get('locations', [])), definition)
        hover = self.call(peer, 'hover', {'relative_path': a['source'], 'position': position(a['text'], a['call_needle'], 1)})
        self.record('custom_function_hover', 'greet' in json.dumps(hover).lower(), hover)
        preview = self.call(peer, 'rename_symbol', {'relative_path': a['source'], 'name_path': name_path, 'new_name': 'salute'})
        tid = preview['transaction_id']
        self.call(peer, 'transaction_preview', {'transaction_id': tid})
        self.record('rename_preview_is_inert', [source_hashes(a), source_hashes(b)] == before and bool(preview.get('files')), preview)
        concurrent_text = a['text'] + ('\n# independent edit\n' if a['language'] == 'ty' else '\n// independent edit\n')
        concurrent = self.edit(peer, a, concurrent_text)
        try:
            conflict = self.call(peer, 'transaction_apply', {'transaction_id': tid}, error=True)
            self.record('rename_stale_cas_rejected', 'CONFLICT' in json.dumps(conflict) and (a['root'] / a['source']).read_text(encoding='utf-8') == concurrent_text, conflict)
        finally:
            self.call(peer, 'transaction_rollback', {'transaction_id': concurrent})
        expected = {name: expected_fixture_rename((a['root'] / name).read_text(encoding='utf-8')) for name in a['source_files']}
        fresh = self.call(peer, 'rename_symbol', {'relative_path': a['source'], 'name_path': name_path, 'new_name': 'salute'})
        self.call(peer, 'transaction_preview', {'transaction_id': fresh['transaction_id']})
        self.call(peer, 'transaction_apply', {'transaction_id': fresh['transaction_id']})
        try:
            self.record('rename_definition_and_consumer', all((a['root'] / name).read_text(encoding='utf-8') == text for name, text in expected.items()) and all(re.search(r'\bsalute\b', expected[name]) for name in (a['source'], a['consumer'])))
            self.record('other_project_hash_unchanged', source_hashes(b) == before[1])
        finally:
            self.call(peer, 'transaction_rollback', {'transaction_id': fresh['transaction_id']})
        self.record('rename_exact_rollback', [source_hashes(a), source_hashes(b)] == before)

    def format(self, peer, a, b, before):
        text = {'ruff': 'def greet(name):\n return "Hi "+name\nmessage=greet( "Odin" )\n', 'toml': '[project]\nname=    "odin"\nversion="0.1.0"   \n', 'yaml': 'project: {name: Odin, version: "1"}    \n'}[a['language']]
        changed = None
        if text != a['text']:
            changed = self.edit(peer, a, text)
        try:
            original = (a['root'] / a['source']).read_bytes()
            preview = self.call(peer, 'format_document', {'relative_path': a['source'], 'expected_sha256': digest(original)})
            if preview.get('changed') is False:
                self.record('format_required_change_not_returned', False, preview)
                return
            tid = preview['transaction_id']
            self.call(peer, 'transaction_preview', {'transaction_id': tid})
            self.record('format_preview_is_inert', (a['root'] / a['source']).read_bytes() == original and bool(preview.get('files')), preview)
            self.call(peer, 'transaction_apply', {'transaction_id': tid})
            try:
                self.record('format_transaction_changes_source', (a['root'] / a['source']).read_bytes() != original)
                self.record('format_does_not_touch_other_project', source_hashes(b) == before[1])
            finally:
                self.call(peer, 'transaction_rollback', {'transaction_id': tid})
            self.record('format_exact_rollback', (a['root'] / a['source']).read_bytes() == original)
        finally:
            if changed:
                self.call(peer, 'transaction_rollback', {'transaction_id': changed})

    def run(self):
        self.environment = dict(os.environ)
        self.environment['PATH'] = os.pathsep.join([str(self.args.rust_toolchain_bin)] if self.args.rust_toolchain_bin else [])
        self.environment['PATH'] += (os.pathsep if self.environment['PATH'] else '') + str(Path(os.environ['SystemRoot']) / 'System32') + os.pathsep + os.environ['SystemRoot']
        for key in ('NODE_OPTIONS', 'NODE_PATH', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WRAPPER'):
            self.environment.pop(key, None)
        self.report['external_runtime_PATH'] = self.environment['PATH']
        self.report['persistent_user_path_before'] = user_path_snapshot()
        self.report['configuration_sources'] = []
        base = self.args.output.with_suffix('.fixtures')
        base.mkdir()
        for case in self.args.cases:
            config_path = self.args.ty_host_config if case == 'ty' else self.args.ruff_host_config if case == 'ruff' else self.args.vtsls_host_config if case.startswith('vtsls-') else self.args.host_config
            try:
                if config_path is None:
                    raise ValueError('Explicit installed alternative host config is required for ' + case)
                config = tomllib.loads(config_path.read_text(encoding='utf-8'))
                language = 'python' if case in ('ty', 'ruff') else case.removeprefix('vtsls-')
                profiles = [p for p in config['workspace']['languages'] if p['id'] == language]
                assert len(profiles) == 1, 'Expected one configured provider for ' + language
                profile = profiles[0]
                if case == 'cpp':
                    self.header_profile = next(p for p in config['workspace']['languages'] if p['id'] == 'c' and 'h' in p['extensions'])
                    assert self.header_profile['program'] == profile['program'], 'C/header and C++ must use the same clangd executable'
                if case in ('ty', 'ruff') or case.startswith('vtsls-'):
                    marker = json.loads(Path(str(config_path) + '.odin-lsps.json').read_text(encoding='utf-8'))
                    expected = 'vtsls' if case.startswith('vtsls-') else case
                    key = 'typescript' if case.startswith('vtsls-') else 'python'
                    assert marker.get('selected_backends', {}).get(key) == expected, 'Alternative was not explicitly installed through setup'
                    suffix = str(Path('backends') / case / (case + '.exe')) if case in ('ty', 'ruff') else str(Path('node_modules') / '@vtsls' / 'language-server' / 'bin' / 'vtsls.js')
                    assert any(str(value).endswith(suffix) for value in [profile['program'], *profile.get('args', [])]), 'Host profile does not select expected backend'
                self.report['configuration_sources'].append({'case': case, 'host': str(config_path), 'sha256': digest(config_path.read_bytes())})
                self.test_case(case, profile, base)
            except Exception as error:
                self.report['languages'].append({'language': case, 'status': 'FAIL', 'checks': [{'name': 'fixture_or_installed_selection', 'status': 'FAIL', 'detail': str(error)}]})
                self.save()
        self.report['persistent_user_path_after'] = user_path_snapshot()
        unchanged = self.report['persistent_user_path_before'] == self.report['persistent_user_path_after']
        self.report['persistent_user_path_unchanged'] = 'PASS' if unchanged else 'FAIL'
        self.report['counts'] = dict(Counter(c['status'] for item in self.report['languages'] for c in item['checks']))
        statuses = [item['status'] for item in self.report['languages']]
        self.report['status'] = 'FAIL' if not unchanged or 'FAIL' in statuses else 'PARTIAL' if 'PARTIAL' in statuses else 'PASS'
        self.report['completed_unix'] = time.time()
        self.save()
        print(json.dumps({'status': self.report['status'], 'counts': self.report['counts']}), flush=True)
        return 0 if self.report['status'] == 'PASS' else 2 if self.report['status'] == 'PARTIAL' else 1


def main():
    if sys.argv[1:] == ['--check-fixture-expectations']:
        print(json.dumps(check_fixture_expectations()))
        return 0
    if sys.argv[1:] == ['--check-diagnostic-mapping']:
        print(json.dumps(check_diagnostic_mapping()))
        return 0
    if sys.argv[1:] == ['--check-reference-fixture-points']:
        print(json.dumps(check_reference_fixture_points()))
        return 0
    if sys.argv[1:] == ['--check-rust-readiness']:
        print(json.dumps(check_rust_readiness()))
        return 0
    parser = argparse.ArgumentParser(description=__doc__, epilog='Use --check-fixture-expectations, --check-diagnostic-mapping, --check-reference-fixture-points or --check-rust-readiness alone for inert QA regressions.')
    for flag in ('binary', 'host-config', 'install-root', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    for flag in ('ty-host-config', 'ruff-host-config', 'vtsls-host-config', 'rust-toolchain-bin'):
        parser.add_argument('--' + flag, type=Path)
    parser.add_argument('--cases', nargs='+', choices=CASES, default=PRIMARY)
    parser.add_argument('--timeout', type=int, default=120)
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('This driver validates the Windows installed payload')
    for key, value in vars(args).items():
        if isinstance(value, Path):
            setattr(args, key, value.resolve())
    if args.output.exists() or args.output.with_suffix('.fixtures').exists():
        parser.error('Choose a fresh output/fixture path; previous attempts are evidence')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    return Driver(args).run()


if __name__ == '__main__':
    raise SystemExit(main())