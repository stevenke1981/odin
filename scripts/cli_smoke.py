#!/usr/bin/env python3
"""Exercise the real CLI and launch MCP using each emitted client configuration."""
from pathlib import Path
import argparse, hashlib, json, platform, subprocess, sys, tempfile, time, tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tests'))
from protocol_harness import Peer


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'verification/local-cli-smoke.json')
    args = parser.parse_args()
    tests, status, error = [], 'NOT_RUN', None

    def check(name, condition):
        tests.append({'name': name, 'status': 'PASS' if condition else 'FAIL'})
        if not condition:
            raise AssertionError(name)

    try:
        if not args.binary.is_file():
            raise FileNotFoundError('Compile Odin before running CLI acceptance')
        status = 'FAIL'
        binary = str(args.binary.resolve())
        with tempfile.TemporaryDirectory(prefix='odin-cli-中文 space-') as directory:
            base = Path(directory)
            workspace = base/'workspace'
            workspace.mkdir()
            (workspace/'README.md').write_text('Onboarding fixture\n', encoding='utf-8')
            config = base/'host.toml'
            state = base/'state'
            config.write_text(f'state_dir = {json.dumps(str(state))}\n[projects.demo]\nroot = {json.dumps(str(workspace))}\n', encoding='utf-8')

            def run(*command, success=True):
                result = subprocess.run([binary, '--config', str(config), *command], capture_output=True, encoding='utf-8', timeout=20)
                check('exit:'+command[0], (result.returncode == 0) == success)
                return json.loads(result.stdout) if success else result

            metadata = run('metadata')
            check('build_identity', metadata['version'] and metadata['target'] and metadata['revision'])
            check('tools_without_config', subprocess.run([binary, '--config', str(base/'missing.toml'), 'tools'], capture_output=True).returncode == 0)
            validated = run('validate')
            check('validate_does_not_create_state', validated['valid'] and not state.exists())
            catalog = json.loads((ROOT/'schemas/tools.json').read_text(encoding='utf-8'))
            for client in ['codex', 'claude', 'opencode']:
                extension = 'toml' if client == 'codex' else 'json'
                output = base/f'{client}.{extension}'
                original = 'model = "unchanged"\n' if client == 'codex' else '{"unrelated":{"keep":true,"credential":"NOT_A_REAL_SECRET_SENTINEL"}}\n'
                output.write_text(original, encoding='utf-8')
                preview = run('setup', '--client', client, '--output', str(output))
                check('setup_dry_run:'+client, preview['operation'] == 'DRY_RUN' and output.read_text(encoding='utf-8') == original)
                check('preview_omits_unrelated_values:'+client, 'NOT_A_REAL_SECRET_SENTINEL' not in json.dumps(preview))
                expected = hashlib.sha256(output.read_bytes()).hexdigest()
                run('setup', '--client', client, '--output', str(output), '--apply', success=False)
                run('setup', '--client', client, '--output', str(output), '--apply', '--expected-sha256', '0'*64, success=False)
                check('conflict_preserves_original:'+client, output.read_text(encoding='utf-8') == original)
                applied = run('setup', '--client', client, '--output', str(output), '--apply', '--expected-sha256', expected)
                check('setup_backup:'+client, Path(applied['backup']).read_text(encoding='utf-8') == original)
                content = output.read_text(encoding='utf-8')
                document = tomllib.loads(content) if client == 'codex' else json.loads(content)
                if client == 'codex':
                    check('codex_preserves_unrelated', document['model'] == 'unchanged')
                    server = document['mcp_servers']['odin']
                    command = [server['command'], *server['args']]
                elif client == 'claude':
                    check('claude_preserves_unrelated', document['unrelated']['keep'])
                    server = document['mcpServers']['odin']
                    command = [server['command'], *server['args']]
                else:
                    check('opencode_preserves_unrelated', document['unrelated']['keep'])
                    command = document['mcp']['odin']['command']
                with Peer(command) as peer:
                    response = peer.request('initialize', {'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'generated-config-smoke','version':'1'}})
                    check('generated_config_initialize:'+client, response.get('result',{}).get('serverInfo',{}).get('name') == 'odin')
                    peer.notify('notifications/initialized')
                    tools = peer.request('tools/list')['result']['tools']
                    check('generated_config_tools_list:'+client, sorted(t['name'] for t in tools) == sorted(t['name'] for t in catalog))
                current = hashlib.sha256(output.read_bytes()).hexdigest()
                check('setup_idempotent:'+client, run('setup', '--client', client, '--output', str(output), '--apply', '--expected-sha256', current)['operation'] == 'UNCHANGED')
            check('setup_lock_cleanup', not list(base.glob('*.odin-setup.lock')))
            guidance = base/'host-guidance.md'
            original = 'Keep host instructions.\n'
            guidance.write_text(original, encoding='utf-8')
            preview = run('hook', '--project', 'demo', '--output', str(guidance))
            check('hook_dry_run', preview['operation'] == 'DRY_RUN' and guidance.read_text(encoding='utf-8') == original)
            enabled = run('hook', '--project', 'demo', '--output', str(guidance), '--apply', '--expected-sha256', hashlib.sha256(guidance.read_bytes()).hexdigest())
            check('hook_backup', Path(enabled['backup']).read_text(encoding='utf-8') == original)
            run('hook', '--project', 'demo', '--output', str(guidance), '--disable', '--apply', '--expected-sha256', hashlib.sha256(guidance.read_bytes()).hexdigest())
            check('hook_disable_restores_original', guidance.read_text(encoding='utf-8') == original)
            run('hook', '--output', str(workspace/'AGENTS.md'), '--apply', success=False)
            check('workspace_mutations_require_transactions', not (workspace/'AGENTS.md').exists())
            onboard = run('onboard', '--project', 'demo')
            check('onboarding_evidence', any(item['path'] == 'README.md' for item in onboard['evidence']))
            # A configured missing backend must be distinguishable from a successful probe.
            config.write_text(config.read_text(encoding='utf-8')+'\n[[projects.demo.languages]]\nid="missing"\nextensions=["miss"]\nprogram="ODIN_NONEXISTENT_BACKEND"\n', encoding='utf-8')
            failure = run('doctor', '--probe-lsp', success=False)
            check('doctor_missing_backend_not_pass', json.loads(failure.stdout)['checks'][0]['languages'][0]['integration'] == 'FAIL')
        status = 'PASS'
    except Exception as exc:
        error = str(exc)
    report = {'scope':'REAL_ODIN_BINARY_CLI_AND_GENERATED_MCP_CONFIG', 'status':status,
              'binary':str(args.binary.resolve()), 'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary.is_file() else None,
              'platform':platform.platform(), 'checks':tests, 'error':error,
              'actual_codex_claude_opencode_applications_tested':False, 'timestamp_unix':time.time()}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report))
    return 0 if status == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
