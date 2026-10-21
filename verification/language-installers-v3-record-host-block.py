"""Correct an observation wrapper error without retrying a blocked executable."""
import datetime
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
report = ROOT/'verification/language-installers-v3-windows-build-final.json'
original = report.with_name('language-installers-v3-windows-build-final-initial.json')
blocked = ROOT/'verification/language-installers-v3-final-host-block.json'
binary = ROOT/'target/release/odin.exe'
expected = '83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44'
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
if original.exists() or blocked.exists():
    raise FileExistsError('Observation history already exists; never overwrite it')
if digest(binary) != expected:
    raise ValueError('The administrator-selected candidate must remain unchanged')
shutil.copyfile(report, original)
observation = {
    'scope': 'PRESERVED_WINDOWS_RELEASE_BUILD_AND_FAILED_HOST_EXECUTION_OBSERVATIONS',
    'status': 'BLOCKED', 'recorded_utc': datetime.datetime.now(datetime.UTC).isoformat(),
    'binary_path': str(binary), 'binary_sha256': expected,
    'compiled_source_head_sha': '69e769be9baa4e825139b72ce1a9b48bf8143114',
    'native_command_attempted': [str(binary), 'metadata'],
    'native_command_executed': False, 'native_exit_code': None,
    'observations': [
        {'driver': 'PowerShell native invocation after successful cargo build',
         'error': 'StandardOutputEncoding is only supported when standard output is redirected.',
         'result': 'INVOCATION_ERROR_BEFORE_NATIVE_EXECUTION',
         'incorrect_stale_LASTEXITCODE': 0, 'metadata_stdout': ''},
        {'driver': 'C:/Python314/python.exe subprocess.run(capture_output=True, timeout=30)',
         'error': 'OSError [WinError 4551]: application control policy blocked this file',
         'result': 'CREATEPROCESS_DENIED_BEFORE_NATIVE_EXECUTION', 'winerror': 4551},
    ],
    'correction': 'The original build exited zero. Its metadata_exit_code=0 came from stale PowerShell LASTEXITCODE after an invocation error and is not a native result. That original report is preserved verbatim; the canonical build report marks metadata blocked/unexecuted.',
    'original_report': original.relative_to(ROOT).as_posix(),
    'original_report_sha256': digest(original),
    'host_event_log': 'verification/language-installers-v3-host-codeintegrity.json',
    'host_event_log_sha256': digest(ROOT/'verification/language-installers-v3-host-codeintegrity.json'),
    'user_response': 'User will ask the administrator to allow this candidate under the existing policy.',
    'administrator_state_change_confirmed': False,
    'policy_modified_by_agent': False, 'candidate_copied_after_block': False,
    'windows_final_metadata_native_matrix_and_installer': 'BLOCKED',
    'linux_guest_work': 'CONTINUES_INDEPENDENTLY',
}
blocked.write_text(json.dumps(observation, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')
value = json.loads(report.read_text(encoding='utf-8'))
value.update(metadata_exit_code=None, metadata_status='BLOCKED_BEFORE_EXECUTION',
             metadata_output='', metadata_observation_report=blocked.relative_to(ROOT).as_posix(),
             metadata_observation_report_sha256=digest(blocked),
             original_observation_report=original.relative_to(ROOT).as_posix(),
             original_observation_report_sha256=digest(original))
report.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')
print(json.dumps({'build': value['status'], 'native_execution': 'BLOCKED',
                  'binary_sha256': expected, 'original_report_sha256': digest(original),
                  'corrected_report_sha256': digest(report), 'blocked_report_sha256': digest(blocked)}))
