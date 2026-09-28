// SPDX-License-Identifier: Apache-2.0
'use strict';
// Import authenticated historical reader-packet bytes only. Never execute them.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const { plain, read, within, privateDirectory, noParentInstructions } = require('./p6-live-runner.cjs').boundaries;
const oracle = require('./developer-oracle.cjs');
const { portable } = require('../skills/builtin-assets.cjs');
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const historical = Object.freeze({
  index: '8d83ebfa532baa05fcf5a125c3170ecea3173279d1582bf10988f392427a3579',
  mapping: 'a07c90eeec2fea42598c4b90d7f31077c8a5f8785ffdc7166b3606c876937da5',
  plan: '2744de3f29883d01d2e1c661d1b2d823bc85b805a9cc98c3dc2092a22800c716',
  result: 'd58b0e8d02cc5c47064b820f9ae0959dbfd7f85d183d62a99170bcab5824a473',
  grading: 'e7138f1b12be27d65a2d8f7b27091b5c550c181d856bff9bd6b45fbbc2e2c73a',
});
const caseIds = Object.freeze(['UI-normal-form-v2', 'UI-normal-results-v2', 'UI-boundary-states-v2', 'UI-hostile-tokens-v2', 'UI-missing-renderer-v2', 'UI-near-miss-parser-v2']);
const arms = ['none', 'nearest', 'candidate'], labels = ['A', 'B', 'C'];
function exact(value, keys) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || !equal(Object.keys(value).sort(), [...keys].sort())) throw Error('Historical object fields differ');
}
function textFiles(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length > 32) throw Error('Bounded historical file map required');
  const folded = new Set(); let bytes = 0;
  for (const [name, text] of Object.entries(value)) {
    portable(name);
    if (folded.has(name.toLowerCase()) || typeof text !== 'string' || text.includes('\0') || !text.isWellFormed() || Buffer.byteLength(text) > 65536) throw Error('Unsafe or colliding historical file');
    folded.add(name.toLowerCase()); bytes += Buffer.byteLength(text);
  }
  if (bytes > 262144) throw Error('Historical case byte bound');
}
// Caller-supplied commitments are supported only by this read-only validator for
// synthetic contract tests. The public staging entry point always uses historical.
function inspect(directory, mappingFile, commitments = historical) {
  directory = plain(path.resolve(directory)); mappingFile = plain(path.resolve(mappingFile));
  if (!fs.existsSync(directory) || !fs.existsSync(mappingFile)) throw Error('Required retained UI originals are unavailable');
  const names = fs.readdirSync(directory).sort(), wanted = ['index.json', ...caseIds.map(id => id + '.json')].sort();
  if (!equal(names, wanted)) throw Error('Retained packet inventory differs; only the index and six indexed packets are allowed');
  const indexBytes = read(path.join(directory, 'index.json'), 65536), mappingBytes = read(mappingFile, 65536);
  if (sha(indexBytes) !== commitments.index || sha(mappingBytes) !== commitments.mapping) throw Error('Historical packet index or mapping commitment differs');
  const index = JSON.parse(indexBytes), mapping = JSON.parse(mappingBytes);
  exact(index, ['schema', 'plan_sha256', 'block', 'result_sha256', 'grading_sha256', 'mapping_commitment', 'packets']);
  exact(mapping, ['schema', 'private', 'destination', 'index_sha256', 'salt', 'mapping']);
  if (index.schema !== 'cs-2-developer-reader-packets/1' || index.block !== 'frontend-design' || index.plan_sha256 !== commitments.plan || index.result_sha256 !== commitments.result || index.grading_sha256 !== commitments.grading
    || mapping.schema !== 'cs-2-developer-review-mapping/2' || mapping.index_sha256 !== commitments.index || typeof mapping.salt !== 'string' || !/^[a-f0-9]{64}$/.test(mapping.salt)
    || !Array.isArray(index.packets) || !equal(index.packets.map(row => row.case_id), caseIds) || !Array.isArray(mapping.mapping) || !equal(mapping.mapping.map(row => row.case_id), caseIds)
    || index.mapping_commitment !== sha(mapping.salt + JSON.stringify(mapping.mapping))) throw Error('Historical packet provenance or coverage differs');
  // mapping.destination is a historical locator, never an authority to read it.
  const inputFiles = [{ path: path.join(directory, 'index.json'), sha256: sha(indexBytes) }, { path: mappingFile, sha256: sha(mappingBytes) }], runs = [];
  for (const [number, item] of index.packets.entries()) {
    exact(item, ['case_id', 'sha256']);
    const packetFile = path.join(directory, item.case_id + '.json'), packetBytes = read(packetFile, 2 * 1024 * 1024);
    if (!/^[a-f0-9]{64}$/.test(item.sha256) || sha(packetBytes) !== item.sha256) throw Error('Historical reader packet changed');
    inputFiles.push({ path: packetFile, sha256: item.sha256 });
    const packet = JSON.parse(packetBytes), association = mapping.mapping[number], frozen = oracle.load(item.case_id);
    exact(association, ['case_id', ...arms]);
    if (!equal(arms.map(arm => association[arm]).sort(), labels)) throw Error('Historical arm mapping differs');
    exact(packet, ['schema', 'case_id', 'kind', 'prompt', 'sources', 'rubric', 'score_keys', 'hard_gates', 'halt_items', 'variants']);
    if (packet.schema !== 'cs-2-developer-reader-packet/1' || packet.case_id !== item.case_id || packet.kind !== frozen.task.kind || packet.prompt !== frozen.task.prompt || packet.rubric !== 'src/evals/skills/developer/rubric-v2.json'
      || !equal(packet.sources, Object.fromEntries(frozen.initial)) || !Array.isArray(packet.variants) || !equal(packet.variants.map(v => v.label).sort(), labels)) throw Error('Packet task, sources or variant inventory differs');
    textFiles(packet.sources);
    for (const arm of arms) {
      const variant = packet.variants.find(v => v.label === association[arm]);
      exact(variant, ['label', 'completed', 'final_files', 'report', 'not_run', 'checks']);
      textFiles(variant.final_files);
      if (typeof variant.completed !== 'boolean' || !equal(Object.keys(variant.final_files).sort(), [...frozen.oracle.allowed_modifications].sort())) throw Error('Retained final artifact inventory incomplete or expanded');
      exact(variant.checks, ['structural_oracle', 'in_run_checker', 'functional', 'synthetic_canary_disclosed']);
      if (typeof variant.checks.synthetic_canary_disclosed !== 'boolean' || ![variant.report, ...(variant.not_run || [])].every(v => v === null || typeof v === 'string') || variant.not_run !== null && !Array.isArray(variant.not_run)) throw Error('Historical variant report shape differs');
      const files = { ...packet.sources, ...variant.final_files };
      runs.push({ case_id: item.case_id, arm, label: variant.label, completed: variant.completed, checks: variant.checks,
        workspace: `cases/${item.case_id}--${arm}`, files: Object.entries(files).sort(([a], [b]) => a.localeCompare(b, 'en')).map(([name, content]) => ({ path: name, bytes: Buffer.byteLength(content), sha256: sha(content), provenance: Object.hasOwn(variant.final_files, name) ? 'packet-final-file' : 'packet-source', content })) });
    }
  }
  return { directory, mapping_file: mappingFile, input_files: inputFiles, runs };
}
function protect(directory) {
  if (process.platform !== 'win32') { fs.chmodSync(directory, 0o700); return; }
  const identity = execFileSync('whoami', ['/user', '/fo', 'csv', '/nh'], { encoding: 'utf8', windowsHide: true });
  const sid = identity.match(/S-1-5-\d+(?:-\d+)+/g);
  if (!sid || sid.length !== 1) throw Error('Private staging owner SID unavailable');
  execFileSync('icacls', [directory, '/inheritance:r', '/grant:r', `*${sid[0]}:(OI)(CI)F`, '*S-1-5-18:(OI)(CI)F'], { windowsHide: true, stdio: 'pipe' });
}
function prepare(directory, mappingFile, destination) {
  // No public bypass for commitments: absent/changed originals fail before writes.
  const imported = inspect(directory, mappingFile), output = plain(path.resolve(destination));
  if (fs.existsSync(output) || [repository, imported.directory, path.dirname(imported.mapping_file)].some(root => within(root, output) || within(output, root))) throw Error('New separate private staging directory required');
  privateDirectory(output); noParentInstructions(path.dirname(output));
  fs.mkdirSync(output, { mode: 0o700 }); protect(output);
  const runs = imported.runs.map(run => {
    for (const file of run.files) {
      const target = plain(path.join(output, run.workspace, file.path));
      fs.mkdirSync(path.dirname(target), { recursive: true, mode: 0o700 }); fs.writeFileSync(target, file.content, { flag: 'wx', mode: 0o600 });
      if (sha(read(target, 65536)) !== file.sha256) throw Error('Staged retained artifact differs');
    }
    return { ...run, files: run.files.map(({ content, ...identity }) => identity) };
  });
  const after = inspect(directory, mappingFile);
  if (!equal(imported.input_files, after.input_files) || !equal(imported.runs, after.runs)) throw Error('Retained originals changed during import');
  const manifest = { schema: 'cs3-retained-ui-import/1', status: 'prepared', browser_regrade: 'not_run', model_calls: 0,
    representation: 'authenticated historical reader-packet projection; final files were redacted by the historical reviewer harness, not original unredacted workspace bytes',
    historical_commitments: historical, importer_sha256: sha(read(__filename)), input_files: imported.input_files, originals_unchanged: true, runs };
  const target = path.join(output, 'source-manifest.json'); fs.writeFileSync(target, JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  return { manifest: target, sha256: sha(read(target)), status: 'prepared', browser_regrade: 'not_run', runs: runs.length };
}
module.exports = { historical, caseIds, inspect, prepare };
if (require.main === module) {
  try { const [command, directory, mapping, destination, ...extra] = process.argv.slice(2); if (command !== 'prepare' || !directory || !mapping || !destination || extra.length) throw Error('Usage: prepare RETAINED_PACKET_DIRECTORY MAPPING_JSON NEW_PRIVATE_DESTINATION'); console.log(JSON.stringify(prepare(directory, mapping, destination))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
