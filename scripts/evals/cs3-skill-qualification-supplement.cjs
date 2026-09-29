// SPDX-License-Identifier: Apache-2.0
'use strict';
// One additional fixed Friendli pair. The failed pair and its full allocation
// remain immutable; this does not reopen any comparison or preflight claim.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), Module = require('node:module');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), doc = require('./cs3-document-remediation.cjs');
const { plain, read, write, within, privateDirectory, noParentInstructions, noSecrets } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), decisionFile = path.join(root, 'src/evals/skills/cs3-skill-remediation/qualification-supplement.json');
const reuse = require('./cs3-proof-footprint.cjs').createProofReuse();
const sha = data => crypto.createHash('sha256').update(data).digest('hex'), json = ref => JSON.parse(doc.bound(ref));
const reference = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) });
const skill = () => require('./cs3-skill-remediation.cjs'), core = () => require('./cs3-comparison.cjs');
function need(value, reason) { if (!value) throw Error(reason); }
function pin(value, actual, label) { need(/^[a-f0-9]{64}$/.test(value || '') && value === actual, 'Supplement ' + label + ' differs'); }
function decision(ref) {
  const value = json(ref), fixed = { schema: 'cs3-skill-qualification-supplement-decision/1', authority: 'owner_explicit_cs3_completion_within_existing_100_usd',
    original_qualification_cap_micros: 250000, original_known_cost_micros: 274, per_request_quote_micros: 249832,
    additional_cap_micros: 500000, additional_requests: 2, preserved_cap_micros: 98913737, preserved_requests: 2631,
    combined_cap_micros: 99413737, combined_requests: 2633, outer_cap_micros: 100000000,
    consumed_liabilities_released: false, qualification_waiver: false, comparison_replay: false };
  need(equal(value, JSON.parse(read(decisionFile))) && Object.entries(fixed).every(([key, expected]) => equal(value[key], expected)), 'Exact qualification supplement decision required');
  need(path.isAbsolute(value.original_root || '') && path.isAbsolute(value.qualification_claim_path || '')
    && value.combined_cap_micros <= value.outer_cap_micros, 'Bounded fixed supplement paths required');
  return value;
}
function identity(spec) {
  const selected = spec.skill_remediation;
  return { executable: spec.executable, build_receipt: spec.build_receipt, decision: selected.decision, history: selected.history,
    recovery_native: selected.recovery_native, package_acceptance: selected.package_acceptance };
}
function source(ref, directory) {
  const archive = json(ref), oldRoot = plain(directory);
  need(archive.source_before_after_equal === true && archive.archive_source_equal === true && archive.model_calls === 0 && archive.claims_created === 0
    && equal(prep.identity(plain(archive.archive), archive.source.scope), archive.source)
    && equal(prep.identity(oldRoot, archive.source.scope), archive.source), 'Original allocation source/archive differs');
  const inventory = prep.identity(archive.archive, ['.']), directories = new Set(archive.source.directories);
  for (const relative of [...directories, ...archive.source.files.map(item => item.path)])
    for (let parent = path.posix.dirname(relative); parent !== '.'; parent = path.posix.dirname(parent)) directories.add(parent);
  need(equal(inventory.files.map(item => item.path.slice(2)).sort(), archive.source.files.map(item => item.path).sort())
    && equal(inventory.directories.filter(item => item !== '.').map(item => item.slice(2)).sort(), [...directories].sort()), 'Unbound original archive entries');
  return { archive, oldRoot };
}
function archivedAllocation(input, approved) {
  pin(approved.original_allocation_sha256, input.original_allocation.sha256, 'original allocation');
  pin(approved.original_archive_sha256, input.original_source.sha256, 'original archive');
  const allocation = json(input.original_allocation), history = json(allocation.spec.history), native = json(allocation.spec.recovery_native), oldDecision = json(native.decision);
  const archives = () => [source(input.original_source, approved.original_root), source(history.archive, oldDecision.historical_root)];
  const guard = () => {
    for (const { archive, oldRoot } of [
      { archive: json(input.original_source), oldRoot: approved.original_root }, { archive: json(history.archive), oldRoot: oldDecision.historical_root }
    ]) for (const item of archive.source.files) need(!require.cache[path.resolve(oldRoot, item.path)], 'Unverified original allocation module cache');
  };
  return reuse(JSON.stringify({ allocation: input.original_allocation, archive: input.original_source }), () => {
    const before = archives(), allowed = new Map(before.flatMap(row => row.archive.source.files.map(item => [path.resolve(row.oldRoot, item.path), item])));
    const loaded = new Map(), loading = new Set(), originalLoad = Module._load;
    Module._load = function (request, parent, isMain) {
      if (Module.isBuiltin(request)) return originalLoad.apply(this, arguments);
      const file = Module._resolveFilename(request, parent, isMain), entry = allowed.get(file);
      need(entry && plain(file) === file && fs.lstatSync(file).nlink === 1 && sha(read(file)) === entry.sha256, 'Original allocation dependency outside approved closure');
      need(!require.cache[file] || loading.has(file) || loaded.get(file) === require.cache[file], 'Unverified original allocation cached dependency');
      const nested = loading.has(file); loading.add(file);
      try { const value = originalLoad.apply(this, arguments); if (require.cache[file]) loaded.set(file, require.cache[file]); return value; }
      finally { if (!nested) loading.delete(file); }
    };
    try {
      const oldRequire = Module.createRequire(path.join(before[0].oldRoot, 'scripts/evals/cs3-skill-remediation.cjs')), old = oldRequire('./cs3-skill-remediation.cjs');
      const selected = allocation.spec, originalSpec = { executable: selected.executable, build_receipt: selected.build_receipt,
        skill_remediation: { decision: selected.decision, allocation: input.original_allocation, history: selected.history,
          recovery_native: selected.recovery_native, package_acceptance: selected.package_acceptance } };
      const observed = old.validateAllocation(originalSpec), candidates = old.candidateRegistry.inspect(), tasks = old.tasks();
      need(equal(archives(), before) && [...loaded.keys()].every(file => sha(read(file)) === allowed.get(file).sha256), 'Original allocation source changed during validation');
      return { observed, candidates, tasks };
    } finally {
      Module._load = originalLoad;
      for (const [file, value] of loaded) if (require.cache[file] === value) delete require.cache[file];
    }
  }, guard);
}
function failedProbe(input, approved, allocation) {
  const failed = input.failed_probe;
  need(equal(Object.keys(failed || {}).sort(), ['binary', 'claim', 'inventory', 'records', 'result']), 'Exact failed probe raw references required');
  for (const [name, key] of [['claim', 'failed_claim_sha256'], ['result', 'failed_result_sha256'], ['records', 'failed_records_sha256']]) pin(approved[key], failed[name].sha256, 'failed ' + name);
  const claim = json(failed.claim), result = json(failed.result), records = json(failed.records), base = plain(path.dirname(failed.claim.path));
  pin(approved.failed_inventory_sha256, sha(JSON.stringify(failed.inventory)), 'failed full inventory');
  need(equal(prep.identity(base, ['.']), failed.inventory), 'Failed probe retained inventory changed');
  doc.bound(failed.binary, 1024 * 1024 * 1024);
  pin(approved.original_conformance_sha256, failed.binary.sha256, 'original conformance binary');
  need(claim.binary_sha256 === failed.binary.sha256, 'Failed probe executable claim differs');
  need(plain(failed.claim.path) === plain(allocation.spec.qualification_claim_path)
    && plain(failed.result.path) === path.join(base, 'result.json') && plain(failed.records.path) === path.join(base, 'canonical-records.json'), 'Original funded probe paths differ');
  const probeSpec = reference(path.join(path.dirname(base), 'probe-spec.json'));
  need(claim.spec_sha256 === probeSpec.sha256 && equal(claim.spec, json(probeSpec)) && claim.spec.model === 'deepseek/deepseek-v3.2'
    && claim.spec.endpoint === 'friendli' && claim.spec.cap_usd === '0.250000' && claim.spec.max_output_tokens === 2048, 'Original probe claim/spec differs');
  doc.bound({ path: claim.spec.catalog, sha256: claim.spec.catalog_sha256 });
  const oldRoot = plain(approved.original_root), sourceFiles = { binary: 'src/crates/vcp-cli/src/bin/vcp-provider-conformance.rs',
    lease: 'src/crates/vcp-lifecycle/src/foundation/conformance.rs', settlement: 'src/crates/vcp-lifecycle/src/foundation/worker/conformance.rs', catalog: 'src/crates/vcp-models/src/catalog.rs' };
  need(equal(claim.source_sha256, Object.fromEntries(Object.entries(sourceFiles).map(([key, file]) => [key, sha(read(path.join(oldRoot, file)))]))), 'Original conformance source differs');
  const rows = name => records.filter(item => item.collection === name).map(item => item.value), attempts = rows('attempt'), reservations = rows('reservation'), settlements = rows('settlement'), ledgers = rows('ledger');
  need(result.schema === 'p6-provider-conformance/1' && result.status === 'failed' && result.error === 'budget exhausted: root cap'
    && result.actual_cost_micros === '274' && result.responses_text_tools === false && attempts.length === 1 && reservations.length === 1 && settlements.length === 1 && ledgers.length === 1,
  'Only exact settled first-call, second-admission denial is eligible');
  const [attempt] = attempts, [reservation] = reservations, [settlement] = settlements, [ledger] = ledgers;
  need(equal(ledger, result.ledger) && ledger.cap === '250000' && ledger.settled === '274' && ledger.active === '0' && ledger.unresolved === '0' && ledger.overrun === false
    && [attempt, reservation, settlement, ledger].every(row => equal(row.scope, result.scope))
    && attempt.phase === 'settled' && attempt.charged === '274' && !!attempt.send_intent && attempt.reservation === reservation.id
    && reservation.attempt === attempt.id && reservation.phase === 'settled' && reservation.amount.micros === '249832' && reservation.charged === '274' && reservation.liability === '0'
    && attempt.quote.amount.micros === '249832' && settlement.attempt === attempt.id && settlement.applied === true && settlement.direction === 'debit' && settlement.total === '274'
    && settlement.observation.amount.micros === '274' && settlement.observation.provider_request === attempt.provider_request,
  'Failed probe canonical accounting differs');
  need(!records.some(item => ['effect', 'invocation'].includes(item.collection)), 'Original probe has unexpected execution effects');
  return { actual_cost_micros: 274, observed_attempts: 1, original_reserved_cap_micros: 250000, active_micros: 0, unresolved_micros: 0 };
}
function current(input, approved, original) {
  const selected = input.current, old = original.observed.allocation.spec;
  need(equal(Object.keys(selected || {}).sort(), ['build_receipt', 'decision', 'executable', 'history', 'package_acceptance', 'recovery_native'])
    && equal(selected.decision, old.decision) && equal(selected.history, old.history), 'Exact preserved decision/history required');
  pin(approved.new_executable_sha256, selected.executable.sha256, 'current executable');
  pin(approved.new_native_sha256, selected.recovery_native.sha256, 'current native gates');
  pin(approved.new_package_sha256, selected.package_acceptance.sha256, 'current package acceptance');
  doc.build(selected, { executable_sha256: approved.new_executable_sha256 });
  const candidates = skill().candidateRegistry.inspect(), oldCandidates = original.candidates;
  need(equal(candidates.files, oldCandidates.files) && equal(candidates.entries, oldCandidates.entries) && candidates.source_id === oldCandidates.source_id
    && equal(skill().tasks(), original.tasks), 'Original candidate bytes or held-out tasks changed');
  require('./cs3-controller-recovery-qualification.cjs').nativePrerequisites(json(selected.recovery_native));
  // Byte-identical executable and identical installed receipt can retain its
  // genuinely executed archived tests. Changed executable/package evidence
  // must pass a new current-source installed acceptance; never infer it.
  const pkg = selected.executable.sha256 === old.executable.sha256 && equal(selected.package_acceptance, old.package_acceptance)
    ? original.observed.allocation.proof.package : skill().packageAcceptance(selected.package_acceptance, approved.new_executable_sha256);
  return { candidates, package: pkg };
}
function claimFile() { return path.join(path.dirname(core().claimFile(true)), 'vcp-cs3-skill-qualification-supplement1.json'); }
function projection(input) {
  const approved = decision(input.decision), original = archivedAllocation(input, approved), observed = failedProbe(input, approved, original.observed.allocation);
  current(input, approved, original);
  need(plain(input.qualification_claim_path) === plain(approved.qualification_claim_path)
    && plain(input.qualification_claim_path) !== plain(original.observed.allocation.spec.qualification_claim_path), 'Exact fresh funded probe claim required');
  return { approved, original, observed };
}
function safeDestination(input, destination, proof) {
  const directory = plain(path.resolve(destination)), probe = plain(path.dirname(input.qualification_claim_path));
  const allocation = proof.original.observed.allocation, archive = json(input.original_source), history = json(allocation.spec.history), native = json(allocation.spec.recovery_native);
  const protectedRoots = [root, proof.approved.original_root, archive.archive, path.dirname(input.original_allocation.path), path.dirname(input.failed_probe.claim.path),
    json(history.archive).archive, json(native.decision).historical_root, ...allocation.proof.historical.protected_inventories.map(row => row.directory),
    path.dirname(allocation.proof.release.plan.path), allocation.proof.release.rows.map(row => path.join(json(allocation.proof.release.plan).directory, row.id))].flat().map(plain);
  need(!within(directory, probe) && !within(probe, directory) && protectedRoots.every(parent => [directory, probe].every(child => !within(parent, child) && !within(child, parent))), 'Supplement overlaps protected evidence');
  for (const file of [directory, probe]) { privateDirectory(file); noParentInstructions(path.dirname(file)); }
  return directory;
}
function reserve(file, destination) {
  const input = JSON.parse(read(file)); noSecrets(input);
  need(equal(Object.keys(input).sort(), ['current', 'decision', 'failed_probe', 'original_allocation', 'original_source', 'qualification_claim_path']), 'Exact fixed supplement inputs required');
  const proof = projection(input), directory = safeDestination(input, destination, proof);
  need(!fs.existsSync(directory) && !fs.existsSync(claimFile()) && !fs.existsSync(input.qualification_claim_path)
    && !fs.existsSync(skill().claimFile()) && !fs.existsSync(require('./cs3-document-remediation-preflight.cjs').claimFile(0, 'skill')), 'Only one new pair before any fresh experiment/preflight claim');
  const receipt = { schema: 'cs3-skill-qualification-supplement/1', directory, input, original_observation: proof.observed,
    cap_micros: 500000, request_ceiling: 2, combined_cap_micros: 99413737, combined_request_ceiling: 2633, model_calls: 0 };
  const ref = { path: path.join(directory, 'allocation.json'), sha256: sha(JSON.stringify(receipt, null, 2) + '\n') };
  write(claimFile(), { schema: 'cs3-skill-qualification-supplement-claim/1', allocation: ref });
  fs.mkdirSync(directory, { mode: 0o700 }); write(ref.path, receipt); return ref;
}
function validate(ref, spec) {
  const receipt = json(ref), proof = projection(receipt.input);
  safeDestination(receipt.input, receipt.directory, proof);
  need(plain(path.dirname(ref.path)) === receipt.directory && equal(JSON.parse(read(claimFile())), { schema: 'cs3-skill-qualification-supplement-claim/1', allocation: ref })
    && equal(receipt, { schema: 'cs3-skill-qualification-supplement/1', directory: receipt.directory, input: receipt.input, original_observation: proof.observed,
      cap_micros: 500000, request_ceiling: 2, combined_cap_micros: 99413737, combined_request_ceiling: 2633, model_calls: 0 }), 'Exact one-shot supplemental allocation required');
  if (spec) need(equal(ref, spec.skill_remediation.qualification_supplement) && equal(spec.skill_remediation.allocation, receipt.input.original_allocation)
    && equal(identity(spec), receipt.input.current), 'Supplement does not bind current experiment inputs');
  return { ...proof.original.observed, executable_sha256: proof.approved.new_executable_sha256,
    qualification_claim_path: receipt.input.qualification_claim_path, cap_micros: 500000, request_ceiling: 2,
    combined_cap_micros: 99413737, combined_request_ceiling: 2633, additional_cap_micros: 500000 };
}
function authorizeProbe(ref) {
  const result = validate(ref);
  need(!fs.existsSync(result.qualification_claim_path), 'Supplemented qualification already claimed');
  const oldProbe = json(json(ref).input.failed_probe.claim).spec;
  const original_catalog = { path: oldProbe.catalog, sha256: oldProbe.catalog_sha256 }; doc.bound(original_catalog);
  return { ...Object.fromEntries(['qualification_claim_path', 'cap_micros', 'request_ceiling', 'combined_cap_micros', 'combined_request_ceiling'].map(key => [key, result[key]])), original_catalog };
}
module.exports = { decision, identity, archivedAllocation, failedProbe, projection, reserve, validate, authorizeProbe, claimFile };
if (require.main === module) {
  try { const [command, first, second] = process.argv.slice(2); const result = command === 'reserve' ? reserve(first, second)
    : command === 'authorize-probe' ? authorizeProbe({ path: plain(path.resolve(first)), sha256: second }) : (() => { throw Error('Usage: reserve INPUT NEW_PRIVATE_DIRECTORY | authorize-probe RECEIPT SHA256'); })();
    process.stdout.write(JSON.stringify(result) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
