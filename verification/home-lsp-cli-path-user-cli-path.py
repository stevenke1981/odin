"""External operator PATH helper; preparation/checks never write registry.

Root runs --snapshot and --apply only after the actual-user upgrade seal.
No project/client configuration, machine PATH, legacy binary, or grants change.
"""
from __future__ import annotations

import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid

BASE = Path(__file__).resolve().parent
EXPECTED_BINARY_SHA256 = '2e1c11b040ab1c13e4843a0de7b912b0f8b9452d2284a5c353077df32e021792'
REG_SZ, REG_EXPAND_SZ = 1, 2
GUARD_NOTE = ('Expected raw value and RegistryValueKind are rechecked immediately before SetValueEx. '
              'The registry read and write are separate operations, not atomic CAS with external actors. '
              'No automatic retry or rollback is performed.')


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        while block := stream.read(1024 * 1024):
            value.update(block)
    return value.hexdigest()


def write_new(path, value):
    data = value if isinstance(value, bytes) else json.dumps(value, indent=2, ensure_ascii=True).encode() + b'\n'
    with Path(path).open('xb') as stream:
        stream.write(data)


def local_drive_spelling(value):
    value = os.fspath(value)
    require(not value.startswith(('\\\\.\\', '//./', '//?/', '\\??\\')), 'Unsupported device path')
    if value.startswith('\\\\?\\'):
        require(re.match(r'^[A-Za-z]:\\', value[4:]) is not None, 'Only extended absolute drive paths are supported')
        value = value[4:]
        require('/' not in value, 'Extended drive path is not canonical')
        parts = value[3:].split('\\')
        for index, part in enumerate(parts):
            require(part or index == len(parts) - 1, 'Empty extended path component')
            require(part not in ('.', '..') and not part.endswith((' ', '.')) and ':' not in part,
                    'Noncanonical extended path component')
            stem = part.split('.')[0].upper()
            require(stem not in {'CON', 'PRN', 'AUX', 'NUL', 'CONIN$', 'CONOUT$'}
                    and re.fullmatch(r'(COM|LPT)[1-9]', stem) is None, 'DOS device component')
    return value


def identity(path):
    return os.path.normcase(local_drive_spelling(str(Path(local_drive_spelling(path)).resolve())))


def no_redirect(path):
    candidate = Path(os.path.abspath(path))
    for item in [*reversed(candidate.parents), candidate]:
        if item.exists() or item.is_symlink():
            require(not item.is_symlink() and not getattr(item.lstat(), 'st_file_attributes', 0) & 0x400,
                    'Redirected path or ancestor is not allowed')
    return candidate


def home_and_binary():
    require(os.name == 'nt', 'Actual PATH actions are Windows-only')
    buffer = ctypes.create_unicode_buffer(32768)
    require(ctypes.windll.shell32.SHGetFolderPathW(None, 0x28, None, 0, buffer) == 0, 'OS Home identity is unavailable')
    home = no_redirect(buffer.value)
    require(identity(home) == identity(os.environ.get('USERPROFILE', '')), 'USERPROFILE differs from physical OS Home')
    install = no_redirect(home / 'AppData/Local/Programs/Odin')
    require(identity(os.environ.get('LOCALAPPDATA', '')) == identity(home / 'AppData/Local'), 'Unexpected LOCALAPPDATA identity')
    binary = no_redirect(install / 'odin.exe')
    require(file_sha(binary) == EXPECTED_BINARY_SHA256, 'Installed Odin does not match the sealed binary SHA-256')
    return home, install, binary


def powershell():
    return str(Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe')


def registry_path(machine=False, key=None):
    import winreg
    owned = key is None
    if key is None:
        root = winreg.HKEY_LOCAL_MACHINE if machine else winreg.HKEY_CURRENT_USER
        name = r'SYSTEM\CurrentControlSet\Control\Session Manager\Environment' if machine else 'Environment'
        try:
            key = winreg.OpenKey(root, name, 0, winreg.KEY_QUERY_VALUE)
        except FileNotFoundError:
            return {'present': False, 'raw': None, 'kind': None}
    try:
        try:
            raw, kind = winreg.QueryValueEx(key, 'Path')
            return {'present': True, 'raw': raw, 'kind': kind}
        except FileNotFoundError:
            return {'present': False, 'raw': None, 'kind': None}
    finally:
        if owned:
            key.Close()


def supported(value, allow_absent=False):
    if allow_absent and not value['present']:
        return
    require(value['present'] and isinstance(value['raw'], str) and value['kind'] in (REG_SZ, REG_EXPAND_SZ),
            'An existing string Path value with REG_SZ or REG_EXPAND_SZ is required')
    require('\0' not in value['raw'], 'NUL in persisted Path is unsupported')


def public_value(value):
    return {'present': value['present'], 'registry_kind': value['kind'],
            'raw_and_kind_sha256': sha(canonical(value)),
            'raw_sha256': sha(value['raw'].encode('utf-16le')) if isinstance(value['raw'], str) else None,
            'separator_segment_count': len(value['raw'].split(';')) if isinstance(value['raw'], str) else 0}


def expand(value):
    if not value['present']:
        return ''
    supported(value)
    text = value['raw']
    if value['kind'] == REG_SZ:
        return text
    require(re.search(r'%PATH%', text, re.IGNORECASE) is None,
            'Self-referential PATH expansion is unsupported; no inherited process PATH is substituted')
    size = ctypes.windll.kernel32.ExpandEnvironmentStringsW(text, None, 0)
    require(size > 0, 'Cannot expand persisted Path value')
    buffer = ctypes.create_unicode_buffer(size)
    expanded_size = ctypes.windll.kernel32.ExpandEnvironmentStringsW(text, buffer, size)
    require(0 < expanded_size <= size,
            'Persisted Path expansion changed during observation')
    return buffer.value


def effective_path(machine, user):
    supported(machine, allow_absent=True)
    supported(user)
    left, right = expand(machine), expand(user)
    return left + ';' + right if machine['present'] else right


def prepend_value(before, install, expand_value=expand, identify=identity):
    supported(before)
    first = before['raw'].split(';', 1)[0].strip()
    if len(first) >= 2 and first[0] == first[-1] == '"':
        first = first[1:-1]
    first_expanded = expand_value({**before, 'raw': first})
    is_first = bool(first_expanded) and identify(first_expanded) == identify(install)
    result = dict(before)
    if not is_first:
        result['raw'] = str(install) + ';' + before['raw']
    require(result['kind'] == before['kind'], 'RegistryValueKind must be retained')
    require(result['raw'] == before['raw'] or result['raw'] == str(install) + ';' + before['raw'],
            'Every prior raw entry and its order must be retained as an exact suffix')
    return result, not is_first


def compare_expected(actual, expected):
    require(actual == expected, 'Expected raw Path value/RegistryValueKind conflict; no write performed')


def private_folder(commands=None):
    folder = no_redirect(BASE / 'private')
    folder.mkdir(exist_ok=True)
    command = [powershell(), '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
               '-File', str(BASE / 'secure_private_folder.ps1'), '-Directory', str(folder)]
    logs = BASE / 'acl-commands'
    logs.mkdir(exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    result = subprocess.run(command,
                            capture_output=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
    stdout, stderr = logs / (stamp + '.stdout.log'), logs / (stamp + '.stderr.log')
    write_new(stdout, result.stdout); write_new(stderr, result.stderr)
    row = {'name': 'private_acl', 'argv': command, 'cwd': str(Path.cwd()), 'exit_code': result.returncode,
           'helper_sha256': file_sha(BASE / 'secure_private_folder.ps1'),
           'stdout_log': str(stdout), 'stdout_sha256': file_sha(stdout),
           'stderr_log': str(stderr), 'stderr_sha256': file_sha(stderr),
           'registry_execution': 'NOT_RUN', 'odin_execution': 'NOT_RUN'}
    write_new(logs / (stamp + '.command.json'), row)
    if commands is not None:
        commands.append(row)
    require(result.returncode == 0, 'Cannot enforce the private backup folder ACL')
    return folder


def verify_child(machine, user, binary, report_directory, phase, commands):
    environment = dict(os.environ)
    environment['PATH'] = effective_path(machine, user)
    command = [powershell(), '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
               '-File', str(BASE / 'verify_fresh_cli.ps1'), '-ExpectedBinaryPath', str(binary),
               '-ExpectedBinarySha256', EXPECTED_BINARY_SHA256]
    result = subprocess.run(command, cwd=report_directory, env=environment,
                            capture_output=True, timeout=40, creationflags=subprocess.CREATE_NO_WINDOW)
    # Verifier stdout contains only installed path/public metadata/hashes, never PATH.
    write_new(report_directory / (phase + '.stdout.log'), result.stdout)
    write_new(report_directory / (phase + '.stderr.log'), result.stderr)
    row = {'name': phase, 'argv': command, 'cwd': str(report_directory), 'exit_code': result.returncode,
           'effective_path_sha256': sha(environment['PATH'].encode('utf-16le')),
           'machine_source': public_value(machine), 'user_source': public_value(user),
           'stdout_sha256': sha(result.stdout), 'stderr_sha256': sha(result.stderr)}
    commands.append(row)
    require(result.returncode == 0, 'Fresh fixed CLI verifier failed: ' + phase)
    observed = json.loads(result.stdout.decode('utf-8-sig'))
    require(observed.get('status') == 'PASS' and observed.get('binary_sha256') == EXPECTED_BINARY_SHA256
            and identity(observed.get('selected_application', '')) == identity(binary)
            and observed.get('effective_path_sha256') == row['effective_path_sha256'],
            'Fresh CLI resolution/binary verification did not match the sealed installation')
    row['verification'] = observed
    return row


def source_pins():
    return {name: file_sha(BASE / name) for name in ('user_cli_path.py', 'secure_private_folder.ps1', 'verify_fresh_cli.ps1')}


def snapshot():
    home, install, binary = home_and_binary()
    user, machine = registry_path(), registry_path(machine=True)
    supported(user); supported(machine, allow_absent=True)
    commands = []
    private = private_folder(commands)
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    value = {'schema_version': 1, 'home_identity': identity(home), 'install_identity': identity(install),
             'installed_binary_sha256': file_sha(binary), 'user_path': user, 'machine_path': machine,
             'source_pins': source_pins(), 'observed_at_utc': datetime.now(timezone.utc).isoformat()}
    path = private / ('path-snapshot-' + stamp + '.json')
    write_new(path, value)
    report = {'status': 'PASS', 'scope': 'READONLY_PATH_SNAPSHOT_WITH_PRIVATE_EXTERNAL_BACKUP',
              'snapshot_path': str(path), 'snapshot_sha256': file_sha(path),
              'user_path': public_value(user), 'machine_path': public_value(machine),
              'home_identity_sha256': sha(identity(home).encode()), 'installed_binary_sha256': file_sha(binary),
              'source_pins': source_pins(), 'registry_write': 'NOT_RUN', 'fresh_cli_execution': 'NOT_RUN',
              'commands': commands,
              'guard_limit': GUARD_NOTE}
    output = BASE / ('snapshot-' + stamp + '.report.json')
    write_new(output, report)
    print(json.dumps({'status': 'PASS', 'report': str(output), 'snapshot_path': str(path),
                      'snapshot_sha256': file_sha(path), 'registry_write': 'NOT_RUN'}))
    return 0


def apply(snapshot_path, expected_sha):
    import winreg
    require(re.fullmatch('[a-f0-9]{64}', expected_sha or '') is not None, 'An expected snapshot SHA-256 is required')
    path = no_redirect(snapshot_path)
    require(path.resolve().is_relative_to((BASE / 'private').resolve()), 'Snapshot must be in the helper private folder')
    data = path.read_bytes()
    require(sha(data) == expected_sha, 'Snapshot file SHA-256 conflict; no registry write performed')
    expected = json.loads(data.decode('utf-8-sig'))
    require(expected.get('schema_version') == 1 and expected.get('source_pins') == source_pins(),
            'Snapshot schema or reviewed helper source hashes differ')
    home, install, binary = home_and_binary()
    require(expected['home_identity'] == identity(home) and expected['install_identity'] == identity(install)
            and expected['installed_binary_sha256'] == file_sha(binary), 'Home/installed binary identity changed')
    before = expected['user_path']; machine = expected['machine_path']
    supported(before); supported(machine, allow_absent=True)
    compare_expected(registry_path(), before)
    compare_expected(registry_path(machine=True), machine)
    proposed, changed = prepend_value(before, install)
    directory = BASE / ('apply-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8])
    directory.mkdir()
    report = {'status': 'NOT_RUN', 'scope': 'CURRENT_USER_PATH_PREFIX_ONLY', 'source_pins': source_pins(),
              'snapshot_sha256': expected_sha, 'before_user_path': public_value(before),
              'machine_path': public_value(machine), 'proposed_user_path': public_value(proposed),
              'commands': [], 'registry_write': 'NOT_RUN', 'guard_limit': GUARD_NOTE,
              'current_process_path_updated': False, 'client_restart': 'NOT_RUN',
              'environment_notification': 'NOT_RUN: fresh verifier receives an explicit persisted PATH; existing processes are not refreshed'}
    report_file = directory / 'report.json'
    try:
        verify_child(machine, proposed, binary, directory, 'candidate_prewrite', report['commands'])
        # Backup inherits the protected private directory ACL and is create-only.
        private = private_folder(report['commands'])
        backup = private / ('before-apply-' + directory.name + '.json')
        write_new(backup, {'user_path': before, 'machine_path': machine})
        report['private_backup_path'] = str(backup)
        report['private_backup_sha256'] = file_sha(backup)
        # Final immediate read+write on one opened HKCU key. This is not atomic
        # against another process; no undocumented CAS or external lock is claimed.
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment', 0,
                            winreg.KEY_QUERY_VALUE | winreg.KEY_SET_VALUE) as key:
            compare_expected(registry_path(machine=True), machine)
            compare_expected(registry_path(key=key), before)
            if changed:
                winreg.SetValueEx(key, 'Path', 0, before['kind'], proposed['raw'])
                report['registry_write'] = 'EXECUTED_ONCE'
            else:
                report['registry_write'] = 'N/A_ALREADY_FIRST_NO_WRITE'
        actual_user, actual_machine = registry_path(), registry_path(machine=True)
        compare_expected(actual_user, proposed)
        compare_expected(actual_machine, machine)
        report['after_user_path'] = public_value(actual_user)
        verify_child(actual_machine, actual_user, binary, directory, 'actual_persisted_postwrite', report['commands'])
        report['status'] = 'PASS'
        report['checks'] = {'expected_raw_and_kind_prewrite': 'PASS', 'exact_prior_suffix_preserved': 'PASS',
                            'registry_kind_preserved': 'PASS', 'machine_path_unchanged': 'PASS',
                            'fresh_persisted_machine_plus_user_path_resolves_installed_odin': 'PASS',
                            'installed_binary_sha256_and_metadata': 'PASS'}
    except Exception as error:
        report['status'] = 'FAIL'
        report['first_failure'] = {'type': type(error).__name__, 'message': str(error)}
    finally:
        report['completed_at_utc'] = datetime.now(timezone.utc).isoformat()
        write_new(report_file, report)
    print(json.dumps({'status': report['status'], 'report': str(report_file),
                      'registry_write': report['registry_write'], 'current_process_path_updated': False,
                      'client_restart': 'NOT_RUN'}))
    return 0 if report['status'] == 'PASS' else 1


def inert_checks():
    checks = []
    def check(name, value):
        require(value, name); checks.append(name)
    def reject(name, operation):
        try:
            operation()
        except (AssertionError, ValueError):
            checks.append(name)
        else:
            raise AssertionError('Expected rejection: ' + name)
    install = 'C:\\Users\\fixture\\AppData\\Local\\Programs\\Odin'
    # Inert expansion/identity adapters never consult registry, user config or binaries.
    ident = lambda v: v.lower().rstrip('\\/')
    expand_fixture = lambda v: v['raw'].replace('%LOCALAPPDATA%', 'C:\\Users\\fixture\\AppData\\Local') if v['kind'] == REG_EXPAND_SZ else v['raw']
    old = {'present': True, 'raw': 'legacy;quoted path;;tail;', 'kind': REG_EXPAND_SZ}
    proposed, changed = prepend_value(old, install, expand_fixture, ident)
    check('prepend_preserves_entire_old_raw_suffix', changed and proposed['raw'] == install + ';' + old['raw'])
    check('empty_segments_quotes_order_kind_preserved', proposed['kind'] == old['kind'] and proposed['raw'].endswith(old['raw']))
    later = {**old, 'raw': 'legacy;' + install + ';tail'}
    value, changed = prepend_value(later, install, expand_fixture, ident)
    check('existing_later_entry_preserved_instead_of_removed', changed and value['raw'] == install + ';' + later['raw'])
    first = {**old, 'raw': install + ';legacy'}
    value, changed = prepend_value(first, install, expand_fixture, ident)
    check('already_first_noop_exact_raw_kind', not changed and value == first)
    first = {**old, 'raw': '%LOCALAPPDATA%\\Programs\\Odin;legacy'}
    value, changed = prepend_value(first, install, expand_fixture, ident)
    check('expand_sz_existing_first_noop', not changed and value == first)
    value, changed = prepend_value({**first, 'kind': REG_SZ}, install, expand_fixture, ident)
    check('reg_sz_percent_value_not_silently_expanded', changed and value['raw'] == install + ';' + first['raw'])
    for kind in (REG_SZ, REG_EXPAND_SZ):
        value, changed = prepend_value({**old, 'kind': kind}, install, expand_fixture, ident)
        check('kind_' + str(kind) + '_retained', value['kind'] == kind)
    value, changed = prepend_value({**old, 'raw': ''}, install, expand_fixture, ident)
    check('empty_raw_path_retains_original_empty_segment', changed and value['raw'] == install + ';')
    compare_expected(dict(old), old)
    check('exact_raw_and_kind_guard_accepts_only_same_snapshot', True)
    reject('raw_value_conflict_rejected', lambda: compare_expected({**old, 'raw': old['raw'] + 'new'}, old))
    reject('kind_only_conflict_rejected', lambda: compare_expected({**old, 'kind': REG_SZ}, old))
    reject('absent_value_no_implicit_kind_created', lambda: prepend_value({'present': False, 'raw': None, 'kind': None}, install, expand_fixture, ident))
    reject('unsupported_registry_kind_rejected', lambda: prepend_value({**old, 'kind': 7}, install, expand_fixture, ident))
    reject('nul_raw_path_rejected', lambda: prepend_value({**old, 'raw': 'a\0b'}, install, expand_fixture, ident))
    reject('self_referential_expand_sz_path_rejected', lambda: expand({**old, 'raw': '%Path%;legacy'}))
    check('effective_path_uses_machine_then_user_sources', effective_path({'present': True, 'raw': 'machine', 'kind': REG_SZ}, {'present': True, 'raw': 'user', 'kind': REG_SZ}) == 'machine;user')
    public = public_value(old)
    check('public_value_never_includes_raw_path', 'raw' not in public and old['raw'] not in json.dumps(public))
    check('public_state_kind_and_digest_recorded', public['registry_kind'] == REG_EXPAND_SZ and len(public['raw_and_kind_sha256']) == 64)
    check('extended_drive_spelling_same_as_plain', identity('\\\\?\\C:\\fixture\\odin.exe') == identity('C:\\fixture\\odin.exe'))
    reject('extended_unc_not_stripped', lambda: identity('\\\\?\\UNC\\host\\share'))
    reject('device_namespace_not_stripped', lambda: identity('\\\\.\\C:\\fixture'))
    print(json.dumps({'status': 'PASS', 'scope': 'INERT_PATH_GUARDS_ONLY', 'checks': checks,
                      'registry_read': 'NOT_RUN', 'registry_write': 'NOT_RUN', 'fresh_cli_execution': 'NOT_RUN',
                      'private_acl_execution': 'NOT_RUN'}, indent=2))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--check-fixtures', action='store_true')
    modes.add_argument('--snapshot', action='store_true')
    modes.add_argument('--apply', action='store_true')
    parser.add_argument('--snapshot-path')
    parser.add_argument('--expected-snapshot-sha256')
    args = parser.parse_args()
    if args.check_fixtures:
        return inert_checks()
    if args.snapshot:
        return snapshot()
    if not args.snapshot_path or not args.expected_snapshot_sha256:
        parser.error('--apply requires --snapshot-path and --expected-snapshot-sha256')
    return apply(args.snapshot_path, args.expected_snapshot_sha256)


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    raise SystemExit(main())
