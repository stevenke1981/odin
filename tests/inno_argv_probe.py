#!/usr/bin/env python3
"""Actual production Inno recipe -> Node argv regression; no LSP acceptance.

Builds and installs a private, unregistered probe containing Node and an inert
setup-lsps.mjs argv recorder. It neither loads nor edits an operator host config.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def digest(filename):
    return hashlib.sha256(filename.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--iscc', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if platform.system() != 'Windows':
        raise ValueError('Actual Windows and Inno Setup are required')
    node, iscc = args.node.resolve(strict=True), args.iscc.resolve(strict=True)
    if args.output is None:
        base = Path(tempfile.mkdtemp(prefix='odin-argv-probe-')).resolve()
    else:
        base = args.output.resolve()
        base.mkdir(parents=True, exist_ok=False)
    stage, app = base/'stage', base/'installed app space'
    host, receipt = base/'host space'/'host.toml', base/'receipt space.json'
    project = base/'project space'
    project.mkdir()
    stage.mkdir()
    host.parent.mkdir()
    (stage/'backends'/'node').mkdir(parents=True)
    shutil.copy2(node, stage/'backends'/'node'/'node.exe')
    (stage/'LICENSE').write_text('Inert Inno argv regression fixture. Not an Odin release.\n', encoding='utf-8')
    (stage/'README.md').write_text('No language support is provided by this probe.\n', encoding='utf-8')
    expected = ['--install', '--host-config', str(host), '--workspace-root', str(project)+'\\', '--replace-language-profiles', '--receipt', str(receipt)]
    existing_app = base/'existing host app space'
    existing_host = base/'existing host space'/'host.toml'
    existing_host.parent.mkdir()
    sentinel = b'# Existing operator host sentinel: retain exact bytes.\n'
    existing_host.write_bytes(sentinel)
    existing_receipt = base/'existing host no root receipt.json'
    existing_argv = ['--install', '--host-config', str(existing_host), '--receipt', str(existing_receipt)]
    failure_receipt = base/'intentional failure receipt.json'
    expected_file = {'argv':expected, 'script':str(app/'setup-lsps.mjs'), 'receipt':str(receipt), 'failure_receipt':str(failure_receipt), 'existing_argv':existing_argv, 'existing_script':str(existing_app/'setup-lsps.mjs'), 'existing_receipt':str(existing_receipt)}
    (stage/'expected-argv.json').write_text(json.dumps(expected_file), encoding='utf-8')
    (stage/'setup-lsps.mjs').write_text(
        "import fs from 'node:fs';\n"
        "import path from 'node:path';\n"
        "const expected=JSON.parse(fs.readFileSync(path.join(import.meta.dirname,'expected-argv.json'),'utf8'));\n"
        "const argv=process.argv.slice(2);\n"
        "if(argv[argv.indexOf('--receipt')+1]===expected.failure_receipt){fs.writeFileSync(expected.failure_receipt,JSON.stringify({status:'EXPECTED_FAILURE',argv,exit_code:17}),{flag:'wx'});process.exit(17); }\n"
        "const existing=argv[argv.indexOf('--receipt')+1]===expected.existing_receipt;\n"
        "const want=existing?expected.existing_argv:expected.argv;\n"
        "const script=existing?expected.existing_script:expected.script;\n"
        "const report=existing?expected.existing_receipt:expected.receipt;\n"
        "const ok=JSON.stringify(argv)===JSON.stringify(want) && process.argv[1]===script;\n"
        "fs.writeFileSync(report,JSON.stringify({scope:'ACTUAL_INNO_TO_NODE_ARGV_ONLY',status:ok?'PASS':'FAIL',argv,full_argv:process.argv,expected:want},null,2),{flag:'wx'});\n"
        "if(!ok)process.exitCode=13;\n", encoding='utf-8', newline='\n')
    commands, error = [], None
    installed = []
    shortcut = Path(os.environ['APPDATA'])/'Microsoft'/'Windows'/'Start Menu'/'Programs'/'Odin'/'Odin documentation.lnk'
    shortcut_before = shortcut.read_bytes() if shortcut.is_file() else None

    def run(command, timeout=120, inno=False):
        values = [str(value) for value in command]
        # Inno's Delphi parameter parser retains backslashes before closing
        # quotes. Passing Python's CRT-escaped list would duplicate the input
        # trailing slash before the production helper is ever exercised.
        actual_command = ' '.join(chr(34)+value+chr(34) for value in values) if inno else values
        completed = subprocess.run(actual_command, capture_output=True,
                                   text=True, encoding='utf-8', errors='replace', timeout=timeout,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        commands.append({'argv':values, 'inno_command_line':actual_command if inno else None, 'exit_code':completed.returncode,
                         'stdout':completed.stdout, 'stderr':completed.stderr})
        return completed

    try:
        compilation = run([iscc, '--no-compression', '--no-ide-signtools', '--no-signing',
                           '--define=AppVersion=0.2.0', f'--define=StageDir={stage}',
                           f'--define=OutputPath={base}', ROOT/'packaging'/'odin.iss'])
        if compilation.returncode != 0:
            raise RuntimeError('Probe recipe compilation failed')
        installer = base/'odin-0.2.0-windows-x64-setup.exe'
        result = run([installer, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
                      '/NOICONS', '/ODIN_QA=1', f'/DIR={app}', f'/ODIN_HOST_CONFIG={host}',
                      '/ODIN_WORKSPACE_ROOT='+str(project)+'\\', '/ODIN_REPLACE_LANGUAGE_PROFILES=1',
                      f'/ODIN_SETUP_RECEIPT={receipt}', f'/LOG={base / "install.log"}'], inno=True)
        if (app/'unins000.exe').is_file():
            installed.append(app)
        if result.returncode != 0:
            raise RuntimeError('Probe installer failed')
        actual = json.loads(receipt.read_text(encoding='utf-8'))
        if actual['status'] != 'PASS' or actual['argv'] != expected:
            raise AssertionError('Actual Node argv did not exactly preserve the trailing workspace delimiter')
        if host.exists():
            raise AssertionError('An argv probe must not create an actual host config')
        retained = run([installer, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
                        '/NOICONS', '/ODIN_QA=1', f'/DIR={existing_app}', f'/ODIN_HOST_CONFIG={existing_host}',
                        f'/ODIN_SETUP_RECEIPT={existing_receipt}', f'/LOG={base / "existing-host-no-root.log"}'], inno=True)
        if (existing_app/'unins000.exe').is_file():
            installed.append(existing_app)
        if retained.returncode != 0 or existing_host.read_bytes() != sentinel:
            raise AssertionError('Existing-host installation without a workspace root failed or changed the host')
        no_root = json.loads(existing_receipt.read_text(encoding='utf-8'))
        if no_root['status'] != 'PASS' or no_root['argv'] != existing_argv or '--workspace-root' in no_root['argv']:
            raise AssertionError('Existing-host Node argv must omit the workspace flag and retain all other arguments')
        failed_app = base/'intentional failed app space'
        forced = run([installer, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
                      '/NOICONS', '/ODIN_QA=1', f'/DIR={failed_app}', f'/ODIN_HOST_CONFIG={host}',
                      '/ODIN_WORKSPACE_ROOT='+str(project)+'\\', f'/ODIN_SETUP_RECEIPT={failure_receipt}',
                      f'/LOG={base / "intentional-failure.log"}'], inno=True)
        if (failed_app/'unins000.exe').is_file():
            installed.append(failed_app)
        if forced.returncode != 17 or not failure_receipt.is_file() or host.exists():
            raise AssertionError('Post-install Node failure must propagate exit 17 and retain an absent host config')
        # Invalid receipt arguments must be rejected in PrepareToInstall, before files are copied.
        invalid_app = base/'invalid receipt app'
        rejected = run([installer, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
                        '/NOICONS', '/ODIN_QA=1', f'/DIR={invalid_app}', f'/ODIN_HOST_CONFIG={host}',
                        '/ODIN_WORKSPACE_ROOT='+str(project)+'\\', '/ODIN_SETUP_RECEIPT=relative.json',
                        f'/LOG={base / "invalid-receipt.log"}'], inno=True)
        if rejected.returncode == 0 or (invalid_app/'setup-lsps.mjs').exists():
            raise AssertionError('Relative setup receipt was not rejected before payload copy')
    except Exception as exc:
        error = str(exc)
    finally:
        for index, installed_app in enumerate(installed):
            cleanup = run([installed_app/'unins000.exe', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART',
                           f'/LOG={base / ("uninstall-"+str(index)+".log")}'], inno=True)
            if cleanup.returncode != 0 or (installed_app/'setup-lsps.mjs').exists():
                error = error or 'Probe uninstall failed'
        shortcut_after = shortcut.read_bytes() if shortcut.is_file() else None
        if shortcut_after != shortcut_before:
            error = error or 'Isolated QA must not create, replace or remove the operator shortcut'
    report = {'scope':'ACTUAL_PRODUCTION_INNO_RECIPE_ARGV_REGRESSION_NOT_LSP_ACCEPTANCE',
              'status':'FAIL' if error else 'PASS', 'error':error,
              'observed_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'recipe_sha256':digest(ROOT/'packaging'/'odin.iss'),
              'test_driver_sha256':digest(Path(__file__)), 'node_sha256':digest(node),
              'iscc_sha256':digest(iscc), 'qa_directory':str(base), 'commands':commands,
              'argv_receipt':str(receipt), 'expected_node_argv':expected,
              'existing_host_no_root_receipt':str(existing_receipt),
              'existing_host_preserved':existing_host.read_bytes() == sentinel,
              'forced_setup_failure_exit_code':17, 'failure_receipt':str(failure_receipt),
              'qa_shortcut_preserved':shortcut_after == shortcut_before,
              'backend_health':'NOT_RUN', 'semantic_acceptance':'NOT_RUN',
              'full_product_installer_lifecycle':'NOT_RUN'}
    (base/'report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report))
    if error:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
