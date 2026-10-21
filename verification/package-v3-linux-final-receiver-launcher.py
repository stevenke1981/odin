import argparse,hashlib,json,pathlib,subprocess,sys,time
p=argparse.ArgumentParser();p.add_argument('--revision',required=True);p.add_argument('--manifest',type=pathlib.Path,required=True);p.add_argument('--manifest-sha256',required=True);a=p.parse_args()
tools=pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
assert hashlib.sha256(a.manifest.read_bytes()).hexdigest()==a.manifest_sha256
manifest=json.loads(a.manifest.read_text(encoding='utf-8'))
assert manifest['source_revision']==a.revision
assert manifest['packaging_revision']=='a8399993d7c528ad4858b2e4e246e5994eab4bb8'
prefix=a.revision[:7]
output=pathlib.Path(r'E:\odin\dist')/('package-v3-linux-'+prefix)
command=[sys.executable,r'E:\odin\.odin\receive_linux_artifacts_v3.py','--manifest',str(a.manifest),'--manifest-sha256',a.manifest_sha256,'--output',str(output),'--timeout','600']
log=tools/'vm'/('receiver-'+prefix+'.log');receipt_path=tools/'vm'/('receiver-'+prefix+'.json')
assert not receipt_path.exists()
with log.open('xb') as handle:
    process=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=handle,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
for _ in range(30):
    time.sleep(.1); text=log.read_text(encoding='utf-8')
    if 'LISTENING' in text or process.poll() is not None: break
assert process.poll() is None and 'LISTENING' in text,text
record={'status':'RUNNING','pid':process.pid,'command':command,'manifest_sha256':a.manifest_sha256,'script_sha256':hashlib.sha256(pathlib.Path(command[1]).read_bytes()).hexdigest(),'log':str(log),'bind':'127.0.0.1:47654','files':len(manifest['files'])}
receipt_path.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(record))
