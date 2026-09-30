// SPDX-License-Identifier: Apache-2.0
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {createHash} = require('node:crypto');
const source = path.resolve(__dirname, '../../skills/builtin/skill-authoring');
const {validateSkill} = require(path.join(source, 'scripts/validate.cjs'));
function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-authoring-test-'));
  try {
    fs.writeFileSync(path.join(root, 'SKILL.md'), 'Synthetic task instructions\n');
    const descriptor = {schema_version:1,id:'fixture',version:'1.0.0',description:'Synthetic fixture',source:'vcp-original',license:'Apache-2.0',vcp_version:1,cues:[],environments:[],required_tools:[],body:{path:'SKILL.md',sha256:createHash('sha256').update(fs.readFileSync(path.join(root,'SKILL.md'))).digest('hex')},resources:[]};
    const save = () => fs.writeFileSync(path.join(root, 'skill.json'), JSON.stringify(descriptor));
    save(); run(root,descriptor,save);
  } finally { fs.rmSync(root,{recursive:true,force:true}); }
}
test('valid native package passes without changing its bytes; content changes fail',()=>fixture((root,d,save)=>{
  const before=fs.readFileSync(path.join(root,'skill.json'));
  assert.equal(validateSkill(root).id,'fixture');
  assert.deepEqual(fs.readFileSync(path.join(root,'skill.json')),before);
  fs.appendFileSync(path.join(root,'SKILL.md'),'tamper');
  assert.throws(()=>validateSkill(root),/hash mismatch/);
}));
test('rejects unknown authority, missing metadata and UTF-8 byte overflow',()=>fixture((root,d,save)=>{
  d.authority=true; save(); assert.throws(()=>validateSkill(root),/descriptor field/); delete d.authority;
  delete d.version; save(); assert.throws(()=>validateSkill(root),/descriptor field/); d.version='1.0.0';
  d.description='é'.repeat(513); save(); assert.throws(()=>validateSkill(root),/metadata text/);
}));
test('accepts context/file resource roles and rejects other roles or a file-role body',()=>fixture((root,d,save)=>{
  fs.writeFileSync(path.join(root,'helper.py'),'print(1)\n');
  const helper={path:'helper.py',sha256:createHash('sha256').update(fs.readFileSync(path.join(root,'helper.py'))).digest('hex')};
  d.resources=[{...helper,use:'file'}]; save(); assert.equal(validateSkill(root).files,3);
  d.resources=[{...helper,use:'context'}]; save(); assert.equal(validateSkill(root).files,3);
  d.resources=[{...helper,use:'execute'}]; save(); assert.throws(()=>validateSkill(root),/resource role/);
  d.resources=[]; d.body.use='file'; save(); assert.throws(()=>validateSkill(root),/body must be context/);
}));
test('rejects duplicate matching values and dot-leading ids; warns on softer issues',()=>fixture((root,d,save)=>{
  d.cues=['Cargo.toml','Cargo.toml']; save(); assert.throws(()=>validateSkill(root),/duplicate matching/);
  d.cues=[];
  for(const id of ['.','..','.hidden']) { d.id=id; save(); assert.throws(()=>validateSkill(root),/skill name/); }
  d.id='fixture'; d.version='v1'; d.description='Synthetic fixture'; save();
  fs.writeFileSync(path.join(root,'stray.txt'),'undeclared');
  const {warnings}=validateSkill(root);
  for(const expected of [/directory name differs/,/not semantic/,/Use when/,/Undeclared file: stray.txt/]) assert(warnings.some(w=>expected.test(w)),String(expected));
  d.version='1.0.0'; d.description='Synthetic fixture. Use when testing.'; save(); fs.unlinkSync(path.join(root,'stray.txt'));
  assert.deepEqual(validateSkill(root).warnings.filter(w=>!/directory name/.test(w)),[]);
}));
test('reports a missing descriptor clearly',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-authoring-missing-'));
  try { assert.throws(()=>validateSkill(root),/Missing skill.json/); } finally { fs.rmSync(root,{recursive:true,force:true}); }
});
test('rejects traversal, case aliases and descriptor aliases',()=>fixture((root,d,save)=>{
  for(const p of ['../SKILL.md','/SKILL.md','CON.txt','skill.json','dir/../SKILL.md']) {
    d.body.path=p; save(); assert.throws(()=>validateSkill(root));
  }
  d.body.path='SKILL.md'; d.resources=[{...d.body,path:'skill.md'}];save();assert.throws(()=>validateSkill(root),/Duplicate/);
}));
test('rejects linked resources without reading the outside target',()=>fixture((root,d,save)=>{
  const outside=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-authoring-outside-'));
  try {
    fs.writeFileSync(path.join(outside,'SKILL.md'),'outside');
    fs.symlinkSync(outside,path.join(root,'linked'),process.platform==='win32'?'junction':'dir');
    d.body.path='linked/SKILL.md';save();assert.throws(()=>validateSkill(root),/Linked/);
    assert.equal(fs.readFileSync(path.join(outside,'SKILL.md'),'utf8'),'outside');
  } finally { fs.rmSync(outside,{recursive:true,force:true}); }
}));
const sha = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const add = (root, d, relative, content, use) => {
  fs.mkdirSync(path.dirname(path.join(root, relative)), {recursive: true});
  fs.writeFileSync(path.join(root, relative), content);
  d.resources.push({path: relative, sha256: sha(path.join(root, relative)), ...(use ? {use} : {})});
};
const warned = (root, pattern) => validateSkill(root).warnings.some(w => pattern.test(w));
test('runtime parity: descriptor bound, UTF-8 body, reference role and missing resources', () => fixture((root, d, save) => {
  add(root, d, 'references/deep.md', 'On-demand guidance\n', 'reference');
  d.description = 'Synthetic fixture. Use when testing.';
  save(); assert.deepEqual(validateSkill(root).warnings.filter(w => !/directory name/.test(w)), []);
  d.description = 'Synthetic fixture. Use when testing. ' + 'x'.repeat(900);
  d.license = 'Apache-2.0'; d.source = 'vcp-original ' + 'y'.repeat(2000);
  save(); assert.ok(fs.statSync(path.join(root, 'skill.json')).size <= 16 * 1024);
  d.source = 'vcp-original'; d.description = 'Synthetic fixture. Use when testing.';
  const big = JSON.parse(JSON.stringify(d)); big.environments = Array.from({length: 32}, (_, i) => 'e'.repeat(120) + i);
  fs.writeFileSync(path.join(root, 'skill.json'), JSON.stringify(big) + ' '.repeat(16 * 1024));
  assert.throws(() => validateSkill(root), /16 KiB/);
  save();
  fs.writeFileSync(path.join(root, 'SKILL.md'), Buffer.from([0xff, 0xfe, 0x00]));
  d.body.sha256 = sha(path.join(root, 'SKILL.md')); save();
  assert.throws(() => validateSkill(root), /body must be UTF-8/);
  fs.writeFileSync(path.join(root, 'SKILL.md'), 'Synthetic task instructions\n');
  d.body.sha256 = sha(path.join(root, 'SKILL.md'));
  d.resources.push({path: 'references/missing.md', sha256: 'a'.repeat(64)}); save();
  assert.throws(() => validateSkill(root), /Missing resource: references\/missing.md/);
}));
test('warnings: unemittable cues, unknown vcp tools, unmaterializable helpers, broken links, non-UTF-8 context', () => fixture((root, d, save) => {
  d.cues = ['Makefile']; d.required_tools = ['vcp_read', 'vcp_teleport', 'python']; save();
  assert.ok(warned(root, /never emitted by the host.*Makefile/));
  assert.ok(warned(root, /Unknown VCP tool.*vcp_teleport/));
  assert.ok(!warned(root, /python/), 'profile names are not VCP tool names');
  d.cues = ['Cargo.toml', '*.csproj']; d.required_tools = ['vcp_read']; save();
  assert.ok(!warned(root, /never emitted/));
  add(root, d, 'scripts/crlf.py', 'print(1)\r\n', 'file');
  add(root, d, 'scripts/no-newline.py', 'print(1)', 'file');
  add(root, d, 'LICENSE.txt', 'license text without newline', 'file');
  add(root, d, 'references/binary.dat', Buffer.from([0xff, 0xfe]));
  fs.writeFileSync(path.join(root, 'SKILL.md'), 'See [guide](references/guide.md) and [site](https://example.invalid/x).\n');
  d.body.sha256 = sha(path.join(root, 'SKILL.md')); save();
  assert.ok(warned(root, /cannot be materialized.*scripts\/crlf.py/));
  assert.ok(warned(root, /cannot be materialized.*scripts\/no-newline.py/));
  assert.ok(!warned(root, /LICENSE.txt/), 'licenses are not helpers');
  assert.ok(warned(root, /not UTF-8.*references\/binary.dat/));
  assert.ok(warned(root, /undeclared file: references\/guide.md/));
  assert.ok(!warned(root, /example.invalid/));
  const result = validateSkill(root);
  assert.equal(result.context_bytes, fs.statSync(path.join(root, 'SKILL.md')).size + 2);
}));
test('CLI exits 0 when clean, 2 with warnings and 1 on errors', () => fixture((root, d, save) => {
  const {spawnSync} = require('node:child_process');
  const run = directory => spawnSync(process.execPath, [path.join(source, 'scripts/validate.cjs'), directory], {encoding: 'utf8'}).status;
  const clean = path.join(path.dirname(root), 'fixture');
  d.description = 'Synthetic fixture. Use when testing.'; save();
  fs.rmSync(clean, {recursive: true, force: true}); fs.cpSync(root, clean, {recursive: true});
  try { assert.equal(run(clean), 0); } finally { fs.rmSync(clean, {recursive: true, force: true}); }
  assert.equal(run(root), 2, 'directory name differs from id');
  fs.writeFileSync(path.join(root, 'SKILL.md'), 'tampered\n');
  assert.equal(run(root), 1);
}));
test('validator marker list matches the host marker contract', () => {
  const {ROOT_MARKERS, ROOT_PATTERN_EXTENSIONS} = require(path.join(source, 'scripts/validate.cjs'));
  const shared = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../skills/markers.json'), 'utf8'));
  assert.deepEqual(ROOT_MARKERS, shared.root_markers);
  assert.deepEqual(ROOT_PATTERN_EXTENSIONS, shared.root_pattern_extensions);
});
