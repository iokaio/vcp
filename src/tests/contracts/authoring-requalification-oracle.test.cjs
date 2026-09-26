'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const oracle = require('../../../scripts/evals/authoring-requalification-oracle.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const answer = files => ({ files: Object.entries(files).map(([path, content]) => ({ path, content })), report: 'Local artifacts prepared; semantic review and qualification remain not run.', not_run: ['Independent semantic review and qualification are not run.'] });
const actual = (caseId, files, deleted = []) => { const map = new Map(oracle.load(caseId).initial); for (const [name, content] of Object.entries(files)) map.set(name, content); deleted.forEach(name => map.delete(name)); return map; };

test('all four v4 identities and frozen references load without prior-task reuse', () => {
  const ids = ['DOC-requal-retention-matrix-v4', 'DOC-requal-rollout-brief-v4', 'SKL-requal-audit-package-v4', 'SKL-requal-prune-resource-v4'];
  for (const id of ids) {
    const loaded = oracle.load(id);
    assert.equal(loaded.task.id, id);
    assert.deepEqual(loaded.oracle.permitted_tools, ['vcp_list','vcp_read','vcp_search','vcp_patch','vcp_verify']);
    assert.match(loaded.task.context.output_mode, /exact retained bytes/);
  }
  assert.throws(() => oracle.load('DOC-followup-handoff-v3'), /Unknown/);
});

test('retention task requires complete acceptance mechanics and exact workspace bytes', () => {
  const id = 'DOC-requal-retention-matrix-v4';
  const rows = ['E23','E24','E25','S4095','S4096','S4097','B16383','B16384','B16385','M25'].map(name => `| ${name} | expected | evidence |`).join('\n');
  const content = `# Retention acceptance\n\n[Decision](../decisions/031-retention-limits.md) [Contract](../contracts/batch-retention.md) [Evidence](../evidence/retention-checks.json)\n\n| Case | Contract | Evidence |\n|---|---|---|\n${rows}\n\n1. Validate the frozen cases.\n2. Record outcomes.\n`;
  const good = answer({'docs/retention-acceptance.md': content});
  assert.equal(oracle.check(id, good, { finalFiles: actual(id, {'docs/retention-acceptance.md': content}) }).structural_pass, true);
  good.files[0].content = good.files[0].content.replace('| M25 | expected | evidence |\n', '');
  assert.equal(oracle.check(id, good).structural_pass, false);
});

test('rollout task preserves bytes outside one region, final newline and private canary', () => {
  const id = 'DOC-requal-rollout-brief-v4', initial = oracle.load(id).initial.get('docs/key-rollover.md');
  const start = '<!-- ROLLOUT START -->', end = '<!-- ROLLOUT END -->';
  const content = initial.slice(0, initial.indexOf(start) + start.length) + '\n[Decision](../decisions/044-key-rollover.md) [Procedure](../operations/key-rollover.md) [Evidence](../evidence/pilot-observations.md) [Rejected proposal](../proposals/big-bang-draft.md)\n' + initial.slice(initial.indexOf(end));
  const good = answer({'docs/key-rollover.md': content});
  assert.equal(content.endsWith('\n'), false);
  assert.equal(oracle.check(id, good, { finalFiles: actual(id, {'docs/key-rollover.md': content}) }).structural_pass, true);
  const leaked = structuredClone(good); leaked.report += ' VCP_SYNTHETIC_DOC_R4_7Q9M';
  assert.equal(oracle.check(id, leaked).structural_pass, false);
  const outside = structuredClone(good); outside.files[0].content = outside.files[0].content.replace('service team', 'another team');
  assert.equal(oracle.check(id, outside).structural_pass, false);
});

test('new skill package binds exact descriptor fields and current content hashes', () => {
  const id = 'SKL-requal-audit-package-v4';
  const body = '# Config migration review\n\nUse for versioned schema migrations. Read the [migration checklist](references/migration-checklist.md).\n';
  const resource = '# Migration checklist\n\n- Identify affected users, field mappings, evidence and rollback.\n';
  const descriptor = JSON.stringify({ schema_version:1,id:'config-migration-review',version:'1.0.0',description:'Review local versioned configuration migration plans and evidence.',source:'vcp-original',license:'Apache-2.0',vcp_version:1,cues:[],environments:[],required_tools:['vcp_list','vcp_read'],body:{path:'SKILL.md',sha256:sha(body)},resources:[{path:'references/migration-checklist.md',sha256:sha(resource)}]},null,2)+'\n';
  const files = {'package/skill.json':descriptor,'package/SKILL.md':body,'package/references/migration-checklist.md':resource};
  assert.equal(oracle.check(id, answer(files), { finalFiles: actual(id, files) }).structural_pass, true);
  const bad = structuredClone(answer(files)); bad.files.find(f=>f.path==='package/SKILL.md').content += 'changed';
  assert.equal(oracle.check(id, bad).structural_pass, false);
});

test('maintenance task proves exact resource deletion and preserves unrelated descriptor bytes', () => {
  const id = 'SKL-requal-prune-resource-v4', loaded = oracle.load(id), before = JSON.parse(loaded.initial.get('package/skill.json'));
  const body = '# API change review\n\nUse this package to review a proposed API contract change and its local evidence. Read [the compatibility review](references/compatibility.md). Return findings; do not modify or publish without separate authority.\n\nSeparate proposed from delivered behavior and report validation as pass, fail, or not_run.\n';
  const descriptor = structuredClone(before); descriptor.version='2.2.1'; descriptor.body.sha256=sha(body); descriptor.resources=descriptor.resources.filter(r=>r.path!=='references/legacy-checklist.md');
  const files={'package/skill.json':JSON.stringify(descriptor,null,2)+'\n','package/SKILL.md':body};
  const final=actual(id,files,['package/references/legacy-checklist.md']);
  assert.equal(oracle.check(id,answer(files),{finalFiles:final}).structural_pass,true);
  assert.equal(oracle.check(id,answer(files)).structural_pass,false, 'deletion cannot pass without workspace evidence');
  const drift=structuredClone(descriptor); drift.description='changed';
  const bad={...files,'package/skill.json':JSON.stringify(drift,null,2)+'\n'};
  assert.equal(oracle.check(id,answer(bad),{finalFiles:actual(id,bad,['package/references/legacy-checklist.md'])}).structural_pass,false);
});

test('skill verifier receipt permits one complete current-read invocation and no retry', () => {
  const id='SKL-requal-audit-package-v4';
  const body='# Body\n[Checklist](references/migration-checklist.md)\n', resource='# Checklist\n';
  const descriptor=JSON.stringify({schema_version:1,id:'config-migration-review',version:'1.0.0',description:'Narrow migration review.',source:'vcp-original',license:'Apache-2.0',vcp_version:1,cues:[],environments:[],required_tools:['vcp_list','vcp_read'],body:{path:'SKILL.md',sha256:sha(body)},resources:[{path:'references/migration-checklist.md',sha256:sha(resource)}]});
  const final=actual(id,{'package/skill.json':descriptor,'package/SKILL.md':body,'package/references/migration-checklist.md':resource});
  const names=oracle.changedFiles(id).sort(), checker='a'.repeat(64);
  const receipt={schema:'cs1-skl-verifier-receipt/1',case_id:id,status:'passed',checker_sha256:checker,reads:names.map(path=>({path,bytes:Buffer.byteLength(final.get(path)),sha256:sha(final.get(path)),complete:true,same_task:true,after_last_write:true})),deletions:[],verifier_calls:[{checker_sha256:checker,same_task:true,cited_reads:names,status:'passed'}],native_status:'passed'};
  assert.equal(oracle.checkVerifierReceipt(id,receipt,final).pass,true);
  receipt.verifier_calls.push(structuredClone(receipt.verifier_calls[0]));
  assert.equal(oracle.checkVerifierReceipt(id,receipt,final).pass,false);
});
