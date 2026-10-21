"""Bind actual transferred Linux VM results into public, source-bound evidence."""
import copy
import hashlib
import json
import pathlib
import socket

ROOT = pathlib.Path(r'C:\Users\steven\.codex\worktrees\odin-packaging\odin')
TOOLS = pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
ARTIFACTS = pathlib.Path(r'E:\odin\dist\package-v3-linux-0fe03ba')
APP = '0fe03ba30da9600c25588d506d7e094df89cf56f'
PACKAGING = 'a8399993d7c528ad4858b2e4e246e5994eab4bb8'
PUBLIC_PREFIX = 'package-v3-linux-final-'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalized(data):
    if not data:
        return data
    text = data.decode('utf-8')
    return ('\n'.join(line.rstrip() for line in text.replace('\r\n', '\n').splitlines()).rstrip()+'\n').encode('utf-8')


def save(name, data, normalize=False):
    output = ROOT/'verification'/name
    assert not output.exists(), output
    content = normalized(data) if normalize else data
    output.write_bytes(content)
    reference = {'path': output.relative_to(ROOT).as_posix(), 'sha256': sha(content)}
    if content != data:
        reference['original_transferred_sha256'] = sha(data)
    return reference


def require_successful_runner(runner):
    if runner.get('status') != 'PASS' or 'driver_error' in runner:
        raise ValueError('The original native runner must have completed successfully without driver_error')
    if runner.get('revision') != APP:
        raise ValueError('The native runner source revision differs')
    if runner.get('driver') != {
            'path': 'run-linux-candidate-v5.py',
            'sha256': '7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984'}:
        raise ValueError('The original native runner driver path/hash differs')
    for command in runner.get('commands', []):
        if command.get('exit_code') != 0 or command.get('status') != 'PASS':
            raise ValueError('The native runner contains an unsuccessful command')


def main():
    transfer = json.loads((ARTIFACTS/'transfer-manifest.json').read_text(encoding='utf-8'))
    assert transfer['source_revision'] == APP and transfer['packaging_revision'] == PACKAGING
    receipt = json.loads((ARTIFACTS/'transfer-receipt.json').read_text(encoding='utf-8'))
    assert receipt['status'] == 'PASS'
    assert receipt['manifest_sha256'] == sha((ARTIFACTS/'transfer-manifest.json').read_bytes())
    assert len(receipt['received']) == len(transfer['files'])
    for row in transfer['files']:
        path = ARTIFACTS/row['path']
        assert path.is_file() and not path.is_symlink()
        assert path.stat().st_size == row['bytes'] and sha(path.read_bytes()) == row['sha256'], row['path']
    with socket.socket() as probe:
        probe.settimeout(2)
        assert probe.connect_ex(('127.0.0.1', 47654)) != 0, 'Receiver must have shut down'

    raw = json.loads((ARTIFACTS/'linux-validation.json').read_text(encoding='utf-8'))
    runner = json.loads((ARTIFACTS/'runner-summary.json').read_text(encoding='utf-8'))
    require_successful_runner(runner)
    assert sha((ARTIFACTS/runner['driver']['path']).read_bytes()) == runner['driver']['sha256']
    actual_commands = runner['commands']+[
        json.loads((ARTIFACTS/(name+'.command.json')).read_text(encoding='utf-8'))
        for name in ('final-package', 'final-checksums')]+[
        json.loads((ARTIFACTS/'installer-command.json').read_text(encoding='utf-8'))]
    assert raw['commands'] == actual_commands
    assert all(row['exit_code'] == 0 and row['status'] == 'PASS' for row in actual_commands)
    assert raw['status'] == 'PASS' and raw['app_build_revision'] == APP
    assert raw['packaging_revision'] == raw['bundled_docs_revision'] == PACKAGING
    assert raw['workspace_tests']['passed'] == 211 and raw['workspace_tests']['failed'] == 0
    assert raw['workspace_tests']['ignored'] == 3
    assert raw['source_manifest']['checked_files'] == 688
    assert raw['packaging_source_manifest']['checked_files'] == 728
    assert raw['product_equivalence']['compared_files'] == 99
    assert raw['installer_lifecycle']['status'] == 'PASS'
    assert all(raw['installer_lifecycle'][key] == 0 for key in ('install_exit_code', 'reinstall_exit_code', 'uninstall_exit_code'))
    assert raw['signal_shutdown_acceptance']['status'] == 'NOT_RUN'

    refs = {}
    for row in transfer['files']:
        name = row['path']
        if name == 'odin' or name.endswith(('.deb', '.tar.gz')):
            continue
        public_name = PUBLIC_PREFIX+(name[:-4]+'.txt' if name.endswith('.log') else name)
        # Keep immutable source/manifest/inventory bytes exact; normalize text logs only.
        refs[name] = save(public_name, (ARTIFACTS/name).read_bytes(), normalize=name.endswith('.log'))

    report = copy.deepcopy(raw)
    for command in report['commands']:
        original = command['log']
        assert sha((ARTIFACTS/original).read_bytes()) == command['log_sha256']
        reference = refs[original]
        command.update(log=reference['path'], log_sha256=reference['sha256'], original_guest_log_sha256=command['log_sha256'])
    for key in ('source_manifest', 'packaging_source_manifest', 'payload_inventory'):
        original = report[key]['path']
        assert report[key]['sha256'] == sha((ARTIFACTS/original).read_bytes())
        report[key].update(path=refs[original]['path'], sha256=refs[original]['sha256'])
    original = report['installer_lifecycle']['report']['path']
    assert report['installer_lifecycle']['report']['sha256'] == sha((ARTIFACTS/original).read_bytes())
    report['installer_lifecycle']['report'] = refs[original]
    for artifact in report['artifacts']:
        path = ARTIFACTS/artifact['path']
        assert sha(path.read_bytes()) == artifact['sha256'] and path.stat().st_size == artifact['bytes']
        artifact['path'] = 'dist/package-v3-linux-0fe03ba/'+artifact['path']

    first_preflight = json.loads((ARTIFACTS/'installer-preflight-first-failure.json').read_text(encoding='utf-8'))
    assert first_preflight['status'] == 'FAIL' and first_preflight['outer_exit_code'] == 1
    assert first_preflight['git_reported_exit_code'] == 128 and first_preflight['uid'] == 0
    assert first_preflight['installer_command_started'] is False and first_preflight['dpkg_started'] is False
    assert first_preflight['driver_sha256'] == sha((ARTIFACTS/'run-linux-installer-v3-before-owner-preflight.py').read_bytes())
    assert first_preflight['log_sha256'] == sha((ARTIFACTS/'installer-preflight-first-failure.log').read_bytes())
    assert sha((ARTIFACTS/'run-linux-installer-v3.py').read_bytes()) == 'c1b4d33aa4fc32cbd76e107d571f24edcd4f9603be5fe0e72db9b0a149c062c2'

    host_refs = {}
    for name in ('native-0fe03ba-stage.json', 'native-0fe03ba-stage.txt',
                 'native-a839999-packaging-stage.json', 'native-a839999-packaging-stage.txt',
                 'native-final-helpers-stage.json', 'native-final-helpers-stage.txt'):
        path = TOOLS/'vm'/name
        host_refs[name] = save(PUBLIC_PREFIX+name, path.read_bytes(), normalize=True)
    for name in ('transfer-manifest.json', 'transfer-receipt.json'):
        host_refs[name] = save(PUBLIC_PREFIX+name, (ARTIFACTS/name).read_bytes())
    for source, public_name in (
            (pathlib.Path(r'E:\odin\.odin\receive_linux_artifacts_v3.py'), 'artifact-receiver.py'),
            (pathlib.Path(r'E:\odin\.odin\start_linux_receiver_v3.py'), 'receiver-launcher.py'),
            (TOOLS/'transfer/upload-linux-v3.py', 'artifact-uploader.py'),
            (TOOLS/'receiver-smoke-report-03.json', 'receiver-validation.json')):
        host_refs[public_name] = save(PUBLIC_PREFIX+public_name, source.read_bytes())
    report.update(
        evidence=refs,
        transfer_evidence=host_refs,
        qa_preflight_history=[{
            'status': 'FAIL_RETAINED_BEFORE_SUCCESSFUL_OWNER_PREFLIGHT',
            'outer_exit_code': first_preflight['outer_exit_code'],
            'git_reported_exit_code': first_preflight['git_reported_exit_code'],
            'receipt': refs['installer-preflight-first-failure.json'],
            'log': refs['installer-preflight-first-failure.log'],
            'driver': refs['run-linux-installer-v3-before-owner-preflight.py'],
            'successful_driver': refs['run-linux-installer-v3.py'],
            'actual_followup': refs['installer-command.json'],
            'resolution': 'Only the read-only Git source preflight changed to run as checkout owner odin-qa. Package-management QA still ran as root in the disposable guest, without global safe.directory changes or an app rebuild.'}],
        qa_evidence_repair={'finding': 'R4-E01', 'status': 'CLOSED',
                            'report': {'path': 'verification/package-v3-evidence-outcomes-02.json',
                                       'sha256': sha((ROOT/'verification/package-v3-evidence-outcomes-02.json').read_bytes())}},
        transport={'status': 'PASS', 'mode': 'TASK_ONLY_FIXED_MANIFEST_LOOPBACK_HTTP_POST',
                   'files_verified': len(transfer['files']), 'bytes_verified': sum(row['bytes'] for row in transfer['files']),
                   'receiver_closed_after_completion': True, 'expected_file_sha256_and_lengths_verified': True},
        normalization='Public text logs use UTF-8 LF and remove trailing whitespace; original guest log digests and exact transferred source/manifest/receipt bytes are preserved. Actual argv, exits and outcomes are unchanged.',
        acceptance_scope='Native application build and execution, workspace gates, fixture/CLI/transport smokes, unsigned Debian and portable install lifecycle inside a disposable QEMU TCG Debian 12 x86_64 VM. This does not establish Linux OS sandboxing, real Linux language backend parity, macOS, or final Windows acceptance.')
    reference = save('package-v3-linux-final.json', (json.dumps(report, indent=2)+'\n').encode('utf-8'))
    print(json.dumps({'status': 'PASS', 'report': reference, 'binary_sha256': report['binary_sha256'],
                      'workspace': report['workspace_tests'], 'smokes': report['smokes'],
                      'installer_checks': len(report['installer_lifecycle']['checks']),
                      'artifacts': report['artifacts'], 'public_files': len(refs)+len(host_refs)+1}))


if __name__ == '__main__':
    main()
