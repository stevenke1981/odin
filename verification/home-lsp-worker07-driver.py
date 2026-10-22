from pathlib import Path
import json,os,subprocess,time,tempfile,uuid,hashlib,shutil
T=Path(__file__).resolve().parent; stage=json.loads((T/'expanded-stage07.json').read_text(encoding='utf-8'));home=Path(stage['synthetic_home']);app=Path(stage['install_root']);host=home/'.config/odin/host.toml'
for name in ['setup-lsps.mjs','setup-progress.mjs']:shutil.copyfile(Path('E:/odin/packaging')/name,app/name)
env={**os.environ,'USERPROFILE':str(home),'HOME':str(home)}
import ctypes
from ctypes import wintypes
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
kernel.CreateFileW.argtypes=[wintypes.LPCWSTR,wintypes.DWORD,wintypes.DWORD,ctypes.c_void_p,wintypes.DWORD,wintypes.DWORD,wintypes.HANDLE];kernel.CreateFileW.restype=wintypes.HANDLE
kernel.ReadFile.argtypes=[wintypes.HANDLE,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(wintypes.DWORD),ctypes.c_void_p];kernel.ReadFile.restype=wintypes.BOOL
kernel.CloseHandle.argtypes=[wintypes.HANDLE];kernel.CloseHandle.restype=wintypes.BOOL
def read_shared(filename):
 h=kernel.CreateFileW(str(filename),0x80000000,7,None,3,0x80,None)
 if h==ctypes.c_void_p(-1).value:return ''
 try:
  buffer=ctypes.create_string_buffer(4096);read=wintypes.DWORD()
  return buffer.raw[:read.value].decode('ascii') if kernel.ReadFile(h,buffer,4096,ctypes.byref(read),None) else ''
 finally:kernel.CloseHandle(h)
reports=[]
for mode in ['success','cancel','timeout']:
 temp=Path(tempfile.mkdtemp(prefix='odin-hp-real-'));token=uuid.uuid4().hex;progress=temp/'progress.txt';cancel=temp/'cancel.txt';receipt=T/('expanded07-'+mode+'-worker-receipt.json')
 before=hashlib.sha256(host.read_bytes()).hexdigest() if host.exists() else None
 argv=[str(app/'backends/node/node.exe'),str(app/'setup-lsps.mjs'),'--install','--host-config',str(host),'--workspace-root',str(home),'--progress-file',str(progress),'--cancel-file',str(cancel),'--progress-run-id',token,'--receipt',str(receipt)]
 if mode=='timeout':argv+=['--setup-timeout-seconds','1']
 start=time.monotonic();events=[];sent=False;lastphase=None
 with (T/('expanded07-'+mode+'-worker.stdout.log')).open('wb') as out,(T/('expanded07-'+mode+'-worker.stderr.log')).open('wb') as err:
  proc=subprocess.Popen(argv,env=env,cwd=app,stdout=out,stderr=err)
  while proc.poll() is None:
   if progress.exists():
    rows=read_shared(progress).splitlines()
    if len(rows)==8 and rows[0]=='ODIN_SETUP_PROGRESS_V1' and rows[1]==token:
     event={'seconds':round(time.monotonic()-start,3),'phase':rows[2],'completed':rows[3],'total':rows[4],'component':rows[5],'exit_code':rows[6]}
     if not events or event['phase']!=events[-1]['phase'] or event['completed']!=events[-1]['completed']:events.append(event)
     if event['phase']!=lastphase:print(json.dumps({'mode':mode,**event}),flush=True);lastphase=event['phase']
     if mode=='cancel' and not sent and rows[2]=='verify_before':cancel.write_text(token,encoding='ascii');sent=True
   if time.monotonic()-start>1000:raise RuntimeError('Driver deadline exceeded')
   time.sleep(.05)
 after=hashlib.sha256(host.read_bytes()).hexdigest() if host.exists() else None
 final=read_shared(progress).splitlines();expected={'success':0,'cancel':1223,'timeout':1460}[mode]
 assert proc.returncode==expected,(mode,proc.returncode,(T/('expanded07-'+mode+'-worker.stderr.log')).read_text(encoding='utf-8'))
 assert final[2]=={'success':'done','cancel':'cancelled','timeout':'timeout'}[mode]
 if mode!='success':assert before==after,(mode,'host was modified')
 else:
  report=json.loads(receipt.read_text(encoding='utf-8'));assert report['result']['status']==report['health']['status']==report['health']['post_health_integrity']['status']=='PASS';assert len(report['health']['checks'])==19
 report={'mode':mode,'argv':argv,'cwd':str(app),'exit_code':proc.returncode,'seconds':round(time.monotonic()-start,3),'events':events,'final_status':final,'host_before_sha256':before,'host_after_sha256':after,'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'worker_sha256':hashlib.sha256((app/'setup-lsps.mjs').read_bytes()).hexdigest(),'progress_module_sha256':hashlib.sha256((app/'setup-progress.mjs').read_bytes()).hexdigest()}
 reports.append(report);(T/'worker-expanded07-report.json').write_text(json.dumps({'status':'PASS' if len(reports)==3 else 'NOT_RUN','runs':reports},indent=2),encoding='utf-8');print(json.dumps({k:report[k] for k in ['mode','exit_code','seconds']}),flush=True)
