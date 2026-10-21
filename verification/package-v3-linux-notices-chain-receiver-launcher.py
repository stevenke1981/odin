import argparse,hashlib,json,pathlib,subprocess,sys,time
p=argparse.ArgumentParser();p.add_argument('--manifest',type=pathlib.Path,required=True);p.add_argument('--manifest-sha256',required=True);p.add_argument('--packaging-revision',required=True);a=p.parse_args()
tools=pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
assert hashlib.sha256(a.manifest.read_bytes()).hexdigest()==a.manifest_sha256
manifest=json.loads(a.manifest.read_text(encoding='utf-8'))
assert manifest['source_revision']=='0fe03ba30da9600c25588d506d7e094df89cf56f' and manifest['packaging_revision']==a.packaging_revision
receiver=pathlib.Path(r'E:\odin\.odin\receive_linux_artifacts_v3.py')
assert hashlib.sha256(receiver.read_bytes()).hexdigest()=='4642cc28bce373d9cb97aada5e7516407849613e0bfb9dfead1385160e44d70e'
output=pathlib.Path(r'E:\odin\dist\package-v3-linux-notices-02')
assert not output.exists()
argv=[sys.executable,str(receiver),'--manifest',str(a.manifest),'--manifest-sha256',a.manifest_sha256,'--output',str(output),'--timeout','600']
log=tools/'vm/notices-receiver-02.log';receipt=tools/'vm/notices-receiver-02.json'
assert not receipt.exists()
with log.open('xb') as handle:process=subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=handle,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
for _ in range(30):
    time.sleep(.1);data=log.read_text(encoding='utf-8')
    if 'LISTENING' in data or process.poll() is not None:break
assert process.poll() is None and 'LISTENING' in data,data
record={'scope':'TASK_ONLY_NOTICE_ARTIFACT_RECEIVER','status':'RUNNING','pid':process.pid,'argv':argv,'manifest_sha256':a.manifest_sha256,'receiver_sha256':hashlib.sha256(receiver.read_bytes()).hexdigest(),'bind':'127.0.0.1:47654','files':len(manifest['files'])}
receipt.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(record))
