#!/usr/bin/env python3
"""Run ONLY against a real compiled Odin binary. Missing binary is NOT_RUN + failure exit."""
from pathlib import Path
import argparse, json, sys, tempfile, time
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tests'))
from protocol_harness import Peer

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,default=ROOT/'verification/local-mcp-smoke.json');args=p.parse_args()
    results=[];status='NOT_RUN';error=None
    def check(name, condition):
        results.append({'name':name,'status':'PASS' if condition else 'FAIL'})
        if not condition:raise AssertionError(name)
    try:
        if not args.binary.is_file():raise FileNotFoundError('Build the real Odin binary before running this test')
        status='FAIL'
        with tempfile.TemporaryDirectory(prefix='odin-mcp-') as directory:
            base=Path(directory);rw=base/'rw';ro=base/'ro';rw.mkdir();ro.mkdir()
            (rw/'a.txt').write_text('alpha\n', encoding='utf-8');(ro/'b.txt').write_text('beta\n', encoding='utf-8')
            config=base/'host.toml'
            config.write_text(f'state_dir = {json.dumps(str(base/"state"))}\n[projects.rw]\nroot = {json.dumps(str(rw))}\nallow_write = true\n[projects.ro]\nroot = {json.dumps(str(ro))}\nallow_write = false\n', encoding='utf-8')
            with Peer([str(args.binary.resolve()),'--config',str(config),'serve','--transport','stdio']) as peer:
                response=peer.request('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'odin-smoke','version':'0.1'}})
                check('initialize',response.get('result',{}).get('serverInfo',{}).get('name')=='odin')
                peer.notify('notifications/initialized')
                tools=peer.request('tools/list')['result']['tools']
                check('fifty_eight_tools',len(tools)==58)
                check('no_upstream_bridge_tools',not any(t['name'].startswith('serena_') for t in tools))
                def call(name,args,expect_error=False):
                    response=peer.request('tools/call',{'name':name,'arguments':args})
                    if 'error' in response:raise AssertionError(response)
                    result=response['result'];check('tool_result:'+name,bool(result.get('isError',False))==expect_error)
                    return result.get('structuredContent') or json.loads(result['content'][0]['text'])
                call('read_file',{'project':'rw','relative_path':'../host.toml'},True)
                body=call('read_file',{'project':'rw','relative_path':'a.txt'})
                check('read_sha',len(body['sha256'])==64 and body['content']=='alpha\n')
                call('create_text_file',{'project':'ro','relative_path':'no.txt','content':'x','expected_sha256':None},True)
                preview=call('replace_content',{'project':'rw','relative_path':'a.txt','expected_sha256':body['sha256'],'needle':'alpha','replacement':'changed'})
                check('preview_does_not_mutate',(rw/'a.txt').read_text(encoding='utf-8')=='alpha\n')
                transaction=preview['transaction_id']
                call('transaction_apply',{'project':'rw','transaction_id':transaction})
                check('apply_changes_file',(rw/'a.txt').read_text(encoding='utf-8')=='changed\n')
                call('transaction_rollback',{'project':'rw','transaction_id':transaction})
                check('rollback_restores_file',(rw/'a.txt').read_text(encoding='utf-8')=='alpha\n')
                call('write_memory',{'project':'rw','name':'core','content':'Architecture','expected_sha256':None})
                memory=call('read_memory',{'project':'rw','name':'core'})
                check('memory_content',memory['content']=='Architecture')
                flow=call('flow_run',{'project':'rw','session':'test','steps':[{'as':'x','tool':'list_dir','arguments':{}},{'as':'y','select':{'$ref':'x#/files'}}],'return':{'$ref':'y'}})
                check('flow_result','a.txt' in flow['result'])
                status_body=call('status',{'project':'rw'})
                check('standalone_status',status_body['standalone'] and not status_body['external_tool_bridge'])
                for removed_name in ['serena_tools','serena_call']:
                    removed=peer.request('tools/call',{'name':removed_name,'arguments':{'project':'rw'}})
                    check('removed_tool:'+removed_name,removed.get('error',{}).get('code')==-32601)
        status='PASS'
    except Exception as exc:error=str(exc)
    report={'scope':'REAL_ODIN_BINARY_STDIO_SMOKE','status':status,'binary':str(args.binary),'checks':results,'error':error,'real_lsp_dap_tested':False,'timestamp_unix':time.time()}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report));return 0 if status=='PASS' else 1
if __name__=='__main__':raise SystemExit(main())
