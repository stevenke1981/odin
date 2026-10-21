import argparse,hashlib,json,pathlib,re,shutil,struct,subprocess,tarfile

def candidate_passed(runner, commands, smokes, installer):
    return (runner.get('status') == 'PASS' and 'driver_error' not in runner
            and all(row['exit_code'] == 0 for row in commands)
            and all(row['status'] == 'PASS' for row in smokes.values())
            and installer['status'] == 'PASS')


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--revision',required=True)
    p.add_argument('--tree',type=pathlib.Path,required=True)
    p.add_argument('--manifest',type=pathlib.Path,required=True)
    p.add_argument('--delivery',type=pathlib.Path,required=True)
    p.add_argument('--expected-binary-sha256',required=True)
    p.add_argument('--packaging-revision',required=True)
    p.add_argument('--packaging-tree',type=pathlib.Path,required=True)
    p.add_argument('--packaging-manifest',type=pathlib.Path,required=True)
    p.add_argument('--package-output',type=pathlib.Path,required=True)
    p.add_argument('--export',type=pathlib.Path,required=True)
    a=p.parse_args()
    def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
    manifest=json.loads(a.manifest.read_text())
    assert manifest['revision']==a.revision
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.tree,text=True).strip()==a.revision
    assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=a.tree)
    for row in manifest['files']:
        assert sha(a.tree/row['path'])==row['sha256'],row['path']
    packaging_manifest=json.loads(a.packaging_manifest.read_text())
    assert packaging_manifest['revision']==a.packaging_revision
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.packaging_tree,text=True).strip()==a.packaging_revision
    assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=a.packaging_tree)
    for row in packaging_manifest['files']:assert sha(a.packaging_tree/row['path'])==row['sha256'],row['path']
    runner=json.loads((a.delivery/'runner-summary.json').read_text())
    assert runner['status'] in ('PASS','FAIL') and runner['revision']==a.revision
    commands=runner['commands']+[json.loads((a.delivery/(name+'.command.json')).read_text()) for name in ['final-package','final-checksums']]+[json.loads((a.delivery/'installer-command.json').read_text())]
    assert all(isinstance(row['exit_code'],int) for row in commands)
    for row in commands:
        assert row['status']==('PASS' if row['exit_code']==0 else 'FAIL')
        assert sha(a.delivery/row['log'])==row['log_sha256']
    a.export.mkdir(parents=True,exist_ok=False)
    def copy(path,name=None):
        assert path.is_file() and not path.is_symlink(),path
        target=a.export/(name or path.name)
        assert not target.exists(),target
        shutil.copyfile(path,target)
        return target
    for path in sorted(a.delivery.iterdir()):
        if path.is_file(): copy(path)
    copy(a.delivery/'installer-qa/installer-report.json')
    packages=a.package_output
    for path in sorted(packages.iterdir()):
        if path.is_file(): copy(path)
    copy(a.manifest,'source-sha256.json')
    copy(a.packaging_manifest,'packaging-source-sha256.json')
    copy(a.tree/'dist/linux-candidate/package-report.json','entry-package-report.json')
    driver=runner['driver']
    assert driver['path']=='run-linux-candidate-v5.py'
    assert driver['sha256']=='7403d0729ccbd30c45df38c9a67422ea36ba6bc82641939eee63d204cac05984'
    assert sha(pathlib.Path('/opt/odin-transfer')/driver['path'])==driver['sha256']
    for name in [driver['path'],'run-linux-installer-v3.py','run-linux-doc-package-v3.py','export-linux-v3.py']:
        copy(pathlib.Path('/opt/odin-transfer')/name)
    metadata=json.loads((a.delivery/'metadata.log').read_text())
    assert metadata['revision']==a.revision and metadata['target']=='x86_64-unknown-linux-musl'
    assert metadata['profile']=='release' and metadata['tracked_source_dirty'] is False
    binary=a.delivery/'odin'
    assert sha(binary)==a.expected_binary_sha256
    binary_data=binary.read_bytes()
    assert binary_data[:6]==b'\x7fELF\x02\x01' and struct.unpack_from('<H',binary_data,18)[0]==62
    phoff=struct.unpack_from('<Q',binary_data,32)[0]
    entry_size,count=struct.unpack_from('<HH',binary_data,54)
    assert not any(struct.unpack_from('<I',binary_data,phoff+i*entry_size)[0]==3 for i in range(count))
    package=json.loads((packages/'package-report.json').read_text())
    installer=json.loads((a.delivery/'installer-qa/installer-report.json').read_text())
    assert package['binary_sha256']==installer['binary_sha256']==a.expected_binary_sha256
    for row in package['artifacts']:
        assert sha(a.export/row['path'])==row['sha256']
    portable=next(packages/row['path'] for row in package['artifacts'] if row['path'].endswith('.tar.gz'))
    with tarfile.open(portable,'r:gz') as archive:
        inventory_member=next(row for row in archive.getmembers() if row.name.endswith('/SHA256SUMS.json'))
        inventory_bytes=archive.extractfile(inventory_member).read()
    (a.export/'payload-inventory.json').write_bytes(inventory_bytes)
    inventory=json.loads(inventory_bytes)
    smokes={}
    for name in ['cli','mcp','http','cancellation']:
        value=json.loads((a.delivery/(name+'-report.json')).read_text())
        assert value['binary_sha256']==a.expected_binary_sha256
        smokes[name]={'status':value['status'],'passed_checks':sum(row['status']=='PASS' for row in value['checks']),
                      'failed_checks':sum(row['status']=='FAIL' for row in value['checks'])}
    def counts(log):
        suites=[]; current=None
        for line in log.read_text().splitlines():
            line=line.strip()
            if line.startswith(('Running ','Doc-tests ')): current=line
            m=re.fullmatch(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in ([0-9.]+)s',line)
            if m: suites.append({'suite':current,'result':m[1],'passed':int(m[2]),'failed':int(m[3]),'ignored':int(m[4]),'measured':int(m[5]),'filtered_out':int(m[6]),'elapsed_seconds':float(m[7])})
        assert suites
        return {'passed':sum(row['passed'] for row in suites),'failed':sum(row['failed'] for row in suites),'ignored':sum(row['ignored'] for row in suites),'suites':suites}
    workspace=counts(a.delivery/'tests.log')
    test_command=next(row for row in commands if row['name']=='tests')
    workspace.update(status=test_command['status'],exit_code=test_command['exit_code'])
    focused=counts(a.delivery/'focused-core.log')
    focused_command=next(row for row in commands if row['name']=='focused-core')
    focused.update(status=focused_command['status'],exit_code=focused_command['exit_code'])
    focused_protocol=counts(a.delivery/'focused-protocol.log')
    focused_protocol_command=next(row for row in commands if row['name']=='focused-protocol')
    focused_protocol.update(status=focused_protocol_command['status'],exit_code=focused_protocol_command['exit_code'])
    installs=[row for row in installer['commands'] if row.get('command',[])[:2]==['dpkg','--install']]
    removals=[row for row in installer['commands'] if row.get('command',[])[:2]==['dpkg','--remove']]
    def exit_at(rows,index): return rows[index]['exit_code'] if len(rows)>index else None
    def product(path):
        return path.startswith(('crates/','schemas/','dashboard/','scripts/','packaging/','tests/')) or path in ('Cargo.toml','Cargo.lock','rust-toolchain.toml')
    products=[row for row in manifest['files'] if product(row['path'])]
    assert all(sha(a.tree/row['path'])==row['sha256'] for row in products)
    packaging_products=[row for row in packaging_manifest['files'] if product(row['path'])]
    assert {row['path']:row['sha256'] for row in products}=={row['path']:row['sha256'] for row in packaging_products}
    passed=candidate_passed(runner,commands,smokes,installer)
    report={'scope':'ACTUAL_LINUX_VM_BUILD_WORKSPACE_GATES_SMOKES_AND_INSTALLERS','status':'PASS' if passed else 'FAIL',
            'source_revision':a.revision,'app_build_revision':a.revision,'packaging_revision':a.packaging_revision,'bundled_docs_revision':a.packaging_revision,
            'source_transfer_bundle_sha256':sha(a.manifest.parent/('odin-'+a.revision[:7]+'.bundle')),
            'packaging_transfer_bundle_sha256':sha(a.packaging_manifest.parent/('odin-'+a.packaging_revision[:7]+'.bundle')),
            'build_metadata':metadata,'binary_sha256':a.expected_binary_sha256,'linux_target':'x86_64-unknown-linux-musl',
            'elf':{'format':'ELF64_X86_64','pt_interp':None,'static_pie':struct.unpack_from('<H',binary_data,16)[0]==3},
            'execution':'NATIVE_COMPILE_AND_NATIVE_ELF_EXECUTION_IN_LOCAL_QEMU_TCG_DEBIAN_12_X86_64',
            'build_tests_smoke_user':'odin-qa uid=1000','installer_package_management_user':'root in disposable guest',
            'source_manifest':{'path':'source-sha256.json','sha256':sha(a.export/'source-sha256.json'),'checked_files':len(manifest['files'])},
            'packaging_source_manifest':{'path':'packaging-source-sha256.json','sha256':sha(a.export/'packaging-source-sha256.json'),'checked_files':len(packaging_manifest['files'])},
            'product_equivalence':{'status':'PASS','compared_files':len(products),'scope':['crates/','schemas/','dashboard/','scripts/','packaging/','tests/','Cargo.toml','Cargo.lock','rust-toolchain.toml'],
                                   'note':'Pre-build app source and the separate clean packaging tree were compared file by file; packaging uses the same selected ELF without recompilation.'},
            'rust':'1.90.0','rust_linker':'cc','c_compiler':'musl-gcc','original_release_profile_preserved':True,
            'commands':commands,'workspace_tests':workspace,'focused_core':focused,'focused_protocol':focused_protocol,'smokes':smokes,
            'artifacts':package['artifacts']+[{'path':'odin','sha256':a.expected_binary_sha256,'bytes':binary.stat().st_size}],
            'payload_inventory':{'path':'payload-inventory.json','sha256':sha(a.export/'payload-inventory.json'),'entries':len(inventory)},
            'installer_lifecycle':{'status':installer['status'],'report':{'path':'installer-report.json','sha256':sha(a.export/'installer-report.json')},
                                   'install_exit_code':exit_at(installs,0),'reinstall_exit_code':exit_at(installs,1),'uninstall_exit_code':exit_at(removals,0),'checks':installer['checks']},
            'package_manifest_note':'The packaging recipe writes its own NOT_RUN lifecycle/test fields because it does not execute those commands; this companion binds separately executed gates and lifecycle QA to the same binary/artifacts.',
            'signed':False,'linux_native_os_sandbox':'NOT_IMPLEMENTED',
            'signal_shutdown_acceptance':{'requirement':'ODIN-029-A2','status':'NOT_RUN','reason':'No dedicated SIGTERM/SIGINT descendant and journal acceptance was executed.'},
            'not_run':['physical_linux_host','cross_version_upgrade','interruption_rollback','macOS_runtime','Linux_real_language_servers_and_debuggers'],
            'windows_acceptance':'SEPARATE_EVIDENCE_REQUIRED'}
    (a.export/'linux-validation.json').write_text(json.dumps(report,indent=2)+'\n')
    for path in a.export.iterdir(): path.chmod(0o444)
    transfer={'scope':'FIXED_LINUX_FINAL_ARTIFACT_TRANSFER','source_revision':a.revision,'packaging_revision':a.packaging_revision,
              'files':[{'path':path.name,'bytes':path.stat().st_size,'sha256':sha(path)} for path in sorted(a.export.iterdir())]}
    manifest_path=a.export.parent/('manifest-'+a.revision[:7]+'.json')
    assert not manifest_path.exists()
    manifest_path.write_text(json.dumps(transfer,indent=2)+'\n')
    print(json.dumps({'status':report['status'],'files':len(transfer['files']),'bytes':sum(row['bytes'] for row in transfer['files']),'manifest_sha256':sha(manifest_path),'binary_sha256':a.expected_binary_sha256}))


if __name__ == '__main__':
    main()
