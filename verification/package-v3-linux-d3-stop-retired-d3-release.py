import fcntl,hashlib,json,os,pathlib,signal,time
delivery=pathlib.Path('/opt/odin-delivery/v3-d3e5505')
locks=[]
paths=[pathlib.Path('/root/odin-linux-target/release/.cargo-lock'),pathlib.Path('/root/odin-linux-target/x86_64-unknown-linux-musl/release/.cargo-lock')]
for path in paths:
    handle=path.open('a+b')
    fcntl.flock(handle,fcntl.LOCK_EX|fcntl.LOCK_NB)
    locks.append(handle)
report={'scope':'ROOT_REQUESTED_STOP_OF_RETIRED_SOURCE_BEFORE_RELEASE_COMPILATION','source_revision':'d3e550594783ee9881c6aba251403ff1add0af86','release_profile_locks':[str(path) for path in paths],'status':'WAITING_FOR_SUITE_AND_CLIPPY','reason':'Root requested preserving current suite/Clippy while avoiding release ThinLTO for source with protocol failures.'}
(delivery/'release-stop-guard.json').write_text(json.dumps(report,indent=2)+'\n')
deadline=time.monotonic()+900
while time.monotonic()<deadline:
    log=delivery/'build.log'
    if (delivery/'job.exit').exists(): break
    if log.exists() and 'Blocking waiting for file lock' in log.read_text():
        for process in pathlib.Path('/proc').glob('[0-9]*'):
            try:
                argv=[value.decode() for value in (process/'cmdline').read_bytes().split(b'\0') if value]
                if argv[:3]==['cargo','build','-p'] and '--release' in argv:
                    parent=int((process/'status').read_text().split('PPid:\t',1)[1].splitlines()[0])
                    parent_argv=(pathlib.Path('/proc')/str(parent)/'cmdline').read_bytes()
                    if b'run-linux-candidate-v3.py' not in parent_argv or b'd3e550594783ee9881c6aba251403ff1add0af86' not in parent_argv: continue
                    assert 'Compiling ' not in log.read_text()
                    os.kill(int(process.name),signal.SIGTERM)
                    report.update(status='STOPPED_BEFORE_COMPILATION',cargo_pid=int(process.name),verified_argv=argv,signal='SIGTERM',blocked_release_log_sha256=hashlib.sha256(log.read_bytes()).hexdigest())
                    (delivery/'release-stop-guard.json').write_text(json.dumps(report,indent=2)+'\n')
                    print(json.dumps(report),flush=True)
                    raise SystemExit(0)
            except (FileNotFoundError,ProcessLookupError): pass
    time.sleep(.1)
report.update(status='NO_RELEASE_STARTED_BEFORE_GUARD_END')
(delivery/'release-stop-guard.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
