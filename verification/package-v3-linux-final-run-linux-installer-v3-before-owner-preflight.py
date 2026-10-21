import argparse,hashlib,json,os,pathlib,subprocess,time

p=argparse.ArgumentParser()
p.add_argument('--revision',required=True)
p.add_argument('--app-revision',required=True)
p.add_argument('--tree',type=pathlib.Path,required=True)
p.add_argument('--manifest',type=pathlib.Path,required=True)
p.add_argument('--delivery',type=pathlib.Path,required=True)
p.add_argument('--package-output',type=pathlib.Path,required=True)
p.add_argument('--expected-binary-sha256',required=True)
a=p.parse_args()
assert os.geteuid()==0
summary=json.loads((a.delivery/'runner-summary.json').read_text())
assert summary['revision']==a.app_revision
for name in ['build','metadata','package','checksums']:
    assert next(row for row in summary['commands'] if row['name']==name)['exit_code']==0
assert hashlib.sha256((a.delivery/'odin').read_bytes()).hexdigest()==a.expected_binary_sha256
manifest=json.loads(a.manifest.read_text())
assert manifest['revision']==a.revision
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.tree,text=True).strip()==a.revision
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=a.tree)
for row in manifest['files']:
    assert hashlib.sha256((a.tree/row['path']).read_bytes()).hexdigest()==row['sha256'],row['path']
for name in ['final-package','final-checksums']:
    assert json.loads((a.delivery/(name+'.command.json')).read_text())['exit_code']==0
packages=a.package_output
argv=['python3','scripts/verify_linux_installer.py','--package',str(packages/'odin-agent_0.2.0_amd64.deb'),
      '--portable',str(packages/'odin-0.2.0-x86_64-unknown-linux-musl.tar.gz'),
      '--expected-binary-sha256',a.expected_binary_sha256,'--output',str(a.delivery/'installer-qa'),'--disposable-vm']
log=a.delivery/'installer-command.log'; receipt=a.delivery/'installer-command.json'
assert not log.exists() and not receipt.exists()
started=time.monotonic()
with log.open('xb') as output:
    result=subprocess.run(argv,cwd=a.tree,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.STDOUT,timeout=600)
record={'name':'installer_lifecycle','scope':'ACTUAL_DISPOSABLE_GUEST_INSTALLER_QA_COMMAND','argv':argv,'cwd':str(a.tree),
        'uid':os.geteuid(),'exit_code':result.returncode,'status':'PASS' if result.returncode==0 else 'FAIL',
        'elapsed_seconds':round(time.monotonic()-started,3),'binary_sha256':a.expected_binary_sha256,
        'app_build_revision':a.app_revision,'packaging_revision':a.revision,
        'packaging_files_checked_before_install':len(manifest['files']),
        'log':log.name,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
receipt.write_text(json.dumps(record,indent=2)+'\n')
(a.delivery/'installer.exit').write_text(str(result.returncode)+'\n')
print(json.dumps(record))
raise SystemExit(result.returncode)
