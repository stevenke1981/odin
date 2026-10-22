#!/usr/bin/env python3
"""Run the production Inno recipe with inert workers, real windows and process jobs.

This verifies installer progress/cancel/failure controls, not LSP health or semantics.
Only windows/processes descended from this driver's private installer are operated.
"""
import argparse
import ctypes
from ctypes import wintypes as w
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
WORKER = r'''
import fs from 'node:fs';
import path from 'node:path';
import {spawn} from 'node:child_process';
import {SetupProgress} from './setup-progress.mjs';
const args=process.argv.slice(2), get=key=>args[args.indexOf(key)+1];
const receipt=get('--receipt'), mode=path.basename(receipt).replace('.json','');
const status=get('--progress-file'), cancel=get('--cancel-file'), id=get('--progress-run-id');
const observation=receipt+'.observation', history=receipt+'.history';
let monitor, record;
function rawError(error){
 fs.writeFileSync(receipt+'.error',JSON.stringify({message:error.message,code:error.code,syscall:error.syscall,path:error.path,dest:error.dest,stack:error.stack}));
 if(error.setupExitCode&&monitor){
  monitor.failed(error);fs.writeFileSync(receipt,JSON.stringify({...record,result:error.phase,exit_code:error.setupExitCode,cancel_nonce:fs.existsSync(cancel)?fs.readFileSync(cancel,'utf8'):null}));
 }
 process.exit(error.setupExitCode||1);
}
process.on('uncaughtException',rawError);process.on('unhandledRejection',rawError);
if(!/^[a-f0-9]{32}$/.test(id)||path.dirname(status)!==path.dirname(cancel)||fs.existsSync(status)||fs.existsSync(cancel))throw Error('Bad run contract');
const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true});
record={mode,worker:process.pid,installer:process.ppid,child:child.pid,args,
  node_options_inherited:Object.hasOwn(process.env,'NODE_OPTIONS'),
  sentinel_inherited:Object.hasOwn(process.env,'ODIN_PROGRESS_TEST_SENTINEL')};
fs.writeFileSync(observation,JSON.stringify(record));
if(mode==='deadline')await new Promise(resolve=>setTimeout(resolve,650));
monitor=new SetupProgress({progressFile:status,cancelFile:cancel,progressRunId:mode==='wrong_nonce'?'0'.repeat(32):id,setupTimeoutSeconds:Number(get('--setup-timeout-seconds'))});
function emit(phase,n=0,total=100,code=0){
 // The inert worker controls phases, but the production writer owns every atomic status update.
 monitor.phase=phase;monitor.completed=n;monitor.total=total;monitor.component='fixture';
 monitor.write(code,'Inert installer progress probe');
 fs.appendFileSync(history,JSON.stringify({phase,n,code,at:Date.now()})+'\n');
}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function ticks(phase,ms){for(let n=0;n<ms/25;n++){emit(phase,n,Math.ceil(ms/25));await delay(25);}}
function finish(phase,code){emit(phase,1,1,code);fs.writeFileSync(receipt,JSON.stringify({...record,result:phase,exit_code:code,cancel_nonce:fs.existsSync(cancel)?fs.readFileSync(cancel,'utf8'):null}));process.exit(code);}
emit('starting');
if(mode==='deadline'){
 emit('probe',0,1);
 await delay(monitor.probeTimeout()+25);
 try{monitor.guard(true);}catch(error){finish(error.phase,error.setupExitCode);}
 throw Error('Expected the production worker deadline guard to throw');
}
await ticks('verify_before',750);
if(mode==='timeout'){await ticks('probe',2200);finish('timeout',1460);}
if(mode==='parent_exit'){await ticks('probe',60000);finish('failed',19);}
if(mode==='cancel'){
 await ticks('probe',2500);
 if(!fs.existsSync(cancel)||fs.readFileSync(cancel,'utf8')!==id)finish('failed',18);
 finish('cancelled',1223);
}
await ticks('probe',900);
if(mode==='failure')finish('failed',17);
await ticks('verify_after',750);
await ticks('applying',600);
finish('done',0);
'''


def digest(filename):
    return hashlib.sha256(Path(filename).read_bytes()).hexdigest()


class Windows:
    def __init__(self):
        self.u = ctypes.WinDLL('user32', use_last_error=True)
        self.k = ctypes.WinDLL('kernel32', use_last_error=True)
        self.callback = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
        self.u.EnumWindows.argtypes = [self.callback, w.LPARAM]
        self.u.EnumChildWindows.argtypes = [w.HWND, self.callback, w.LPARAM]
        self.u.GetWindowThreadProcessId.argtypes = [w.HWND, ctypes.POINTER(w.DWORD)]
        self.u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, ctypes.c_int]
        self.u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, ctypes.c_int]
        self.u.IsWindowVisible.argtypes = [w.HWND]
        self.u.IsWindowEnabled.argtypes = [w.HWND]
        self.u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
        self.u.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM,
                                              w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]
        self.u.SendMessageTimeoutW.restype = ctypes.c_size_t
        self.k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
        self.k.OpenProcess.restype = w.HANDLE
        self.k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
        self.k.CloseHandle.argtypes = [w.HANDLE]
        self.k.TerminateProcess.argtypes = [w.HANDLE, w.UINT]
        self.k.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
        self.k.CreateToolhelp32Snapshot.restype = w.HANDLE
        class Entry(ctypes.Structure):
            _fields_ = [('size',w.DWORD),('usage',w.DWORD),('pid',w.DWORD),
                        ('heap',ctypes.c_size_t),('module',w.DWORD),('threads',w.DWORD),
                        ('parent',w.DWORD),('priority',w.LONG),('flags',w.DWORD),
                        ('exe',w.WCHAR*260)]
        self.Entry = Entry
        self.k.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]
        self.k.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]

    def children(self, parent):
        snap = self.k.CreateToolhelp32Snapshot(2,0)
        if snap == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        parents = {}
        entry = self.Entry(size=ctypes.sizeof(self.Entry))
        try:
            more = self.k.Process32FirstW(snap,ctypes.byref(entry))
            while more:
                parents[entry.pid] = entry.parent
                more = self.k.Process32NextW(snap,ctypes.byref(entry))
        finally:
            self.k.CloseHandle(snap)
        owned = {parent}
        while True:
            updated = owned | {pid for pid,ppid in parents.items() if ppid in owned}
            if updated == owned:
                return owned
            owned = updated

    def text(self, handle):
        value = ctypes.create_unicode_buffer(4096)
        self.u.GetWindowTextW(handle,value,len(value))
        return value.value

    def windows(self, pids):
        handles = []
        @self.callback
        def visit(handle, unused):
            pid = w.DWORD()
            self.u.GetWindowThreadProcessId(handle,ctypes.byref(pid))
            if pid.value in pids and self.u.IsWindowVisible(handle):
                handles.append(handle)
            return True
        self.u.EnumWindows(visit,0)
        return handles

    def controls(self, handle):
        result = []
        @self.callback
        def visit(child, unused):
            if self.u.IsWindowVisible(child):
                cls = ctypes.create_unicode_buffer(100)
                self.u.GetClassNameW(child,cls,len(cls))
                result.append({'handle':child,'text':self.text(child),'class':cls.value,
                               'enabled':bool(self.u.IsWindowEnabled(child))})
            return True
        self.u.EnumChildWindows(handle,visit,0)
        return result

    def click(self, control):
        if not control['enabled']:
            raise AssertionError('Refusing a disabled probe control')
        self.u.PostMessageW(control['handle'],0x00F5,0,0)  # BM_CLICK, owned window only.

    def responsive(self, handle):
        result = ctypes.c_size_t()
        return bool(self.u.SendMessageTimeoutW(handle,0,0,0,2,500,ctypes.byref(result)))

    def alive(self, pid):
        handle = self.k.OpenProcess(0x00100000,False,pid)
        if not handle:
            return False
        try:
            return self.k.WaitForSingleObject(handle,0) == 0x102
        finally:
            self.k.CloseHandle(handle)

    def terminate_fixture(self, pid):
        handle = self.k.OpenProcess(1,False,pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            if not self.k.TerminateProcess(handle,93):
                raise ctypes.WinError(ctypes.get_last_error())
        finally:
            self.k.CloseHandle(handle)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--node',type=Path,required=True)
    parser.add_argument('--iscc',type=Path,required=True)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--progress-module',type=Path,help='Production SetupProgress module; defaults to packaging/setup-progress.mjs')
    parser.add_argument('--case',action='append',dest='selected_cases',choices=['success','fresh_home','switch_existing','cancel','failure','timeout','deadline','wrong_nonce','parent_exit'])
    args=parser.parse_args()
    if platform.system()!='Windows':
        raise RuntimeError('Real Windows is required')
    node,iscc=args.node.resolve(strict=True),args.iscc.resolve(strict=True)
    progress_module=(args.progress_module or ROOT/'packaging'/'setup-progress.mjs').resolve(strict=True)
    base=args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='odin-progress-probe-')).resolve()
    if args.output:
        base.mkdir(parents=True,exist_ok=False)
    stage=base/'stage';stage.mkdir()
    (stage/'backends'/'node').mkdir(parents=True)
    shutil.copy2(node,stage/'backends'/'node'/'node.exe')
    (stage/'LICENSE').write_text('Inert installer progress probe only.\n',encoding='utf-8')
    (stage/'README.md').write_text('No LSP support is supplied by this fixture.\n',encoding='utf-8')
    (stage/'setup-lsps.mjs').write_text(WORKER,encoding='utf-8',newline='\n')
    (stage/'setup-progress.mjs').write_bytes(progress_module.read_bytes())
    source_hashes={'recipe_sha256':digest(ROOT/'packaging'/'odin.iss'),'helper_sha256':digest(ROOT/'packaging'/'installer-progress.iss'),'test_driver_sha256':digest(__file__)}
    commands,cases,errors=[],[],[]
    shortcut=Path(os.environ['APPDATA'])/'Microsoft'/'Windows'/'Start Menu'/'Programs'/'Odin'/'Odin documentation.lnk'
    shortcut_before=shortcut.read_bytes() if shortcut.is_file() else None
    def run(values,timeout=120,inno=False):
        values=[str(item) for item in values]
        command=' '.join('"'+item+'"' for item in values) if inno else values
        result=subprocess.run(command,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=timeout,
                              creationflags=subprocess.CREATE_NO_WINDOW)
        commands.append({'argv':values,'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
        return result
    compile_result=run([iscc,'--no-compression','--no-ide-signtools','--no-signing','--define=AppVersion=0.2.0',
                        f'--define=StageDir={stage}',f'--define=OutputPath={base}',ROOT/'packaging'/'odin.iss'])
    if compile_result.returncode:
        errors.append('Actual Inno compilation failed')
    ui=Windows()
    installer=base/'odin-0.2.0-windows-x64-setup.exe'
    for mode in ([] if errors else args.selected_cases or ['success','fresh_home','switch_existing','cancel','failure','timeout','deadline','wrong_nonce','parent_exit']):
        app=base/(mode+' app space');host=base/(mode+' host.toml');receipt=base/(mode+'.json')
        sentinel=b'# Inert existing host; must remain byte-for-byte unchanged.\n'
        if mode not in ['fresh_home','switch_existing']:host.write_bytes(sentinel)
        expected_host=base/'switch target existing host.toml' if mode=='switch_existing' else host
        if mode=='switch_existing':expected_host.write_bytes(sentinel)
        observation=Path(str(receipt)+'.observation')
        command=[str(installer),'/SUPPRESSMSGBOXES','/NORESTART','/SP-','/NOICONS','/ODIN_QA=1',
                 f'/DIR={app}',f'/ODIN_HOST_CONFIG={host}',f'/ODIN_SETUP_RECEIPT={receipt}',f'/LOG={base/(mode+".log")}']
        if mode=='deadline':command.append('/ODIN_SETUP_TIMEOUT_SECONDS=1')
        environment=dict(os.environ,NODE_OPTIONS='--no-warnings',ODIN_PROGRESS_TEST_SENTINEL='inert-test-value')
        proc=subprocess.Popen(' '.join('"'+item+'"' for item in command),env=environment,
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
        evidence={'mode':mode,'launcher_pid':proc.pid,'argv':command,'observed_text':[],
                  'responsive_samples':0,'stop_clicked':False,'applying_stop_disabled':False,
                  'failure_heading_seen':False,'raw_observation':None,'parent_exit_injected':False,'host_field_changed':False}
        start=time.monotonic();last_click=0;seen=set();record=None;failure=None;injected=False
        try:
            while proc.poll() is None and time.monotonic()-start<35:
                if record is None and observation.exists():
                    record=json.loads(observation.read_text(encoding='utf-8'))
                    evidence['raw_observation']=record
                    if record['node_options_inherited'] or record['sentinel_inherited']:
                        raise AssertionError('Setup worker inherited a blocked host variable')
                owned=ui.children(proc.pid)
                if record is not None:
                    owned.add(record['installer'])
                for handle in ui.windows(owned):
                    controls=ui.controls(handle)
                    texts=[control['text'] for control in controls if control['text']]
                    for text in texts:
                        if text not in seen:
                            evidence['observed_text'].append(text);seen.add(text)
                    if record is not None:
                        if not ui.responsive(handle):
                            raise AssertionError('Progress window did not pump messages within 500 ms')
                        evidence['responsive_samples']+=1
                    if mode=='switch_existing' and not evidence['host_field_changed']:
                        edit=next((c for c in controls if c['text']==str(host) and 'edit' in c['class'].lower()),None)
                        if edit:
                            value=ctypes.create_unicode_buffer(str(expected_host))
                            ui.u.SendMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM]
                            ui.u.SendMessageW(edit['handle'],0x000C,0,ctypes.addressof(value))
                            evidence['host_field_changed']=True
                    stop=next((c for c in controls if c['text']=='Stop LSP checks'),None)
                    applying=any('4/4  Saving host' in text for text in texts)
                    if applying and stop and not stop['enabled']:
                        evidence['applying_stop_disabled']=True
                    if mode=='cancel' and stop and stop['enabled'] and any('2/4' in text for text in texts):
                        ui.click(stop);evidence['stop_clicked']=True
                    if mode=='parent_exit' and record is not None and not injected and any('2/4' in text for text in texts):
                        if record['installer'] not in ui.children(proc.pid):
                            raise AssertionError('Refusing to kill a process outside the exact fixture lineage')
                        ui.terminate_fixture(record['installer']);injected=True
                        evidence['parent_exit_injected']=True
                        break
                    heading=any('Odin language server setup needs attention' in text for text in texts)
                    if heading:
                        evidence['failure_heading_seen']=True
                    if time.monotonic()-last_click>.2:
                        acceptable=next((c for c in controls if c['text'] in ['I &accept the agreement','I accept the agreement'] and c['enabled']),None)
                        advance=next((c for c in controls if c['text'].replace('&','').removesuffix(' >') in ['Next','Install','Finish'] and c['enabled']),None)
                        if acceptable and not record:
                            ui.click(acceptable)
                        if advance:
                            ui.click(advance);last_click=time.monotonic()
                time.sleep(.05)
            if proc.poll() is None:
                raise TimeoutError('Isolated interactive probe exceeded 35 seconds')
            stdout,stderr=proc.communicate(timeout=5)
            evidence.update(exit_code=proc.returncode,stdout=stdout.decode('utf-8',errors='replace'),
                            stderr=stderr.decode('utf-8',errors='replace'),elapsed_seconds=round(time.monotonic()-start,3))
            if not record:
                raise AssertionError('Fixed Node worker did not start')
            if (mode=='fresh_home' and host.exists()) or (mode!='fresh_home' and expected_host.read_bytes()!=sentinel):
                raise AssertionError('Inert fixture changed host bytes')
            deadline=time.monotonic()+3
            while time.monotonic()<deadline and (ui.alive(record['worker']) or ui.alive(record['child'])):
                time.sleep(.05)
            evidence['worker_and_descendant_reaped']=not ui.alive(record['worker']) and not ui.alive(record['child'])
            if not evidence['worker_and_descendant_reaped']:
                raise AssertionError('Owned worker or child survived job cleanup')
            if mode=='parent_exit':
                if not injected:raise AssertionError('Parent exit was not injected')
            else:
                expected={'success':0,'fresh_home':0,'switch_existing':0,'cancel':1223,'failure':17,'timeout':1460,'deadline':1460,'wrong_nonce':1}[mode]
                if proc.returncode!=expected:raise AssertionError(f'Expected setup exit {expected}, got {proc.returncode}')
                if mode not in ['success','fresh_home','switch_existing'] and not evidence['failure_heading_seen']:
                    raise AssertionError('Visible failure heading was not shown')
                if mode in ['success','fresh_home','switch_existing']:
                    if not all(any(phase in text for text in seen) for phase in ['1/4','2/4','3/4','4/4']):
                        raise AssertionError('Not all four setup phases appeared')
                    if not evidence['applying_stop_disabled']:raise AssertionError('Stop was not disabled during commit')
                if mode=='cancel':
                    result=json.loads(receipt.read_text(encoding='utf-8'))
                    nonce=record['args'][record['args'].index('--progress-run-id')+1]
                    if not evidence['stop_clicked'] or result['cancel_nonce']!=nonce:
                        raise AssertionError('Stop did not create the matching cooperative marker')
                if mode=='fresh_home':
                    argv=record['args']
                    if argv[argv.index('--workspace-root')+1]!=os.environ['USERPROFILE']:
                        raise AssertionError('A fresh host did not preselect the user home')
                if mode=='switch_existing':
                    argv=record['args']
                    if not evidence['host_field_changed'] or argv[argv.index('--host-config')+1]!=str(expected_host) or '--workspace-root' in argv or host.exists():
                        raise AssertionError('Selecting an existing host did not clear only the automatic home root')
                if mode=='deadline':
                    result=json.loads(receipt.read_text(encoding='utf-8'))
                    nonce=record['args'][record['args'].index('--progress-run-id')+1]
                    if result['cancel_nonce'] is not None or not any('Time limit reached; waiting for setup worker' in text for text in seen):
                        raise AssertionError('UI timer must wait for the real worker deadline without writing a cancellation marker')
                if mode=='wrong_nonce' and any('1/4' in text or '4/4' in text for text in seen):
                    raise AssertionError('Foreign progress nonce was accepted')
            if not evidence['responsive_samples']:raise AssertionError('No live UI responsiveness sample was obtained')
        except Exception as exc:
            failure=str(exc);errors.append(mode+': '+failure)
            if Path(str(receipt)+'.error').exists():evidence['worker_error']=json.loads(Path(str(receipt)+'.error').read_text(encoding='utf-8'))
        finally:
            if proc.poll() is None:
                targets=ui.children(proc.pid)
                for target in targets - {proc.pid}:
                    if ui.alive(target):ui.terminate_fixture(target)
                proc.kill();proc.communicate(timeout=5)
            if (app/'unins000.exe').exists():
                cleanup=run([app/'unins000.exe','/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',f'/LOG={base/(mode+"-uninstall.log")}'],inno=True)
                evidence['uninstall_exit']=cleanup.returncode
                if cleanup.returncode or (app/'setup-lsps.mjs').exists():
                    errors.append(mode+': fixture uninstall failed')
            evidence['host_preserved']=not host.exists() if mode=='fresh_home' else expected_host.read_bytes()==sentinel
            evidence['status']='FAIL' if failure else 'PASS'
            evidence['error']=failure
            cases.append(evidence)
            (base/(mode+'-ui.json')).write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
    shortcut_after=shortcut.read_bytes() if shortcut.is_file() else None
    if shortcut_after!=shortcut_before:errors.append('Operator shortcut changed')
    report={'scope':'ACTUAL_INNO_UI_PROCESS_JOB_FIXTURE_NOT_LSP_ACCEPTANCE','status':'FAIL' if errors else 'PASS',
            'errors':errors,'observed_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
            **source_hashes,'node_sha256':digest(node),'iscc_sha256':digest(iscc),
            'progress_module_source':str(progress_module),'progress_module_sha256':digest(stage/'setup-progress.mjs'),
            'commands':commands,'cases':cases,'qa_directory':str(base),'qa_shortcut_preserved':shortcut_after==shortcut_before,
            'real_lsp_health':'NOT_RUN','full_payload_installation':'NOT_RUN'}
    (base/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':report['status'],'report':str(base/'report.json'),'errors':errors}))
    if errors:raise SystemExit(1)


if __name__=='__main__':
    main()
