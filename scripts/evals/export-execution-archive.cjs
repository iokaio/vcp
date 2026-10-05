// SPDX-License-Identifier: Apache-2.0
'use strict';
// EE-00c/06: read selected diagnostic captures through the ordinary scoped CLI.
// Never open spool files or reconstruct bytes omitted at capture time.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const {isDeepStrictEqual: equal} = require('node:util');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const schemas = new Set(['context-manifest/1','responses-request/1','openrouter-normalized-response/1',
  'coding-allocation-observation/2','execution-completion-repair/1','verification-result/1',
  'canonical-compaction-projection/1','vcp-process-outcome-v1']);
function privacyGap(gap, id) {
  if (gap.artifact !== id || !Array.isArray(gap.omissions) || !gap.omissions.length ||
      new Set(gap.omissions).size !== gap.omissions.length ||
      gap.omissions.some(value => !['authentication_headers','recovery_material'].includes(value))) return false;
  if (gap.visibility === 'omitted') return gap.capture_state === 'complete' && gap.range === undefined &&
    gap.reason === 'only retained observed bytes are available; not reconstructed';
  return gap.visibility === 'redacted' && gap.range === null &&
    gap.reason === 'excluded at capture boundary; no retained byte offsets exist';
}
function rangeBytes(page, bundle, descriptor, offset, length) {
  const id = descriptor.spec.id, end = Math.min(offset + length, Number(descriptor.length));
  if (!equal(page.scope, bundle.task.scope) || page.source_watermark !== bundle.source_watermark ||
      page.next_cursor || !Array.isArray(page.items) || page.items.length !== 1 ||
      !Array.isArray(page.gaps) || page.gaps.some(gap => !privacyGap(gap,id))) throw Error('Range scope, cut or visibility differs');
  const row = page.items[0];
  if (row.artifact !== id || row.visibility !== 'available' || row.range?.start !== offset || row.range?.end !== end ||
      !Array.isArray(row.bytes) || row.bytes.length !== end - offset ||
      row.bytes.some(value => !Number.isInteger(value) || value < 0 || value > 255)) throw Error('Invalid artifact range');
  return Buffer.from(row.bytes);
}
function selected(bundle) {
  if (bundle.schema_version !== 1 || bundle.kind !== 'inspection_bundle' || !bundle.task?.scope?.task) throw Error('Inspection bundle required');
  const records = new Map();
  for (const [view,pages] of Object.entries(bundle.views ?? {})) {
    if (!['costs','outputs','policy','routing','tools','verification','context','prompts'].includes(view)) continue;
    for (const page of pages) for (const item of page.items ?? []) {
      const descriptor = item.record;
      if (item.collection !== 'artifact' || item.visibility !== 'available' || !descriptor || !schemas.has(descriptor.spec?.schema)) continue;
      const id = descriptor.spec.id, size = Number(descriptor.length);
      if (!/^[A-Za-z0-9_-]+$/.test(id) || item.id !== id || descriptor.state !== 'complete' ||
          !equal(descriptor.spec.scope,bundle.task.scope) || !Number.isSafeInteger(size) || size < 1 || size > 4 * 1024 * 1024 ||
          !/^[a-f0-9]{64}$/.test(descriptor.sha256)) throw Error('Unsupported diagnostic descriptor');
      if (records.has(id) && !equal(records.get(id).descriptor,descriptor)) throw Error('Conflicting diagnostic descriptor');
      records.set(id,{descriptor,view});
    }
  }
  const rows = [...records.values()];
  if (!rows.length || rows.length > 4096 || rows.reduce((sum,row) => sum + Number(row.descriptor.length),0) > 64 * 1024 * 1024) throw Error('Diagnostic archive bound exceeded or empty');
  return rows;
}
function exportArchive({executable,workspace,data,bundleFile,directory}, invoke) {
  if (fs.statSync(bundleFile).size > 32 * 1024 * 1024) throw Error('Bundle exceeds read bound');
  const bundleBytes = fs.readFileSync(bundleFile);
  if (bundleBytes.length > 32 * 1024 * 1024) throw Error('Bundle exceeds read bound');
  const bundle = JSON.parse(bundleBytes), rows = selected(bundle);
  fs.mkdirSync(directory); // Exclusive destination: preserve every previous attempt.
  fs.mkdirSync(path.join(directory,'artifacts'));
  fs.mkdirSync(path.join(directory,'inspection'));
  fs.writeFileSync(path.join(directory,'inspection-bundle.json'),bundleBytes,{flag:'wx'});
  const artifacts = [], inspections = [];
  for (const {descriptor,view} of rows) {
    const id = descriptor.spec.id, chunks = [];
    for (let offset = 0; offset < Number(descriptor.length); offset += 65536) {
      const args = ['--format','jsonl','--non-interactive','--workspace',workspace,'--data-dir',data,
        'inspect',id,'--view',view,'--offset',String(offset),'--length','65536'];
      const raw = invoke(executable,args);
      const inspectionPath = `inspection/${id}-${offset}.jsonl`;
      fs.writeFileSync(path.join(directory,inspectionPath),raw,{flag:'wx'});
      const frames = raw.toString('utf8').split(/\r?\n/).filter(line => line.trim()).map(line => JSON.parse(line));
      const results = frames.filter(row => row.type === 'result');
      if (frames.length !== 1 || results.length !== 1 || results[0].schema_version !== 1 ||
          typeof results[0].correlation !== 'string' || !results[0].correlation.length ||
          results[0].exit_code !== 0 || (results[0].scope != null && !equal(results[0].scope,bundle.task.scope))) throw Error('Artifact inspection failed');
      chunks.push(rangeBytes(results[0].data,bundle,descriptor,offset,65536));
      inspections.push({path:inspectionPath,bytes:raw.length,sha256:hash(raw),artifact:id,offset});
    }
    const bytes = Buffer.concat(chunks);
    if (hash(bytes) !== descriptor.sha256) throw Error('Artifact hash differs');
    const relative = `artifacts/${id}.bin`;
    fs.writeFileSync(path.join(directory,relative),bytes,{flag:'wx'});
    artifacts.push({descriptor,path:relative,bytes:bytes.length,sha256:hash(bytes)});
  }
  const manifest = {schema_version:1,scope:bundle.task.scope,backend:bundle.store_diagnostics?.backend ?? 'not_recorded',
    fixture:'live-scoped-diagnostic-selection',selection:[...schemas],
    bundle:{path:'inspection-bundle.json',bytes:bundleBytes.length,sha256:hash(bundleBytes)},artifacts,inspections};
  const bytes = Buffer.from(JSON.stringify(manifest,null,2)+'\n');
  fs.writeFileSync(path.join(directory,'manifest.json'),bytes,{flag:'wx'});
  fs.writeFileSync(path.join(directory,'manifest.sha256'),hash(bytes)+'\n',{flag:'wx'});
  return {artifacts:artifacts.length,bytes:artifacts.reduce((sum,row) => sum + row.bytes,0)};
}
module.exports = {selected,rangeBytes,exportArchive};
if (require.main === module) {
  try {
    const [executable,workspace,data,bundleFile,directory,...extra] = process.argv.slice(2);
    if (!directory || extra.length) throw Error('Usage: node export-execution-archive.cjs <vcp> <workspace> <data> <bundle.json> <new-directory>');
    const result = exportArchive({executable,workspace,data,bundleFile,directory},(exe,args) =>
      execFileSync(exe,args,{windowsHide:true,maxBuffer:16*1024*1024,timeout:1800000,stdio:['ignore','pipe','pipe']}));
    process.stdout.write(JSON.stringify(result)+'\n');
  } catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode=1; }
}
