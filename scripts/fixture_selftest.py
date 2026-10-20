#!/usr/bin/env python3
"""Tests MOCK PEERS THEMSELVES, not Rust or real LSP/DAP implementations."""
from pathlib import Path
import argparse, json, sys, time
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tests'))
from protocol_harness import Peer

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,default=ROOT/'verification/fixture-selftest.json');args=parser.parse_args()
    results=[]
    def check(name, condition):
        results.append({'name':name,'status':'PASS' if condition else 'FAIL'})
        if not condition:raise AssertionError(name)
    fixture=str(ROOT/'tests/fixtures/protocol_peer.py')
    try:
        with Peer([sys.executable,fixture,'lsp'],'content-length') as peer:
            check('mock_lsp_initialize',peer.request('initialize')['result']['capabilities']['positionEncoding']=='utf-16')
            uri='file:///fixture/lib.rs'
            peer.notify('textDocument/didOpen',{'textDocument':{'uri':uri,'version':1,'text':'fn foo() {}\n'}})
            result=peer.request('textDocument/documentSymbol',{'textDocument':{'uri':uri}})['result']
            check('mock_lsp_symbol_reply',result[0]['name']=='foo')
            check('mock_lsp_diagnostic_reply',peer.request('textDocument/diagnostic',{'textDocument':{'uri':uri}})['result']['items']==[])
        with Peer([sys.executable,fixture,'dap'],'content-length') as peer:
            peer.send({'seq':1,'type':'request','command':'initialize','arguments':{}})
            check('mock_dap_initialize',peer.receive(1,dap=True)['body']['supportsConfigurationDoneRequest'])
            peer.send({'seq':2,'type':'request','command':'launch','arguments':{}})
            peer.send({'seq':3,'type':'request','command':'configurationDone','arguments':{}})
            check('mock_dap_configuration_done',peer.receive(3,dap=True)['success'])
            check('mock_dap_launch_unblocks',peer.receive(2,dap=True)['body']['fixture'])
        status='PASS'
    except Exception as error:
        status='FAIL';results.append({'name':'execution','status':'FAIL','error':str(error)})
    report={'scope':'PYTHON_MOCK_PEERS_ONLY','status':status,'python':sys.version,'checks':results,
            'rust_compiled':False,'real_backend_coverage':False,'timestamp_unix':time.time()}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n', encoding='utf-8')
    print(json.dumps({'scope':report['scope'],'status':status,'checks':len(results)},ensure_ascii=False))
    return 0 if status=='PASS' else 1
if __name__=='__main__':raise SystemExit(main())
