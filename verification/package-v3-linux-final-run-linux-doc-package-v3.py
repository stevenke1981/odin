import argparse,hashlib,json,os,pathlib,subprocess,time
p=argparse.ArgumentParser()
for name in ['app-revision','packaging-revision','expected-binary-sha256']:p.add_argument('--'+name,required=True)
for name in ['app-tree','packaging-tree','app-manifest','packaging-manifest','delivery','output']:p.add_argument('--'+name,type=pathlib.Path,required=True)
a=p.parse_args();assert os.geteuid()==1000
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
app=json.loads(a.app_manifest.read_text());pack=json.loads(a.packaging_manifest.read_text())
assert app['revision']==a.app_revision and pack['revision']==a.packaging_revision
for tree,manifest in [(a.app_tree,app),(a.packaging_tree,pack)]:
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=tree,text=True).strip()==manifest['revision']
    assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=tree)
    for row in manifest['files']:assert sha(tree/row['path'])==row['sha256'],row['path']
def product(path):return path.startswith(('crates/','schemas/','dashboard/','scripts/','packaging/','tests/')) or path in ('Cargo.toml','Cargo.lock','rust-toolchain.toml')
before={row['path']:row['sha256'] for row in app['files'] if product(row['path'])}
after={row['path']:row['sha256'] for row in pack['files'] if product(row['path'])}
assert before==after
binary=a.delivery/'odin';assert sha(binary)==a.expected_binary_sha256
assert not a.output.exists()
def run(name,argv,cwd):
    log=a.delivery/(name+'.log');receipt=a.delivery/(name+'.command.json')
    assert not log.exists() and not receipt.exists()
    started=time.monotonic()
    with log.open('xb') as output:result=subprocess.run([str(value) for value in argv],cwd=cwd,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.STDOUT)
    row={'name':name,'argv':[str(value) for value in argv],'cwd':str(cwd),'uid':os.geteuid(),'exit_code':result.returncode,'status':'PASS' if result.returncode==0 else 'FAIL','elapsed_seconds':round(time.monotonic()-started,3),'log':log.name,'log_sha256':sha(log)}
    receipt.write_text(json.dumps(row,indent=2)+'\n')
    if result.returncode:raise SystemExit(result.returncode)
run('final-package',['python3','scripts/package_linux.py','--binary',binary,'--expected-binary-sha256',a.expected_binary_sha256,'--output',a.output],a.packaging_tree)
run('final-checksums',['sha256sum','--check','SHA256SUMS'],a.output)
for row in pack['files']:assert sha(a.packaging_tree/row['path'])==row['sha256']
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=a.packaging_tree)
report={'status':'PASS','app_build_revision':a.app_revision,'packaging_revision':a.packaging_revision,'bundled_docs_revision':a.packaging_revision,'app_files_checked':len(app['files']),'packaging_files_checked':len(pack['files']),'product_equivalence':{'status':'PASS','compared_files':len(before),'scope':['crates/','schemas/','dashboard/','scripts/','packaging/','tests/','Cargo.toml','Cargo.lock','rust-toolchain.toml']},'binary_sha256':a.expected_binary_sha256,'binary_rebuilt':False}
(a.delivery/'final-packaging-preflight.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
