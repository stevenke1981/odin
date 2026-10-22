// Host-only installer status protocol. Status files are data, never instructions.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';

const PHASES = new Set(['starting', 'verify_before', 'probe', 'verify_after', 'applying', 'done', 'failed', 'cancelled', 'timeout']);
const clean = value => String(value).replace(/[^\x20-\x7e]/g, '?').slice(0, 240);
const inside = (child, parent) => { const rel = path.relative(parent, child); return rel === '' || (!path.isAbsolute(rel) && rel !== '..' && !rel.startsWith('..' + path.sep)); };
function regular(filename) {
  const info = fs.lstatSync(filename);
  if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || info.size > 2048) throw new Error('Unsafe installer status file');
  return info;
}
export class SetupInterrupted extends Error {
  constructor(phase, message) { super(message); this.phase = phase; this.setupExitCode = phase === 'timeout' ? 1460 : 1223; }
}
export class SetupProgress {
  constructor(options = {}, clock = () => performance.now()) {
    this.clock = clock; this.started = clock(); this.lastWrite = -Infinity; this.nextCheck = 0;
    const seconds = Number(options.setupTimeoutSeconds ?? 900);
    this.budget = seconds * 1000;
    if (!Number.isInteger(seconds) || seconds < 1 || seconds > 1800) throw new Error('Setup timeout must be 1..1800 seconds');
    this.phase = 'starting'; this.completed = 0; this.total = 1; this.component = ''; this.committing = false;
    const fields = [options.progressFile, options.cancelFile, options.progressRunId];
    this.enabled = fields.some(value => value !== undefined);
    if (this.enabled) {
      if (fields.some(value => typeof value !== 'string') || !/^[a-f0-9]{32}$/.test(options.progressRunId)) throw new Error('All three installer progress arguments and a 32-character run ID are required');
      this.runId = options.progressRunId;
      for (const value of [options.progressFile, options.cancelFile]) if (!path.isAbsolute(value) || /[\r\n\0]/.test(value)) throw new Error('Installer status paths must be absolute');
      const firstParent = fs.realpathSync.native(path.dirname(options.progressFile));
      const secondParent = fs.realpathSync.native(path.dirname(options.cancelFile));
      if (firstParent !== secondParent || !inside(firstParent, fs.realpathSync.native(os.tmpdir()))) throw new Error('Installer status must use one existing directory inside the user temporary directory');
      this.progressFile = path.join(firstParent, path.basename(options.progressFile));
      this.cancelFile = path.join(secondParent, path.basename(options.cancelFile));
      if (this.progressFile === this.cancelFile || fs.existsSync(this.progressFile) || fs.existsSync(this.cancelFile)) throw new Error('Installer status and cancellation paths must be distinct and new');
      const fd = fs.openSync(this.progressFile, 'wx', 0o600); fs.closeSync(fd);
      this.owned = true;
      this.write(0, 'Preparing offline LSP verification');
    }
  }
  guard(force = false) {
    if (this.committing) return;
    const now = this.clock();
    if (now - this.started >= this.budget) throw new SetupInterrupted('timeout', 'LSP setup exceeded its time limit; host configuration was not committed');
    if (!force && now < this.nextCheck) return;
    this.nextCheck = now + 250;
    if (this.enabled && fs.existsSync(this.cancelFile)) {
      regular(this.cancelFile);
      if (fs.readFileSync(this.cancelFile, 'utf8').trim() !== this.runId) throw new Error('Cancellation run ID mismatch');
      throw new SetupInterrupted('cancelled', 'LSP verification stopped; host configuration was not committed');
    }
  }
  checkpoint(phase, completed, total, component = '', force = false) {
    this.guard(force);
    if (!PHASES.has(phase) || !Number.isInteger(completed) || !Number.isInteger(total) || total < 1 || completed < 0 || completed > total) throw new Error('Invalid installer progress');
    const changed = phase !== this.phase;
    this.phase = phase; this.completed = completed; this.total = total; this.component = component;
    if (this.enabled && (force || changed || this.clock() - this.lastWrite >= 250)) this.write(0, '');
  }
  probeTimeout() { this.guard(true); return Math.max(1, Math.min(60000, Math.ceil(this.budget - (this.clock() - this.started)))); }
  beginApply() {
    this.guard(true);
    this.phase = 'applying'; this.completed = 0; this.total = 1; this.component = '';
    if (this.enabled) this.write(0, 'Committing validated host profiles; this short step cannot be interrupted');
    this.guard(true);
    this.committing = true;
  }
  finish(code, message = '') {
    this.phase = code === 0 ? 'done' : 'failed'; this.completed = code === 0 ? 1 : 0; this.total = 1; this.component = '';
    if (this.enabled) this.write(code, message);
  }
  failed(error) {
    this.phase = error.phase || 'failed'; this.completed = 0; this.total = 1; this.component = '';
    if (this.enabled) this.write(error.setupExitCode || 1, error.message);
  }
  write(code, message) {
    regular(this.progressFile);
    if (this.owned && fs.statSync(this.progressFile).size) {
      const rows = fs.readFileSync(this.progressFile, 'ascii').split('\n');
      if (rows[0] !== 'ODIN_SETUP_PROGRESS_V1' || rows[1] !== this.runId) throw new Error('Installer status identity changed');
    }
    const data = ['ODIN_SETUP_PROGRESS_V1', this.runId, this.phase, this.completed, this.total, clean(this.component), code, clean(message)].join('\n') + '\n';
    const temporary = this.progressFile + '.writing-' + crypto.randomUUID();
    const fd = fs.openSync(temporary, 'wx', 0o600);
    try { fs.writeFileSync(fd, data, 'ascii'); } finally { fs.closeSync(fd); }
    try {
      for (let attempt = 0; ; attempt++) {
        try { fs.renameSync(temporary, this.progressFile); break; }
        catch (error) {
          // A scanner may briefly open a Windows file without delete sharing.
          // Bound retries to 500 ms; never claim success if publication fails.
          if (attempt >= 25 || !['EACCES', 'EPERM', 'EBUSY'].includes(error.code)) throw error;
          if (!['done', 'failed', 'cancelled', 'timeout'].includes(this.phase)) this.guard();
          Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20);
        }
      }
    } finally { if (fs.existsSync(temporary)) fs.unlinkSync(temporary); }
    this.lastWrite = this.clock();
  }
}
