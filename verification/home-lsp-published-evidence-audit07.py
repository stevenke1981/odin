from pathlib import Path
import json,hashlib,time,sys
R=Path('E:/odin');T=Path('C:/OdinQA/home-progress-1022');checks=[]
def sha(b):return hashlib.sha256(b).hexdigest()
def check(n,value,detail=None):
 checks.append(dict(name=n,status='PASS' if value else 'FAIL',detail=detail))
 if not value:raise AssertionError(n)
for suffix in ['initial','second','third']:
 idx=R/f'verification/home-lsp-evidence-{suffix}-index.json';v=json.loads(idx.read_text(encoding='utf-8'))
 for e in v['entries']:
  b=(R/e['path']).read_bytes();check('published hash '+e['path'],sha(b)==e.get('published_sha256',e.get('sha256')))
  if e.get('original_source'):
   raw=Path(e['original_source']).read_bytes();check('original raw hash '+e['path'],sha(raw)==e['original_raw_sha256'])
   if e.get('lf_normalized') is not None:check('exact LF normalization '+e['path'],raw.decode('utf-8').replace('\r\n','\n').encode('utf-8')==b)
for stem in ['home-lsp-seven06','home-lsp-expanded-retry06-a','home-lsp-expanded-retry06-b','home-lsp-expanded-retry06-c']:
 index=json.loads((R/f'verification/{stem}-index.json').read_text(encoding='utf-8'))
 orig=Path(index.get('original_source',index.get('source_raw_path'))).read_bytes();check('matrix original raw '+stem,sha(orig)==index.get('original_raw_sha256',index.get('source_raw_sha256')))
 original=json.loads(orig);check('metadata equality '+stem,index['metadata']=={k:v for k,v in original.items() if k not in ['languages','transcript']})
 indices=[]
 for case in index['cases']:
  parts=case.get('parts') or [case];data=b''
  for p in parts:
   b=(R/p['path']).read_bytes();check('matrix part SHA '+p['path'],sha(b)==p['sha256']);data+=b
  if case.get('concatenated_utf8_sha256'):check('concatenated SHA '+stem+'/'+case['language'],sha(data)==case['concatenated_utf8_sha256'])
  obj=json.loads(data);expected=next(x for x in original['languages'] if x['language']==case['language'])
  check('case equality '+stem+'/'+case['language'],obj['language_report']==expected)
  ids=obj['transcript_original_indices'];indices+=ids
  check('raw transcript equality '+stem+'/'+case['language'],obj['transcript']==[original['transcript'][i] for i in ids])
 check('every transcript entry exactly once '+stem,sorted(indices)==list(range(len(original['transcript']))))
report=dict(schema_version=1,scope='PUBLISHED_EVIDENCE_BYTES_AND_LOGICAL_TRANSCRIPT_PARTITIONS_NOT_NEW_LANGUAGE_TESTS',status='PASS',checks=checks,command=[sys.executable,'-X','utf8','-B',str(Path(__file__))],cwd=str(Path.cwd()),script_sha256=sha(Path(__file__).read_bytes()))
(T/'published-evidence-audit07.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'status':'PASS','checks':len(checks),'scope':report['scope']}))
