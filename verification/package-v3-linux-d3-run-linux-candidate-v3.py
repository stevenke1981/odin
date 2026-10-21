import argparse,hashlib,json,os,pathlib,shutil,subprocess,time,traceback

p=argparse.ArgumentParser()
p.add_argument('--revision',required=True)
p.add_argument('--tree',type=pathlib.Path,required=True)
p.add_argument('--manifest',type=pathlib.Path,required=True)
p.add_argument('--delivery',type=pathlib.Path,required=True)
a=p.parse_args()
assert os.geteuid()==1000
a.delivery.mkdir(parents=True,exist_ok=True)
manifest=json.loads(a.manifest.read_text())
assert manifest['revision']==a.revision
env=dict(os.environ,PATH='/opt/odin-rust-1.90/bin:'+os.environ['PATH'],CARGO_HOME='/home/odin-qa/.cargo',
         CARGO_TARGET_DIR='/root/odin-linux-target',CARGO_BUILD_JOBS='4',CC_x86_64_unknown_linux_musl='musl-gcc',
         AR_x86_64_unknown_linux_musl='ar',CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER='cc',
         ODIN_PYTHON='/usr/bin/python3',RUST_TEST_THREADS='1',CARGO_NET_OFFLINE='true')
records=[]
summary={'scope':'NATIVE_LINUX_QA_DRIVER','status':'RUNNING','revision':a.revision,'uid':os.geteuid(),'commands':records}
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def save(): (a.delivery/'runner-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
def verify_source(label):
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.tree,text=True).strip()==a.revision
    assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=a.tree)
    for row in manifest['files']:
        content=(a.tree/row['path']).read_bytes()
        assert hashlib.sha256(content).hexdigest()==row['sha256'] and len(content)==row['bytes'],row['path']
    (a.delivery/label).write_text(json.dumps({'status':'PASS','revision':a.revision,'checked_files':len(manifest['files']),'clean_source':True},indent=2)+'\n')
def run(name,argv,cwd=None):
    log=a.delivery/(name+'.log'); receipt=a.delivery/(name+'.command.json')
    assert not log.exists() and not receipt.exists()
    row={'name':name,'argv':[str(v) for v in argv],'cwd':str(cwd or a.tree),'uid':os.geteuid(),'status':'RUNNING','exit_code':None,'log':log.name}
    records.append(row); save(); started=time.monotonic()
    with log.open('xb') as output:
        completed=subprocess.run(row['argv'],cwd=row['cwd'],env=env,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.STDOUT)
    row.update(exit_code=completed.returncode,status='PASS' if completed.returncode==0 else 'FAIL',elapsed_seconds=round(time.monotonic()-started,3),log_sha256=sha(log))
    receipt.write_text(json.dumps(row,indent=2)+'\n'); save()
    print(name,row['status'],'exit',row['exit_code'],flush=True)
    return completed.returncode==0
def finish():
    summary['status']='PASS' if all(row['exit_code']==0 for row in records) else 'FAIL'
    save(); return 0 if summary['status']=='PASS' else 1
try:
    verify_source('source-preflight.json')
    (a.delivery/'build-environment.json').write_text(json.dumps({'uid':os.geteuid(),'environment':{k:env[k] for k in ('PATH','CARGO_HOME','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CC_x86_64_unknown_linux_musl','AR_x86_64_unknown_linux_musl','CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER','ODIN_PYTHON','RUST_TEST_THREADS','CARGO_NET_OFFLINE')},'rustc':subprocess.check_output(['/opt/odin-rust-1.90/bin/rustc','--version','--verbose'],text=True),'cargo':subprocess.check_output(['/opt/odin-rust-1.90/bin/cargo','--version'],text=True)},indent=2)+'\n')
    run('generated',['python3','scripts/check_generated.py'])
    verify_source('source-after-generators.json')
    run('static-source',['python3','scripts/check_source.py','--output',a.delivery/'static-source-report.json'])
    run('fixture',['python3','scripts/fixture_selftest.py','--output',a.delivery/'fixture-report.json'])
    run('requirements',['python3','scripts/check_requirements.py'])
    run('publish-policy',['python3','scripts/check_publish_policy.py'])
    run('fmt',['cargo','fmt','--all','--','--check'])
    if not run('focused-core',['cargo','test','-p','odin-core','--lib','--target','x86_64-unknown-linux-musl','--locked','--offline','--','--test-threads=1']):
        raise SystemExit(finish())
    run('tests',['cargo','test','--workspace','--no-fail-fast','--target','x86_64-unknown-linux-musl','--locked','--offline','--','--test-threads=1'])
    run('clippy',['cargo','clippy','--workspace','--all-targets','--target','x86_64-unknown-linux-musl','--locked','--offline','--','-D','warnings'])
    if not run('build',['cargo','build','-p','odin-agent','--release','--target','x86_64-unknown-linux-musl','--locked','--offline']):
        raise SystemExit(finish())
    binary=a.delivery/'odin'
    with pathlib.Path('/root/odin-linux-target/x86_64-unknown-linux-musl/release/odin').open('rb') as source,binary.open('xb') as output:
        shutil.copyfileobj(source,output)
    binary.chmod(0o755)
    if not run('metadata',[binary,'metadata']): raise SystemExit(finish())
    metadata=json.loads((a.delivery/'metadata.log').read_text())
    assert metadata['revision']==a.revision and metadata['profile']=='release' and metadata['tracked_source_dirty'] is False and metadata['target']=='x86_64-unknown-linux-musl'
    (a.delivery/'binary-sha256.json').write_text(json.dumps({'status':'PASS','sha256':sha(binary),'bytes':binary.stat().st_size,'metadata':metadata},indent=2)+'\n')
    run('elf',['readelf','-l','-d',binary])
    for name,script in [('cli','cli_smoke.py'),('mcp','mcp_smoke.py'),('http','http_smoke.py'),('cancellation','mcp_cancel_smoke.py')]:
        run(name,['python3','scripts/'+script,'--binary',binary,'--output',a.delivery/(name+'-report.json')])
    if run('package',['bash','scripts/package.sh','dist/linux-candidate']):
        run('checksums',['sha256sum','--check','SHA256SUMS'],cwd=a.tree/'dist/linux-candidate')
        package=json.loads((a.tree/'dist/linux-candidate/package-report.json').read_text())
        assert package['binary_sha256']==sha(binary)
    verify_source('source-after-qa.json')
    raise SystemExit(finish())
except SystemExit:
    raise
except Exception as error:
    summary.update(status='FAIL',driver_error=type(error).__name__+': '+str(error));save();traceback.print_exc();raise
