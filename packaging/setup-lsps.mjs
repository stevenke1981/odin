#!/usr/bin/env node
/** Offline, host-operated setup for the LSP payload shipped with Odin.
 * No repository configuration, shell, npm hooks, arbitrary program or eval input.
 */
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const SELF = fileURLToPath(import.meta.url);
const REQUIRED = ['node', 'basedpyright', 'typescript-language-server', 'typescript', 'bash-language-server', 'go', 'gopls', 'dotnet', 'csharp-ls', 'lua-language-server'];
const ENTRY_POINTS = ['node/node.exe', 'node_modules/smol-toml/package.json', 'node_modules/basedpyright/index.js', 'node_modules/basedpyright/langserver.index.js', 'node_modules/typescript-language-server/lib/cli.mjs', 'node_modules/typescript/lib/tsserver.js', 'node_modules/typescript/lib/tsc.js', 'node_modules/bash-language-server/out/cli.js', 'go/bin/go.exe', 'gopls/gopls.exe', 'dotnet/dotnet.exe', 'csharp/CSharpLanguageServer.dll', 'lua/bin/lua-language-server.exe'];
export const LUA_SETTINGS = Object.freeze({ 'runtime.plugin': '', 'workspace.checkThirdParty': 'Disable', 'addonManager.enable': false });
const LUA_JSON = JSON.stringify(LUA_SETTINGS, null, 2) + '\n';
const IDS = ['python', 'go', 'csharp', 'lua', 'javascript', 'typescript', 'bash'];
const HASH = /^[a-f0-9]{64}$/;
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fail(message) { throw new Error(message); }
function plain(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (plain(value)) return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
  return value;
}
const objectHash = value => digest(JSON.stringify(canonical(value)));
export function under(child, parent) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative));
}
function absolute(value, label) {
  if (typeof value !== 'string' || !path.isAbsolute(value) || /[\r\n\0]/.test(value)) fail(`${label} must be an absolute path`);
  return path.normalize(value);
}
export function canonicalFuture(value) {
  let candidate = absolute(value, 'Path');
  const tail = [];
  while (!fs.existsSync(candidate)) {
    tail.unshift(path.basename(candidate));
    const parent = path.dirname(candidate);
    if (parent === candidate) fail('Path has no existing ancestor');
    candidate = parent;
  }
  return path.join(fs.realpathSync.native(candidate), ...tail);
}
function readOptional(filename) {
  try {
    const stat = fs.lstatSync(filename);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink > 1) fail(`Host file must be a regular unlinked file: ${filename}`);
    if (stat.size > 4 * 1024 * 1024) fail(`Host file exceeds 4 MiB: ${filename}`);
    return fs.readFileSync(filename);
  } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
function beforeHash(bytes) { return bytes === null ? 'absent' : digest(bytes); }
function checkedRelative(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || /[:\0\r\n]/.test(value) || path.posix.isAbsolute(value) || value.split('/').some(part => !part || part === '.' || part === '..')) fail('Unsafe payload inventory path');
  return value;
}
export function verifyPayload(installRoot, runner = null) {
  const root = fs.realpathSync.native(absolute(installRoot, 'Install root'));
  const backendRoot = path.join(root, 'backends');
  if (fs.lstatSync(backendRoot).isSymbolicLink()) fail('Backend root cannot be a link');
  const raw = fs.readFileSync(path.join(backendRoot, 'manifest.json'));
  const manifest = JSON.parse(raw);
  if (manifest.schema_version !== 1 || manifest.target !== 'x86_64-pc-windows-msvc' || !Array.isArray(manifest.files) || !Array.isArray(manifest.components)) fail('Unsupported Windows LSP manifest');
  const components = new Map();
  for (const row of manifest.components) {
    if (!plain(row) || typeof row.id !== 'string' || typeof row.version !== 'string' || !row.version || components.has(row.id)) fail('Invalid or duplicate payload component');
    components.set(row.id, row);
  }
  for (const id of REQUIRED) if (!components.has(id)) fail(`Missing required payload component: ${id}`);
  const files = new Map();
  const names = new Set();
  for (const row of manifest.files) {
    const relative = checkedRelative(row.path);
    const key = relative.toLowerCase();
    if (relative === 'manifest.json' || names.has(key) || !HASH.test(row.sha256)) fail('Invalid or duplicate payload digest');
    names.add(key);
    files.set(relative, row.sha256);
  }
  const actual = [];
  function visit(directory, prefix = '') {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const relative = prefix + entry.name;
      const full = path.join(directory, entry.name);
      const stat = fs.lstatSync(full);
      if (stat.isSymbolicLink() || (!stat.isFile() && !stat.isDirectory())) fail(`Payload links/devices are forbidden: ${relative}`);
      if (stat.isDirectory()) visit(full, relative + '/');
      else if (relative !== 'manifest.json') {
        actual.push(relative);
        if (!files.has(relative) || digest(fs.readFileSync(full)) !== files.get(relative)) fail(`Payload digest mismatch or unexpected file: ${relative}`);
      }
    }
  }
  if (runner === null) {
    visit(backendRoot);
    if (actual.length !== files.size) fail('Payload inventory has missing files');
  } else {
    // Runtime trusts the host-owned installation; verify fixed launchers, not
    // all SDK documentation/source files on every short LSP startup.
    const launchers = runner === 'go' ? ['gopls/gopls.exe', 'go/bin/go.exe'] : runner === 'csharp' ? ['dotnet/dotnet.exe', 'csharp/CSharpLanguageServer.dll'] : runner === 'lua' ? ['lua/bin/lua-language-server.exe'] : fail('Unknown fixed backend');
    for (const relative of launchers) {
      const full = path.join(backendRoot, relative);
      const stat = fs.lstatSync(full);
      if (!stat.isFile() || stat.isSymbolicLink() || digest(fs.readFileSync(full)) !== files.get(relative)) fail(`Backend launcher digest mismatch: ${relative}`);
    }
  }
  for (const relative of ENTRY_POINTS) if (!files.has(relative)) fail(`Missing required payload entry: ${relative}`);
  const inventoryNames = [...files.keys()];
  if (!inventoryNames.some(name => name.startsWith('dotnet/sdk/') && name.endsWith('/MSBuild.dll')) || !inventoryNames.some(name => name.startsWith('go/src/')) || !inventoryNames.some(name => name.startsWith('lua/script/'))) fail('Incomplete Go/.NET/Lua runtime closure');
  const tomlPackage = JSON.parse(fs.readFileSync(path.join(backendRoot, 'node_modules/smol-toml/package.json'), 'utf8'));
  if (tomlPackage.version !== '1.9.1') fail('Setup requires the reviewed smol-toml 1.9.1 parser');
  return { root, backendRoot, manifest, manifest_sha256: digest(raw), files, components };
}
function loadToml(payload) { return createRequire(path.join(payload.backendRoot, 'manifest.json'))('smol-toml'); }
function paths(payload) {
  const at = relative => path.join(payload.backendRoot, ...relative.split('/'));
  return { node: at('node/node.exe'), python: at('node_modules/basedpyright/langserver.index.js'), pyCli: at('node_modules/basedpyright/index.js'), ts: at('node_modules/typescript-language-server/lib/cli.mjs'), tsserver: at('node_modules/typescript/lib/tsserver.js'), tsc: at('node_modules/typescript/lib/tsc.js'), bash: at('node_modules/bash-language-server/out/cli.js'), go: at('go'), gopls: at('gopls/gopls.exe'), dotnet: at('dotnet/dotnet.exe'), csharp: at('csharp/CSharpLanguageServer.dll'), lua: at('lua/bin/lua-language-server.exe') };
}
function systemPath() {
  const windows = process.env.SystemRoot || process.env.WINDIR || 'C:\\Windows';
  return [path.join(windows, 'System32'), windows].join(path.delimiter);
}
export function backendEnvironment(payload, stateDir, kind) {
  const allowed = new Set(['SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT', 'USERPROFILE', 'HOMEDRIVE', 'HOMEPATH', 'HOME', 'TEMP', 'TMP', 'APPDATA', 'LOCALAPPDATA', 'PROGRAMDATA', 'PROGRAMFILES', 'PROGRAMFILES(X86)', 'COMMONPROGRAMFILES', 'COMMONPROGRAMFILES(X86)', 'OS', 'PROCESSOR_ARCHITECTURE', 'NUMBER_OF_PROCESSORS', 'LANG', 'LC_ALL']);
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => allowed.has(key.toUpperCase())));
  env.PATH = systemPath();
  env.NODE_NO_WARNINGS = '1';
  if (kind === 'go') Object.assign(env, { GOROOT: path.join(payload.backendRoot, 'go'), GOPATH: path.join(stateDir, 'lsp-cache', 'go', 'gopath'), GOCACHE: path.join(stateDir, 'lsp-cache', 'go', 'build'), GOMODCACHE: path.join(stateDir, 'lsp-cache', 'go', 'modules'), GOTOOLCHAIN: 'local', GOENV: 'off', GOFLAGS: '-mod=readonly', GOPROXY: 'https://proxy.golang.org', GOSUMDB: 'sum.golang.org', PATH: path.join(payload.backendRoot, 'go', 'bin') + path.delimiter + systemPath() });
  if (kind === 'csharp') Object.assign(env, { DOTNET_ROOT: path.join(payload.backendRoot, 'dotnet'), DOTNET_ROOT_X64: path.join(payload.backendRoot, 'dotnet'), DOTNET_MULTILEVEL_LOOKUP: '0', DOTNET_CLI_TELEMETRY_OPTOUT: '1', DOTNET_SKIP_FIRST_TIME_EXPERIENCE: '1', DOTNET_NOLOGO: '1', DOTNET_CLI_HOME: path.join(stateDir, 'lsp-cache', 'dotnet', 'home'), NUGET_PACKAGES: path.join(stateDir, 'lsp-cache', 'dotnet', 'packages'), MSBUILDDISABLENODEREUSE: '1' });
  return env;
}
export function makeProfiles(payload, stateDir, luaConfig) {
  const p = paths(payload);
  const js = (entry, args = []) => ['--preserve-symlinks', '--preserve-symlinks-main', entry, ...args];
  const run = kind => [path.join(payload.root, 'setup-lsps.mjs'), '--run-backend', kind, '--state-dir', stateDir, ...(kind === 'lua' ? ['--lua-config', luaConfig] : [])];
  const typescript = { tsserver: { path: p.tsserver }, disableAutomaticTypingAcquisition: true };
  return [
    { id: 'python', extensions: ['py', 'pyi'], program: p.node, args: js(p.python, ['--stdio']) },
    { id: 'go', extensions: ['go'], program: p.node, args: run('go'), initialization_options: { env: Object.fromEntries(Object.entries(backendEnvironment(payload, stateDir, 'go')).filter(([key]) => /^GO|^PATH$/.test(key))) } },
    { id: 'csharp', extensions: ['cs'], program: p.node, args: run('csharp') },
    { id: 'lua', extensions: ['lua'], program: p.node, args: run('lua'), settings: { Lua: { runtime: { plugin: '' }, workspace: { checkThirdParty: 'Disable' }, addonManager: { enable: false } } } },
    { id: 'javascript', extensions: ['js', 'jsx', 'mjs', 'cjs'], program: p.node, args: js(p.ts, ['--stdio']), initialization_options: typescript },
    { id: 'typescript', extensions: ['ts', 'tsx', 'mts', 'cts'], program: p.node, args: js(p.ts, ['--stdio']), initialization_options: typescript },
    { id: 'bash', extensions: ['sh', 'bash'], program: p.node, args: js(p.bash, ['start']), settings: { bashIde: { shellcheckPath: '', shfmt: { path: '' }, explainshellEndpoint: '' } } },
  ];
}
export function buildPlan(options, payload) {
  const requestedHost = absolute(options.hostConfig, '--host-config');
  readOptional(requestedHost); // Reject a file-level link before realpath can erase its identity.
  const host = canonicalFuture(requestedHost);
  if (under(host, payload.root)) fail('Host configuration must be outside the installation');
  const before = readOptional(host);
  const toml = loadToml(payload);
  const config = before === null ? {} : toml.parse(before.toString('utf8'));
  if (!plain(config) || (config.projects !== undefined && !plain(config.projects)) || (config.workspace !== undefined && !plain(config.workspace))) fail('Invalid host configuration shape');
  const roots = (options.workspaceRoots || []).map(root => {
    const resolved = fs.realpathSync.native(absolute(root, '--workspace-root'));
    if (!fs.statSync(resolved).isDirectory()) fail('Workspace root must be a directory');
    if (under(host, resolved) || under(resolved, payload.root) || under(payload.root, resolved)) fail('Host configuration and installation must be outside selected workspaces');
    return resolved;
  });
  if (before === null && roots.length === 0) fail('Fresh host configuration requires an explicit --workspace-root');
  const projectRoots = Object.values(config.projects || {}).map(project => {
    if (!plain(project) || typeof project.root !== 'string') fail('Invalid registered project root');
    return canonicalFuture(path.resolve(path.dirname(host), project.root));
  });
  if ([...roots, ...projectRoots].some(root => under(host, root) || under(payload.root, root))) fail('Host configuration and installation must be outside registered projects');
  const state = canonicalFuture(config.state_dir === undefined ? path.join(path.dirname(host), 'state') : path.resolve(path.dirname(host), config.state_dir));
  if ([...roots, ...projectRoots].some(root => under(state, root) || under(root, state)) || under(state, payload.root)) fail('Host state must be outside projects and installation');
  if (config.state_dir === undefined) config.state_dir = state;
  if (roots.length) {
    config.workspace ??= { allowed_roots: [] };
    if (!Array.isArray(config.workspace.allowed_roots)) fail('workspace.allowed_roots must be an array');
    const current = config.workspace.allowed_roots.map(root => canonicalFuture(path.resolve(path.dirname(host), root)));
    for (const root of roots) if (!current.includes(root)) { config.workspace.allowed_roots.push(root); current.push(root); }
  }
  if (config.workspace && (!Array.isArray(config.workspace.allowed_roots) || config.workspace.allowed_roots.length === 0)) fail('Workspace template requires existing or explicit allowed roots');
  const allowedRoots = (config.workspace?.allowed_roots || []).map(root => {
    const resolved = fs.realpathSync.native(path.resolve(path.dirname(host), root));
    if (!fs.statSync(resolved).isDirectory()) fail('Allowed workspace root must be a directory');
    return resolved;
  });
  const protectedRoots = [...new Set([...roots, ...projectRoots, ...allowedRoots])];
  if (protectedRoots.some(root => under(host, root) || under(payload.root, root) || under(state, root) || under(root, state))) fail('Host configuration, state and installation must be outside every allowed workspace root');
  const markerPath = host + '.odin-lsps.json';
  const markerBefore = readOptional(markerPath);
  const marker = markerBefore === null ? { profiles: {} } : JSON.parse(markerBefore.toString('utf8'));
  if (!plain(marker.profiles) || (marker.schema_version !== undefined && marker.schema_version !== 1) || (marker.host_config !== undefined && marker.host_config !== host)) fail('Invalid managed-profile marker');
  const luaConfig = host + '.lua-settings.' + digest(LUA_JSON).slice(0, 16) + '.json';
  if (protectedRoots.some(root => under(markerPath, root) || under(luaConfig, root))) fail('Host sidecars must be outside every allowed workspace root');
  const profiles = makeProfiles(payload, state, luaConfig);
  const managed = {};
  const changes = [];
  const collisions = [];
  const scopes = [];
  if (config.workspace) scopes.push(['workspace', config.workspace]);
  for (const [id, project] of Object.entries(config.projects || {})) scopes.push([`projects.${id}`, project]);
  for (const [scope, target] of scopes) {
    if (target.languages !== undefined && !Array.isArray(target.languages)) fail(`${scope}.languages must be an array`);
    target.languages ??= [];
    const seen = new Set();
    for (const item of target.languages) { if (!plain(item) || typeof item.id !== 'string' || seen.has(item.id)) fail(`Invalid or duplicate ${scope} language id`); seen.add(item.id); }
    const bundledExtensions = new Set(profiles.flatMap(profile => profile.extensions.map(extension => extension.toLowerCase())));
    for (const existing of target.languages) if (!IDS.includes(existing.id) && Array.isArray(existing.extensions) && existing.extensions.some(extension => typeof extension === 'string' && bundledExtensions.has(extension.toLowerCase()))) collisions.push({ scope, language: existing.id, reason: 'A custom language id already owns bundled file extensions; edit that host profile explicitly before setup' });
    for (const next of profiles) {
      const key = `${scope}.languages.${next.id}`;
      const index = target.languages.findIndex(item => item.id === next.id);
      const old = index < 0 ? null : target.languages[index];
      const known = old !== null && marker.profiles[key] === objectHash(old);
      const same = old !== null && objectHash(old) === objectHash(next);
      if (old !== null && !known && !same && !options.replaceLanguageProfiles) {
        collisions.push({ scope, language: next.id, before_sha256: objectHash(old), proposed_sha256: objectHash(next) });
        continue;
      }
      if (index < 0) target.languages.push(next); else target.languages[index] = next;
      managed[key] = objectHash(next);
      changes.push({ scope, language: next.id, action: old === null ? 'add' : same ? 'unchanged' : known ? 'update-managed' : 'replace-explicit', before: old, after: next });
    }
  }
  const afterText = toml.stringify(config);
  // A real parser roundtrip must preserve every resulting setting value.
  if (objectHash(toml.parse(afterText)) !== objectHash(config)) fail('TOML serialization roundtrip changed configuration values');
  const markerText = JSON.stringify({ schema_version: 1, host_config: host, bundle_manifest_sha256: payload.manifest_sha256, profiles: managed }, null, 2) + '\n';
  const plan = { schema_version: 1, scope: 'OFFLINE_HOST_LSP_CONFIGURATION', status: collisions.length ? 'CONFLICT' : 'READY', host_config: host, before_sha256: beforeHash(before), after_sha256: digest(afterText), install_root: payload.root, bundle_manifest_sha256: payload.manifest_sha256, state_dir: state, added_workspace_roots: roots, authorized_workspace_roots: protectedRoots, replace_language_profiles: !!options.replaceLanguageProfiles, languages: IDS, changes, collisions, permissions: 'Existing grants preserved; new workspace grants remain disabled', optional_rust: 'Existing Rust profiles are retained; Rust toolchain is not bundled', sidecars: [{ path: markerPath, before_sha256: beforeHash(markerBefore), after_sha256: digest(markerText) }, { path: luaConfig, before_sha256: beforeHash(readOptional(luaConfig)), after_sha256: digest(LUA_JSON) }] };
  plan.plan_sha256 = objectHash(plan);
  return { plan, afterText, markerText, before, markerBefore, luaConfig, markerPath, config };
}
function writeNew(filename, bytes) {
  const fd = fs.openSync(filename, 'wx', 0o600);
  try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
}
function matches(filename, expected) { return beforeHash(readOptional(filename)) === expected; }
export function applyPlan(prepared, expectedSha256, expectedPlanSha256) {
  const p = prepared.plan;
  if (p.status !== 'READY') fail('Custom language profiles conflict; inspect --plan and explicitly select --replace-language-profiles');
  if (expectedSha256 !== p.before_sha256 || !/^(absent|[a-f0-9]{64})$/.test(expectedSha256 || '')) fail('Expected host SHA-256 is required and must match the plan');
  if (expectedPlanSha256 && expectedPlanSha256 !== p.plan_sha256) fail('Prepared plan changed; inspect a new --plan');
  fs.mkdirSync(path.dirname(p.host_config), { recursive: true });
  if (canonicalFuture(p.host_config) !== p.host_config) fail('Host path identity changed');
  const lock = p.host_config + '.odin-lsp-setup.lock';
  const token = crypto.randomUUID();
  writeNew(lock, token);
  const staged = [];
  const backups = [];
  try {
    for (const row of [{ path: p.host_config, before_sha256: p.before_sha256 }, ...p.sidecars]) if (!matches(row.path, row.before_sha256)) fail(`Conflict; host file changed: ${row.path}`);
    for (const [filename, bytes] of [[p.host_config, prepared.before], [prepared.markerPath, prepared.markerBefore]]) if (bytes !== null) {
      const backup = filename + '.backup-' + Date.now() + '-' + crypto.randomUUID();
      writeNew(backup, bytes); backups.push({ path: backup, sha256: digest(bytes) });
    }
    const luaBefore = readOptional(prepared.luaConfig);
    if (luaBefore !== null && digest(luaBefore) !== digest(LUA_JSON)) fail('Conflict in content-addressed Lua settings');
    if (luaBefore === null) writeNew(prepared.luaConfig, LUA_JSON);
    for (const [filename, text] of [[prepared.markerPath, prepared.markerText], [p.host_config, prepared.afterText]]) {
      const temporary = filename + '.new-' + crypto.randomUUID();
      writeNew(temporary, text); staged.push(temporary);
    }
    // Recheck all preconditions after backup/staging and immediately before replacing.
    if (!matches(p.host_config, p.before_sha256) || !matches(prepared.markerPath, p.sidecars[0].before_sha256)) fail('Conflict during host staging; existing configuration retained');
    for (const [index, row] of [[0, p.sidecars[0]], [1, { path: p.host_config, before_sha256: p.before_sha256 }]]) {
      if (!matches(row.path, row.before_sha256)) fail(`Conflict before replace: ${row.path}`);
      if (row.before_sha256 === 'absent') { fs.linkSync(staged[index], row.path); fs.unlinkSync(staged[index]); }
      else fs.renameSync(staged[index], row.path);
    }
    return { status: 'PASS', host_config: p.host_config, before_sha256: p.before_sha256, after_sha256: digest(fs.readFileSync(p.host_config)), backups, languages: IDS, bundle_manifest_sha256: p.bundle_manifest_sha256, setup_semantics: 'Host-file CAS with backups; no multi-file atomicity or OS-global CAS claimed' };
  } finally {
    for (const filename of staged) if (fs.existsSync(filename)) fs.unlinkSync(filename);
    if (fs.existsSync(lock) && fs.readFileSync(lock, 'utf8') === token) fs.unlinkSync(lock);
  }
}
export function health(payload) {
  const p = paths(payload);
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'odin-lsp-health-'));
  const components = payload.components;
  const healthLuaConfig = path.join(temporary, 'lua-settings.json');
  writeNew(healthLuaConfig, LUA_JSON);
  const checks = [
    ['node', p.node, ['--version']],
    ['basedpyright', p.node, [p.pyCli, '--version']],
    ['typescript-language-server', p.node, [p.ts, '--version']],
    ['typescript', p.node, [p.tsc, '--version']],
    ['bash-language-server', p.node, [p.bash, '--version']],
    ['go', path.join(p.go, 'bin/go.exe'), ['version']],
    ['gopls', p.gopls, ['version']],
    ['dotnet', p.dotnet, ['--version']],
    ['csharp-ls', p.dotnet, [p.csharp, '--version']],
    ['lua-language-server', p.lua, ['--configpath=' + healthLuaConfig, '--logpath=' + path.join(temporary, 'lua', 'log'), '--metapath=' + path.join(temporary, 'lua', 'meta'), '--version']],
  ];
  const results = [];
  let postHealthIntegrity = { status: 'NOT_RUN' };
  try {
    for (const [id, program, args] of checks) {
      const environment = backendEnvironment(payload, temporary, ['dotnet', 'csharp-ls'].includes(id) ? 'csharp' : ['go', 'gopls'].includes(id) ? 'go' : 'node');
      const run = spawnSync(program, args, { cwd: temporary, env: environment, encoding: 'utf8', windowsHide: true, shell: false, timeout: 60000, maxBuffer: 1024 * 1024 });
      const output = ((run.stdout || '') + '\n' + (run.stderr || '')).trim();
      const expected = components.get(id).version.replace(/^v/, '');
      const passed = run.status === 0 && !run.error && output.includes(expected);
      results.push({ component: id, status: passed ? 'PASS' : 'FAIL', program, args, expected_version: expected, exit_code: run.status, signal: run.signal, stdout: run.stdout || '', stderr: run.stderr || '', error: run.error?.message || null });
    }
    try {
      const verified = verifyPayload(payload.root);
      postHealthIntegrity = { status: verified.manifest_sha256 === payload.manifest_sha256 ? 'PASS' : 'FAIL', manifest_sha256: verified.manifest_sha256 };
    } catch (error) { postHealthIntegrity = { status: 'FAIL', error: error.message }; }
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
  return { scope: 'BUNDLED_BACKEND_VERSION_EXECUTION_NOT_SEMANTIC_ACCEPTANCE', status: results.every(row => row.status === 'PASS') && postHealthIntegrity.status === 'PASS' ? 'PASS' : 'FAIL', bundle_manifest_sha256: payload.manifest_sha256, checks: results, post_health_integrity: postHealthIntegrity, semantic_acceptance: 'NOT_RUN' };
}
async function runBackend(options, payload) {
  const kind = options.runBackend;
  if (!['go', 'csharp', 'lua'].includes(kind)) fail('Only the fixed go/csharp/lua backend runner is supported');
  const state = canonicalFuture(absolute(options.stateDir, '--state-dir'));
  if (under(state, payload.root) || under(state, fs.realpathSync.native(process.cwd())) || under(fs.realpathSync.native(process.cwd()), state)) fail('Backend cache must be outside installation and current project');
  fs.mkdirSync(path.join(state, 'lsp-cache'), { recursive: true });
  const p = paths(payload);
  let program, args;
  if (kind === 'go') { program = p.gopls; args = []; }
  else if (kind === 'csharp') { program = p.dotnet; args = [p.csharp]; }
  else {
    const luaConfig = canonicalFuture(absolute(options.luaConfig, '--lua-config'));
    if (fs.readFileSync(luaConfig, 'utf8') !== LUA_JSON || under(luaConfig, fs.realpathSync.native(process.cwd()))) fail('Lua requires the exact host-owned non-executable JSON settings');
    const privateCache = fs.mkdtempSync(path.join(state, 'lsp-cache', 'lua-'));
    program = p.lua; args = ['--configpath=' + luaConfig, '--logpath=' + path.join(privateCache, 'log'), '--metapath=' + path.join(privateCache, 'meta')];
  }
  const child = spawn(program, args, { env: backendEnvironment(payload, state, kind), stdio: 'inherit', windowsHide: true, shell: false });
  // Odin's native Job/process-group owns the complete tree; also forward normal termination.
  const terminate = signal => { if (!child.killed) child.kill(signal); };
  const onTerm = () => terminate('SIGTERM');
  const onInt = () => terminate('SIGINT');
  process.on('SIGTERM', onTerm); process.on('SIGINT', onInt);
  const code = await new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', (exitCode, signal) => resolve(exitCode ?? (signal ? 1 : 0))); });
  process.off('SIGTERM', onTerm); process.off('SIGINT', onInt);
  return code;
}
export function parseArgs(args) {
  const result = { workspaceRoots: [] };
  const values = new Map([['--host-config', 'hostConfig'], ['--install-root', 'installRoot'], ['--expected-sha256', 'expectedSha256'], ['--expected-plan-sha256', 'expectedPlanSha256'], ['--run-backend', 'runBackend'], ['--state-dir', 'stateDir'], ['--lua-config', 'luaConfig'], ['--receipt', 'receipt']]);
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === '--workspace-root') { if (!args[index + 1] || args[index + 1].startsWith('--')) fail('Missing --workspace-root value'); result.workspaceRoots.push(args[++index]); }
    else if (values.has(arg)) { const key = values.get(arg); if (result[key] !== undefined || !args[index + 1] || args[index + 1].startsWith('--')) fail(`Missing or duplicate ${arg}`); result[key] = args[++index]; }
    else if (['--plan', '--apply', '--health', '--install', '--verify-payload', '--replace-language-profiles', '--help'].includes(arg)) { const key = arg.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase()); if (result[key]) fail(`Duplicate ${arg}`); result[key] = true; }
    else fail(`Unknown argument: ${arg}`);
  }
  const modes = ['plan', 'apply', 'health', 'install', 'verifyPayload', 'runBackend', 'help'].filter(key => result[key]);
  if (modes.length !== 1) fail('Choose exactly one of --plan, --apply, --health, --install, --verify-payload, --run-backend or --help');
  return result;
}
async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) { console.log('Offline Odin LSP setup: --host-config ABS [--workspace-root ABS] --plan; --apply --expected-sha256 HASH|absent [--expected-plan-sha256 HASH]; --install combines plan, version health, CAS apply for the installer. Custom profile replacement additionally requires --replace-language-profiles. --health verifies backend version execution; semantic acceptance is separate.'); return 0; }
  const payload = verifyPayload(options.installRoot || path.dirname(SELF), options.runBackend || null);
  if (options.verifyPayload) { console.log(JSON.stringify({ status: 'PASS', scope: 'PAYLOAD_INTEGRITY_ONLY', files: payload.files.size, manifest_sha256: payload.manifest_sha256 })); return 0; }
  if (options.runBackend) return runBackend(options, payload);
  if (options.health) { const report = health(payload); console.log(JSON.stringify(report, null, 2)); return report.status === 'PASS' ? 0 : 1; }
  const prepared = buildPlan(options, payload);
  if (options.plan) { console.log(JSON.stringify(prepared.plan, null, 2)); return prepared.plan.status === 'READY' ? 0 : 2; }
  if (options.apply && !options.expectedSha256) fail('--apply requires --expected-sha256 from the reviewed plan');
  if (prepared.plan.status !== 'READY') { console.log(JSON.stringify(prepared.plan, null, 2)); return 2; }
  const receiptPath = options.receipt || (options.install ? prepared.plan.host_config + '.lsp-install-' + Date.now() + '-' + crypto.randomUUID() + '.json' : null);
  const receipt = receiptPath === null ? null : canonicalFuture(absolute(receiptPath, '--receipt'));
  if (receipt !== null) {
    const protectedRoots = prepared.plan.authorized_workspace_roots;
    if (under(receipt, payload.root) || protectedRoots.some(root => under(receipt, root))) fail('Receipt must be outside projects and installation');
    if (readOptional(receipt) !== null) fail('Refusing to replace an existing setup receipt');
  }
  const backendHealth = health(payload);
  const report = { plan: prepared.plan, health: backendHealth, result: backendHealth.status === 'PASS' ? applyPlan(prepared, options.install ? prepared.plan.before_sha256 : options.expectedSha256, options.expectedPlanSha256) : { status: 'NOT_RUN', reason: 'Backend version health failed; host configuration retained' } };
  if (receipt !== null) {
    fs.mkdirSync(path.dirname(receipt), { recursive: true });
    report.receipt_path = receipt;
    writeNew(receipt, JSON.stringify(report, null, 2) + '\n');
  }
  console.log(JSON.stringify(report, null, 2));
  return report.result.status === 'PASS' ? 0 : 1;
}
if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(SELF)) {
  main().then(code => { process.exitCode = code; }).catch(error => { console.error(JSON.stringify({ status: 'FAIL', error: error.message })); process.exitCode = 1; });
}
