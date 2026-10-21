"""Bind a docs-only notice repack to previously executed, identical-ELF app gates."""
import argparse,copy,hashlib,json,pathlib,re,socket,subprocess,tarfile

ROOT=pathlib.Path(r'C:\Users\steven\.codex\worktrees\odin-packaging\odin')
ARTIFACTS=pathlib.Path(r'E:\odin\dist\package-v3-linux-notices-02')
APP='0fe03ba30da9600c25588d506d7e094df89cf56f'
BINARY='e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42'
ORIGINAL='c1a9f4a750660873c55be1294a431c048fa3f0094c6f1a314d46f448cb62af93'


def sha(data):return hashlib.sha256(data).hexdigest()


def require_successful_runner(runner):
    if runner.get('status')!='PASS' or 'driver_error' in runner:
        raise ValueError('Original native runner must have completed successfully without driver_error')
    if runner.get('revision')!=APP:
        raise ValueError('Original native runner source revision differs')
    if runner.get('driver')!={'path':'run-linux-candidate-v5.py','sha256':'7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984'}:
        raise ValueError('Original native runner driver path/hash differs')
    for command in runner.get('commands',[]):
        if command.get('exit_code')!=0 or command.get('status')!='PASS':
            raise ValueError('Original native runner contains an unsuccessful command')


def save(name,data,normalize=False):
    output=ROOT/'verification'/('package-v3-linux-notices-02-'+name)
    assert not output.exists(),output
    content=('\n'.join(line.rstrip() for line in data.decode('utf-8').replace('\r','').splitlines()).rstrip()+'\n').encode('utf-8') if normalize and data else data
    output.write_bytes(content)
    result={'path':output.relative_to(ROOT).as_posix(),'sha256':sha(content)}
    if content!=data:result['original_transferred_sha256']=sha(data)
    return result


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--packaging-revision',required=True)
    parser.add_argument('--runtime-notices-report',required=True)
    args=parser.parse_args()
    assert re.fullmatch('[0-9a-f]{40}',args.packaging_revision)
    old_path=ROOT/'verification/package-v3-linux-final.json'
    assert sha(old_path.read_bytes())==ORIGINAL
    old=json.loads(old_path.read_text())
    previous_rows={row['path']:row for row in json.loads((ROOT/'verification/package-v3-linux-final-transfer-manifest.json').read_text())['files']}
    manifest_data=(ARTIFACTS/'transfer-manifest.json').read_bytes()
    manifest=json.loads(manifest_data)
    assert manifest['source_revision']==APP and manifest['packaging_revision']==args.packaging_revision
    receipt=json.loads((ARTIFACTS/'transfer-receipt.json').read_text())
    assert receipt['status']=='PASS' and receipt['manifest_sha256']==sha(manifest_data)
    assert len(receipt['received'])==len(manifest['files'])
    for row in manifest['files']:
        path=ARTIFACTS/row['path']
        assert path.is_file() and not path.is_symlink()
        assert len(path.read_bytes())==row['bytes'] and sha(path.read_bytes())==row['sha256'],row['path']
    with socket.socket() as probe:
        probe.settimeout(2)
        assert probe.connect_ex(('127.0.0.1',47654))!=0
    raw=json.loads((ARTIFACTS/'linux-validation.json').read_text())
    runner=json.loads((ARTIFACTS/'runner-summary.json').read_text())
    require_successful_runner(runner)
    assert sha((ARTIFACTS/runner['driver']['path']).read_bytes())==runner['driver']['sha256']
    actual_commands=runner['commands']+[json.loads((ARTIFACTS/(name+'.command.json')).read_text()) for name in ('final-package','final-checksums')]+[json.loads((ARTIFACTS/'installer-command.json').read_text())]
    assert raw['commands']==actual_commands
    assert all(row['exit_code']==0 and row['status']=='PASS' for row in actual_commands)
    assert raw['status']=='PASS' and raw['app_build_revision']==APP and raw['binary_sha256']==BINARY
    assert raw['packaging_revision']==raw['bundled_docs_revision']==args.packaging_revision
    assert raw['build_metadata']==old['build_metadata'] and raw['workspace_tests']==old['workspace_tests']
    assert raw['smokes']==old['smokes'] and raw['signal_shutdown_acceptance']['status']=='NOT_RUN'
    reuse=json.loads((ARTIFACTS/'reuse-application-gates.json').read_text())
    assert reuse['status']=='PASS' and reuse['app_build_revision']==APP and reuse['binary_sha256']==BINARY
    assert reuse['application_gates_reexecuted'] is False
    assert reuse['original_canonical_report']['sha256']==ORIGINAL
    assert reuse['original_driver']==runner['driver']
    for row in reuse['copied_files_verified_against_original_transfer_manifest']:
        assert row==previous_rows[row['path']]
        assert sha((ARTIFACTS/row['path']).read_bytes())==row['sha256']
    product=lambda name:name.startswith(('crates/','schemas/','dashboard/','scripts/','packaging/','tests/')) or name in ('Cargo.toml','Cargo.lock','rust-toolchain.toml')
    app_manifest=json.loads((ARTIFACTS/'source-sha256.json').read_text())
    docs_manifest=json.loads((ARTIFACTS/'packaging-source-sha256.json').read_text())
    assert app_manifest['revision']==APP and docs_manifest['revision']==args.packaging_revision
    assert raw['source_manifest']['checked_files']==len(app_manifest['files'])==688
    assert raw['packaging_source_manifest']['checked_files']==len(docs_manifest['files'])
    assert {row['path']:row['sha256'] for row in app_manifest['files'] if product(row['path'])}=={row['path']:row['sha256'] for row in docs_manifest['files'] if product(row['path'])}
    notice_name='docs/THIRD_PARTY_RUNTIME_NOTICES.txt'
    notice_source=next(row for row in docs_manifest['files'] if row['path']==notice_name)
    notice=subprocess.check_output(['git','show',args.packaging_revision+':'+notice_name],cwd=ROOT)
    assert sha(notice)==notice_source['sha256']
    inventory=json.loads((ARTIFACTS/'payload-inventory.json').read_text())
    notice_inventory=next(row for row in inventory if row['path']==notice_name)
    assert notice_inventory['sha256']==sha(notice)
    portable=next(ARTIFACTS/row['path'] for row in raw['artifacts'] if row['path'].endswith('.tar.gz'))
    with tarfile.open(portable,'r:gz') as archive:
        member=next(row for row in archive.getmembers() if row.name.endswith('/'+notice_name))
        assert archive.extractfile(member).read()==notice
    provenance_path=(ROOT/args.runtime_notices_report).resolve(strict=True)
    provenance_path.relative_to(ROOT.resolve())
    assert provenance_path.is_file()
    refs={}
    for row in manifest['files']:
        name=row['path']
        if name=='odin' or name.endswith(('.deb','.tar.gz')):continue
        if name in previous_rows and row==previous_rows[name] and name in old['evidence']:
            reference=old['evidence'][name]
            assert sha((ROOT/reference['path']).read_bytes())==reference['sha256']
            refs[name]=reference
        else:
            public_name=name[:-4]+'.txt' if name.endswith('.log') else name
            refs[name]=save(public_name,(ARTIFACTS/name).read_bytes(),name.endswith('.log'))
    report=copy.deepcopy(raw)
    original_names={row['name'] for row in runner['commands']}
    for command in report['commands']:
        name=command['log'];original_sha=command['log_sha256']
        assert sha((ARTIFACTS/name).read_bytes())==original_sha
        command.update(log=refs[name]['path'],log_sha256=refs[name]['sha256'],original_guest_log_sha256=original_sha,
                       execution_origin='REUSED_ORIGINAL_APPLICATION_EXECUTION' if command['name'] in original_names else 'NEW_DOCS_PACKAGE_OR_INSTALLER_EXECUTION')
    for key in ('source_manifest','packaging_source_manifest','payload_inventory'):
        name=report[key]['path']
        assert report[key]['sha256']==sha((ARTIFACTS/name).read_bytes())
        report[key].update(path=refs[name]['path'],sha256=refs[name]['sha256'])
    name=report['installer_lifecycle']['report']['path']
    assert report['installer_lifecycle']['report']['sha256']==sha((ARTIFACTS/name).read_bytes())
    report['installer_lifecycle']['report']=refs[name]
    for artifact in report['artifacts']:artifact['path']='dist/package-v3-linux-notices-02/'+artifact['path']
    proof=ROOT/'verification/package-v3-notices-evidence-outcomes-01.json'
    report.update(
        evidence=refs,
        original_execution_report={'path':'verification/package-v3-linux-final.json','sha256':ORIGINAL},
        application_gates_reexecuted=False,
        application_gate_evidence_origin='EXACT_SAME_SEALED_ELF_0FE_RECEIPTS_REUSED_WITHOUT_REEXECUTION',
        original_driver=runner['driver'],
        original_transfer_started_utc=reuse['original_transfer_started_utc'],
        original_transfer_finished_utc=reuse['original_transfer_finished_utc'],
        original_command_absolute_timestamps=reuse['original_command_absolute_timestamps'],
        reuse_receipt=refs['reuse-application-gates.json'],
        new_execution_scope='Only docs/runtime-notices packaging, archive checksum verification and a fresh disposable-guest install/reinstall/remove lifecycle were newly executed. Cargo, the original 211 passing workspace tests, Clippy and the 132 application smoke checks were not rerun.',
        runtime_notices={'status':'INCLUDED_AND_SOURCE_HASH_CHECKED','payload_path':notice_name,'sha256':sha(notice),
                         'source_revision':args.packaging_revision,'provenance_report':{'path':provenance_path.relative_to(ROOT).as_posix(),'sha256':sha(provenance_path.read_bytes())}},
        qa_evidence_repair={'finding':'R4-E01','status':'CLOSED','report':{'path':proof.relative_to(ROOT).as_posix(),'sha256':sha(proof.read_bytes())}},
        transport={'status':'PASS','mode':'TASK_ONLY_FIXED_MANIFEST_LOOPBACK_HTTP_POST','files_verified':len(manifest['files']),
                   'bytes_verified':sum(row['bytes'] for row in manifest['files']),'receiver_closed_after_completion':True,
                   'manifest':save('transfer-manifest.json',manifest_data),'receipt':save('transfer-receipt.json',(ARTIFACTS/'transfer-receipt.json').read_bytes())},
        signed=False,normalization='New public text logs remove CR/trailing whitespace; original guest digests are retained. Original application gate logs/receipts point to the immutable functional execution report and were not relabeled as new execution.')
    canonical_path=ROOT/'verification/package-v3-linux-notices-final.json'
    assert not canonical_path.exists()
    canonical_bytes=(json.dumps(report,indent=2)+'\n').encode('utf-8')
    canonical_path.write_bytes(canonical_bytes)
    reference={'path':canonical_path.relative_to(ROOT).as_posix(),'sha256':sha(canonical_bytes)}
    print(json.dumps({'status':report['status'],'report':reference,'packaging_revision':args.packaging_revision,
                      'binary_sha256':BINARY,'payload_inventory_entries':len(inventory),'runtime_notice_sha256':sha(notice),
                      'artifacts':report['artifacts'],'reused_app_gates':True,'new_installer_checks':len(report['installer_lifecycle']['checks'])}))


if __name__=='__main__':main()
