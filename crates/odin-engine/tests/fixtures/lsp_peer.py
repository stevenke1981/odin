"""Protocol fixture for engine routing; not evidence of real language semantics."""
import json
import sys

def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b'\r\n', b'\n'):
            break
        key, value = line.decode('ascii').split(':', 1)
        headers[key.lower()] = value.strip()
    return json.loads(sys.stdin.buffer.read(int(headers['content-length'])))

def send(message):
    data = json.dumps(message, ensure_ascii=False).encode('utf-8')
    sys.stdout.buffer.write(f'Content-Length: {len(data)}\r\n\r\n'.encode('ascii') + data)
    sys.stdout.buffer.flush()

RANGE = {'start': {'line': 0, 'character': 0}, 'end': {'line': 1, 'character': 0}}
INSERT = {'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 0}}
while (message := read_message()) is not None:
    method = message.get('method')
    if 'id' not in message:
        if method == 'exit':
            break
        continue
    params = message.get('params') or {}
    uri = params.get('textDocument', {}).get('uri', '')
    if method == 'initialize':
        result = {'capabilities': {'documentSymbolProvider': True, 'definitionProvider': True, 'referencesProvider': True, 'implementationProvider': True,
                                   'codeActionProvider': {'resolveProvider': True}, 'diagnosticProvider': {'interFileDependencies': False, 'workspaceDiagnostics': False}}}
    elif method == 'textDocument/documentSymbol':
        result = [{'name': 'module', 'kind': 2, 'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': 2, 'character': 0}}, 'selectionRange': INSERT,
                   'children': [{'name': name, 'kind': 12, 'range': {'start': {'line': line, 'character': 0}, 'end': {'line': line, 'character': 11}},
                                 'selectionRange': {'start': {'line': line, 'character': 3}, 'end': {'line': line, 'character': 6}}} for line, name in enumerate(['foo', 'bar'])]}]
        if '--bad-first' in sys.argv:
            result[0]['children'][0]['range'] = {'start': {'line': 999, 'character': 0}, 'end': {'line': 999, 'character': 1}}
    elif method == 'textDocument/diagnostic':
        result = {'kind': 'full', 'items': [{'range': RANGE, 'severity': 2, 'code': 'fixture-rule', 'message': 'fixture diagnostic'}]}
    elif method in ('textDocument/definition', 'textDocument/references', 'textDocument/implementation'):
        result = [{'uri': uri, 'range': {'start': {'line': 0, 'character': 3}, 'end': {'line': 0, 'character': 6}}}]
    elif method == 'textDocument/codeAction':
        quickfix = {'changes': {uri: [{'range': INSERT, 'newText': '// fixed\n'}]}}
        target = uri.rsplit('/', 1)[0] + '/target.rs'
        move = {'documentChanges': [{'textDocument': {'uri': uri, 'version': None}, 'edits': [{'range': RANGE, 'newText': ''}]},
                                    {'kind': 'create', 'uri': target}, {'textDocument': {'uri': target, 'version': None}, 'edits': [{'range': INSERT, 'newText': 'fn foo() {}\n'}]}]}
        result = [{'title': 'Move foo', 'kind': 'refactor.move', 'edit': move},
                  {'title': 'Inline foo', 'kind': 'refactor.inline', 'edit': {'changes': {uri: [{'range': RANGE, 'newText': '// fixture inline\n'}]}}},
                  {'title': 'Resolve fix', 'kind': 'quickfix', 'data': {'uri': uri}},
                  {'title': 'Unsafe command', 'kind': 'quickfix', 'edit': quickfix, 'command': {'title': 'No', 'command': 'fixture.forbidden'}},
                  {'title': 'Duplicate', 'kind': 'quickfix', 'edit': quickfix}, {'title': 'Duplicate', 'kind': 'quickfix', 'edit': quickfix}]
        if params.get('context', {}).get('diagnostics'):
            result.append({'title': 'Fix warning', 'kind': 'quickfix', 'edit': quickfix})
    elif method == 'codeAction/resolve':
        result = dict(params)
        result['edit'] = {'changes': {params['data']['uri']: [{'range': INSERT, 'newText': '// resolved\n'}]}}
    elif method == 'shutdown':
        result = None
    else:
        send({'jsonrpc': '2.0', 'id': message['id'], 'error': {'code': -32601, 'message': 'Fixture rejects unimplemented method'}})
        continue
    send({'jsonrpc': '2.0', 'id': message['id'], 'result': result})
