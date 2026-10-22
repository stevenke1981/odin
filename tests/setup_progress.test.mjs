// Status/cancellation contract tests; these do not establish installed LSP support.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { SetupProgress } from '../packaging/setup-progress.mjs';
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'odin-progress-unit-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return { dir, options: { progressFile: path.join(dir, 'status.txt'), cancelFile: path.join(dir, 'cancel.txt'), progressRunId: crypto.randomBytes(16).toString('hex') } };
}
function rows(f) { return fs.readFileSync(f.options.progressFile, 'ascii').trimEnd().split('\n'); }
test('status reports actual work with bounded ASCII and an exact run identity', t => {
  const f=fixture(t), p=new SetupProgress(f.options);
  p.checkpoint('verify_before', 7, 20, '\u4e2d\u6587\nunsafe'+ 'x'.repeat(300), true);
  const value=rows(f); assert.equal(value.length,7); // Empty final message is the eighth protocol line.
  const raw=fs.readFileSync(f.options.progressFile,'ascii').split('\n');
  assert.equal(raw.length,9); assert.equal(raw[0],'ODIN_SETUP_PROGRESS_V1'); assert.equal(raw[1],f.options.progressRunId);
  assert.deepEqual(raw.slice(2,5),['verify_before','7','20']); assert.ok(raw[5].length<=240); assert.ok(!raw[5].includes('\n'));
  assert.throws(()=>p.checkpoint('invented',1,1),/Invalid/); assert.throws(()=>p.checkpoint('probe',11,10),/Invalid/);
});
test('cooperative cancellation stops before the commit boundary', t => {
  const f=fixture(t), p=new SetupProgress(f.options);
  fs.writeFileSync(f.options.cancelFile,f.options.progressRunId);
  let stopped; try { p.beginApply(); } catch(e) { stopped=e; }
  assert.equal(stopped.setupExitCode,1223); assert.equal(p.committing,false); p.failed(stopped);
  assert.equal(rows(f)[2],'cancelled'); assert.equal(rows(f)[6],'1223');
});
test('monotonic total deadline prevents beginning a commit and bounds a probe', t => {
  const f=fixture(t); let now=0; const p=new SetupProgress({...f.options,setupTimeoutSeconds:2},()=>now);
  now=1200; assert.equal(p.probeTimeout(),800); now=2000;
  let timed; try { p.beginApply(); } catch(e) { timed=e; }
  assert.equal(timed.setupExitCode,1460); assert.equal(p.committing,false); p.failed(timed); assert.equal(rows(f)[2],'timeout');
});
test('a committed result stays successful if cancellation arrives after the commit boundary', t => {
  const f=fixture(t); let now=0; const p=new SetupProgress({...f.options,setupTimeoutSeconds:1},()=>now);
  p.beginApply(); now=2000; fs.writeFileSync(f.options.cancelFile,f.options.progressRunId);
  assert.doesNotThrow(()=>p.guard(true)); p.finish(0,'Profiles committed'); assert.equal(rows(f)[2],'done');
});
test('status cannot overwrite existing files or adopt another run', t => {
  const f=fixture(t); fs.writeFileSync(f.options.progressFile,'operator content');
  assert.throws(()=>new SetupProgress(f.options),/distinct and new/); assert.equal(fs.readFileSync(f.options.progressFile,'utf8'),'operator content');
  fs.unlinkSync(f.options.progressFile); const p=new SetupProgress(f.options);
  fs.writeFileSync(f.options.progressFile,'ODIN_SETUP_PROGRESS_V1\nforeign\n');
  assert.throws(()=>p.finish(0),/identity changed/); assert.match(fs.readFileSync(f.options.progressFile,'utf8'),/foreign/);
});
test('status arguments, time bounds and cancellation tokens are validated', t => {
  const f=fixture(t);
  assert.throws(()=>new SetupProgress({progressFile:f.options.progressFile}),/All three/);
  assert.throws(()=>new SetupProgress({...f.options,progressRunId:'../bad'}),/All three/);
  for(const setupTimeoutSeconds of [0,1801,1.5,'NaN']) assert.throws(()=>new SetupProgress({...f.options,setupTimeoutSeconds}),/1..1800/);
  const p=new SetupProgress(f.options); fs.writeFileSync(f.options.cancelFile,'foreign'); assert.throws(()=>p.guard(true),/run ID mismatch/);
});
test('status and cancellation must share a temporary directory', t => {
  const f=fixture(t), second=fs.mkdtempSync(path.join(os.tmpdir(),'odin-progress-other-'));
  t.after(()=>fs.rmSync(second,{recursive:true,force:true}));
  assert.throws(()=>new SetupProgress({...f.options,cancelFile:path.join(second,'stop')}),/one existing directory/);
});
test('status refuses a hardlink replacement', t => {
  const f=fixture(t), p=new SetupProgress(f.options), other=path.join(f.dir,'operator-data');
  fs.writeFileSync(other,'preserve'); fs.unlinkSync(f.options.progressFile); fs.linkSync(other,f.options.progressFile);
  assert.throws(()=>p.finish(0),/Unsafe/); assert.equal(fs.readFileSync(other,'utf8'),'preserve');
});

test('a short Windows sharing conflict does not lose terminal status',t=>{
  const f=fixture(t),p=new SetupProgress(f.options),original=fs.renameSync;let attempts=0;
  t.mock.method(fs,'renameSync',(...args)=>{if(attempts++<2){const e=new Error('sharing conflict');e.code='EPERM';throw e;}return original(...args);});
  p.finish(0,'Committed');assert.equal(attempts,3);assert.equal(rows(f)[2],'done');
});
