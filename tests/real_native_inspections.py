"""Actual Odin MCP -> pinned Windows MXC -> installed analyzer acceptance.

Optional host tools are supplied explicitly. No analyzer or protocol fixtures
substitute for the real processes in this script.
"""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import os
import subprocess
import tempfile
import sys
from protocol_harness import Peer


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--executor', required=True, type=Path)
    parser.add_argument('--executor-sha256', required=True)
    parser.add_argument('--tools-root', required=True, type=Path)
    parser.add_argument('--node', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    report = {'status':'NOT_RUN', 'scope':'REAL_ODIN_MCP_MXC_ANALYZERS',
              'started_at_utc':datetime.datetime.now(datetime.UTC).isoformat(),
              'binary_sha256':digest(args.binary), 'executor_sha256':digest(args.executor),
              'checks':[], 'responses':{}, 'backend_stderr':[], 'commands':[]}
    tools = args.tools_root.resolve()
    cargo = Path(subprocess.check_output(['rustup','which','cargo'], text=True).strip())
    ruff = tools/'python/Scripts/ruff.exe'
    eslint = tools/'inspections/node_modules/eslint/bin/eslint.js'

    def check(name, ok, detail=None):
        report['checks'].append({'name':name, 'status':'PASS' if ok else 'FAIL', 'detail':detail})
        if not ok:
            raise AssertionError(name)

    try:
        check('executor_integrity', report['executor_sha256'] == args.executor_sha256.lower())
        identity = subprocess.run([str(args.binary.resolve()),'metadata'],capture_output=True,timeout=15)
        report['commands'].append({'argv':['odin','metadata'],'exit_code':identity.returncode,
                                   'output':(identity.stdout+identity.stderr).decode('utf-8',errors='replace')})
        check('real_binary_metadata',identity.returncode == 0)
        report['binary_metadata'] = json.loads(identity.stdout)
        with tempfile.TemporaryDirectory(prefix='odin-native-ide-') as temporary:
            base = Path(temporary)
            for directory in ['rust/src','python','typescript','state','scratch','clippy-cache','cargo-home','trusted']:
                (base/directory).mkdir(parents=True)
            files = {'rust':('src/lib.rs','pub fn answer() -> i32 { return 42; }\n'),
                     'python':('sample.py','import os\n\nvalue = 42\n'),
                     'typescript':('sample.ts','export const answer = 42\n')}
            for project, (path, content) in files.items():
                (base/project/path).write_text(content, encoding='utf-8', newline='')
            (base/'rust/Cargo.toml').write_text('[package]\nname="odin-inspection-fixture"\nversion="0.1.0"\nedition="2024"\n[workspace]\n',encoding='utf-8')
            env = dict(os.environ, CARGO_HOME=str(base/'cargo-home'), GOTELEMETRY='off',
                       DOTNET_GENERATE_ASPNET_CERTIFICATE='false', DOTNET_CLI_TELEMETRY_OPTOUT='1')
            env['PATH'] = str(cargo.parent)+os.pathsep+env['PATH']
            generated = subprocess.run([str(cargo),'generate-lockfile','--offline'],cwd=base/'rust',env=env,capture_output=True,timeout=30)
            report['commands'].append({'argv':['cargo','generate-lockfile','--offline'], 'exit_code':generated.returncode,
                                       'output':(generated.stdout+generated.stderr).decode('utf-8',errors='replace')})
            check('real_fixture_lockfile',generated.returncode == 0 and (base/'rust/Cargo.lock').is_file())
            host_eslint = base/'trusted/eslint.config.mjs'
            host_eslint.write_text('export default [{files:["**/*.ts"],rules:{semi:["error","always"]}}];\n',encoding='utf-8')
            # A hostile project config must not be discovered as executable host policy.
            marker = base/'typescript/repository-config-executed'
            (base/'typescript/eslint.config.mjs').write_text('throw new Error("Repository executable config must stay inert");\n',encoding='utf-8')
            (base/'python/ruff.toml').write_text('fix = true\n',encoding='utf-8')
            profiles = {
                'rust':('clippy',str(cargo),['clippy','--locked','--offline','--message-format=json','--target-dir',str(base/'clippy-cache')],['clippy','--version']),
                'python':('ruff',str(ruff),['check','--no-fix','--no-cache','--output-format=json','.'],['--version']),
                'typescript':('eslint',str(args.node.resolve()),['--preserve-symlinks','--preserve-symlinks-main',str(eslint),'--no-config-lookup','--format=json','--config',str(host_eslint),'.'],['--preserve-symlinks','--preserve-symlinks-main',str(eslint),'--version']),
            }
            lines = ['state_dir = '+json.dumps(str(base/'state')), '[limits]', 'rpc_timeout_secs = 60', 'max_output_bytes = 1048576']
            for project,(provider,program,argv,version_args) in profiles.items():
                readonly = [str(base/project),str(tools),str(cargo.parent.parent),str(args.node.resolve().parent),str(base/'trusted')]
                writable = [str(base/'clippy-cache'),str(base/'cargo-home')]
                lines += [f'[projects.{project}]', 'root = '+json.dumps(str(base/project)),
                          'allow_write = true','allow_inspections = true', 'allow_shell = true',
                          f'[projects.{project}.sandbox]', 'executor = '+json.dumps(str(args.executor.resolve())),
                          'executor_sha256 = '+json.dumps(args.executor_sha256),
                          'readonly_paths = '+json.dumps(readonly), 'readwrite_paths = '+json.dumps(writable),
                          'temp_root = '+json.dumps(str(base/'scratch')), 'allow_ui = true',
                          f'[projects.{project}.inspections.analysis]', 'provider = '+json.dumps(provider),
                          'version_args = '+json.dumps(version_args),
                          f'[projects.{project}.inspections.analysis.command]',
                          'program = '+json.dumps(program), 'args = '+json.dumps(argv), 'timeout_secs = 120',
                          f'[projects.{project}.commands.version]', 'program = '+json.dumps(program),
                          'args = '+json.dumps(version_args), 'timeout_secs = 30']
            config = base/'host.toml'
            config.write_text('\n'.join(lines)+'\n',encoding='utf-8')
            with Peer([str(args.binary.resolve()),'--config',str(config),'serve'],env=env) as peer:
                try:
                    initialized = peer.request('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'odin-native-inspections','version':'1'}},timeout=15)
                    check('real_mcp_handshake', initialized.get('result',{}).get('serverInfo',{}).get('name') == 'odin')
                    peer.notify('notifications/initialized')

                    def call(project, tool, arguments):
                        result = peer.request('tools/call',{'name':tool,'arguments':dict(arguments,project=project)},timeout=180)
                        if 'error' in result:
                            raise RuntimeError(json.dumps(result['error']))
                        body = result['result']
                        value = body.get('structuredContent') or json.loads(body['content'][0]['text'])
                        if body.get('isError'):
                            raise RuntimeError(json.dumps(value))
                        return value

                    for project,(relative,original) in files.items():
                        path = base/project/relative
                        version = call(project,'execute_shell_command',{'profile':'version'})
                        report['responses'][project+':sandbox_command'] = version
                        check(project+':shell_uses_native_sandbox',version['success'] and version['sandbox'] is True)
                        analyzed = call(project,'run_inspections',{'profile':'analysis'})
                        report['responses'][project+':findings'] = analyzed
                        check(project+':analysis_readonly',path.read_bytes() == original.encode())
                        findings = analyzed['findings']
                        check(project+':real_normalized_rule', bool(findings) and all(f['provider']==profiles[project][0] and f['range'] is not None and f['sha256']==digest(base/project/f['relative_path']) for f in findings))
                        check(project+':current_complete_report',analyzed['summary']['complete'] and all(f['freshness']=='current' for f in findings))
                        fix = next((f for f in findings if f['fix_available']),None)
                        check(project+':real_safe_fix_available',fix is not None)
                        preview = call(project,'prepare_inspection_fix',{'profile':'analysis','finding_id':fix['finding_id'],'expected_sha256':digest(path)})
                        report['responses'][project+':fix_preview'] = preview
                        check(project+':preview_readonly',path.read_bytes()==original.encode())
                        call(project,'transaction_apply',{'transaction_id':preview['transaction_id']})
                        check(project+':cas_fix_applied',path.read_bytes()!=original.encode())
                        call(project,'transaction_rollback',{'transaction_id':preview['transaction_id']})
                        check(project+':exact_fix_rollback',path.read_bytes()==original.encode())
                    check('repository_executable_config_not_used',not marker.exists())
                    check('private_scratch_cleaned',not list((base/'scratch').iterdir()))
                    report['status'] = 'PASS'
                finally:
                    report['backend_stderr'] = list(peer.errors)
    except OSError as error:
        report['status']='BLOCKED' if getattr(error,'winerror',None)==4551 else 'FAIL'
        report['error']=str(error)
        report['win32_error']=getattr(error,'winerror',None)
    except Exception as error:
        report['status']='FAIL'
        report['error']=str(error)
    report['completed_at_utc']=datetime.datetime.now(datetime.UTC).isoformat()
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':report['status'],'checks':len(report['checks']),'error':report.get('error')}),flush=True)
    return 0 if report['status']=='PASS' else 1


if __name__=='__main__':
    raise SystemExit(main())
