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
