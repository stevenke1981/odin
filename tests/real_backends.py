"""Real binary -> MCP -> installed language/debug servers. No protocol mocks."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import time

from protocol_harness import Peer


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--python', type=Path, required=True)
    parser.add_argument('--python-lsp', type=Path, required=True)
    parser.add_argument('--typescript-lsp', type=Path, required=True)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'scope': 'REAL_ODIN_MCP_AND_INSTALLED_BACKENDS', 'platform': sys.platform,
              'status': 'FAIL', 'checks': [], 'attempt_errors': [], 'backends': {}}

    def check(name, condition, detail=None):
        report['checks'].append({'name': name, 'status': 'PASS' if condition else 'FAIL', 'detail': detail})
        if not condition:
            raise AssertionError(name)

    try:
        for path in [args.binary, args.python, args.python_lsp, args.typescript_lsp, args.node]:
            if not path.is_file():
                raise FileNotFoundError(path.name)
        def version(command):
            result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=20)
            if result.returncode:
                raise RuntimeError(result.stderr)
            return result.stdout.strip()
        report['backends'] = {
            'rust': version(['rust-analyzer', '--version']),
            'python_lsp': version([str(args.python), '-m', 'pip', 'show', 'basedpyright']).splitlines()[1],
            'typescript_lsp': version([str(args.node), str(args.typescript_lsp), '--version']),
            'debugpy': version([str(args.python), '-c', 'import debugpy;print(debugpy.__version__)']),
        }
        with tempfile.TemporaryDirectory(prefix='odin-real-') as temporary:
            base = Path(temporary)
            fixtures = {
                'rust': ('src/lib.rs', 'mod consumer;\npub fn greet(name: &str) -> String { format!("Hi {}", name) }\npub fn run() -> String { greet("Odin") }\n'),
                'python': ('sample.py', 'def greet(name: str) -> str:\n    return f"Hi {name}"\n\ndef run() -> str:\n    return greet("Odin")\n'),
                'typescript': ('sample.ts', 'export function greet(name: string): string { return `Hi ${name}`; }\nexport const message = greet("Odin");\n'),
            }
            for language, (relative, content) in fixtures.items():
                path = base / language / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content.encode())
            consumers = {
                'rust': ('src/consumer.rs', 'pub fn consume() -> String { crate::greet("Caller") }\n'),
                'python': ('consumer.py', 'from sample import greet\n\nmessage = greet("Caller")\n'),
                'typescript': ('consumer.ts', 'import { greet } from "./sample";\nexport const consumerMessage = greet("Caller");\n'),
            }
            for language, (relative, content) in consumers.items():
                (base/language/relative).write_bytes(content.encode())
            (base/'rust'/'Cargo.toml').write_text('[package]\nname="odin-real-fixture"\nversion="0.1.0"\nedition="2024"\n[workspace]\n', encoding='utf-8')
            (base/'typescript'/'tsconfig.json').write_text('{"compilerOptions":{"strict":true,"noEmit":true},"include":["*.ts"]}', encoding='utf-8')
            debuggee = base/'python'/'debuggee.py'
            debuggee.write_bytes(b'def compute():\n    value = 41\n    result = value + 1\n    print(result)\n    return result\n\ncompute()\n')
            configs = {
                'rust': ('rust-analyzer', [], 'rs'),
                'python': (str(args.python_lsp.resolve()), ['--stdio'], 'py'),
                'typescript': (str(args.node.resolve()), [str(args.typescript_lsp.resolve()), '--stdio'], 'ts'),
            }
            lines = [f'state_dir = {json.dumps(str(base/"state"))}', '[limits]', 'rpc_timeout_secs = 30']
            for language, (program, argv, extension) in configs.items():
                lines += [f'[projects.{language}]', f'root = {json.dumps(str(base/language))}',
                          'allow_write = true', f'allow_debug = {str(language == "python").lower()}',
                          f'[[projects.{language}.languages]]', f'id = "{language}"',
                          f'extensions = ["{extension}"]', f'program = {json.dumps(program)}', f'args = {json.dumps(argv)}']
                if language == 'typescript':
                    tsserver = args.typescript_lsp.parent.parent.parent/'typescript'/'lib'/'tsserver.js'
                    lines += ['initialization_options = { tsserver = { path = '+json.dumps(str(tsserver.resolve()))+' } }']
            lines += ['[projects.python.debuggers.python]', f'program = {json.dumps(str(args.python.resolve()))}',
                      'args = ["-m", "debugpy.adapter"]', 'timeout_secs = 30']
            config = base/'host.toml'
            config.write_text('\n'.join(lines)+'\n', encoding='utf-8')
            with Peer([str(args.binary.resolve()), '--config', str(config), 'serve']) as peer:
                initialized = peer.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name':'odin-real-backends', 'version':'1'}})
                check('real_mcp_initialize', initialized.get('result', {}).get('serverInfo', {}).get('name') == 'odin')
                peer.notify('notifications/initialized')
                catalog = {tool['name']: tool for tool in peer.request('tools/list')['result']['tools']}

                def call(project, tool, arguments=None):
                    response = peer.request('tools/call', {'name':tool, 'arguments':dict(arguments or {}, project=project)}, timeout=45)
                    if 'error' in response:
                        raise RuntimeError(json.dumps(response['error']))
                    value = response['result']
                    body = value.get('structuredContent') or json.loads(value['content'][0]['text'])
                    if value.get('isError'):
                        raise RuntimeError(json.dumps(body))
                    return body

                def retry(operation, predicate, seconds=30):
                    deadline = time.monotonic()+seconds
                    while True:
                        try:
                            value = operation()
                            if predicate(value):
                                return value
                        except RuntimeError as error:
                            report['attempt_errors'].append(str(error))
                        if time.monotonic() > deadline:
                            raise TimeoutError('Backend semantic readiness deadline')
                        time.sleep(.25)

                for language, (relative, original) in fixtures.items():
                    consumer_relative, consumer_original = consumers[language]
                    consumer = base/language/consumer_relative
                    symbols = call(language, 'get_symbols_overview', {'relative_path':relative})
                    check(language+':real_document_symbols', any(symbol['name']=='greet' for symbol in symbols['symbols']))
                    references = retry(lambda: call(language, 'find_referencing_symbols', {'relative_path':relative, 'name_path':'greet'}), lambda value: consumer.name in json.dumps(value))
                    check(language+':real_references', len(references.get('references', references.get('locations', []))) >= 1)
                    check(language+':real_cross_file_references', consumer.name in json.dumps(references))
                    preview = retry(lambda: call(language, 'rename_symbol', {'relative_path':relative, 'name_path':'greet', 'new_name':'salute'}), lambda value: bool(value.get('files')))
                    path = base/language/relative
                    check(language+':rename_preview_is_readonly', path.read_bytes()==original.encode() and consumer.read_bytes()==consumer_original.encode())
                    call(language, 'transaction_apply', {'transaction_id':preview['transaction_id']})
                    edited = path.read_text(encoding='utf-8')
                    check(language+':rename_definition_and_reference', edited.count('salute')>=2 and 'greet(' not in edited)
                    check(language+':rename_cross_file_uses', 'salute' in consumer.read_text(encoding='utf-8') and 'greet' not in consumer.read_text(encoding='utf-8'))
                    call(language, 'transaction_rollback', {'transaction_id':preview['transaction_id']})
                    check(language+':rename_rollback', path.read_bytes()==original.encode() and consumer.read_bytes()==consumer_original.encode())

                started = call('python', 'debug_start', {'profile':'python'})
                session = started.get('session_id', started.get('session'))
                def debug(tool, arguments=None):
                    arguments = dict(arguments or {})
                    if 'session_id' in catalog[tool]['inputSchema']['properties']:
                        arguments['session_id'] = session
                    return call('python', tool, arguments)
                launched = debug('debug_request', {'command':'launch', 'arguments':{
                    'name':'Odin real debugpy verification', 'type':'python', 'request':'launch',
                    'program':str(debuggee), 'python':[str(args.python.resolve())], 'console':'internalConsole',
                    'justMyCode':True, 'cwd':str(debuggee.parent)}})
                check('debugpy:launch_ticket', launched.get('pending') is True)
                seen_events = []
                def poll_event(event):
                    response = debug('debug_events')
                    seen_events.extend(response.get('events', []))
                    return {'events':seen_events, 'response':response}
                retry(lambda: poll_event('initialized'), lambda result:any(e.get('event')=='initialized' for e in result['events']))
                breakpoints = debug('debug_request', {'command':'setBreakpoints', 'arguments':{'source':{'path':str(debuggee)}, 'breakpoints':[{'line':3}]}})
                check('debugpy:real_breakpoint', bool(breakpoints.get('breakpoints')))
                debug('debug_request', {'command':'configurationDone', 'arguments':{}})
                retry(lambda: poll_event('stopped'), lambda result:any(e.get('event')=='stopped' for e in result['events']))
                threads = debug('debug_request', {'command':'threads', 'arguments':{}})['threads']
                check('debugpy:real_threads', bool(threads))
                thread_id = threads[0]['id']
                stack = debug('debug_request', {'command':'stackTrace', 'arguments':{'threadId':thread_id}})['stackFrames']
                frame_id = stack[0]['id']
                scopes = debug('debug_request', {'command':'scopes', 'arguments':{'frameId':frame_id}})['scopes']
                variables = debug('debug_request', {'command':'variables', 'arguments':{'variablesReference':scopes[0]['variablesReference']}})['variables']
                check('debugpy:real_variables', any(v.get('name')=='value' and v.get('value')=='41' for v in variables))
                evaluated = debug('debug_request', {'command':'evaluate', 'arguments':{'expression':'value + 1', 'frameId':frame_id, 'context':'watch'}})
                check('debugpy:real_evaluate', evaluated.get('result')=='42')
                seen_events.clear()
                debug('debug_request', {'command':'next', 'arguments':{'threadId':thread_id}})
                retry(lambda: poll_event('stopped'), lambda result:any(e.get('event')=='stopped' for e in result['events']))
                check('debugpy:real_step', True)
                debug('debug_stop')
                check('debugpy:disconnect', True)
            report['status'] = 'PASS'
    except Exception as error:
        report['error'] = str(error)
    report['binary_sha256'] = hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary.is_file() else None
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report, ensure_ascii=False))
    return 0 if report['status']=='PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
