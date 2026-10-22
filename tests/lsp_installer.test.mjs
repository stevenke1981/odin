// Host-configuration and payload guards only. This is not real LSP/installer acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { verifyPayload, buildPlan, applyPlan, backendEnvironment, parseArgs, LUA_SETTINGS, hostHomePaths, runnerHomeContext, throwSetupFailure } from '../packaging/setup-lsps.mjs';

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
  const entries = ['node/node.exe','node_modules/basedpyright/index.js','node_modules/basedpyright/langserver.index.js','node_modules/typescript-language-server/lib/cli.mjs','node_modules/typescript/lib/tsserver.js','node_modules/typescript/lib/tsc.js','node_modules/bash-language-server/out/cli.js','go/bin/go.exe','go/src/runtime/runtime.go','gopls/gopls.exe','dotnet/dotnet.exe','dotnet/sdk/10.0.402/MSBuild.dll','csharp/CSharpLanguageServer.dll','lua/bin/lua-language-server.exe','lua/script/config.lua','slint/slint-lsp.exe','clangd/bin/clangd.exe','taplo/taplo.exe','node_modules/yaml-language-server/bin/yaml-language-server','marksman/marksman.exe','ruff/ruff.exe','ty/ty.exe','rust-analyzer/rust-analyzer.exe','node_modules/@vtsls/language-server/bin/vtsls.js'];
  for (const entry of entries) { const name = path.join(backendRoot, entry); fs.mkdirSync(path.dirname(name), { recursive: true }); fs.writeFileSync(name, 'inert fixture: '+entry); }
  const manifest = { schema_version: 1, target: 'x86_64-pc-windows-msvc', components: ['node','basedpyright','typescript-language-server','typescript','bash-language-server','go','gopls','dotnet','csharp-ls','lua-language-server','slint-lsp','clangd','taplo','yaml-language-server','marksman','ruff','ty','rust-analyzer','vtsls'].map(id => ({id, version:'fixture-only'})), files: [] };
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
test('fresh plan is inert and contains fourteen fixed profiles without write/shell/debug grants', t => {
  const f=fixture(t), prepared=buildPlan(f.options(),f.payload());
  assert.equal(prepared.plan.status,'READY'); assert.equal(prepared.plan.before_sha256,'absent');
  assert.deepEqual(prepared.config.workspace.allowed_roots,[fs.realpathSync.native(f.project)]);
  assert.deepEqual(new Set(prepared.config.workspace.languages.map(x=>x.id)),new Set(['python','go','csharp','lua','javascript','typescript','bash','slint','c','cpp','toml','yaml','markdown','rust']));
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
test('explicit replacement replaces only requested bundled language profiles', t => {
  const f=fixture(t), custom={id:'python',extensions:['py'],program:'C:/custom/python-lsp.exe'}, rust={id:'rust',extensions:['rs'],program:'rust-analyzer'};
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[custom,rust]}}));
  const p=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),f.payload());
  assert.equal(p.plan.status,'READY'); assert.equal(p.plan.changes.find(x=>x.language==='python').action,'replace-explicit');
  assert.equal(hashObject(p.config.workspace.languages.find(x=>x.id==='rust')),hashObject(rust));
});
test('CAS apply creates sidecars, backup and all fourteen host profiles', t => {
  const f=fixture(t); fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project]}}));
  const original=fs.readFileSync(f.host), p=buildPlan(f.options({workspaceRoots:[]}),f.payload());
  const result=applyPlan(p,p.plan.before_sha256,p.plan.plan_sha256);
  assert.equal(result.status,'PASS'); assert.equal(result.backups.length,1); assert.deepEqual(fs.readFileSync(result.backups[0].path),original);
  assert.equal(toml.parse(fs.readFileSync(f.host,'utf8')).workspace.languages.length,14);
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

function homeFixture(t) {
  const f=fixture(t), home=path.join(f.base,'home space \u4e2d\u6587'); fs.mkdirSync(home);
  const saved={USERPROFILE:process.env.USERPROFILE,HOME:process.env.HOME};
  process.env.USERPROFILE=home; process.env.HOME=home;
  t.after(()=>{for(const [key,value] of Object.entries(saved)) if(value===undefined)delete process.env[key];else process.env[key]=value;});
  const host=path.join(home,'.config','odin','host.toml'); fs.mkdirSync(path.dirname(host),{recursive:true});
  return {...f,home,homeHost:host,homeOptions:(more={})=>({hostConfig:host,workspaceRoots:[home],...more})};
}
test('standard home default has no ancestor project registration and protects private trees',t=>{
  const f=homeFixture(t),p=buildPlan(f.homeOptions(),f.payload());
  assert.equal(hostHomePaths().home,fs.realpathSync.native(f.home));
  assert.equal(p.config.workspace.default_root,f.home); assert.deepEqual(p.config.workspace.allowed_roots,[f.home]);
  assert.equal(p.config.projects,undefined);
  for(const privatePath of [path.dirname(f.homeHost),p.plan.state_dir,f.installRoot,path.join(f.home,'.codex'),path.join(f.home,'.ssh'),path.join(f.home,'AppData')]) assert.ok(p.config.workspace.protected_roots.includes(privatePath),privatePath);
  applyPlan(p,'absent'); const stored=toml.parse(fs.readFileSync(f.homeHost,'utf8'));
  assert.equal(stored.workspace.default_root,f.home); assert.equal(stored.workspace.languages.length,14);
  assert.equal(runnerHomeContext({hostConfig:f.homeHost},f.payload(),f.home,p.plan.state_dir),true);
  assert.equal(runnerHomeContext({hostConfig:f.homeHost},f.payload(),f.project,p.plan.state_dir),false);
  assert.equal(runnerHomeContext({},f.payload(),f.home,p.plan.state_dir),false);
  assert.equal(runnerHomeContext({hostConfig:f.homeHost},f.payload(),f.home,path.join(f.home,'arbitrary-cache')),false);
});
test('home cannot make repository host files or private roots trusted',t=>{
  const f=homeFixture(t);
  assert.throws(()=>buildPlan(f.homeOptions({hostConfig:path.join(f.home,'repo','host.toml')}),f.payload()),/outside selected/);
  const hostDir=path.dirname(f.homeHost);
  assert.throws(()=>buildPlan(f.homeOptions({workspaceRoots:[f.home,hostDir]}),f.payload()),/outside selected|protected host/);
  fs.writeFileSync(f.homeHost,toml.stringify({workspace:{allowed_roots:[f.home],default_root:f.home,protected_roots:[]}}));
  assert.throws(()=>buildPlan(f.homeOptions({workspaceRoots:[path.join(f.home,'.config')]}),f.payload()),/outside selected|protected host/);
  assert.throws(()=>buildPlan(f.homeOptions({workspaceRoots:[f.base]}),f.payload()),/outside selected/);
});
test('home installation and host state are protected without granting writes',t=>{
  const f=homeFixture(t),installation=path.join(f.home,'AppData','Local','Programs','Odin'); fs.mkdirSync(path.dirname(installation),{recursive:true}); fs.renameSync(f.installRoot,installation);
  const payload=verifyPayload(installation),p=buildPlan(f.homeOptions(),payload);
  assert.ok(p.config.workspace.protected_roots.includes(installation)); assert.equal(p.config.workspace.allow_write,undefined);
  const root=path.join(installation,'backends'); assert.throws(()=>buildPlan(f.homeOptions({workspaceRoots:[f.home,root]}),payload),/outside selected|protected/);
});
test('existing configuration without a new root retains its authorized roots and default',t=>{
  const f=homeFixture(t),existing={workspace:{allowed_roots:[f.project]}};fs.writeFileSync(f.homeHost,toml.stringify(existing));
  const p=buildPlan(f.homeOptions({workspaceRoots:[]}),f.payload());
  assert.deepEqual(p.config.workspace.allowed_roots,[f.project]); assert.equal(p.config.workspace.default_root,undefined);
  assert.ok(!p.config.workspace.allowed_roots.includes(f.home));
});
test('default_root cannot name another directory or rely on repository host trust',t=>{
  const f=homeFixture(t);fs.writeFileSync(f.homeHost,toml.stringify({workspace:{allowed_roots:[f.home],default_root:f.project}}));
  assert.throws(()=>buildPlan(f.homeOptions({workspaceRoots:[]}),f.payload()),/default_root/);
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],default_root:f.home}}));
  assert.throws(()=>buildPlan(f.options({workspaceRoots:[]}),f.payload()),/default_root/);
});
test('full payload streaming verification checks cancellation within large files',t=>{
  const f=fixture(t),large=path.join(f.backendRoot,'go','large-source');fs.writeFileSync(large,Buffer.alloc(4*1024*1024,7));f.refresh();
  let chunks=0;const monitor={checkpoint(){},guard(){if(++chunks===2){const e=new Error('cancelled during stream');e.setupExitCode=1223;throw e;}}};
  assert.throws(()=>verifyPayload(f.installRoot,null,monitor),/cancelled during stream/);assert.equal(chunks,2);assert.equal(fs.existsSync(f.host),false);
});
test('progress-only arguments cannot be used by a runtime backend',()=>{
  assert.throws(()=>parseArgs(['--run-backend','go','--progress-file','C:/arbitrary']),/require --install/);
});

test('legacy host sibling projects remain valid without implicitly enabling full home policy',t=>{
  const f=fixture(t),sibling=path.join(path.dirname(f.host),'projects','a');fs.mkdirSync(sibling,{recursive:true});
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[sibling]},projects:{registered:{root:sibling,allow_write:true}}}));
  const before=fs.readFileSync(f.host),p=buildPlan(f.options({workspaceRoots:[]}),f.payload());
  assert.equal(p.plan.status,'READY');assert.deepEqual(p.config.workspace.allowed_roots,[sibling]);
  assert.equal(p.config.workspace.default_root,undefined);assert.equal(p.config.workspace.protected_roots,undefined);
  assert.equal(p.config.projects.registered.allow_write,true);assert.deepEqual(fs.readFileSync(f.host),before);
});

test('receipt I/O failure preserves the original cancellation and deadline errors',t=>{
  const f=fixture(t),blocked=path.join(f.base,'not-a-directory');fs.writeFileSync(blocked,'operator content');
  for(const code of [1223,1460]) {
    const original=new Error('original interrupted setup');original.setupExitCode=code;original.phase=code===1223?'cancelled':'timeout';
    assert.throws(()=>throwSetupFailure(original,path.join(blocked,'receipt.json'),{health:{status:'NOT_RUN'}}),error=>error===original && error.setupExitCode===code);
  }
  assert.equal(fs.readFileSync(blocked,'utf8'),'operator content');
});


test('explicit alternative backend choices install one provider per extension and persist on reinstall', t => {
  const f=fixture(t), first=buildPlan(f.options({pythonBackend:'ty',typescriptBackend:'vtsls'}),f.payload());
  assert.deepEqual(first.plan.selected_backends,{python:'ty',typescript:'vtsls'});
  const profiles=first.config.workspace.languages, py=profiles.find(x=>x.id==='python');
  assert.equal(py.program,path.join(f.backendRoot,'ty','ty.exe')); assert.deepEqual(py.args,['server']);
  for(const id of ['javascript','typescript']) {const p=profiles.find(x=>x.id===id);assert.ok(p.args.some(x=>x.endsWith(path.join('@vtsls','language-server','bin','vtsls.js'))));assert.equal(p.settings.vtsls.autoUseWorkspaceTsdk,false);assert.equal(p.settings.vtsls.typescript.globalTsdk,path.join(f.backendRoot,'node_modules','typescript','lib'));}
  const owners=new Map();for(const p of profiles)for(const ext of p.extensions){assert.equal(owners.has(ext.toLowerCase()),false,ext);owners.set(ext.toLowerCase(),p.id);}
  applyPlan(first,'absent');
  const reinstall=buildPlan(f.options({workspaceRoots:[]}),f.payload());assert.deepEqual(reinstall.plan.selected_backends,first.plan.selected_backends);
  assert.equal(reinstall.config.workspace.languages.find(x=>x.id==='python').program,py.program);
  const switched=buildPlan(f.options({workspaceRoots:[],pythonBackend:'ruff',typescriptBackend:'typescript-language-server'}),f.payload());
  assert.equal(switched.plan.status,'READY');assert.equal(switched.plan.changes.find(x=>x.language==='python').action,'update-managed');
  const ruff=switched.config.workspace.languages.find(x=>x.id==='python');assert.deepEqual(ruff.args,['server','--untrusted-workspace','--isolated']);assert.equal(ruff.initialization_options.settings.configurationPreference,'editorOnly');
  assert.match(switched.plan.backend_limits.python,/semantic navigation.*not supplied/);
  for(const grant of ['allow_write','allow_shell','allow_debug','allow_inspections','allow_formatters'])assert.equal(switched.config.workspace[grant],undefined);
});

test('backend selections are enumerated and accepted only for configuration modes',()=>{
  for(const value of ['auto','python.exe','basedpyright,ruff']) assert.throws(()=>parseArgs(['--plan','--python-backend',value]),/Choose --python-backend/);
  assert.throws(()=>parseArgs(['--plan','--typescript-backend','tsserver']),/Choose --typescript-backend/);
  assert.throws(()=>parseArgs(['--health','--python-backend','ruff']),/requires --plan/);
  assert.throws(()=>parseArgs(['--run-backend','go','--typescript-backend','vtsls']),/requires --plan/);
  assert.throws(()=>parseArgs(['--plan','--python-backend','ty','--python-backend','ruff']),/duplicate/);
  assert.equal(parseArgs(['--install','--python-backend','ruff']).pythonBackend,'ruff');
});

test('new profiles use fixed native and npm entry points with constrained Rust and schema behavior',t=>{
  const f=fixture(t),p=buildPlan(f.options(),f.payload());
  const languages=p.config.workspace.languages,byId=id=>languages.find(x=>x.id===id);
  assert.deepEqual(p.plan.selected_backends,{python:'basedpyright',typescript:'typescript-language-server'});
  for(const id of ['c','cpp'])assert.deepEqual(byId(id).args,['--enable-config=false']);
  assert.equal(byId('c').program,byId('cpp').program);assert.deepEqual(byId('c').extensions,['c','h']);
  assert.deepEqual(byId('taplo'),undefined);assert.deepEqual(byId('toml').args,['lsp','stdio']);
  assert.deepEqual(byId('markdown').args,['server']);assert.equal(byId('yaml').settings.yaml.schemaStore.enable,false);
  assert.equal(byId('toml').settings.evenBetterToml.schema.enabled,false);assert.deepEqual(byId('toml').settings.evenBetterToml.schema.catalogs,[]);assert.equal(byId('toml').settings.evenBetterToml.taplo.configFile.enabled,false);
  assert.equal(byId('rust').program,path.join(f.backendRoot,'rust-analyzer','rust-analyzer.exe'));
  assert.equal(byId('rust').settings['rust-analyzer'].cargo.buildScripts.enable,false);assert.equal(byId('rust').settings['rust-analyzer'].procMacro.enable,false);
});

test('Rust configurations with custom ids and operator edits are preserved even with explicit replacement',t=>{
  const f=fixture(t),custom={id:'custom_rust',extensions:['RS'],program:'C:/operator/rust.exe',args:['--fixed']};
  fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[custom]}}));
  const p=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),f.payload());
  assert.equal(p.plan.status,'READY');assert.equal(p.config.workspace.languages.some(x=>x.id==='rust'),false);
  assert.equal(hashObject(p.config.workspace.languages.find(x=>x.id===custom.id)),hashObject(custom));
});

test('missing expanded components or entry points fail before host planning',t=>{
  const f=fixture(t),manifest=path.join(f.backendRoot,'manifest.json');
  f.manifest.components=f.manifest.components.filter(x=>x.id!=='ruff');fs.writeFileSync(manifest,JSON.stringify(f.manifest));assert.throws(()=>f.payload(),/Missing required payload component: ruff/);
  f.manifest.components.push({id:'ruff',version:'fixture-only'});fs.unlinkSync(path.join(f.backendRoot,'ty','ty.exe'));f.refresh();assert.throws(()=>f.payload(),/Missing required payload entry: ty/);
});


test('only unedited managed Rust profiles can follow a moved installation',t=>{
  const f=fixture(t),first=buildPlan(f.options(),f.payload());applyPlan(first,'absent');
  const installed=f.payload(),moved={...installed,root:path.join(f.base,'moved install'),backendRoot:path.join(f.base,'moved install','backends')};
  fs.cpSync(installed.root,moved.root,{recursive:true});
  const update=buildPlan(f.options({workspaceRoots:[]}),moved);
  assert.equal(update.plan.status,'READY');assert.equal(update.plan.changes.find(x=>x.language==='rust').action,'update-managed');
  assert.equal(update.config.workspace.languages.find(x=>x.id==='rust').program,path.join(moved.backendRoot,'rust-analyzer','rust-analyzer.exe'));
  const config=toml.parse(fs.readFileSync(f.host,'utf8'));config.workspace.languages.find(x=>x.id==='rust').args=['--operator'];fs.writeFileSync(f.host,toml.stringify(config));
  const preserve=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),moved);
  assert.deepEqual(preserve.config.workspace.languages.find(x=>x.id==='rust').args,['--operator']);
  assert.equal(preserve.config.workspace.languages.find(x=>x.id==='rust').program,path.join(installed.backendRoot,'rust-analyzer','rust-analyzer.exe'));
});


test('an explicitly selected alternative cannot replace an operator edit without reviewed replacement',t=>{
  const f=fixture(t),first=buildPlan(f.options({pythonBackend:'ty'}),f.payload());applyPlan(first,'absent');
  const config=toml.parse(fs.readFileSync(f.host,'utf8'));config.workspace.languages.find(x=>x.id==='python').args=['server','operator-change'];fs.writeFileSync(f.host,toml.stringify(config));
  const before=fs.readFileSync(f.host),plan=buildPlan(f.options({workspaceRoots:[],pythonBackend:'ruff'}),f.payload());
  assert.equal(plan.plan.status,'CONFLICT');assert.ok(plan.plan.collisions.some(row=>row.language==='python'));
  assert.throws(()=>applyPlan(plan,plan.plan.before_sha256),/Custom language profiles/);assert.deepEqual(fs.readFileSync(f.host),before);
});


test('preserved Rust profiles cannot shadow another bundled extension or be overwritten through its id',t=>{
  const f=fixture(t);
  for(const rust of [{id:'rust',extensions:['rs','toml'],program:'C:/operator/rust.exe'},{id:'python',extensions:['rs','py'],program:'C:/operator/rust.exe'}]) {
    fs.writeFileSync(f.host,toml.stringify({workspace:{allowed_roots:[f.project],languages:[rust]}}));
    const original=fs.readFileSync(f.host),plan=buildPlan(f.options({workspaceRoots:[],replaceLanguageProfiles:true}),f.payload());
    assert.equal(plan.plan.status,'CONFLICT');assert.ok(plan.plan.collisions.some(row=>row.language===rust.id));
    assert.equal(hashObject(plan.config.workspace.languages.find(row=>row.id===rust.id)),hashObject(rust));
    assert.throws(()=>applyPlan(plan,plan.plan.before_sha256),/Custom language profiles/);assert.deepEqual(fs.readFileSync(f.host),original);
  }
});
