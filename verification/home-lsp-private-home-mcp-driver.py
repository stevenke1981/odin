from pathlib import Path
from types import SimpleNamespace
import os,json,sys,hashlib,subprocess
sys.path.insert(0,'E:/odin/tests')
from home_installed_smoke import Driver, real_os_homes
from real_ide_backends import toml
T=Path('C:/OdinQA/home-progress-1022');stage=json.loads((T/'expanded-stage06.json').read_text(encoding='utf-8'));home=Path(stage['synthetic_home']).resolve();app=Path(stage['install_root']);host=home/'.config/odin/host.toml';binary=app/'odin.exe'
assert home not in real_os_homes() and home.is_relative_to(Path('C:/OdinQA/x6').resolve())
a=SimpleNamespace(binary=binary,install_root=app,host_config=host,synthetic_home=home,output=T/'private-home-mcp.json',timeout=90)
d=Driver(a);d.report['scope']='REAL_INSTALLED_HOME_PRIVATE_READ_LIST_FIND_SEARCH_INDEX_AND_CAS';d.report['external_privacy_driver_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();d.environment={**os.environ,'HOME':str(home),'USERPROFILE':str(home)}
public=home/'qa-public-policy-marker.txt';private=home/'.ssh/qa-private-policy-marker.txt';private.parent.mkdir(exist_ok=True)
for p in [public,private]:
 with p.open('x',encoding='utf-8') as f:f.write('odinpolicyfixture ODIN_POLICY_FIXTURE '+p.name)
before=host.read_bytes();error=None
try:
 with d.start_home(home) as peer:
  d.check_root(peer,home)
  for tool,args in [('read_file',{'relative_path':'.ssh/qa-private-policy-marker.txt'}),('list_dir',{'relative_path':'.ssh','recursive':True})]:
   v=d.call(peer,tool,args,error=True);d.record(tool+'_private_denied','PERMISSION' in json.dumps(v) and private.name not in json.dumps(v).split('message',1)[0])
  for tool,args in [('list_dir',{'recursive':True}),('find_file',{'file_mask':'*policy-marker*'}),('search_for_pattern',{'pattern':'ODIN_POLICY_FIXTURE'})]:
   v=d.call(peer,tool,args);raw=json.dumps(v);d.record(tool+'_positive_public_and_no_private',public.name in raw and private.name not in raw,v)
  d.call(peer,'index_build')
  v=d.call(peer,'index_query',{'query':'odinpolicyfixture','limit':20});raw=json.dumps(v);d.record('index_positive_public_and_no_private',public.name in raw and private.name not in raw,v)
  d.current['stderr'].extend(peer.errors)
 lab=T/'privacy-lab';lab.mkdir();labhost=lab/'host.toml'
 text='state_dir = '+toml(str(lab/'state'))+'\n[workspace]\ndefault_root = '+toml(str(home))+'\nallowed_roots = '+toml([str(home)])+'\n[projects.privacy_lab]\nroot = '+toml(str(home))+'\nallow_write = true\nallow_shell = false\nallow_debug = false\n'
 labhost.write_text(text,encoding='utf-8');d.report['lab_grant']='Explicit external synthetic-home test project allow_write=true only; actual production home config unchanged'
 with d.start(labhost,home) as peer:
  v=d.call(peer,'transaction_prepare',{'changes':[{'path':'.ssh/qa-private-policy-marker.txt','expected_sha256':hashlib.sha256(private.read_bytes()).hexdigest(),'content':'blocked'}]},error=True)
  d.record('explicit_writable_lab_private_CAS_denied','PERMISSION' in json.dumps(v),v)
  d.record('private_fixture_bytes_unchanged',private.read_text(encoding='utf-8').startswith('odinpolicyfixture'))
 for private_root in [private.parent,host.parent,app]:
  argv=[str(binary),'--config',str(host),'--workspace',str(private_root),'analyze'];p=subprocess.run(argv,env=d.environment,capture_output=True);d.report.setdefault('denied_binding_commands',[]).append({'argv':argv,'exit_code':p.returncode,'stdout':p.stdout.decode('utf-8',errors='replace'),'stderr':p.stderr.decode('utf-8',errors='replace')});d.record('binding_private_rejected:'+private_root.name,p.returncode!=0 and 'PERMISSION' in p.stderr.decode('utf-8',errors='replace'))
except Exception as e:error=str(e);d.current['checks'].append({'name':'execution','status':'FAIL','detail':error})
d.record('actual_home_host_unchanged',host.read_bytes()==before)
d.current['status']='FAIL' if any(c['status']!='PASS' for c in d.current['checks']) else 'PASS';d.report['status']=d.current['status'];d.report['error']=error;d.save();print(json.dumps({'status':d.report['status'],'checks':len(d.current['checks']),'error':error}));raise SystemExit(0 if d.report['status']=='PASS' else 1)
