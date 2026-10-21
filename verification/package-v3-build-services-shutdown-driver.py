import hashlib,json,pathlib,shutil,socket,subprocess,time
tools=pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
powershell_path=shutil.which('pwsh')
assert powershell_path and pathlib.Path(powershell_path).is_file()
source_stop=[powershell_path,'-NoProfile','-File',r'E:\odin\.odin\stop_packaging_source_http_v3.ps1']
assert pathlib.Path(source_stop[0]).is_file()
log=tools/'vm/final-source-http-stop.txt'
with log.open('xb') as handle:
    result=subprocess.run(source_stop,stdin=subprocess.DEVNULL,stdout=handle,stderr=subprocess.STDOUT,timeout=30)
assert result.returncode==0,log.read_text(encoding='utf-8')
time.sleep(2)
ports=[]
for port in (47651,47652,47654):
    with socket.socket() as connection:
        connection.settimeout(2)
        closed=connection.connect_ex(('127.0.0.1',port))!=0
        assert closed,port
        ports.append({'bind':'127.0.0.1','port':port,'closed':closed})
query=[powershell_path,'-NoProfile','-Command',"@(Get-CimInstance Win32_Process -Filter 'ProcessId=27184 OR ProcessId=29504 OR ProcessId=43700').Count"]
process_check=subprocess.run(query,check=True,capture_output=True,text=True,encoding='utf-8',timeout=30)
assert process_check.stdout.strip()=='0',process_check.stdout
poweroff=json.loads((tools/'vm/native-final-normal-poweroff.json').read_text(encoding='utf-8'))
assert poweroff['exit_code']==0 and not poweroff['timed_out']
output=pathlib.Path(r'E:\odin\dist\package-v3-linux-0fe03ba')
manifest=json.loads((output/'transfer-manifest.json').read_text(encoding='utf-8'))
for row in manifest['files']:
    path=output/row['path']
    assert path.stat().st_size==row['bytes'] and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
overlay=tools/'vm/odin-validation.qcow2'
assert overlay.is_file()
report={'scope':'NORMAL_TASK_BUILD_ENVIRONMENT_SHUTDOWN_ONLY','status':'PASS',
        'guest_poweroff':{'argv':['systemctl','poweroff','--no-block'],'guest_command_exit_code':0,'qemu_pid':27184,'process_absent_after_poweroff':True},
        'source_http_stop':{'argv':source_stop,'exit_code':result.returncode,'log':log.name,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'script_sha256':hashlib.sha256(pathlib.Path(source_stop[-1]).read_bytes()).hexdigest()},
        'process_absence_check':{'argv':query,'exit_code':process_check.returncode,'stdout':process_check.stdout},
        'completed_receiver_pid':43700,'ports':ports,'overlay_preserved':str(overlay),'host_artifacts_verified_after_shutdown':len(manifest['files']),
        'application_signal_acceptance':'NOT_RUN; shutting down the build VM does not establish SIGTERM/SIGINT LSP/DAP descendant or journal acceptance.'}
(tools/'vm/final-build-services-stop.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(report))
