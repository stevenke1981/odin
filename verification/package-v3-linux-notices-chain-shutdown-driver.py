import hashlib,json,pathlib,shutil,socket,subprocess,time
tools=pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
pwsh=shutil.which('pwsh');assert pwsh
argv=[pwsh,'-NoProfile','-File',r'E:\odin\.odin\stop_notices_source_http_v3.ps1']
log=tools/'vm/notices-source-http-stop-02.txt'
with log.open('xb') as handle:result=subprocess.run(argv,stdin=subprocess.DEVNULL,stdout=handle,stderr=subprocess.STDOUT,timeout=30)
assert result.returncode==0,log.read_text(encoding='utf-8')
time.sleep(2)
ports=[]
for port in (47651,47652,47654):
    with socket.socket() as probe:
        probe.settimeout(2);closed=probe.connect_ex(('127.0.0.1',port))!=0
        assert closed,port
        ports.append({'bind':'127.0.0.1','port':port,'closed':True})
query=[pwsh,'-NoProfile','-Command',"@(Get-CimInstance Win32_Process -Filter 'ProcessId=26612 OR ProcessId=40740 OR ProcessId=26672').Count"]
checked=subprocess.run(query,capture_output=True,text=True,encoding='utf-8',timeout=30,check=True)
assert checked.stdout.strip()=='0',checked.stdout
assert json.loads((tools/'vm/notices-normal-poweroff.json').read_text())['exit_code']==0
output=pathlib.Path(r'E:\odin\dist\package-v3-linux-notices-02')
manifest=json.loads((output/'transfer-manifest.json').read_text())
assert manifest['packaging_revision']=='f5878bfa725db8c0dcab7e45ca3cf89159508975'
for row in manifest['files']:
    p=output/row['path'];assert p.stat().st_size==row['bytes'] and hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256']
assert (tools/'vm/odin-validation.qcow2').is_file()
report={'scope':'NORMAL_DOCS_NOTICE_REPACK_BUILD_ENVIRONMENT_SHUTDOWN_ONLY','status':'PASS',
        'guest_poweroff':{'argv':['systemctl','poweroff','--no-block'],'exit_code':0,'qemu_pid':26612,'process_absent':True},
        'source_http_stop':{'argv':argv,'exit_code':result.returncode,'log':log.name,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()},
        'process_absence_check':{'argv':query,'exit_code':checked.returncode,'stdout':checked.stdout},
        'receiver_pid':26672,'receiver_completed_then_exited':True,'ports':ports,'host_artifacts_verified_after_shutdown':len(manifest['files']),
        'preserved_source_stop_first_attempt':{'exit_code':1,'log':'notices-source-http-stop.txt','reason':'Exact ownership guard expected quotes around the no-space Python path, but this task-owned subprocess command line omitted those quotes. The guard stopped before terminating any process; followup uses the independently observed exact command string.'},
        'overlay_and_old_artifacts_preserved':True,'app_recompiled':False,'app_gates_reexecuted':False,
        'application_signal_acceptance':'NOT_RUN; normal VM/helper shutdown is not SIGTERM/SIGINT LSP/DAP descendant or journal acceptance.'}
(tools/'vm/notices-services-stop.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(report))
