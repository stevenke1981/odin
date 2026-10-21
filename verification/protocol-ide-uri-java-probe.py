"""Explicit local JDTLS probe; not repository-driven product configuration."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--fixture-module-dir', type=Path, required=True)
parser.add_argument('--tools-root', type=Path, required=True)
args = parser.parse_args()
sys.path.insert(0, str(args.fixture_module_dir))
from real_ide_backends import fixture

with tempfile.TemporaryDirectory(prefix='odin-protocol-java-probe-') as directory:
    item = fixture(Path(directory), args.tools_root, 'java', 'a')
    profile = {key: getattr(item, key) for key in (
        'root', 'source', 'consumer', 'program', 'argv', 'initialization', 'settings')}
    inputs = [args.fixture_module_dir / 'real_ide_backends.py', item.program,
              args.tools_root / 'jdtls/config_win/config.ini',
              *args.tools_root.glob('jdtls/plugins/org.eclipse.equinox.launcher_*.jar'),
              *args.tools_root.glob('jdtls/plugins/org.eclipse.jdt.ls.core_*.jar')]
    print('JAVA_PROBE_SETUP ' + json.dumps({
        'input_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs},
        'fixture_sha256': {path: hashlib.sha256((item.root / path).read_bytes()).hexdigest()
                           for path in (item.source, item.consumer, '.project', '.classpath')},
        'settings': item.settings, 'initialization': item.initialization,
    }), flush=True)
    environment = dict(os.environ, ODIN_JAVA_RENAME_PROFILE_JSON=json.dumps(profile, default=str))
    raise SystemExit(subprocess.run([
        'cargo', 'test', '-p', 'odin-protocol', '--locked',
        'real_java_rename_workspace_edit_probe', '--', '--ignored', '--nocapture',
    ], env=environment, check=False).returncode)
