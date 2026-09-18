// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { sourceIdentity, writeManifest, digest } = require('../../src/tests/support/harness.cjs');
const { traceCase, CASES } = require('../../src/tests/support/cli-trace.cjs');
const { readComponent, compareTree } = require('../../src/tests/support/upstream-selection.cjs');
async function main() {
  if (process.argv.length !== 4 || process.argv[2] !== '--binary') throw Error('Usage: node scripts/upstream/trace-cli.cjs --binary <built codex.exe>');
  const repository = path.resolve(__dirname, '../..');
  const binary = path.resolve(process.argv[3]);
  const directory = path.join(repository, 'artifacts', 'cli-trace', crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, 'manifest.json');
  const manifest = { schema_version: 1, task_ids: ['P0-07'], status: 'running', started_at: new Date().toISOString(),
    node: process.version, platform: process.platform, binary, provider: 'synthetic-loopback', paid_requests: 0,
    usage_semantics: 'Scripted provider usage is recorded separately from parent CLI usage. The pinned review path reports zero parent usage; this baseline records that discrepancy, not working VCP accounting.',
    limitations: ['Upstream native CLI trace only, not VCP runtime qualification.', 'Loopback observer does not prove absence of other network traffic.', 'Caller supplies the built executable; its hash is recorded, not an attestation of its compiler inputs.'], attempts: [] };
  writeManifest(file, manifest);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  process.once('SIGINT', cancel); process.once('SIGTERM', cancel);
  try {
    if (process.platform !== 'win32') { manifest.status = 'not_run'; manifest.reason = 'Native Windows required'; process.exitCode = 3; return; }
    if (!fs.existsSync(binary) || !fs.statSync(binary).isFile()) {
      manifest.status = 'not_run'; manifest.reason = 'Built native CLI binary required'; process.exitCode = 3; return;
    }
    const hash = crypto.createHash('sha256');
    for await (const chunk of fs.createReadStream(binary)) hash.update(chunk);
    manifest.binary_sha256 = hash.digest('hex');
    manifest.source = await sourceIdentity(repository, controller.signal);
    const selected = readComponent(repository, 'codex');
    const inventory = JSON.parse(fs.readFileSync(path.join(repository, selected.selection.result_inventory), 'utf8'));
    if (inventory.component !== selected.component.id || inventory.commit !== selected.component.commit ||
        inventory.selection_sha256 !== digest(JSON.stringify(selected.selection))) throw Error('Inventory identity mismatch');
    const sourceErrors = compareTree(path.join(repository, selected.selection.destination), inventory);
    if (sourceErrors.length) throw Error('Imported source differs: ' + sourceErrors.join(', '));
    manifest.source_verification = { status: 'pass', files: inventory.files.length, files_sha256: inventory.files_sha256 };
    manifest.component_commit = selected.component.commit;
    manifest.runner_sha256 = digest(fs.readFileSync(__filename));
    manifest.helper_sha256 = digest(fs.readFileSync(path.join(repository, 'src/tests/support/cli-trace.cjs')));
    for (const kind of CASES) {
      if (controller.signal.aborted) throw Error('cancelled');
      console.log('Native CLI trace: ' + kind);
      try { manifest.attempts.push(await traceCase(binary, kind, path.join(directory, kind), controller.signal)); }
      catch (error) { manifest.attempts.push({ kind, status: 'fail', reason: error.message }); throw error; }
      writeManifest(file, manifest);
    }
    manifest.status = 'pass';
  } catch (error) { manifest.status = 'fail'; manifest.reason = error.message; process.exitCode = controller.signal.aborted ? 130 : 1; }
  finally {
    manifest.ended_at = new Date().toISOString(); writeManifest(file, manifest);
    process.removeListener('SIGINT', cancel); process.removeListener('SIGTERM', cancel);
    console.log(JSON.stringify({ status: manifest.status,
      attempts: manifest.attempts.map(({ kind, status, requests, tool_effects, exit_code, reason, provider_usage, cli_usage, usage_matches_provider }) =>
        ({ kind, status, requests, tool_effects, exit_code, reason, provider_usage, cli_usage, usage_matches_provider })), manifest: file }));
  }
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
