#!/usr/bin/env python3
"""DETERMINISTIC PROTOCOL FIXTURE, NOT A COMPILER / A DEBUGGER.
Used only to test framing, request dispatch, lifecycle, and timeout behavior.
"""
import argparse
import json
import re
import sys
import time
from pathlib import Path
from urllib.parse import urlsplit
from urllib.request import url2pathname

P = argparse.ArgumentParser()
P.add_argument('mode', choices=['lsp', 'dap'])
P.add_argument('--sync-kind', type=int, default=1)
P.add_argument('--push-only', action='store_true')
P.add_argument('--stale-diagnostics', action='store_true')
P.add_argument('--dap-minimal', action='store_true')
P.add_argument('--dap-restart-config', action='store_true')
P.add_argument('--ide', action='store_true')
P.add_argument('--capabilities-json', default='{}')
P.add_argument('--encoding', default='utf-16')
P.add_argument('--mutate-on')
P.add_argument('--diagnostic-kind', default='clean')
P.add_argument('--diagnostic-uri-alias', choices=['drive-colon', 'drive-colon-lower', 'escaped-name'])
P.add_argument('--rename-workspace-edit', choices=['changes-empty', 'changes-null', 'documents-empty', 'documents-null', 'ambiguous', 'malformed'])
P.add_argument('--ide-malformed')
P.add_argument('--started-marker')
P.add_argument('--expect-language-id')
P.add_argument('--hierarchy-prepare-count', type=int, default=1)
P.add_argument('--hierarchy-related-count', type=int, default=1)
P.add_argument('--hierarchy-payload-bytes', type=int, default=0)
P.add_argument('--hierarchy-range-count', type=int, default=1)
P.add_argument('--hierarchy-delay-ms', type=int, default=0)
P.add_argument('--hierarchy-request-log')
A = P.parse_args()
if A.started_marker:
    Path(A.started_marker).write_text('fixture started', encoding='utf-8')
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
    if A.diagnostic_kind == 'none':
        return
    report = {'uri': diagnostic_uri(uri), 'version': version, 'diagnostics': diagnostic_items(uri)}
    if A.diagnostic_kind == 'unversioned':
        report.pop('version')
    if A.diagnostic_kind == 'stale-out-of-bounds':
        report['version'] = version - 1
        report['diagnostics'] = [{'range': rng(50, 0, 5), 'message': 'old error', 'severity': 1}]
    send({'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics', 'params': report})


def diagnostic_uri(uri):
    if A.diagnostic_uri_alias in ('drive-colon', 'drive-colon-lower'):
        match = re.match(r'file:///([A-Za-z]):(/.*)', uri)
        if not match:
            raise ValueError('drive-colon fixture requires an absolute Windows drive URI')
        colon = '%3A' if A.diagnostic_uri_alias == 'drive-colon' else '%3a'
        return 'file:///' + match[1].lower() + colon + match[2]
    if A.diagnostic_uri_alias == 'escaped-name':
        return re.sub(r'/([A-Za-z])([^/]*)$', lambda m: '/%' + format(ord(m[1]), '02X') + m[2], uri)
    return uri


def diagnostic_items(uri=None):
    if A.diagnostic_kind == 'malformed':
        return 'invalid'
    if A.diagnostic_kind in ('errors', 'unversioned') or (
            A.diagnostic_kind == 'source-toggle' and 'broken' in DOCS.get(uri, '')):
        return [{'range': rng(0, 3, 6), 'message': 'fixture error', 'severity': 1, 'code': 'E-FIXTURE', 'source': 'fixture-linter'},
                {'range': rng(0, 3, 6), 'message': 'fixture warning', 'severity': 2, 'code': 42}]
    return []


def hierarchy_item(uri):
    return {'name': 'foo', 'kind': 12, 'uri': uri, 'range': rng(0, 0, 11), 'selectionRange': rng(0, 3, 6), 'data': {'keep': 'opaque'}}


def hierarchy_request(method):
    if A.hierarchy_request_log:
        with Path(A.hierarchy_request_log).open('a', encoding='utf-8') as log:
            log.write(method + '\n')
    time.sleep(A.hierarchy_delay_ms / 1000)


def hierarchy_related_item(item):
    result = dict(item)
    result['data'] = dict(item['data'], payload='x' * A.hierarchy_payload_bytes)
    return result


def end_position(text):
    lines = re.split(r'\r\n|\r|\n', text)
    return {'line': len(lines) - 1, 'character': len(lines[-1].encode('utf-16-le')) // 2}


def lsp(request):
    global INITIALIZE
    method = request.get('method')
    params = request.get('params', {})
    if A.mutate_on == method:
        uri = params.get('textDocument', {}).get('uri')
        if uri is None and DOCS:
            uri = next(iter(DOCS))
        if uri is not None:
            Path(url2pathname(urlsplit(uri).path)).write_text('fn external_change() {}', encoding='utf-8')
    if method == 'initialize':
        INITIALIZE = params
        capabilities = {
            'positionEncoding': A.encoding, 'textDocumentSync': {'openClose': True, 'change': A.sync_kind},
            'documentSymbolProvider': True, 'referencesProvider': True,
            'definitionProvider': True, 'implementationProvider': True,
            'renameProvider': {'prepareProvider': True},
            'codeActionProvider': {'resolveProvider': True},
        }
        if not A.push_only:
            capabilities['diagnosticProvider'] = {'interFileDependencies': False, 'workspaceDiagnostics': False}
        if A.ide:
            capabilities.update({'completionProvider': {'resolveProvider': True}, 'hoverProvider': True,
                                 'signatureHelpProvider': {'triggerCharacters': ['(']}, 'workspaceSymbolProvider': True,
                                 'callHierarchyProvider': True, 'typeHierarchyProvider': True,
                                 'documentFormattingProvider': True, 'documentRangeFormattingProvider': True,
                                 'inlayHintProvider': {'resolveProvider': True},
                                 'semanticTokensProvider': {'full': True, 'range': True, 'legend': {'tokenTypes': ['function'], 'tokenModifiers': ['declaration']}}})
        capabilities.update(json.loads(A.capabilities_json))
        respond(request, {'capabilities': capabilities})
    elif method == 'textDocument/didOpen':
        doc = params['textDocument']
        if A.expect_language_id:
            assert doc['languageId'] == A.expect_language_id
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
        if A.diagnostic_kind == 'backend-error':
            send({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32603, 'message': 'Fixture diagnostic backend failed'}})
        else:
            respond(request, {'kind': 'full', 'items': diagnostic_items(params['textDocument']['uri'])})
    elif method == 'textDocument/completion':
        respond(request, {'isIncomplete': False, 'items': [{'label': 'foo', 'kind': 3, 'data': {'uri': params['textDocument']['uri']},
                                                         'command': {'title': 'data only', 'command': 'must.not.execute'}}]})
    elif method == 'completionItem/resolve':
        item = dict(params)
        item['detail'] = 'resolved fixture completion'
        item['additionalTextEdits'] = []
        respond(request, item)
    elif method == 'textDocument/hover':
        respond(request, {'contents': {'kind': 'plaintext', 'value': 'fixture hover'}, 'range': rng(0, 3, 6)})
    elif method == 'textDocument/signatureHelp':
        respond(request, {'signatures': [{'label': 'foo(arg: i32)', 'parameters': [{'label': [4, 12]}]}], 'activeSignature': 0, 'activeParameter': 0})
    elif method == 'workspace/symbol':
        symbols = [{'name': 'foo', 'kind': 12, 'location': {'uri': uri, 'range': rng(0, 3, 6)}} for uri in DOCS]
        symbols.append({'name': 'outside', 'kind': 12, 'location': {'uri': (Path.cwd().parent / 'outside.rs').as_uri(), 'range': rng(0, 0, 1)}})
        respond(request, symbols)
    elif method in ('textDocument/prepareCallHierarchy', 'textDocument/prepareTypeHierarchy'):
        hierarchy_request(method)
        respond(request, {} if A.ide_malformed == 'hierarchy' else
                [hierarchy_item(params['textDocument']['uri']) for _ in range(A.hierarchy_prepare_count)])
    elif method in ('callHierarchy/incomingCalls', 'callHierarchy/outgoingCalls'):
        hierarchy_request(method)
        key = 'from' if method.endswith('incomingCalls') else 'to'
        respond(request, [{key: hierarchy_related_item(params['item']),
                           'fromRanges': [rng(0, 3, 6) for _ in range(A.hierarchy_range_count)]}
                          for _ in range(A.hierarchy_related_count)])
    elif method in ('typeHierarchy/supertypes', 'typeHierarchy/subtypes'):
        hierarchy_request(method)
        respond(request, [hierarchy_related_item(params['item']) for _ in range(A.hierarchy_related_count)])
    elif method in ('textDocument/formatting', 'textDocument/rangeFormatting'):
        range_to_edit = params.get('range', rng(0, 0, 11))
        respond(request, [{'range': range_to_edit, 'newText': 'fn formatted() {}'}])
    elif method in ('textDocument/semanticTokens/full', 'textDocument/semanticTokens/range'):
        respond(request, {'data': [0, 3, 3] if A.ide_malformed == 'tokens' else [0, 3, 3, 0, 0], 'resultId': 'fixture-tokens'})
    elif method == 'textDocument/inlayHint':
        respond(request, [{'position': {'line': 0, 'character': 6}, 'label': ': i32', 'data': {'uri': params['textDocument']['uri']}}])
    elif method == 'inlayHint/resolve':
        hint = dict(params)
        hint['tooltip'] = 'resolved fixture hint'
        hint['textEdits'] = []
        respond(request, hint)
    elif method == 'textDocument/definition':
        respond(request, {'uri': params['textDocument']['uri'], 'range': rng(0, 3, 6)})
    elif method == 'textDocument/prepareRename':
        respond(request, {'range': rng(0, 3, 6), 'placeholder': 'foo'})
    elif method == 'textDocument/rename':
        edits = [{'range': rng(0, 3, 6), 'newText': params['newName']}]
        changes = {params['textDocument']['uri']: edits}
        documents = [{'textDocument': dict(params['textDocument'], version=None), 'edits': edits}]
        result = {'changes': changes}
        if A.rename_workspace_edit == 'changes-empty':
            result = {'changes': {}, 'documentChanges': documents}
        elif A.rename_workspace_edit == 'changes-null':
            result = {'changes': None, 'documentChanges': documents}
        elif A.rename_workspace_edit == 'documents-empty':
            result['documentChanges'] = []
        elif A.rename_workspace_edit == 'documents-null':
            result['documentChanges'] = None
        elif A.rename_workspace_edit == 'ambiguous':
            result['documentChanges'] = documents
        elif A.rename_workspace_edit == 'malformed':
            result['documentChanges'] = {}
        respond(request, result)
    elif method == 'textDocument/codeAction':
        respond(request, [{'title': 'fixture action', 'data': {'uri': params['textDocument']['uri'], 'diagnostics': params['context']['diagnostics']}}])
    elif method == 'codeAction/resolve':
        result = dict(params)
        result['edit'] = {'changes': {params['data']['uri']: [{'range': rng(0, 3, 6), 'newText': 'resolved'}]}}
        respond(request, result)
    elif method == 'fixture/state':
        respond(request, {'cancelled': CANCELLED, 'documents': DOCS, 'versions': VERSIONS, 'initialize': INITIALIZE})
    elif method == 'fixture/publishDiagnostics':
        send({'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics', 'params': params})
        respond(request, {})
    elif method == 'fixture/progress':
        send({'jsonrpc': '2.0', 'method': '$/progress', 'params': {'token': 'index', 'value': {'kind': 'begin', 'title': 'indexing'}}})
        send({'jsonrpc': '2.0', 'method': 'experimental/serverStatus', 'params': {'health': 'ok', 'quiescent': False}})
        respond(request, {})
    elif method == 'fixture/ready':
        send({'jsonrpc': '2.0', 'method': '$/progress', 'params': {'token': 'index', 'value': {'kind': 'end'}}})
        send({'jsonrpc': '2.0', 'method': 'experimental/serverStatus', 'params': {'health': 'ok', 'quiescent': True}})
        respond(request, {})
    elif method == 'fixture/cancelable':
        send({'jsonrpc': '2.0', 'method': 'fixture/awaiting', 'params': {'id': request['id']}})
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
    elif cmd == 'restart' and A.dap_restart_config:
        LAUNCHES.append(request)
        send({'seq': 107, 'type': 'event', 'event': 'initialized'})
    elif cmd in ('continue', 'restart'):
        respond(request, {})
        send({'seq': 103, 'type': 'event', 'event': 'continued', 'body': {'threadId': 1, 'allThreadsContinued': True}})
    elif cmd == 'pause':
        respond(request, {})
        send({'seq': 104, 'type': 'event', 'event': 'stopped', 'body': {'reason': 'pause', 'threadId': 1}})
    elif cmd in ('next', 'stepIn', 'stepOut'):
        # A fast target can stop again before the step response arrives.
        send({'seq': 106, 'type': 'event', 'event': 'stopped', 'body': {'reason': 'step', 'threadId': 1}})
        respond(request, {})
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
