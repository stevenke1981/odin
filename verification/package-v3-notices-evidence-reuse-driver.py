"""Copy sealed existing application evidence; do not execute Cargo, Odin or backends."""
import datetime,hashlib,json,os,pathlib,shutil

assert os.geteuid()==1000
base=pathlib.Path('/opt/odin-delivery/v3-0fe03ba')
target=pathlib.Path('/opt/odin-delivery/v3-notices-02')
assert target.is_dir() and not any(target.iterdir())
transfer=pathlib.Path('/opt/odin-export/manifest-0fe03ba.json')
assert hashlib.sha256(transfer.read_bytes()).hexdigest()=='215c13a90175e86beba3db9681ec83cb32360436292bdcc9ee2bf741c67a95f1'
rows={row['path']:row for row in json.loads(transfer.read_text())['files']}
canonical_path=pathlib.Path('/opt/odin-transfer/original-functional-canonical-c1a9.json')
assert hashlib.sha256(canonical_path.read_bytes()).hexdigest()=='c1a9f4a750660873c55be1294a431c048fa3f0094c6f1a314d46f448cb62af93'
canonical=json.loads(canonical_path.read_text())
assert canonical['status']=='PASS' and canonical['app_build_revision']=='0fe03ba30da9600c25588d506d7e094df89cf56f'
runner=json.loads((base/'runner-summary.json').read_text())
assert runner['status']=='PASS' and 'driver_error' not in runner
assert runner['driver']=={'path':'run-linux-candidate-v5.py','sha256':'7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984'}
for command in runner['commands']:
    original=next(row for row in canonical['commands'] if row['name']==command['name'])
    assert command['exit_code']==0 and command['status']=='PASS'
    for key,value in command.items():
        if key in ('log','log_sha256'):continue
        assert original[key]==value,(command['name'],key)
    assert original['original_guest_log_sha256']==command['log_sha256']
names={'runner-summary.json','build-environment.json','binary-sha256.json','odin','source-preflight.json',
       'source-after-generators.json','source-after-qa.json','job.log','job.exit'}
for command in runner['commands']:
    names.add(command['name']+'.command.json');names.add(command['log'])
for name in ('static-source','fixture','cli','mcp','http','cancellation'):names.add(name+'-report.json')
copied=[]
for name in sorted(names):
    source=base/name;data=source.read_bytes();row=rows[name]
    assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256'],name
    shutil.copyfile(source,target/name)
    (target/name).chmod(0o555 if name=='odin' else 0o444)
    copied.append(row)
assert hashlib.sha256((target/'odin').read_bytes()).hexdigest()=='e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42'
timestamps_path=pathlib.Path('/opt/odin-transfer/original-functional-transfer-receipt.json')
assert hashlib.sha256(timestamps_path.read_bytes()).hexdigest()=='ce0e8d15fcef61952bacab1a1a4ed6f8d06993d911101ce256a01d5dead68be6'
timestamps=json.loads(timestamps_path.read_text())
report={'scope':'REUSE_EXACT_SEALED_ELF_APPLICATION_GATE_RECEIPTS_WITHOUT_REEXECUTION','status':'PASS',
        'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'uid':os.geteuid(),
        'app_build_revision':'0fe03ba30da9600c25588d506d7e094df89cf56f',
        'binary_sha256':'e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42',
        'original_canonical_report':{'path':'verification/package-v3-linux-final.json','sha256':'c1a9f4a750660873c55be1294a431c048fa3f0094c6f1a314d46f448cb62af93'},
        'original_driver':runner['driver'],'original_transfer_started_utc':timestamps['started_utc'],
        'original_transfer_finished_utc':timestamps['finished_utc'],
        'original_command_absolute_timestamps':'NOT_RECORDED; actual original argv/exit/elapsed receipts are retained. No start time was reconstructed.',
        'application_gates_reexecuted':False,'cargo_commands_executed_by_this_step':0,'app_or_backend_processes_executed_by_this_step':0,
        'copied_files_verified_against_original_transfer_manifest':copied,
        'new_work_scope':'Only runtime-notices documentation packaging, checksums and a fresh installer lifecycle for this same sealed ELF.'}
(target/'reuse-application-gates.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'status':report['status'],'files':len(copied),'binary_sha256':report['binary_sha256'],'application_gates_reexecuted':False}))
