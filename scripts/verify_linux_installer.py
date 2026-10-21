#!/usr/bin/env python3
"""Execute install/reinstall/remove QA inside an explicitly selected disposable Linux VM."""
import argparse
import hashlib
import json
import os
import platform
import subprocess
import tarfile
import tempfile
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--portable', type=Path, required=True)
    parser.add_argument('--expected-binary-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--disposable-vm', action='store_true', help='Explicit consent to install/remove odin-agent in this guest')
    args = parser.parse_args()
    if not args.disposable_vm or platform.system() != 'Linux' or os.geteuid() != 0:
        raise ValueError('Run as root only in a disposable Linux validation VM with --disposable-vm')
    args.output.mkdir(parents=True, exist_ok=False)
    commands = []
    checks = []
    error = None
    installed = False
    metadata = None

    def check(name, passed):
        checks.append({'name':name, 'status':'PASS' if passed else 'FAIL'})
        if not passed:
            raise AssertionError(name)

    def run(command, allowed=(0,)):
        completed = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=120)
        commands.append({'command':[str(item) for item in command], 'exit_code':completed.returncode, 'stdout':completed.stdout, 'stderr':completed.stderr})
        if completed.returncode not in allowed:
            raise RuntimeError(f'Command exit {completed.returncode}: {command[0]}')
        return completed

    try:
        query = run(['dpkg-query', '-W', '-f=${Status}', 'odin-agent'], allowed=(0, 1))
        check('fresh_guest_has_no_odin_package', query.returncode == 1 or query.stdout.strip() != 'install ok installed')
        check('no_unmanaged_odin_binary', not Path('/usr/bin/odin').exists())
        fields = run(['dpkg-deb', '--field', str(args.package), 'Package', 'Architecture'])
        check('native_debian_package_identity', 'Package: odin-agent' in fields.stdout and 'Architecture: amd64' in fields.stdout)
        with tarfile.open(args.portable, 'r:gz') as archive:
            members = archive.getmembers()
            check('portable_regular_files_only', all(item.isfile() for item in members))
            prefixes = {item.name.split('/')[0] for item in members}
            check('portable_single_safe_prefix', len(prefixes) == 1 and not any(item.name.startswith('/') or '..' in Path(item.name).parts for item in members))
            prefix = prefixes.pop()+'/'
            contents = {item.name[len(prefix):]:archive.extractfile(item).read() for item in members}
        inventory = json.loads(contents['SHA256SUMS.json'])
        check('portable_inventory_has_exact_payload', {row['path'] for row in inventory} == set(contents)-{'SHA256SUMS.json'})
        check('portable_inventory_sha256', all(hashlib.sha256(contents[row['path']]).hexdigest() == row['sha256'] for row in inventory))
        check('portable_selected_binary_sha256', hashlib.sha256(contents['odin']).hexdigest() == args.expected_binary_sha256.lower())
        with tempfile.TemporaryDirectory(prefix='odin-portable-qa-') as temporary:
            portable_binary = Path(temporary)/'odin'
            portable_binary.write_bytes(contents['odin'])
            portable_binary.chmod(0o755)
            portable_metadata = json.loads(run([str(portable_binary), 'metadata']).stdout)
            check('portable_native_metadata', portable_metadata['target'] == 'x86_64-unknown-linux-musl' and portable_metadata['tracked_source_dirty'] is False)
        run(['dpkg', '--install', str(args.package)])
        installed = True
        check('installed_binary_sha256', digest(Path('/usr/bin/odin')) == args.expected_binary_sha256.lower())
        metadata = json.loads(run(['/usr/bin/odin', 'metadata']).stdout)
        check('installed_native_metadata_matches_portable', metadata == portable_metadata)
        check('installed_payload_matches_inventory', all(digest(Path('/usr/bin/odin') if row['path'] == 'odin' else Path('/usr/share/odin-agent')/row['path']) == row['sha256'] for row in inventory))
        tools = json.loads(run(['/usr/bin/odin', 'tools']).stdout)
        check('installed_tool_contracts', tools == json.loads(contents['schemas/tools.json']))
        languages = json.loads(run(['/usr/bin/odin', 'languages']).stdout)
        check('installed_language_registry', {'rust','python','typescript','go','java','csharp'}.issubset({item['id'] for item in languages['adapters']}))
        sentinel = Path('/usr/share/odin-agent/operator-notes.txt')
        sentinel.write_text('operator file: retain on reinstall/remove\n', encoding='utf-8')
        run(['dpkg', '--install', str(args.package)])
        check('same_version_reinstall_binary_sha256', digest(Path('/usr/bin/odin')) == args.expected_binary_sha256.lower())
        check('reinstall_retains_operator_file', sentinel.read_text(encoding='utf-8') == 'operator file: retain on reinstall/remove\n')
        run(['dpkg', '--remove', 'odin-agent'])
        installed = False
        check('uninstall_removes_owned_binary', not Path('/usr/bin/odin').exists())
        check('uninstall_removes_owned_payload', not any((Path('/usr/share/odin-agent')/row['path']).exists() for row in inventory if row['path'] != 'odin'))
        check('uninstall_retains_operator_file', sentinel.read_text(encoding='utf-8') == 'operator file: retain on reinstall/remove\n')
    except Exception as exception:
        error = f'{type(exception).__name__}: {exception}'
    finally:
        if installed:
            try:
                run(['dpkg', '--remove', 'odin-agent'])
            except Exception as exception:
                commands.append({'cleanup_error':str(exception)})
    report = {'scope':'DISPOSABLE_LINUX_DEBIAN_INSTALL_REINSTALL_REMOVE', 'status':'FAIL' if error else 'PASS',
              'platform':platform.platform(), 'package_sha256':digest(args.package), 'portable_sha256':digest(args.portable),
              'binary_sha256':args.expected_binary_sha256.lower(), 'build':metadata,
              'checks':checks, 'commands':commands, 'error':error,
              'cross_version_upgrade':'NOT_RUN', 'interruption_rollback':'NOT_RUN',
              'linux_native_sandbox':'NOT_IMPLEMENTED', 'physical_linux_host':'NOT_RUN'}
    (args.output/'installer-report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8', newline='\n')
    print(json.dumps(report))
    if error:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
