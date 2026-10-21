'use strict';
// Fixed host diagnostic only. Never loaded by Odin or a language server.
const fs = require('node:fs');
const net = require('node:net');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const data = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
function observe() {
  const result = { process_cwd: process.cwd() };
  try { result.dot_entries = fs.readdirSync('.').sort().slice(0, 12); }
  catch (error) { result.dot_error = { code: error.code, errno: error.errno }; }
  // No cwd option: the child inherits the parent's native current directory.
  // This avoids trusting Node's cached process.cwd() after an exception.
  const native = spawnSync(data.python, ['-I', '-B', data.winapi_helper, '--cwd-only'],
    { encoding: 'utf8', timeout: 5000, maxBuffer: 16384, windowsHide: true });
  result.inherited_winapi = { exit_code: native.status, stderr: native.stderr,
    error: native.error && { code: native.error.code, message: native.error.message } };
  try { result.inherited_winapi.result = JSON.parse(native.stdout); }
  catch { result.inherited_winapi.stdout = native.stdout; }
  return result;
}
(async () => {
  const report = { node: process.version, libuv: process.versions.uv,
    executable: process.execPath, initial: observe(), targets: [], checks: [] };
  for (const item of data.targets) {
    const attempt = { ...item, before: observe() };
    try { process.chdir(item.path); attempt.returned = true; }
    catch (error) { attempt.error = { code: error.code, errno: error.errno,
      syscall: error.syscall, message: error.message, path: error.path, dest: error.dest }; }
    attempt.after = observe();
    report.targets.push(attempt);
  }
  const check = (name, action, denied) => {
    try { const value = action(); report.checks.push({ name, status: denied ? 'FAIL' : 'PASS', value }); }
    catch (error) { report.checks.push({ name, status: denied ? 'PASS' : 'FAIL', code: error.code }); }
  };
  check('readonly_source_read', () => crypto.createHash('sha256').update(fs.readFileSync(data.source)).digest('hex') === data.source_sha256, false);
  check('explicit_scratch_write', () => { fs.writeFileSync(data.positive, 'fixed-positive'); return fs.readFileSync(data.positive, 'utf8') === 'fixed-positive'; }, false);
  if (data.cache_positive) {
    check('explicit_private_backend_write', () => {
      fs.writeFileSync(data.cache_positive, 'private-backend-positive');
      return fs.readFileSync(data.cache_positive, 'utf8') === 'private-backend-positive';
    }, false);
  }
  for (const [index, outside] of data.outside.entries()) {
    check(`outside_content_read_${index}`, () => fs.readFileSync(outside), true);
    check(`outside_overwrite_${index}`, () => fs.writeFileSync(outside, 'forbidden'), true);
  }
  check('readonly_source_overwrite', () => fs.writeFileSync(data.source, 'forbidden'), true);
  check('readonly_source_create', () => fs.writeFileSync(data.source + '.forbidden', 'forbidden'), true);
  for (const [index, path] of (data.readonly_files || []).entries()) {
    // Request write access without modifying any installed runtime bytes.
    check(`installed_readonly_write_open_${index}`, () => {
      const handle = fs.openSync(path, 'r+'); fs.closeSync(handle);
    }, true);
  }
  check('denied_policy_read', () => fs.readFileSync(data.policy), true);
  const network = await new Promise(resolve => {
    const client = net.createConnection({ host: '127.0.0.1', port: data.port });
    client.once('connect', () => { client.destroy(); resolve({ status: 'FAIL', connected: true }); });
    client.once('error', error => resolve({ status: 'PASS', code: error.code }));
    client.setTimeout(1000, () => { client.destroy(); resolve({ status: 'NOT_VERIFIED', timeout: true }); });
  });
  report.checks.push({ name: 'loopback_denied', ...network });
  console.log(JSON.stringify(report));
  process.exitCode = report.checks.every(check => check.status === 'PASS' && check.value !== false) ? 0 : 1;
})().catch(error => { console.error(error); process.exitCode = 2; });
