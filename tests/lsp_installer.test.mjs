// Host-configuration and payload guards only. This is not real LSP/installer acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { verifyPayload, buildPlan, applyPlan, backendEnvironment, parseArgs, LUA_SETTINGS } from '../packaging/setup-lsps.mjs';

const parserRoot = process.env.ODIN_TEST_SMOL_TOML;
if (!parserRoot || !path.isAbsolute(parserRoot)) throw new Error('Set ODIN_TEST_SMOL_TOML to the reviewed smol-toml 1.9.1 package root');
const toml = createRequire(import.meta.url)(parserRoot);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const hashObject = value => {
  const sort = x => Array.isArray(x) ? x.map(sort) : x && typeof x === 'object' ? Object.fromEntries(Object.keys(x).sort().map(k => [k, sort(x[k])])) : x;
  return sha(JSON.stringify(sort(value)));
};
function fixture(t) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'odin-installer-unit-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const installRoot = path.join(base, 'installation space \u4e2d\u6587');
  const backendRoot = path.join(installRoot, 'backends');
  const project = path.join(base, 'project \u4e2d\u6587');
  const host = path.join(base, 'host', 'host.toml');
  fs.mkdirSync(backendRoot, { recursive: true }); fs.mkdirSync(project); fs.mkdirSync(path.dirname(host));
  fs.cpSync(parserRoot, path.join(backendRoot, 'node_modules', 'smol-toml'), { recursive: true, verbatimSymlinks: true });
  const entries = ['node/node.exe','node_modules/basedpyright/index.js','node_modules/basedpyright/langserver.index.js','node_modules/typescript-language-server/lib/cli.mjs','node_modules/typescript/lib/tsserver.js','node_modules/typescript/lib/tsc.js','node_modules/bash-language-server/out/cli.js','go/bin/go.exe','go/src/runtime/runtime.go','gopls/gopls.exe','dotnet/dotnet.exe','dotnet/sdk/10.0.402/MSBuild.dll','csharp/CSharpLanguageServer.dll','lua/bin/lua-language-server.exe','lua/script/config.lua'];
  for (const entry of entries) { const name = path.join(backendRoot, entry); fs.mkdirSync(path.dirname(name), { recursive: true }); fs.writeFileSync(name, 'inert fixture: '+entry); }
  const manifest = { schema_version: 1, target: 'x86_64-pc-windows-msvc', components: ['node','basedpyright','typescript-language-server','typescript','bash-language-server','go','gopls','dotnet','csharp-ls','lua-language-server'].map(id => ({id, version:'fixture-only'})), files: [] };
  function refresh() {
    manifest.files = [];
    const walk = (directory, prefix='') => {
      for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        const name = prefix+entry.name, full = path.join(directory, entry.name);
        if (entry.isDirectory()) walk(full, name+'/');
        else if (name !== 'manifest.json') manifest.files.push({path:name, sha256:sha(fs.readFileSync(full))});
      }
    };
    walk(backendRoot);
    fs.writeFileSync(path.join(backendRoot, 'manifest.json'), JSON.stringify(manifest));
  }
  refresh();
  return {base, installRoot, backendRoot, project, host, manifest, refresh, payload:()=>verifyPayload(installRoot), options:(more={})=>({hostConfig:host,workspaceRoots:[project],...more})};
}

test('fresh setup needs an explicit workspace decision', t => {
  const f=fixture(t);
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/explicit --workspace-root/);
  assert.equal(fs.existsSync(f.host),false);
});
test('fresh plan is inert and contains seven fixed profiles without write/shell/debug grants', t => {
  const f=fixture(t), prepared=buildPlan(f.options(),f.payload());
  assert.equal(prepared.plan.status,'READY'); assert.equal(prepared.plan.before_sha256,'absent');
  assert.deepEqual(prepared.config.workspace.allowed_roots,[fs.realpathSync.native(f.project)]);
  assert.deepEqual(new Set(prepared.config.workspace.languages.map(x=>x.id)),new Set(['python','go','csharp','lua','javascript','typescript','bash']));
  for(const grant of ['allow_write','allow_shell','allow_debug']) assert.equal(prepared.config.workspace[grant],undefined);
  assert.equal(fs.existsSync(f.host),false); assert.equal(fs.existsSync(prepared.markerPath),false);
  assert.equal(toml.parse(prepared.afterText).state_dir,prepared.plan.state_dir);
});
test('preserve existing grants, roots, Rust, commands and unrelated values', t => {
  const f=fixture(t), rust={id:'rust',extensions:['rs'],program:'C:/trusted/rust-analyzer.exe',args:[]};
  const existing={state_dir:path.join(f.base,'state'),limits:{max_files:5000},workspace:{allowed_roots:[f.project]},projects:{old:{root:f.project,allow_write:true,allow_shell:false,allow_debug:false,languages:[rust],commands:{check:{program:'trusted.exe',args:['literal'],timeout_secs:30}}}}};
  fs.writeFileSync(f.host,toml.stringify(existing));
  const p=buildPlan(f.options({workspaceRoots:[]}),f.payload());
  assert.equal(p.config.projects.old.allow_write,true); assert.equal(p.config.projects.old.allow_shell,false);
  assert.deepEqual(p.config.workspace.allowed_roots,existing.workspace.allowed_roots);
  assert.equal(hashObject(p.config.projects.old.languages.find(x=>x.id==='rust')),hashObject(rust));
  assert.equal(hashObject(p.config.projects.old.commands),hashObject(existing.projects.old.commands)); assert.equal(hashObject(p.config.limits),hashObject(existing.limits));
});
test('custom profile collision is reviewable and blocks apply without changing config', t => {
  const f=fixture(t), custom={id:'python',extensions:['py'],program:'C:/custom/python-lsp.exe'};
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[custom]}}));
  const original=fs.readFileSync(f.host), p=buildPlan(f.options({workspaceRoots:[]}),f.payload());
  assert.equal(p.plan.status,'CONFLICT'); assert.equal(p.plan.collisions.length,1);
  assert.equal(hashObject(p.config.workspace.languages.find(x=>x.id==='python')),hashObject(custom));
  assert.throws(()=>applyPlan(p,p.plan.before_sha256),/Custom language profiles/);
  assert.deepEqual(fs.readFileSync(f.host),original);
});
test('explicit replacement replaces only requested seven language profiles', t => {
  const f=fixture(t), custom={id:'python',extensions:['py'],program:'C:/custom/python-lsp.exe'}, rust={id:'rust',extensions:['rs'],program:'rust-analyzer'};
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[custom,rust]}}));
  const p=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),f.payload());
  assert.equal(p.plan.status,'READY'); assert.equal(p.plan.changes.find(x=>x.language==='python').action,'replace-explicit');
  assert.equal(hashObject(p.config.workspace.languages.find(x=>x.id==='rust')),hashObject(rust));
});
test('CAS apply creates sidecars, backup and all seven host profiles', t => {
  const f=fixture(t); fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project]}}));
  const original=fs.readFileSync(f.host), p=buildPlan(f.options({workspaceRoots:[]}),f.payload());
  const result=applyPlan(p,p.plan.before_sha256,p.plan.plan_sha256);
  assert.equal(result.status,'PASS'); assert.equal(result.backups.length,1); assert.deepEqual(fs.readFileSync(result.backups[0].path),original);
  assert.equal(toml.parse(fs.readFileSync(f.host,'utf8')).workspace.languages.length,7);
  assert.deepEqual(JSON.parse(fs.readFileSync(p.luaConfig,'utf8')),LUA_SETTINGS);
  assert.equal(fs.existsSync(f.host+'.odin-lsp-setup.lock'),false);
});
test('a managed profile can be refreshed, while an edited managed profile becomes a conflict', t => {
  const f=fixture(t), first=buildPlan(f.options(),f.payload()); applyPlan(first,'absent');
  const stable=buildPlan(f.options({workspaceRoots:[]}),f.payload()); assert.equal(stable.plan.status,'READY');
  const config=toml.parse(fs.readFileSync(f.host,'utf8')); config.workspace.languages.find(x=>x.id==='go').program='C:/operator/go.exe';
  fs.writeFileSync(f.host,toml.stringify(config));
  const changed=buildPlan(f.options({workspaceRoots:[]}),f.payload()); assert.equal(changed.plan.status,'CONFLICT'); assert.equal(changed.plan.collisions[0].language,'go');
});
test('stale config hash refuses to overwrite an operator edit', t => {
  const f=fixture(t), p=buildPlan(f.options(),f.payload()); fs.writeFileSync(f.host,'# operator concurrent creation\n');
  assert.throws(()=>applyPlan(p,'absent'),/Conflict/); assert.equal(fs.readFileSync(f.host,'utf8'),'# operator concurrent creation\n');
});
test('stale expected plan hash refuses apply', t => {
  const f=fixture(t), p=buildPlan(f.options(),f.payload());
  assert.throws(()=>applyPlan(p,'absent','a'.repeat(64)),/Prepared plan changed/); assert.equal(fs.existsSync(f.host),false);
});
test('changed managed marker refuses the prepared apply', t => {
  const f=fixture(t), p=buildPlan(f.options(),f.payload()); fs.writeFileSync(p.markerPath,'{}');
  assert.throws(()=>applyPlan(p,'absent'),/Conflict/); assert.equal(fs.existsSync(f.host),false); assert.equal(fs.readFileSync(p.markerPath,'utf8'),'{}');
});
test('payload guards reject altered, missing and unexpected files', t => {
  const f=fixture(t); assert.ok(f.payload().files.size>15);
  const entry=path.join(f.backendRoot,'node/node.exe'); fs.writeFileSync(entry,'tampered'); assert.throws(()=>f.payload(),/digest mismatch/);
  f.refresh(); fs.unlinkSync(entry); assert.throws(()=>f.payload(),/missing files/);
  f.refresh(); fs.writeFileSync(entry,'new file'); assert.throws(()=>f.payload(),/unexpected file/);
});
test('payload guards reject traversal and case-colliding inventory entries', t => {
  const f=fixture(t), name=path.join(f.backendRoot,'manifest.json');
  f.manifest.files.push({path:'../outside',sha256:'a'.repeat(64)}); fs.writeFileSync(name,JSON.stringify(f.manifest)); assert.throws(()=>f.payload(),/Unsafe payload/);
  f.refresh(); f.manifest.files.push({path:'NODE/node.exe',sha256:'a'.repeat(64)}); fs.writeFileSync(name,JSON.stringify(f.manifest)); assert.throws(()=>f.payload(),/duplicate payload digest/);
});
test('host config, cache and installation cannot be placed in projects', t => {
  const f=fixture(t);
  assert.throws(()=>buildPlan(f.options({hostConfig:path.join(f.project,'host.toml')}),f.payload()),/outside selected/);
  fs.writeFileSync(f.host,toml.stringify({state_dir:path.join(f.project,'state'),workspace:{allowed_roots:[f.project]},projects:{demo:{root:f.project}}}));
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/state must be outside/);
});
test('relative state paths resolve against host config, and exact profile digest is stable', t => {
  const f=fixture(t); fs.writeFileSync(f.host,toml.stringify({state_dir:'private-state',workspace:{allowed_roots:[f.project]}}));
  const p=buildPlan(f.options({workspaceRoots:[]}),f.payload()); assert.equal(p.plan.state_dir,path.join(path.dirname(f.host),'private-state'));
  const result=applyPlan(p,p.plan.before_sha256); const marker=JSON.parse(fs.readFileSync(p.markerPath,'utf8'));
  const stored=toml.parse(fs.readFileSync(result.host_config,'utf8')).workspace.languages;
  for(const profile of stored) assert.equal(marker.profiles['workspace.languages.'+profile.id],hashObject(profile));
});
test('runner profiles have fixed arguments and SDK env, with no arbitrary shell input', t => {
  const f=fixture(t), p=buildPlan(f.options(),f.payload());
  const go=p.config.workspace.languages.find(x=>x.id==='go'); assert.ok(go.args.includes('--run-backend')); assert.equal(go.initialization_options.env.GOTOOLCHAIN,'local');
  const env=backendEnvironment(f.payload(),path.join(f.base,'state'),'go'); assert.equal(env.GOENV,'off'); assert.equal(env.GOFLAGS,'-mod=readonly'); assert.equal(env.GOPROXY,'https://proxy.golang.org'); assert.equal(env.NODE_OPTIONS,undefined); assert.ok(env.PATH.startsWith(path.join(f.backendRoot,'go','bin')));
  const ts=p.config.workspace.languages.find(x=>x.id==='typescript'); assert.equal(ts.initialization_options.disableAutomaticTypingAcquisition,true); assert.ok(path.isAbsolute(ts.initialization_options.tsserver.path));
  const bash=p.config.workspace.languages.find(x=>x.id==='bash'); assert.deepEqual(bash.settings.bashIde,{shellcheckPath:'',shfmt:{path:''},explainshellEndpoint:''});
});
test('CLI rejects unknown programs, duplicate modes and missing expected values',()=>{
  assert.throws(()=>parseArgs(['--plan','--apply']),/exactly one/); assert.throws(()=>parseArgs(['--run-backend','go','--program','arbitrary.exe']),/Unknown/); assert.throws(()=>parseArgs(['--plan','--host-config']),/Missing/);
  assert.deepEqual(parseArgs(['--plan','--workspace-root','C:/one','--workspace-root','D:/two']).workspaceRoots,['C:/one','D:/two']);
});

test('foreign custom language ids owning bundled extensions block setup even with replacement', t => {
  const f=fixture(t), custom={id:'custompy',extensions:['PY'],program:'C:/custom/lsp.exe'};
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[custom]}}));
  const p=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),f.payload());
  assert.equal(p.plan.status,'CONFLICT'); assert.equal(p.plan.collisions[0].language,'custompy');
  assert.equal(hashObject(p.config.workspace.languages.find(x=>x.id==='custompy')),hashObject(custom));
});
test('runtime selective integrity checks launchers while explicit full verification checks SDK files', t => {
  const f=fixture(t); fs.writeFileSync(path.join(f.backendRoot,'go/src/runtime/runtime.go'),'changed SDK source');
  assert.doesNotThrow(()=>verifyPayload(f.installRoot,'go'));
  assert.throws(()=>verifyPayload(f.installRoot),/digest mismatch/);
  fs.writeFileSync(path.join(f.backendRoot,'gopls/gopls.exe'),'changed launcher');
  assert.throws(()=>verifyPayload(f.installRoot,'go'),/launcher digest mismatch/);
});

test('existing allowed roots protect host state and sidecars even without new root arguments', t => {
  const f=fixture(t);
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[path.dirname(f.host)]}}));
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/outside every allowed/);
  fs.writeFileSync(f.host,toml.stringify({state_dir:path.join(f.project,'private-cache'),workspace:{allowed_roots:[f.project]}}));
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/outside every allowed/);
});
test('backend environment does not forward host credentials or arbitrary runtime options', t => {
  const f=fixture(t), saved=process.env.ODIN_TEST_SECRET, options=process.env.NODE_OPTIONS;
  process.env.ODIN_TEST_SECRET='never forward'; process.env.NODE_OPTIONS='--require untrusted.js';
  try { const env=backendEnvironment(f.payload(),path.join(f.base,'state'),'csharp'); assert.equal(env.ODIN_TEST_SECRET,undefined); assert.equal(env.NODE_OPTIONS,undefined); assert.equal(env.DOTNET_MULTILEVEL_LOOKUP,'0'); }
  finally { if(saved===undefined) delete process.env.ODIN_TEST_SECRET; else process.env.ODIN_TEST_SECRET=saved; if(options===undefined) delete process.env.NODE_OPTIONS; else process.env.NODE_OPTIONS=options; }
});

test('existing hardlinked host configuration is rejected without mutating either name', t => {
  const f=fixture(t), original=path.join(f.base,'original-host.toml');
  fs.writeFileSync(original,toml.stringify({workspace:{allowed_roots:[f.project]}})); fs.linkSync(original,f.host);
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/regular unlinked file/);
  assert.equal(fs.readFileSync(f.host,'utf8'),fs.readFileSync(original,'utf8'));
});

test('Csharp runner and health disable SDK first-run PATH registration and certificate generation', t => {
  const f=fixture(t), keys=['DOTNET_ADD_GLOBAL_TOOLS_TO_PATH','DOTNET_GENERATE_ASPNET_CERTIFICATE'];
  const saved=Object.fromEntries(keys.map(key=>[key,process.env[key]]));
  for(const key of keys) process.env[key]='true';
  try {
    const state=path.join(f.base,'state'), env=backendEnvironment(f.payload(),state,'csharp');
    assert.equal(env.DOTNET_CLI_HOME,path.join(state,'lsp-cache','dotnet','home'));
    for(const key of keys) assert.equal(env[key],'false',`${key} must override host first-run settings`);
  } finally {
    for(const key of keys) if(saved[key]===undefined) delete process.env[key]; else process.env[key]=saved[key];
  }
});
