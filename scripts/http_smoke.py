#!/usr/bin/env python3
"""Authenticated API and MCP requests to a real native binary; no external services."""
from pathlib import Path
import argparse, hashlib, json, os, platform, secrets, socket, subprocess, tempfile, time
import urllib.request, urllib.error

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'verification/local-http-smoke.json')
    args = parser.parse_args()
    tests, status, error, process = [], 'NOT_RUN', None, None

    def check(name, condition):
        tests.append({'name': name, 'status': 'PASS' if condition else 'FAIL'})
        if not condition:
            raise AssertionError(name)

    try:
        if not args.binary.is_file():
            raise FileNotFoundError('Compile Odin before running HTTP acceptance')
        status = 'FAIL'
        with tempfile.TemporaryDirectory(prefix='odin-http-') as directory:
            root = Path(directory)
            workspace = root/'project'
            workspace.mkdir()
            secret_sentinel = 'SHOULD_NOT_APPEAR_IN_DASHBOARD_' + secrets.token_hex(8)
            (workspace/'private.txt').write_text(secret_sentinel, encoding='utf-8')
            config = root/'host.toml'
            config.write_text(f'state_dir = {json.dumps(str(root/"state"))}\n[projects.demo]\nroot = {json.dumps(str(workspace))}\n', encoding='utf-8')
            with socket.socket() as probe:
                probe.bind(('127.0.0.1', 0))
                port = probe.getsockname()[1]
            token = secrets.token_urlsafe(32)
            env = dict(os.environ, ODIN_TOKEN=token)
            with (root/'http.log').open('w', encoding='utf-8') as log:
                process = subprocess.Popen([str(args.binary.resolve()), '--config', str(config), 'serve', '--transport', 'http', '--listen', f'127.0.0.1:{port}'], env=env, stdout=log, stderr=log)
                base = f'http://127.0.0.1:{port}'
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

                def request(path, headers=None, payload=None, method=None):
                    data = json.dumps(payload).encode('utf-8') if payload is not None else None
                    try:
                        req = urllib.request.Request(base+path, data=data, headers=headers or {}, method=method)
                        with opener.open(req, timeout=10) as reply:
                            return reply.status, reply.read(), reply.headers
                    except urllib.error.HTTPError as reply:
                        return reply.code, reply.read(), reply.headers

                started = False
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError('HTTP server exited before readiness')
                    try:
                        if request('/')[0] == 200:
                            started = True
                            break
                    except (urllib.error.URLError, TimeoutError):
                        time.sleep(.1)
                check('server_starts', started)
                auth = {'Authorization': 'Bearer '+token}
                for path in ['/api/projects', '/api/status', '/api/metadata', '/api/audit/demo', '/api/audit/demo/export', '/mcp']:
                    check('unauthenticated_rejected:'+path, request(path)[0] == 401)
                code, body, headers = request('/api/projects', auth)
                check('authenticated_projects', code == 200 and json.loads(body)['projects'][0]['id'] == 'demo')
                check('no_store_api', headers.get('Cache-Control') == 'no-store')
                check('host_header_rejected', request('/api/projects', dict(auth, Host='attacker.invalid'))[0] == 403)
                check('cross_origin_rejected', request('/api/projects', dict(auth, Origin='https://attacker.invalid'))[0] == 403)
                check('wrong_token_rejected', request('/api/projects', {'Authorization': 'Bearer '+secrets.token_urlsafe(32)})[0] == 401)
                check('dashboard_csp', "frame-ancestors 'none'" in request('/')[2].get('Content-Security-Policy', ''))
                code, body, _ = request('/api/metadata', auth)
                check('build_metadata', code == 200 and json.loads(body)['standalone'] is True and bool(json.loads(body)['target']))
                code, body, _ = request('/api/status', auth)
                snapshot = json.loads(body)
                check('readonly_status', code == 200 and snapshot['dashboard_read_only'] and snapshot['backends_probed'] is False and snapshot['model_token_usage'] == 'UNKNOWN')
                check('api_methods_readonly', request('/api/status', auth, {}, 'POST')[0] == 405)
                mcp_headers = dict(auth, Accept='application/json, text/event-stream', **{'Content-Type': 'application/json'})

                def rpc(identifier, method, params=None):
                    envelope = {'jsonrpc': '2.0', 'method': method, 'params': params or {}}
                    if identifier is not None:
                        envelope['id'] = identifier
                    code, raw, reply_headers = request('/mcp', mcp_headers, envelope)
                    if identifier is None:
                        check('mcp_notification_accepted', code in (200, 202, 204))
                        return {}
                    check('mcp_http_status:'+method, code == 200)
                    if reply_headers.get('Mcp-Session-Id'):
                        mcp_headers['Mcp-Session-Id'] = reply_headers['Mcp-Session-Id']
                    if reply_headers.get('Content-Type', '').startswith('text/event-stream'):
                        messages = [json.loads(line[5:].strip()) for line in raw.decode('utf-8').splitlines() if line.startswith('data:') and line[5:].strip()]
                        result = next((item for item in messages if item.get('id') == identifier), None)
                    else:
                        result = json.loads(raw)
                    check('mcp_response:'+method, isinstance(result, dict) and result.get('id') == identifier and 'error' not in result)
                    return result['result']

                initialized = rpc(1, 'initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'odin-http-smoke', 'version': '1'}})
                check('mcp_initialize', initialized['serverInfo']['name'] == 'odin')
                mcp_headers['Mcp-Protocol-Version'] = initialized['protocolVersion']
                rpc(None, 'notifications/initialized')
                tools = rpc(2, 'tools/list')['tools']
                catalog = json.loads((ROOT/'schemas/tools.json').read_text(encoding='utf-8'))
                check('mcp_catalog_names', sorted(t['name'] for t in tools) == sorted(t['name'] for t in catalog))
                live = {tool['name']: tool for tool in tools}
                check('mcp_catalog_schemas', all(live[t['name']]['inputSchema'] == t['inputSchema'] for t in catalog))
                called = rpc(3, 'tools/call', {'name': 'read_file', 'arguments': {'project': 'demo', 'relative_path': 'private.txt'}})
                check('mcp_real_tool_call', not called.get('isError') and called.get('structuredContent', {}).get('content') == secret_sentinel)
                for path in ['/api/projects', '/api/status', '/api/metadata', '/api/audit/demo', '/api/audit/demo/export']:
                    code, raw, _ = request(path, auth)
                    check('api_no_raw_content_or_token:'+path, code == 200 and secret_sentinel.encode() not in raw and token.encode() not in raw)
                code, raw, _ = request('/api/audit/demo', auth)
                entries = json.loads(raw)['entries']
                check('audit_records_real_tool', any(item['tool'] == 'read_file' and item['ok'] and isinstance(item['elapsed_ms'], int) for item in entries))
                check('audit_metadata_allowlist', all(set(item) <= {'at_ms', 'project', 'tool', 'ok', 'elapsed_ms', 'error_code', 'trace_id'} for item in entries))
                code, raw, headers = request('/api/audit/demo/export', auth)
                check('audit_jsonl_export', code == 200 and headers.get('Content-Type', '').startswith('application/x-ndjson') and all(isinstance(json.loads(line), dict) for line in raw.splitlines() if line.strip()))
                check('no_token_in_server_log', token not in (root/'http.log').read_text(encoding='utf-8'))
                process.terminate()
                process.wait(timeout=5)
                process = None
        status = 'PASS'
    except Exception as exc:
        error = str(exc)
    finally:
        if process is not None:
            process.kill()
            process.wait(timeout=5)
    report = {'scope': 'REAL_ODIN_BINARY_LOOPBACK_HTTP_AND_MCP_SMOKE', 'status': status, 'binary': str(args.binary.resolve()),
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary.is_file() else None,
              'platform': platform.platform(), 'checks': tests, 'error': error, 'full_mcp_http_conformance': False, 'timestamp_unix': time.time()}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report))
    return 0 if status == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
