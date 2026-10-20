#!/usr/bin/env python3
"""DETERMINISTIC PROTOCOL FIXTURE, NOT A COMPILER / A DEBUGGER.
Used only to test framing, request dispatch, lifecycle, and timeout behavior.
"""
import argparse
import json
import re
import sys
import time

P = argparse.ArgumentParser()
P.add_argument('mode', choices=['lsp', 'dap'])
P.add_argument('--sync-kind', type=int, default=1)
P.add_argument('--push-only', action='store_true')
P.add_argument('--stale-diagnostics', action='store_true')
P.add_argument('--dap-minimal', action='store_true')
A = P.parse_args()
DOCS = {}
VERSIONS = {}
LAUNCHES = []
CANCELLED = []
INITIALIZE = None


def read():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b'\n', b'\r\n'):
            break
        name, value = line.decode().split(':', 1)
        if name.lower() == 'content-length':
            length = int(value.strip())
    if length is None:
        raise ValueError('missing Content-Length')
    raw = sys.stdin.buffer.read(length)
    if len(raw) != length:
        raise ValueError('truncated frame')
    return json.loads(raw)


def send(value):
    data = json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()
    sys.stdout.buffer.write(f'Content-Length: {len(data)}\r\n\r\n'.encode() + data)
    sys.stdout.buffer.flush()


def respond(request, result):
    if A.mode == 'dap':
        send({'seq': 100, 'type': 'response', 'request_seq': request['seq'], 'command': request['command'], 'success': True, 'body': result})
    else:
        send({'jsonrpc': '2.0', 'id': request['id'], 'result': result})


def rng(line, start, end):
    return {'start': {'line': line, 'character': start}, 'end': {'line': line, 'character': end}}


def diagnostics(uri, version):
    send({'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics', 'params': {'uri': uri, 'version': version, 'diagnostics': []}})


def end_position(text):
    lines = re.split(r'\r\n|\r|\n', text)
    return {'line': len(lines) - 1, 'character': len(lines[-1].encode('utf-16-le')) // 2}


def lsp(request):
    global INITIALIZE
    method = request.get('method')
    params = request.get('params', {})
    if method == 'initialize':
        INITIALIZE = params
        capabilities = {
            'positionEncoding': 'utf-16', 'textDocumentSync': {'openClose': True, 'change': A.sync_kind},
            'documentSymbolProvider': True, 'referencesProvider': True,
            'definitionProvider': True, 'implementationProvider': True,
            'renameProvider': {'prepareProvider': True},
            'codeActionProvider': {'resolveProvider': True},
        }
        if not A.push_only:
            capabilities['diagnosticProvider'] = {'interFileDependencies': False, 'workspaceDiagnostics': False}
        respond(request, {'capabilities': capabilities})
    elif method == 'textDocument/didOpen':
        doc = params['textDocument']
        DOCS[doc['uri']] = doc['text']
        VERSIONS[doc['uri']] = doc['version']
        diagnostics(doc['uri'], doc['version'])
    elif method == 'textDocument/didChange':
        doc = params['textDocument']
        uri = doc['uri']
        changes = params['contentChanges']
        if A.sync_kind == 2:
            assert changes[0]['range'] == {'start': {'line': 0, 'character': 0}, 'end': end_position(DOCS[uri])}
        elif A.sync_kind == 1:
            assert 'range' not in changes[0]
        else:
            raise ValueError('didChange not supported by this fixture')
        DOCS[uri] = changes[0]['text']
        VERSIONS[uri] = doc['version']
        diagnostics(uri, doc['version'] - int(A.stale_diagnostics))
    elif method == 'textDocument/didClose':
        DOCS.pop(params['textDocument']['uri'], None)
        VERSIONS.pop(params['textDocument']['uri'], None)
    elif method == 'textDocument/documentSymbol':
        text = DOCS[params['textDocument']['uri']]
        symbols = []
        for line, text_line in enumerate(text.splitlines()):
            match = re.search(r'fn (\w+)', text_line)
            if match:
                symbols.append({'name': match[1], 'kind': 12, 'range': rng(line, 0, len(text_line)), 'selectionRange': rng(line, match.start(1), match.end(1))})
        respond(request, symbols)
    elif method in ('textDocument/references', 'textDocument/implementation'):
        respond(request, [])
    elif method == 'textDocument/diagnostic':
        respond(request, {'kind': 'full', 'items': []})
    elif method == 'textDocument/definition':
        respond(request, {'uri': params['textDocument']['uri'], 'range': rng(0, 3, 6)})
    elif method == 'textDocument/prepareRename':
        respond(request, {'range': rng(0, 3, 6), 'placeholder': 'foo'})
    elif method == 'textDocument/rename':
        respond(request, {'changes': {params['textDocument']['uri']: [{'range': rng(0, 3, 6), 'newText': params['newName']}]}})
    elif method == 'textDocument/codeAction':
        respond(request, [{'title': 'fixture action', 'data': {'uri': params['textDocument']['uri'], 'diagnostics': params['context']['diagnostics']}}])
    elif method == 'codeAction/resolve':
        result = dict(params)
        result['edit'] = {'changes': {params['data']['uri']: [{'range': rng(0, 3, 6), 'newText': 'resolved'}]}}
        respond(request, result)
    elif method == 'fixture/state':
        respond(request, {'cancelled': CANCELLED, 'documents': DOCS, 'versions': VERSIONS, 'initialize': INITIALIZE})
    elif method == 'fixture/progress':
        send({'jsonrpc': '2.0', 'method': '$/progress', 'params': {'token': 'index', 'value': {'kind': 'begin', 'title': 'indexing'}}})
        send({'jsonrpc': '2.0', 'method': 'experimental/serverStatus', 'params': {'health': 'ok', 'quiescent': False}})
        respond(request, {})
    elif method == 'fixture/ready':
        send({'jsonrpc': '2.0', 'method': '$/progress', 'params': {'token': 'index', 'value': {'kind': 'end'}}})
        send({'jsonrpc': '2.0', 'method': 'experimental/serverStatus', 'params': {'health': 'ok', 'quiescent': True}})
        respond(request, {})
    elif method == 'fixture/cancelable':
        pass
    elif method == '$/cancelRequest':
        CANCELLED.append(params['id'])
    elif method == 'fixture/exit':
        sys.exit(0)
    elif method == 'fixture/hang':
        time.sleep(30)
    elif method == 'shutdown':
        respond(request, None)
    elif method == 'exit':
        sys.exit(0)
    elif 'id' in request:
        send({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32601, 'message': 'Fixture method unsupported'}})


def dap(request):
    cmd = request['command']
    if cmd == 'initialize':
        capabilities = {'supportsConfigurationDoneRequest': True}
        if not A.dap_minimal:
            capabilities.update({'supportsCancelRequest': True, 'supportsRestartRequest': True, 'supportsTerminateRequest': True})
        respond(request, capabilities)
        send({'seq': 101, 'type': 'event', 'event': 'initialized'})
    elif cmd in ('launch', 'attach'):
        LAUNCHES.append(request)
    elif cmd == 'configurationDone':
        respond(request, {})
        for launch in LAUNCHES:
            respond(launch, {'fixture': True})
        LAUNCHES.clear()
        send({'seq': 102, 'type': 'event', 'event': 'stopped', 'body': {'reason': 'breakpoint', 'threadId': 1}})
    elif cmd == 'threads':
        respond(request, {'threads': [{'id': 1, 'name': 'fixture'}]})
    elif cmd == 'cancel':
        CANCELLED.append(request['arguments']['requestId'])
        respond(request, {})
    elif cmd in ('continue', 'restart'):
        respond(request, {})
        send({'seq': 103, 'type': 'event', 'event': 'continued', 'body': {'threadId': 1, 'allThreadsContinued': True}})
    elif cmd == 'pause':
        respond(request, {})
        send({'seq': 104, 'type': 'event', 'event': 'stopped', 'body': {'reason': 'pause', 'threadId': 1}})
    elif cmd == 'terminate':
        respond(request, {})
        send({'seq': 105, 'type': 'event', 'event': 'terminated'})
    else:
        respond(request, {'fixture': True})


while True:
    request = read()
    if request is None:
        break
    {'lsp': lsp, 'dap': dap}[A.mode](request)
