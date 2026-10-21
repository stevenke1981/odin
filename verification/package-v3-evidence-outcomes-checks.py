"""Inert QA classification regression checks; never execute Odin or backend programs."""
import copy
import hashlib
import importlib.util
import json
import pathlib
import sys


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


export_path = pathlib.Path(sys.argv[1])
collector_path = pathlib.Path(sys.argv[2])
output = pathlib.Path(sys.argv[3])
exporter = load(export_path, 'task_linux_exporter')
collector = load(collector_path, 'task_linux_collector')
commands = [{'name': 'inert_fixture_only', 'exit_code': 0, 'status': 'PASS'}]
smokes = {'cli': {'status': 'PASS'}}
installer = {'status': 'PASS'}
base = {'status': 'PASS', 'revision': collector.APP,
        'driver': {'path': 'run-linux-candidate-v5.py',
                   'sha256': '7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984'},
        'commands': commands}
checks = []
for name, status, driver_error, expected in (
        ('control_complete_runner', 'PASS', None, True),
        ('all_commands_zero_but_runner_fail', 'FAIL', None, False),
        ('all_commands_zero_but_runner_fail_and_error', 'FAIL', 'inert failure fixture', False),
        ('all_commands_zero_but_runner_pass_with_error', 'PASS', 'inert failure fixture', False)):
    runner = copy.deepcopy(base)
    runner['status'] = status
    if driver_error is not None:
        runner['driver_error'] = driver_error
    actual_export = exporter.candidate_passed(runner, commands, smokes, installer)
    try:
        collector.require_successful_runner(runner)
        actual_collect = True
    except ValueError:
        actual_collect = False
    assert actual_export == actual_collect == expected, name
    checks.append({'name': name, 'expected_accepted': expected,
                   'exporter_accepted': actual_export, 'collector_accepted': actual_collect, 'status': 'PASS'})
for name, driver in (('collector_rejects_legacy_driver', {'path': 'run-linux-candidate-v3.py', 'sha256': '62acc5fc7bd10341c6dd0f7e51326f3665a96b5b7abf348a01c0b70c98d514bf'}),
                     ('collector_rejects_missing_driver', None)):
    runner = copy.deepcopy(base)
    if driver is None:
        del runner['driver']
    else:
        runner['driver'] = driver
    try:
        collector.require_successful_runner(runner)
    except ValueError:
        checks.append({'name': name, 'expected_accepted': False, 'collector_accepted': False, 'status': 'PASS'})
    else:
        raise AssertionError(name)
report = {'scope': 'INERT_QA_OUTCOME_CLASSIFICATION_REGRESSIONS', 'status': 'PASS',
          'native_app_processes_executed': 0, 'backend_processes_executed': 0,
          'exporter_sha256': hashlib.sha256(export_path.read_bytes()).hexdigest(),
          'collector_sha256': hashlib.sha256(collector_path.read_bytes()).hexdigest(),
          'check_script_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
          'checks': checks}
assert not output.exists()
output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8', newline='\n')
print(json.dumps(report))
