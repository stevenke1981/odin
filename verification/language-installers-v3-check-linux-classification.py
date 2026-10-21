"""Regression checks for documentation classification, using inert evidence fixtures."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('summary', ROOT/'verification/language-installers-v3-summarize.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
results = []
with tempfile.TemporaryDirectory(prefix='odin-linux-classification-') as directory:
    module.ROOT = Path(directory)
    (module.ROOT/'verification').mkdir()

    def reference(name):
        path = module.ROOT/name
        path.write_bytes(('INERT DOCUMENTATION FIXTURE: '+name+'\n').encode())
        return {'path': name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    binary = reference('inert-linux-candidate.txt')
    module.LINUX_BINARY_SHA256 = binary['sha256']
    source = dict(reference('source.json'), checked_files=1)
    package_source = dict(reference('package-source.json'), checked_files=1)
    inventory = dict(reference('inventory.json'), entries=1)
    lifecycle = reference('installer.json')
    logs = [reference(name+'.txt') for name in ('build', 'metadata')]
    artifacts = [binary, reference('inert.deb'), reference('inert.tar.gz')]
    for artifact in artifacts:
        artifact['bytes'] = (module.ROOT/artifact['path']).stat().st_size
    seed = {
        'scope': 'ACTUAL_LINUX_VM_BUILD_WORKSPACE_GATES_SMOKES_AND_INSTALLERS', 'status': 'PASS',
        'app_build_revision': module.PRODUCT_REVISION, 'packaging_revision': module.PACKAGING_REVISION,
        'bundled_docs_revision': module.PACKAGING_REVISION, 'linux_target': module.LINUX_TARGET,
        'build_metadata': {'revision': module.PRODUCT_REVISION, 'target': module.LINUX_TARGET,
                           'profile': 'release', 'tracked_source_dirty': False},
        'binary_sha256': binary['sha256'], 'elf': {'format': 'ELF64_X86_64', 'pt_interp': None},
        'source_manifest': source, 'packaging_source_manifest': package_source,
        'product_equivalence': {'status': 'PASS', 'compared_files': 1},
        'artifacts': artifacts, 'payload_inventory': inventory,
        'installer_lifecycle': {'status': 'PASS', 'report': lifecycle,
                                'install_exit_code': 0, 'reinstall_exit_code': 0, 'uninstall_exit_code': 0},
        'signal_shutdown_acceptance': {'requirement': 'ODIN-029-A2', 'status': 'NOT_RUN'},
        'commands': [{'name': name, 'argv': argv, 'exit_code': 0, 'status': 'PASS',
                      'log': log['path'], 'log_sha256': log['sha256']}
                     for name, argv, log in zip(('build', 'metadata'),
                         (['cargo', 'build', '--release', '--locked'], ['inert-fixture', 'metadata']), logs)],
    }

    def evaluate(value, name=module.LINUX_CANONICAL_REPORT):
        (module.ROOT/module.LINUX_CANONICAL_REPORT).write_text(json.dumps(value), encoding='utf-8')
        with patch.object(module.subprocess, 'check_output', side_effect=AssertionError('No process allowed')) as check, \
             patch.object(module.subprocess, 'run', side_effect=AssertionError('No process allowed')) as run:
            result = module.linux_evidence(name)
            assert check.call_count == run.call_count == 0
            return result['acceptance_results']

    actual = evaluate(seed)
    assert [actual['ODIN-029-A'+str(i)]['status'] for i in (1, 2, 3)] == ['PARTIAL', 'NOT_RUN', 'PARTIAL']
    results.append('each_criterion_classified_independently')
    for name, mutation in [
        ('wrong_source', lambda x: x.update(app_build_revision='0'*40)),
        ('wrong_packaging_source', lambda x: x.update(packaging_revision='0'*40)),
        ('wrong_target', lambda x: x.update(linux_target='x86_64-pc-windows-msvc')),
        ('wrong_metadata', lambda x: x['build_metadata'].update(tracked_source_dirty=True)),
        ('diagnostic_scope', lambda x: x.update(scope='DIAGNOSTIC_CROSS_BUILD_NOT_FINAL')),
        ('invalid_log_hash', lambda x: x['commands'][0].update(log_sha256='0'*64)),
        ('false_exit_status', lambda x: x['commands'][0].update(exit_code=101)),
        ('inferred_signal_acceptance', lambda x: x['signal_shutdown_acceptance'].update(status='PASS')),
    ]:
        value = copy.deepcopy(seed)
        mutation(value)
        try:
            evaluate(value)
        except ValueError:
            results.append(name)
        else:
            raise AssertionError(name+': expected rejection')
    try:
        evaluate(seed, 'verification/diagnostic.json')
    except ValueError:
        results.append('diagnostic_report_path')
    else:
        raise AssertionError('Diagnostic report accepted')
    value = copy.deepcopy(seed)
    value['artifacts'] = value['artifacts'][:-1]
    assert evaluate(value)['ODIN-029-A3']['status'] == 'NOT_RUN'
    results.append('missing_archive_keeps_ac3_not_run')
    value = copy.deepcopy(seed)
    value['commands'] = value['commands'][:1]
    assert evaluate(value)['ODIN-029-A1']['status'] == 'NOT_RUN'
    results.append('missing_metadata_keeps_ac1_not_run')
print(json.dumps({'scope': 'INERT_DOCUMENTATION_CLASSIFIER_ONLY_NOT_LINUX_ACCEPTANCE',
    'status': 'PASS', 'native_process_calls': 0, 'checks': [{'name': name, 'status': 'PASS'} for name in results]}))
