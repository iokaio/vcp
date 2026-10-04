// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {isDeepStrictEqual} = require('node:util');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function readArchive(directory, readBundle) {
  const root = fs.realpathSync(directory);
  const bounded = (relative, limit) => {
    if (typeof relative !== 'string' || path.isAbsolute(relative) || relative.includes('\\') || relative.split('/').some(part => !part || part === '..' || part === '.')) throw Error('Unsafe archive path');
    const file = fs.realpathSync(path.join(root,relative));
    if (!file.startsWith(root + path.sep)) throw Error('Archive path escapes evidence root');
    const stat = fs.statSync(file);
    if (!stat.isFile() || stat.size > limit) throw Error('Archive file exceeds bounded read');
    const bytes = fs.readFileSync(file);
    if (bytes.length > limit) throw Error('Archive file grew beyond bounded read');
    return bytes;
  };
  const manifestBytes = bounded('manifest.json',2 * 1024 * 1024);
  const manifestHash = bounded('manifest.sha256',66).toString('ascii').trim();
  if (!/^[a-f0-9]{64}$/.test(manifestHash) || hash(manifestBytes) !== manifestHash) throw Error('Archive manifest hash mismatch');
  const manifest = JSON.parse(manifestBytes);
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.artifacts) || manifest.artifacts.length > 4096 || manifest.bundle?.path !== 'inspection-bundle.json') throw Error('Unsupported execution archive');
  const bundleBytes = bounded(manifest.bundle.path,32 * 1024 * 1024);
  if (hash(bundleBytes) !== manifest.bundle.sha256 || bundleBytes.length !== manifest.bundle.bytes) throw Error('Archive bundle mismatch');
  const bundle = JSON.parse(bundleBytes);
  if (!isDeepStrictEqual(manifest.scope,bundle.task?.scope)) throw Error('Archive scope mismatch');
  const result = readBundle(path.join(root,manifest.bundle.path));
  if (result.source_sha256 !== manifest.bundle.sha256) throw Error('Archive bundle changed during analysis');
  const descriptors = new Map();
  for (const pages of Object.values(bundle.views ?? {})) for (const page of pages) for (const row of page.items ?? []) {
    if (row.collection !== 'artifact' || row.visibility !== 'available' || !row.record) continue;
    const descriptor = row.record;
    const prior = descriptors.get(descriptor.spec?.id);
    if (prior && !isDeepStrictEqual(prior,descriptor)) throw Error('Conflicting bundle artifact descriptor');
    descriptors.set(descriptor.spec?.id,descriptor);
  }
  let bytesRead = 0;
  const seen = new Set();
  const requests = new Map();
  const requestedObservations = bundle.fixture_observation_artifacts ?? [];
  if (!Array.isArray(requestedObservations) || requestedObservations.length > 64 || new Set(requestedObservations).size !== requestedObservations.length || requestedObservations.some(id => typeof id !== 'string' || !/^[A-Za-z0-9_-]+$/.test(id))) throw Error('Invalid fixture observation references');
  const fixtureObservations = [];
  const repairs = [], allocations = [], checkpoints = [], contextProjections = [], partialCaptures = [];
  for (const row of manifest.artifacts) {
    const descriptor = row.descriptor, id = descriptor?.spec?.id;
    if (typeof id !== 'string' || !/^[A-Za-z0-9_-]+$/.test(id) || seen.has(id) || row.path !== `artifacts/${id}.bin`) throw Error('Invalid or duplicate archive artifact identity');
    seen.add(id);
    if (!['complete','aborted'].includes(descriptor.state) || !isDeepStrictEqual(descriptor.spec.scope,manifest.scope) || !isDeepStrictEqual(descriptors.get(id),descriptor)) throw Error('Artifact differs from authorized bundle descriptor');
    const bytes = bounded(row.path,64 * 1024 * 1024 - bytesRead);
    bytesRead += bytes.length;
    if (row.bytes !== bytes.length || descriptor.length !== String(bytes.length) || row.sha256 !== descriptor.sha256 || hash(bytes) !== row.sha256) throw Error('Archive artifact integrity mismatch');
    if (descriptor.state === 'aborted') {
      partialCaptures.push({artifact:id,schema:descriptor.spec.schema,channel:descriptor.spec.channel,state:'aborted',bytes:bytes.length,sha256:row.sha256,omissions:descriptor.spec.omissions ?? [],meaning:'Retained prefix only; no terminal response or successful operation is inferred.'});
      continue;
    }
    if (descriptor.spec.channel === 'request_body') {
      if (!requests.has(row.sha256)) requests.set(row.sha256,[]);
      requests.get(row.sha256).push(id);
    }
    if (!requestedObservations.includes(id) && !['execution-completion-repair/1','verification-result/1','context-manifest/1','canonical-compaction-projection/1'].includes(descriptor.spec.schema)) continue;
    const value = JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
    if (requestedObservations.includes(id)) {
      if (descriptor.spec.schema !== 'retained-output/1' || value.schema_version !== 1 || value.kind !== 'fixture_observed_verification_refresh' || !isDeepStrictEqual(value.scope,manifest.scope) || !['missing_verification','stale_verification'].includes(value.rejection) || (value.prior_verification != null && (typeof value.prior_verification !== 'string' || value.prior_verification.length > 128 || !/^[A-Za-z0-9_-]+$/.test(value.prior_verification))) || ![value.verified_source_sha256,value.current_source_sha256].every(hash => typeof hash === 'string' && /^[a-f0-9]{64}$/.test(hash))) throw Error('Invalid fixture refresh observation');
      const verification = result.analysis.facts.verification.find(item => item.id === value.prior_verification);
      fixtureObservations.push({artifact:id,kind:value.kind,rejection:value.rejection,prior_verification:value.prior_verification,
        verification_present:!!verification,observed_prior_checks:verification?.checks ?? null,
        verified_source_sha256:value.verified_source_sha256,current_source_sha256:value.current_source_sha256,
        interpretation:'Captured fixture assertion of an observed typed rejection; no execution authority or independently attested causation.'});
    }
    if (descriptor.spec.schema === 'execution-completion-repair/1') {
      const verification = result.analysis.facts.verification.find(item => item.id === value.verification);
      repairs.push({artifact:id,verification:value.verification,verification_present:!!verification,
        observed_checks:verification?.checks ?? null,reason_code:value.reason_code,repeats:value.repeats,
        threshold:value.threshold,pause_requested:value.pause_requested,progress:value.progress});
    }
    if (descriptor.spec.schema === 'context-manifest/1') allocations.push({artifact:id,request_sha256:value.request_sha256,
      input_estimate:value.input_estimate,estimate_method:value.estimate_method,allocation:value.allocation ?? null,
      included_count:value.included?.length,excluded_count:value.excluded?.length,exclusions:value.excluded ?? []});
    if (descriptor.spec.schema === 'canonical-compaction-projection/1') contextProjections.push({artifact:id,
      summary_artifact:value.summary_artifact,before_request_sha256:value.before_request_sha256,
      after_request_sha256:value.after_request_sha256,estimated:value.estimated,estimate_method:value.estimate_method,
      input_estimate_before:value.input_estimate_before,input_estimate_after:value.input_estimate_after,gain:value.gain,
      compacted_pairs:value.projection?.compacted_pairs,sources:value.projection?.sources ?? []});
    if (value.execution_diagnostics) checkpoints.push({artifact:id,owner:value.execution_diagnostics.owner,
      snapshot_micros:value.execution_diagnostics.snapshot_micros ?? null,complete_history:value.execution_diagnostics.complete_history,
      dropped:value.execution_diagnostics.dropped,observations:value.execution_diagnostics.observations?.length});
  }
  for (const allocation of allocations) allocation.request_artifacts = requests.get(allocation.request_sha256) ?? [];
  if (fixtureObservations.length !== requestedObservations.length) throw Error('Fixture observation is not a complete verified archived artifact');
  result.archive = {manifest_sha256:manifestHash,backend:manifest.backend,fixture:manifest.fixture,
    verified_artifacts:seen.size,verified_bytes:bytesRead,repairs,allocations,context_projections:contextProjections,partial_captures:partialCaptures,fixture_observations:fixtureObservations,diagnostic_checkpoints:checkpoints,
    integrity:'Supplied bundle, manifest and artifact digests agree; this is not independent provenance or execution authority.',
    limitations:manifest.limitations,assessment:'Scripted outcome and semantic identity links; independent application quality remains a separate check.'};
  return result;
}
module.exports = {readArchive};
