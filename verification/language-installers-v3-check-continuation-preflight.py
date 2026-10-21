"""Check evidence overwrite protection with inert files, never an Odin process."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('continuation', ROOT/'verification/language-installers-v3-continue.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
results = []
with tempfile.TemporaryDirectory(prefix='odin-continuation-preflight-') as directory:
    runner.ROOT = Path(directory)
    (runner.ROOT/'verification').mkdir()
    binary = runner.ROOT/'inert-fixture.txt'
    binary.write_bytes(b'not executable; no native process may be started\n')
    sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    base = ['continuation', '--binary', str(binary), '--expected-binary-sha256', sha,
            '--output', str(runner.ROOT/'summary.json'), '--logs', str(runner.ROOT/'logs')]
    cases = [
        ('existing_child_report', runner.ROOT/'verification/language-installers-v3-continued-cli.json', FileExistsError, []),
        ('existing_summary', runner.ROOT/'summary.json', FileExistsError, []),
        ('existing_log_path', runner.ROOT/'logs', FileExistsError, []),
        ('invalid_prefix', None, SystemExit, ['--artifact-prefix', '../escape']),
        ('summary_child_collision', None, ValueError,
         ['--artifact-prefix', 'fresh', '--output', str(runner.ROOT/'verification/fresh-cli.json')]),
    ]
    for name, existing, error_type, extra in cases:
        if existing:
            existing.write_bytes(b'preserved evidence fixture\n')
        before = existing.read_bytes() if existing else None
        with patch.object(sys, 'argv', base+extra), \
             patch.object(runner.subprocess, 'check_output', side_effect=AssertionError('Native process forbidden')) as check, \
             patch.object(runner.subprocess, 'run', side_effect=AssertionError('Native process forbidden')) as run, \
             contextlib.redirect_stderr(io.StringIO()):
            try:
                runner.main()
            except error_type as error:
                if isinstance(error, SystemExit):
                    assert error.code == 2
            else:
                raise AssertionError(f'{name}: missing preflight rejection')
            assert check.call_count == run.call_count == 0
        assert not (runner.ROOT/'logs').is_dir()
        if existing:
            assert existing.read_bytes() == before
            existing.unlink()
        results.append({'name': name, 'status': 'PASS', 'native_process_calls': 0})
print(json.dumps({'scope': 'INERT_QA_RUNNER_PREFLIGHT_ONLY', 'status': 'PASS', 'checks': results}))
