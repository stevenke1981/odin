#!/usr/bin/env python3
"""Local authentication checks against a REAL compiled Odin binary. Not an MCP conformance suite."""
from pathlib import Path
import argparse,json,os,secrets,socket,subprocess,tempfile,time,urllib.request,urllib.error
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,default=ROOT/'verification/local-http-smoke.json');args=p.parse_args()
    tests=[];status='NOT_RUN';error=None;process=None
    def check(name,condition):
        tests.append({'name':name,'status':'PASS' if condition else 'FAIL'})
        if not condition:raise AssertionError(name)
    try:
        if not args.binary.is_file():raise FileNotFoundError('Compile Odin before running HTTP acceptance')
        status='FAIL'
        with tempfile.TemporaryDirectory(prefix='odin-http-') as directory:
            root=Path(directory);workspace=root/'project';workspace.mkdir()
            config=root/'host.toml';config.write_text(f'state_dir = {json.dumps(str(root/"state"))}\n[projects.demo]\nroot = {json.dumps(str(workspace))}\n', encoding='utf-8')
            with socket.socket() as probe:probe.bind(('127.0.0.1',0));port=probe.getsockname()[1]
            token=secrets.token_urlsafe(32);env=dict(os.environ,ODIN_TOKEN=token)
            with (root/'http.log').open('w') as log:
                process=subprocess.Popen([str(args.binary.resolve()),'--config',str(config),'serve','--transport','http','--listen',f'127.0.0.1:{port}'],env=env,stdout=log,stderr=log)
                base=f'http://127.0.0.1:{port}'
                # Do not forward localhost credentials through a configured proxy.
                opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
                def get(path,headers=None):
                    try:
                        with opener.open(urllib.request.Request(base+path,headers=headers or {}),timeout=2) as reply:return reply.status,reply.read()
                    except urllib.error.HTTPError as reply:return reply.code,reply.read()
                started=False
                for _ in range(100):
                    if process.poll() is not None:raise RuntimeError((root/'http.log').read_text(encoding='utf-8'))
                    try:
                        if get('/')[0]==200:started=True;break
                    except (urllib.error.URLError,TimeoutError):time.sleep(.1)
                check('server_starts',started)
                check('unauthenticated_api_rejected',get('/api/projects')[0]==401)
                auth={'Authorization':'Bearer '+token}
                code,body=get('/api/projects',auth);check('authenticated_api',code==200 and json.loads(body)['projects'][0]['id']=='demo')
                check('host_header_rejected',get('/api/projects',dict(auth,Host='attacker.invalid'))[0]==403)
                check('cross_origin_rejected',get('/api/projects',dict(auth,Origin='https://attacker.invalid'))[0]==403)
                check('wrong_token_rejected',get('/api/projects',{'Authorization':'Bearer '+secrets.token_urlsafe(32)})[0]==401)
                check('audit_api',get('/api/audit/demo',auth)[0]==200)
                process.terminate();process.wait(timeout=5);process=None
        status='PASS'
    except Exception as exc:error=str(exc)
    finally:
        if process is not None:
            process.kill();process.wait(timeout=5)
    report={'scope':'REAL_ODIN_BINARY_LOOPBACK_HTTP_AUTH_SMOKE','status':status,'checks':tests,'error':error,'full_mcp_http_conformance':False}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n', encoding='utf-8');print(json.dumps(report))
    return 0 if status=='PASS' else 1
if __name__=='__main__':raise SystemExit(main())
