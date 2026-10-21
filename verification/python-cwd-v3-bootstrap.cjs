'use strict';
// Fixed diagnostic descendant, not a product transport or configurable eval.
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const manifest = process.argv[2];
const data = JSON.parse(fs.readFileSync(manifest, 'utf8'));
const child = spawnSync(data.bundled_node,
  ['--preserve-symlinks', '--preserve-symlinks-main', data.node_helper, manifest],
  { cwd: data.lower_dist, encoding: 'utf8', timeout: 20000, maxBuffer: 1048576, windowsHide: true });
const report = { route: 'fixed_lowercase_readonly_cwd_descendant', child_cwd: data.lower_dist,
  child_executable: data.bundled_node, exit_code: child.status, stderr: child.stderr,
  error: child.error && { code: child.error.code, errno: child.error.errno,
    syscall: child.error.syscall, message: child.error.message } };
try { report.result = JSON.parse(child.stdout); }
catch { report.stdout = child.stdout; }
console.log(JSON.stringify(report));
process.exitCode = child.status === 0 && !child.error ? 0 : 1;
