"""IDE protocol routing fixture; this does not prove real language semantics."""
import json
import sys
import time
from pathlib import Path


def receive():
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


def send(frame):
    payload = json.dumps(frame, ensure_ascii=False).encode('utf-8')
    sys.stdout.buffer.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode('ascii') + payload)
    sys.stdout.buffer.flush()


def pos(column):
    return {'line': 0, 'character': column}


SPAN = {'start': pos(0), 'end': pos(2)}
POINT = {'start': pos(0), 'end': pos(0)}
opened = {}
root_uri = ''
while (frame := receive()) is not None:
    method = frame.get('method')
    args = frame.get('params') or {}
    if method == 'textDocument/didOpen':
        opened[args['textDocument']['uri']] = args['textDocument']['text']
    if 'id' not in frame:
        if method == 'exit':
            break
        continue
    uri = args.get('textDocument', {}).get('uri', '')
    if method == 'initialize':
        root_uri = args['rootUri']
        caps = {'completionProvider': {'resolveProvider': True}, 'hoverProvider': True,
                'signatureHelpProvider': {}, 'workspaceSymbolProvider': True,
                'callHierarchyProvider': True, 'documentFormattingProvider': True,
                'documentRangeFormattingProvider': True, 'inlayHintProvider': {'resolveProvider': True},
                'semanticTokensProvider': {'full': True, 'range': True,
                                          'legend': {'tokenTypes': ['keyword'], 'tokenModifiers': ['declaration']}}}
        if '--no-capabilities' in sys.argv:
            caps = {}
        result = {'capabilities': caps}
    elif method in ('textDocument/hover', 'textDocument/formatting'):
        if '--pause' in sys.argv:
            marker = Path(sys.argv[sys.argv.index('--request-marker') + 1])
            release = Path(sys.argv[sys.argv.index('--release-marker') + 1])
            marker.write_text('request received', encoding='utf-8')
            deadline = time.monotonic() + 5
            while not release.exists():
                if time.monotonic() > deadline:
                    raise RuntimeError('Test did not release paused IDE response')
                time.sleep(0.01)
        if method == 'textDocument/hover':
            result = {'contents': {'kind': 'markdown', 'value': 'fixture hover'}, 'range': SPAN}
            if '--hover-at-request' in sys.argv:
                result['range'] = {'start': args['position'], 'end': args['position']}
        elif '--empty-formatting' in sys.argv:
            result = []
        else:
            result = [{'range': POINT, 'newText': '// formatted\n'}]
    elif method == 'textDocument/completion':
        result = {'isIncomplete': False, 'items': [
            {'label': f'item_{index}', 'kind': 3, 'data': {'index': index},
             'command': {'title': 'Never run', 'command': 'fixture.forbidden'}} for index in range(3)]}
    elif method == 'completionItem/resolve':
        result = dict(args)
        result.update({'detail': 'resolved fixture completion',
                       'textEdit': {'range': SPAN, 'newText': 'fn'},
                       'command': {'title': 'Never run', 'command': 'fixture.forbidden'}})
    elif method == 'textDocument/signatureHelp':
        result = {'signatures': [{'label': 'foo(value: i32)'}], 'activeSignature': 0, 'activeParameter': 0}
    elif method == 'workspace/symbol':
        result = [{'name': name, 'kind': 12, 'location': {'uri': root_uri + '/source.rs', 'range': SPAN}}
                  for name in ['foo', 'other']]
        result.append({'name': 'external', 'kind': 12, 'location': {'uri': 'file:///outside.rs', 'range': SPAN}})
    elif method == 'textDocument/prepareCallHierarchy':
        result = [{'name': 'foo', 'kind': 12, 'uri': uri, 'range': SPAN, 'selectionRange': SPAN}]
    elif method in ('callHierarchy/incomingCalls', 'callHierarchy/outgoingCalls'):
        target = {'name': 'foo', 'kind': 12, 'uri': args['item']['uri'], 'range': SPAN, 'selectionRange': SPAN}
        result = [{'from' if method.endswith('incomingCalls') else 'to': target, 'fromRanges': [SPAN]}]
    elif method == 'textDocument/rangeFormatting':
        result = [{'range': args['range'], 'newText': 'FN'}]
    elif method == 'textDocument/inlayHint':
        result = [{'position': pos(2), 'label': [{'value': ': fixture', 'command': {'title': 'Never', 'command': 'fixture.forbidden'}}],
                   'data': {'index': index}} for index in range(3)]
    elif method == 'inlayHint/resolve':
        result = dict(args)
        result.update({'tooltip': 'resolved fixture hint', 'textEdits': [{'range': POINT, 'newText': '// hint\n'}]})
    elif method.startswith('textDocument/semanticTokens/'):
        result = {'resultId': 'fixture-v1', 'data': [0, 0, 2, 0, 1]}
        if '--tokens-in-range' in sys.argv and 'range' in args:
            start, end = args['range']['start'], args['range']['end']
            result['data'] = [start['line'], start['character'], end['character'] - start['character'], 0, 1]
    elif method == 'workspace/executeCommand':
        raise RuntimeError('Engine must never execute a server command')
    elif method == 'shutdown':
        result = None
    else:
        send({'jsonrpc': '2.0', 'id': frame['id'], 'error': {'code': -32601, 'message': 'Unknown fixture method'}})
        continue
    send({'jsonrpc': '2.0', 'id': frame['id'], 'result': result})
