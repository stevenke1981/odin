#!/usr/bin/env python3
"""Real Odin MCP cancellation regression with an explicitly configured slow LSP protocol fixture."""
from pathlib import Path
import argparse, hashlib, json, platform, subprocess, sys, tempfile, threading, time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tests'))
from protocol_harness import Peer


def fixture(log, release):
    """No semantic claims: this peer merely holds documentSymbol until released or cancelled."""
    output_lock, state_lock = threading.Lock(), threading.Lock()
    pending = {}

    def record(kind, identifier):
        with state_lock, log.open('a', encoding='utf-8') as handle:
            handle.write(json.dumps({'kind':kind, 'id':identifier})+'\n')

    def send(message):
        raw = json.dumps(message).encode('utf-8')
        with output_lock:
            sys.stdout.buffer.write(f'Content-Length: {len(raw)}\r\n\r\n'.encode('ascii')+raw)
            sys.stdout.buffer.flush()

    def slow(identifier, cancelled):
        while not release.is_file() and not cancelled.wait(.01):
            pass
        if cancelled.is_set():
            record('cancel_processed', identifier)
            send({'jsonrpc':'2.0', 'id':identifier, 'error':{'code':-32800, 'message':'Fixture request cancelled'}})
        else:
            record('released', identifier)
            span = {'start':{'line':0, 'character':0}, 'end':{'line':0, 'character':15}}
            selection = {'start':{'line':0, 'character':3}, 'end':{'line':0, 'character':10}}
            send({'jsonrpc':'2.0', 'id':identifier, 'result':[{'name':'fixture', 'kind':12, 'range':span, 'selectionRange':selection}]})
        with state_lock:
            pending.pop(identifier, None)

    while True:
        headers = {}
        while True:
            line = sys.stdin.buffer.readline()
            if not line:
                return
            if line in (b'\r\n', b'\n'):
                break
            key, value = line.decode('ascii').split(':', 1)
            headers[key.lower()] = value.strip()
        message = json.loads(sys.stdin.buffer.read(int(headers['content-length'])))
        method = message.get('method')
        identifier = message.get('id')
        if method == '$/cancelRequest':
            target = message['params']['id']
            record('cancel_received', target)
            with state_lock:
                cancelled = pending.get(target)
            if cancelled is not None:
                cancelled.set()
            continue
        if method == 'exit':
            return
        if identifier is None:
            continue
        if method == 'initialize':
            send({'jsonrpc':'2.0', 'id':identifier, 'result':{'capabilities':{'documentSymbolProvider':True}}})
        elif method == 'textDocument/documentSymbol':
            cancelled = threading.Event()
            with state_lock:
                pending[identifier] = cancelled
            record('slow_started', identifier)
            threading.Thread(target=slow, args=(identifier,cancelled), daemon=True).start()
        elif method == 'shutdown':
            send({'jsonrpc':'2.0', 'id':identifier, 'result':None})
        else:
            send({'jsonrpc':'2.0', 'id':identifier, 'error':{'code':-32601,'message':'Fixture method unavailable'}})


def run(binary, output):
    checks, observations, status, error = [], {}, 'NOT_RUN', None

    def check(name, condition):
        checks.append({'name':name, 'status':'PASS' if condition else 'FAIL'})
        if not condition:
            raise AssertionError(name)

    try:
        if not binary.is_file():
            raise FileNotFoundError('Compile the real binary before this regression')
        status = 'FAIL'
        with tempfile.TemporaryDirectory(prefix='odin-cancel-') as directory:
            base = Path(directory)
            project = base/'project'
            project.mkdir()
            source = project/'main.rs'
            original = b'fn fixture() {}\n'
            source.write_bytes(original)
            log, release, config = base/'fixture.jsonl', base/'release', base/'host.toml'
            log.write_bytes(b'')
            args = [str(Path(__file__).resolve()), '--fixture', '--fixture-log', str(log), '--fixture-release', str(release)]
            config.write_text(f'state_dir={json.dumps(str(base/"state"))}\n[limits]\nrpc_timeout_secs=15\n[projects.demo]\nroot={json.dumps(str(project))}\nallow_write=true\n[[projects.demo.languages]]\nid="fixture"\nextensions=["rs"]\nprogram={json.dumps(sys.executable)}\nargs={json.dumps(args)}\n', encoding='utf-8')

            def events():
                try:
                    return [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines() if line]
                except (OSError, ValueError):
                    return []

            def wait_event(predicate, timeout=3):
                end = time.monotonic()+timeout
                while time.monotonic() < end:
                    match = next((entry for entry in events() if predicate(entry)), None)
                    if match is not None:
                        return match
                    time.sleep(.01)
                raise TimeoutError('Expected fixture event did not arrive')

            with Peer([str(binary.resolve()), '--config', str(config), 'serve', '--transport', 'stdio']) as peer:
                response = peer.request('initialize', {'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'cancellation-regression','version':'1'}})
                check('real_binary_initialize', response.get('result',{}).get('serverInfo',{}).get('name') == 'odin')
                peer.notify('notifications/initialized')

                def call(name, arguments):
                    response = peer.request('tools/call', {'name':name, 'arguments':dict(arguments, project='demo')}, timeout=5)
                    if 'error' in response or response['result'].get('isError'):
                        raise AssertionError(f'Tool failed: {name}: {response}')
                    return response['result']['structuredContent']

                def send_call(name, arguments):
                    peer.sequence += 1
                    identifier = peer.sequence
                    peer.send({'jsonrpc':'2.0','id':identifier,'method':'tools/call','params':{'name':name,'arguments':dict(arguments,project='demo')}})
                    return identifier

                preview = call('replace_content', {'relative_path':'main.rs','expected_sha256':hashlib.sha256(original).hexdigest(),'needle':'fixture','replacement':'cancelled_write'})
                transaction = preview['transaction_id']
                call('get_capabilities', {'language':'fixture','probe':True})
                slow_id = send_call('get_symbols_overview', {'relative_path':'main.rs'})
                first_slow = wait_event(lambda entry: entry['kind'] == 'slow_started')
                check('slow_backend_holds_project_gate', first_slow is not None and source.read_bytes() == original)
                apply_id = send_call('transaction_apply', {'transaction_id':transaction})
                # A protocol round-trip lets the SDK register the queued request before cancellation.
                peer.request('tools/list')
                peer.notify('notifications/cancelled', {'requestId':apply_id,'reason':'Cancel queued write regression'})
                # SDK 3.6 intentionally discards responses for cancelled request IDs.
                # Release the earlier request, then a later same-project read is a gate barrier.
                release.write_bytes(b'release')
                completed = peer.receive(slow_id, timeout=5)
                check('earlier_backend_request_completes', not completed.get('error') and not completed['result'].get('isError'))
                after = call('read_file', {'relative_path':'main.rs'})
                observations['queued_write_source_sha256'] = hashlib.sha256(source.read_bytes()).hexdigest()
                observations['original_sha256'] = hashlib.sha256(original).hexdigest()
                check('cancelled_queued_write_never_applied', source.read_bytes() == original and after['sha256'] == hashlib.sha256(original).hexdigest())
                draft = call('transaction_preview', {'transaction_id':transaction})
                observations['transaction_phase_after_cancel'] = draft.get('phase')
                check('cancelled_transaction_remains_draft', draft.get('phase') in ('draft','Draft'))
                release.unlink()
                pending_id = send_call('get_symbols_overview', {'relative_path':'main.rs'})
                second_slow = wait_event(lambda entry: entry['kind']=='slow_started' and entry['id']!=first_slow['id'])
                peer.notify('notifications/cancelled', {'requestId':pending_id,'reason':'Cancel pending backend regression'})
                cancelled = wait_event(lambda entry: entry['kind']=='cancel_processed' and entry['id']==second_slow['id'])
                check('pending_backend_receives_protocol_cancellation', cancelled is not None)
                started = time.monotonic()
                unchanged = call('read_file', {'relative_path':'main.rs'})
                observations['gate_release_after_cancel_seconds'] = time.monotonic()-started
                check('cancelled_pending_backend_releases_gate', unchanged['sha256']==hashlib.sha256(original).hexdigest())
                release.write_bytes(b'release')
                symbols = call('get_symbols_overview', {'relative_path':'main.rs'})
                check('backend_transport_usable_after_cancel', bool(symbols.get('symbols')))
                observations['fixture_events'] = events()
        status = 'PASS'
    except Exception as exc:
        error = str(exc)
    report = {'scope':'REAL_BINARY_MCP_CANCELLATION_WITH_PROTOCOL_FIXTURE', 'status':status,
              'binary':str(binary.resolve()),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest() if binary.is_file() else None,
              'platform':platform.platform(),'checks':checks,'observations':observations,'error':error,
              'fixture_is_real_language_server':False,'real_language_semantics_tested':False,
              'sdk_cancelled_response_behavior':'rmcp 3.6 suppresses wire responses for cancelled request IDs',
              'timestamp_unix':time.time()}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report))
    return 0 if status=='PASS' else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',type=Path)
    parser.add_argument('--output',type=Path,default=ROOT/'verification/platform-mcp-cancellation.json')
    parser.add_argument('--fixture',action='store_true')
    parser.add_argument('--fixture-log',type=Path)
    parser.add_argument('--fixture-release',type=Path)
    args = parser.parse_args()
    if args.fixture:
        if args.fixture_log is None or args.fixture_release is None:
            parser.error('fixture requires explicit log and release paths')
        fixture(args.fixture_log,args.fixture_release)
        return 0
    if args.binary is None:
        parser.error('--binary is required')
    return run(args.binary,args.output)


if __name__=='__main__':
    raise SystemExit(main())
