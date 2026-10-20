#!/usr/bin/env python3
"""DETERMINISTIC PROTOCOL FIXTURE, NOT A COMPILER / A DEBUGGER.
Used only to test framing, request dispatch, lifecycle, and timeout behavior.
"""
import argparse, json, re, sys, time
from pathlib import Path
P=argparse.ArgumentParser();P.add_argument('mode',choices=['lsp','dap']);A=P.parse_args()
DOCS={}; LAUNCHES=[]
def read():
    length=None
    while True:
        line=sys.stdin.buffer.readline()
        if not line:return None
        if line in (b'\n',b'\r\n'):break
        name,value=line.decode().split(':',1)
        if name.lower()=='content-length':length=int(value.strip())
    if length is None:raise ValueError('missing Content-Length')
    raw=sys.stdin.buffer.read(length)
    if len(raw)!=length:raise ValueError('truncated frame')
    return json.loads(raw)
def send(value):
    data=json.dumps(value,ensure_ascii=False,separators=(',',':')).encode()
    sys.stdout.buffer.write(f'Content-Length: {len(data)}\r\n\r\n'.encode()+data)
    sys.stdout.buffer.flush()
def respond(request,result):
    if A.mode=='dap':send({'seq':100,'type':'response','request_seq':request['seq'],'command':request['command'],'success':True,'body':result})
    else:send({'jsonrpc':'2.0','id':request['id'],'result':result})
def rng(line,start,end):return {'start':{'line':line,'character':start},'end':{'line':line,'character':end}}
def lsp(request):
    method=request.get('method');params=request.get('params',{})
    if method=='initialize':respond(request,{'capabilities':{'positionEncoding':'utf-16','documentSymbolProvider':True,'referencesProvider':True,'definitionProvider':True,'implementationProvider':True,'renameProvider':True,'diagnosticProvider':{'interFileDependencies':False,'workspaceDiagnostics':False}}})
    elif method=='textDocument/didOpen':
        doc=params['textDocument'];DOCS[doc['uri']]=doc['text']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{'uri':doc['uri'],'version':doc['version'],'diagnostics':[]}})
    elif method=='textDocument/didChange':DOCS[params['textDocument']['uri']]=params['contentChanges'][0]['text']
    elif method=='textDocument/documentSymbol':
        text=DOCS[params['textDocument']['uri']];symbols=[]
        for line,text_line in enumerate(text.splitlines()):
            m=re.search(r'fn (\w+)',text_line)
            if m:symbols.append({'name':m[1],'kind':12,'range':rng(line,0,len(text_line)),'selectionRange':rng(line,m.start(1),m.end(1))})
        respond(request,symbols)
    elif method in ('textDocument/references','textDocument/implementation'):respond(request,[])
    elif method=='textDocument/diagnostic':respond(request,{'kind':'full','items':[]})
    elif method=='textDocument/definition':respond(request,{'uri':params['textDocument']['uri'],'range':rng(0,3,6)})
    elif method=='textDocument/rename':respond(request,{'changes':{params['textDocument']['uri']:[{'range':rng(0,3,6),'newText':params['newName']}]}})
    elif method=='fixture/hang':time.sleep(30)
    elif 'id' in request:send({'jsonrpc':'2.0','id':request['id'],'error':{'code':-32601,'message':'Fixture method unsupported'}})
def dap(request):
    cmd=request['command']
    if cmd=='initialize':respond(request,{'supportsConfigurationDoneRequest':True});send({'seq':101,'type':'event','event':'initialized'})
    elif cmd in ('launch','attach'):LAUNCHES.append(request)
    elif cmd=='configurationDone':
        respond(request,{})
        for launch in LAUNCHES:respond(launch,{'fixture':True})
        LAUNCHES.clear();send({'seq':102,'type':'event','event':'stopped','body':{'reason':'breakpoint','threadId':1}})
    elif cmd=='threads':respond(request,{'threads':[{'id':1,'name':'fixture'}]})
    else:respond(request,{'fixture':True})
while True:
    request=read()
    if request is None:break
    {'lsp':lsp,'dap':dap}[A.mode](request)
