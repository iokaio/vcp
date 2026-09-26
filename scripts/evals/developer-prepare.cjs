// SPDX-License-Identifier: Apache-2.0
'use strict';
// CS-2 developer campaign preparation only: no process execution, provider calls
// or credential reads. It freezes fifty-four matched runs in three per-skill
// blocks and pins every identity the owner pre-approved for process authority.
// CS-1 campaign-identity scripts are imported, never edited.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const cs1 = require('./authoring-prepare.cjs');
const { requireEmbeddedCatalog } = require('./builtin-generation-prepare.cjs');
const { fixedProfileReasons } = require('./builtin-live-runner.cjs');
const { inspectAssets, portable } = require('../skills/builtin-assets.cjs');
const oracle = require('./developer-oracle.cjs');
const candidates = require('./developer-candidates.cjs');
const { plain, read, write, within, safeChild, noParentInstructions, privateDirectory, noSecrets, usd } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const fixtures = path.join(repository, 'src/evals/skills/developer');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const object = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...names].sort());

// Owner-authorized envelope (September 25, 2026): fifty-four runs at USD 3 and
// sixteen requests each, 2,048 output tokens per request, no headroom.
const limits = Object.freeze({ runs: 54, write_runs: 39, cap_micros: 162000000, requests: 864, slot_micros: 3000000, slot_requests: 16, output_tokens: '2048' });
const arms = ['none', 'nearest', 'candidate'];
// The CS-1 source-profile field set, unchanged.
const profileFields = ['version', 'workspace', 'trust_workspace', 'sync_roots', 'maximum_autonomy', 'automatic_effects', 'budget_usd', 'provider', 'routing', 'decisions', 'skills', 'mcp', 'mcp_http', 'catalog', 'affected_paths', 'canonical_tools', 'max_requests', 'output_tokens', 'provider_timeout_seconds', 'max_transport_retries', 'deadline_seconds', 'processes', 'hooks', 'observers', 'checks', 'qualification_endpoint'];
const knownTools = ['vcp_read', 'vcp_list', 'vcp_search', 'vcp_patch', 'vcp_exec', 'vcp_verify', 'vcp_mcp'];
const checkerTests = ['developer input preservation', 'developer output structure'];
const checkerArguments = ['--test', '--test-reporter=tap', '--test-concurrency=1', 'checks/developer.test.cjs'];
const marker = '// Inert VCP developer verifier marker; never executed as JavaScript.\n';
// Owner-approved Node identity for AppContainer grading; any other build needs new approval.
const nodeSha256 = 'ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32';
// Must equal developer-check-build.ps1's scope; the receipt records it and is compared here.
const checkerBuildScope = ['src/crates', 'src/evals/skills/developer', 'src/third_party/codex/codex-rs/Cargo.toml', 'src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/rust-toolchain.toml'];
// The builder resolves its target directory to one absolute path before building.
const checkerCommand = target => ['cargo', 'build', '--manifest-path', 'src/third_party/codex/codex-rs/Cargo.toml', '--locked', '--offline', '--target-dir', target, '-j2', '-p', 'vcp-cli', '--features', 'qualification', '--bin', 'vcp-developer-check'];
const graderFiles = ['scripts/evals/developer-grader.cjs', 'scripts/evals/developer-oracle.cjs', 'scripts/evals/node-fixture-runner.ps1', 'scripts/evals/node-fixture-bootstrap.cjs', 'scripts/evals/node-fixture-interactive-bootstrap.cjs', 'scripts/evals/node-fixture-protocol.cjs', 'scripts/evals/node-fixture-session.cjs', 'src/tests/support/windows/AppContainerFixture.cs'];
const sourceScope = ['src/crates', 'src/skills/builtin', ...candidates.ids.map(id => `src/skills/candidates/${id}`), 'src/evals/skills/developer',
  'scripts/evals/developer-candidates.cjs', 'scripts/evals/developer-prepare.cjs', 'scripts/evals/developer-runner.cjs', 'scripts/evals/developer-review.cjs', 'scripts/evals/developer-check-build.ps1',
  ...graderFiles, 'scripts/evals/authoring-prepare.cjs', 'scripts/evals/authoring-runner.cjs', 'scripts/evals/p6-live-runner.cjs', 'scripts/evals/p6-task-quality.cjs',
  'scripts/evals/builtin-live-runner.cjs', 'scripts/evals/builtin-generation-prepare.cjs', 'scripts/skills/builtin-assets.cjs',
  'src/third_party/codex/codex-rs/Cargo.toml', 'src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/rust-toolchain.toml'];
const benefitRule = 'A candidate benefits on a normal case when, for both independent blind readers, either (a) it passes every executable check (structural oracle, in-run checker and functional grading) that both baselines fail and its completeness, clarity and usefulness scores are each no lower than each baseline, or (b) its usefulness is at least each baseline + 1 with completeness and clarity no lower. Either way every candidate hard gate passes, so there is no correctness or preservation regression. Ties remain unqualified.';

function frozen(root, ref) {
  portable(ref.path);
  const bytes = read(safeChild(root, ref.path), 1024 * 1024);
  if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error('Frozen developer input changed');
  return bytes;
}
// The frozen v5 cohort with each task's oracle, editable paths and exact source bytes.
function tasks() {
  const manifestBytes = read(path.join(fixtures, 'manifest.json'), 1024 * 1024), manifest = JSON.parse(manifestBytes);
  if (manifest.schema_version !== 1 || manifest.revision !== oracle.revision || manifest.case_count !== 18 || manifest.planned_task_runs !== limits.runs || manifest.cases.length !== 18) throw Error('Frozen CS-2 v5 cohort required');
  const ids = new Set();
  const items = manifest.cases.map(task => {
    const { oracle: definition } = oracle.load(task.id);
    const tools = task.context?.tools;
    if (ids.has(task.id) || !candidates.ids.includes(task.skill) || !equal(task.comparison_arms, arms) || task.project !== 'projects/' + task.id || typeof task.prompt !== 'string' || !task.prompt || task.prompt.length > 16384) throw Error('Invalid developer assignment');
    if (!Array.isArray(tools) || new Set(tools).size !== tools.length || tools.some(tool => !knownTools.includes(tool)) || tools.includes('vcp_exec') || tools.includes('vcp_mcp')) throw Error('Invalid frozen canonical tool ceiling');
    ids.add(task.id);
    const edits = definition.allowed_modifications.map(portable);
    const writing = edits.length > 0;
    if (definition.allowed_outputs.length || writing !== tools.includes('vcp_patch') || !tools.includes('vcp_verify')) throw Error('Write and report-only tool ceilings differ from the frozen oracle');
    const project = safeChild(fixtures, task.project), files = new Map();
    for (const ref of task.expected.source_files) {
      if (files.has(ref.path)) throw Error('Duplicate task file');
      files.set(ref.path, frozen(project, ref));
    }
    for (const [relative, bytes] of scaffold(task, edits)) {
      if (files.has(relative)) throw Error('Verification scaffold collides with frozen task source');
      files.set(relative, bytes);
    }
    return { task, definition, edits, files };
  });
  return { manifest, manifestBytes, items };
}
function scaffold(task, edits) {
  if (!edits.length) return new Map();
  return new Map([[oracle.scaffoldPaths[0], Buffer.from(marker)], [oracle.scaffoldPaths[1], Buffer.from(JSON.stringify({ schema_version: 1, case_id: task.id }) + '\n')]]);
}
function preparedFiles(item) {
  return [...item.task.expected.source_files, ...[...scaffold(item.task, item.edits)].map(([relative, bytes]) => ({ path: relative, bytes: bytes.length, sha256: sha(bytes) }))];
}
function preparedDirectories(item) {
  const directories = new Set();
  for (const file of [...preparedFiles(item).map(file => file.path), ...item.edits]) {
    let parent = path.posix.dirname(portable(file));
    while (parent !== '.') { directories.add(parent); parent = path.posix.dirname(parent); }
  }
  return [...directories].sort();
}
// CS-1's source-profile rules evaluated at an explicit time. Preparation and every
// dispatch use the current time; identity checks of completed evidence use the
// recorded preparation time, so a later expiry never invalidates paid runs.
function profileReasons(profile, now = Date.now()) {
  const reasons = fixedProfileReasons(profile, now), tools = profile.canonical_tools;
  if (tools !== undefined && (!Array.isArray(tools) || new Set(tools).size !== tools.length || tools.some(tool => !knownTools.includes(tool)))) reasons.push('Invalid source canonical tool ceiling');
  if (typeof profile.provider?.observed_at !== 'string' || !/^(0|[1-9][0-9]*)$/.test(profile.provider.observed_at) || BigInt(profile.provider.observed_at) > BigInt(now)) reasons.push('Provider observation must be explicitly dated and not in the future');
  if (Object.keys(profile).some(key => !profileFields.includes(key))) reasons.push('Unknown developer profile field');
  if (profile.hooks !== undefined && (!Array.isArray(profile.hooks) || profile.hooks.length) || profile.observers != null) reasons.push('Hooks and observers are outside the comparison');
  if (profile.maximum_autonomy !== 'workspace' || JSON.stringify([...(profile.automatic_effects || [])].sort()) !== '["read","write"]') reasons.push('Workspace read/write profile required');
  if (profile.max_requests !== limits.slot_requests || profile.output_tokens !== limits.output_tokens) reasons.push('CS-2 requires 16 requests and 2048 output tokens per run');
  return reasons;
}
// The earliest end of the provider, compatibility and price qualification windows.
function qualificationEnds(profile) {
  return Math.min(...[profile.provider?.valid_until, profile.provider?.compatibility?.valid_until, profile.provider?.price?.valid_until].map(Number));
}
function derivedProfile(profile, item, workspace, catalog, runtime, arm) {
  const { task, edits } = item, tools = task.context.tools;
  if (profile.canonical_tools !== undefined && tools.some(tool => !profile.canonical_tools.includes(tool))) throw Error('Source profile tool ceiling would be broadened');
  return { ...profile, canonical_tools: [...tools], ...(arm === 'candidate' ? { skills: candidates.configuration(task.skill) } : {}), workspace, catalog, budget_usd: usd(limits.slot_micros), max_requests: limits.slot_requests,
    maximum_autonomy: edits.length ? 'autonomous' : 'plan', automatic_effects: edits.length ? [...cs1.checkerEffects] : [],
    // Report-only cases still need a nonempty acceptance scope; plan autonomy grants no edits.
    affected_paths: edits.length ? [...edits] : task.expected.source_files.map(file => portable(file.path)),
    processes: edits.length ? [{ name: 'developer-check', executable: runtime.checker, environment: { SystemRoot: runtime.system_root }, required_isolation: [], reduced_isolation: true, inputs: [] }] : [],
    checks: edits.length ? [{ manifest: 'package.json', runner: 'node', profile: 'developer-check', timeout_ms: 10000, expected_tests: checkerTests, rationale: 'Frozen CS-2 native read-only developer validation' }] : [] };
}
function checkerRuntime(input) {
  if (process.platform !== 'win32' || !object(input, ['checker', 'build_receipt']) || ![input.checker, input.build_receipt].every(value => typeof value === 'string' && path.isAbsolute(value))) throw Error('Explicit absolute Windows checker and build receipt required');
  const checker = plain(path.resolve(input.checker)), receiptPath = plain(path.resolve(input.build_receipt));
  const bytes = read(checker, 256 * 1024 * 1024), receiptBytes = read(receiptPath, 8 * 1024 * 1024), receipt = JSON.parse(receiptBytes);
  const source = 'src/crates/vcp-cli/src/bin/vcp-developer-check.rs', manifest = 'src/evals/skills/developer/manifest.json', builder = path.join(repository, 'scripts/evals/developer-check-build.ps1');
  const target = receipt.cargo_command?.[checkerCommand('').indexOf('--target-dir') + 1], command = checkerCommand(target);
  if (receipt.schema !== 'cs2-developer-check-build/1' || receipt.exit_code !== 0 || receipt.source_inputs_unchanged !== true || receipt.source !== source || receipt.source_sha256 !== sha(read(path.join(repository, source))) || receipt.fixture_manifest !== manifest || receipt.fixture_manifest_sha256 !== sha(read(path.join(repository, manifest))) || receipt.executable !== checker || receipt.executable_sha256 !== sha(bytes) || typeof target !== 'string' || !path.isAbsolute(target) || !equal(receipt.cargo_command, command) || !equal(receipt.source_scope, checkerBuildScope) || !equal(receipt.source_inputs, cs1.identity(repository, checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 }))) || receipt.builder !== builder || receipt.builder_sha256 !== sha(read(builder)) || typeof receipt.toolchain?.rustc !== 'string' || !receipt.toolchain.rustc.startsWith('rustc ') || receipt.toolchain.rustc.length > 4096) throw Error('Checker build receipt does not bind current developer checker sources');
  if (!process.env.SystemRoot || !path.isAbsolute(process.env.SystemRoot)) throw Error('Explicit Windows SystemRoot required');
  return { source_checker: checker, checker_sha256: sha(bytes), build_receipt: receiptPath, build_receipt_sha256: sha(receiptBytes), system_root: process.env.SystemRoot, provenance: 'recorded_local_build_not_cryptographic_attestation' };
}
function graderIdentity(input) {
  if (process.platform !== 'win32' || !object(input, ['node', 'node_sha256']) || typeof input.node !== 'string' || !path.isAbsolute(input.node) || input.node_sha256 !== nodeSha256) throw Error('Owner-approved Windows Node for AppContainer grading required');
  const node = plain(path.resolve(input.node));
  if (sha(read(node, 256 * 1024 * 1024)) !== nodeSha256) throw Error('Grading Node differs from the owner-approved identity');
  return { node, node_sha256: nodeSha256, files: graderFiles.map(relative => ({ path: relative, sha256: sha(read(path.join(repository, relative), 4 * 1024 * 1024)) })) };
}
// One staged checker and owner case map per preparation; only write runs appear.
function staged(directory, runtime, entries) {
  const cases = entries.filter(entry => entry.item.edits.length).map(entry => ({ workspace: path.join(directory, entry.id, 'workspace'), case_id: entry.item.task.id }));
  if (cases.length !== limits.write_runs) throw Error('Checker map must hold exactly the write-case runs');
  const bytes = Buffer.from(JSON.stringify({ schema_version: 1, cases }) + '\n');
  return { runtime: { ...runtime, checker: path.join(directory, 'runtime/vcp-developer-check.exe'), cases_file: path.join(directory, 'runtime/developer-cases.json'), cases_sha256: sha(bytes) }, cases: bytes };
}
function permissionReview(runtime, grader) {
  return { approval: 'owner_preapproved_pinned_identities_2026_09_25', write_case_automatic_effects: [...cs1.checkerEffects], sole_run_process: runtime.checker, sole_run_process_sha256: runtime.checker_sha256, process_environment: ['SystemRoot'], report_only_autonomy: 'plan', report_only_automatic_effects: [], reduced_isolation: true, mcp: [], routing: null, grading: { node: grader.node, node_sha256: grader.node_sha256, isolation: 'zero-capability AppContainer inside a kill-on-close Job Object, after the run and outside it' }, restriction: 'Only the pinned data-only checker with its fixed sibling case map may run during a task; conservative broker effects do not imply checker network, install or publish operations. Any identity change needs new owner approval.' };
}
// Blocks run in campaign order; within a block each case's arm order rotates.
function order(items) {
  const entries = [];
  for (const skill of candidates.ids) {
    items.filter(item => item.task.skill === skill).forEach((item, index) => {
      for (let position = 0; position < 3; position++) {
        const arm = arms[(position + index) % 3];
        entries.push({ item, arm, position, block: skill, id: `${item.task.id}--${arm}` });
      }
    });
  }
  if (entries.length !== limits.runs) throw Error('Fixed fifty-four run cohort required');
  return entries;
}
function rowsFor(entries, directory, runtime, profile, catalog) {
  return entries.map(({ item, arm, position, block, id }) => ({ id, block, case_id: item.task.id, kind: item.task.kind, arm, position, skills: candidates.selection(arm, item.task), write: item.edits.length > 0, functional_grading: item.definition.functional_grading.mode,
    cap_micros: limits.slot_micros, call_ceiling: limits.slot_requests, output_tokens: limits.output_tokens, prompt_sha256: sha(Buffer.from(item.task.prompt)), profile: derivedProfile(profile, item, path.join(directory, id, 'workspace'), catalog, runtime, arm),
    files: preparedFiles(item), directories: preparedDirectories(item), scaffold_paths: [...scaffold(item.task, item.edits).keys()], oracle: item.task.expected.oracle, status: 'not_run' }));
}
// Pure description of a preparation at time `at`; the runner re-derives and
// compares it exactly at the recorded preparation time.
function describe(specFile, directory, at = Date.now()) {
  const specBytes = read(specFile, 64 * 1024), spec = JSON.parse(specBytes);
  noSecrets(spec);
  if (!object(spec, ['executable', 'profile', 'runtime', 'grader', 'aggregate_cap_usd', 'aggregate_call_ceiling', 'propose_checker_process']) || spec.propose_checker_process !== true || prior.micros(spec.aggregate_cap_usd) !== limits.cap_micros || spec.aggregate_call_ceiling !== limits.requests) throw Error('CS-2 requires exactly USD 162, 864 requests, a pinned checker and grader, and an explicit checker proposal');
  const executable = plain(path.resolve(spec.executable)), executableBytes = read(executable, 1024 * 1024 * 1024);
  const assetsRoot = path.join(path.dirname(executable), 'skills/builtin'), assets = inspectAssets(assetsRoot).inventory;
  requireEmbeddedCatalog(executableBytes, read(path.join(assetsRoot, 'catalog.json')));
  const profileFile = plain(path.resolve(spec.profile)), profileBytes = read(profileFile, 1024 * 1024), profile = JSON.parse(profileBytes);
  noSecrets(profile);
  const reasons = profileReasons(profile, at);
  if (reasons.length) throw Error(reasons.join('; '));
  const providerCatalog = plain(path.resolve(profile.catalog)), providerCatalogBytes = read(providerCatalog);
  const { manifest, manifestBytes, items } = tasks();
  const catalog = JSON.parse(read(path.join(assetsRoot, 'catalog.json')));
  for (const { task } of items) for (const name of task.arm_skills.nearest) if (!catalog.skills?.some(skill => skill.id === name)) throw Error('Nearest arm skill absent from the packaged catalog');
  const entries = order(items), pinned = staged(directory, checkerRuntime(spec.runtime), entries).runtime;
  const runs = rowsFor(entries, directory, pinned, profile, providerCatalog);
  const grader = graderIdentity(spec.grader);
  return {
    schema: 'cs-2-developer-preparation/1', runnable: true, authorization: false, blockers: ['exact_plan_hash_per_block_required'], model_calls: 0, directory, prepared_at: String(at),
    executable, executable_sha256: sha(executableBytes), assets, candidate_assets: candidates.inspect(), source: cs1.identity(repository, sourceScope),
    fixture_revision: manifest.revision, fixture_sha256: sha(manifestBytes), rubric_sha256: sha(read(path.join(fixtures, 'rubric-v2.json'))), held_out: manifest.shared,
    profile_source: profileFile, profile_sha256: sha(profileBytes), provider_catalog: providerCatalog, provider_catalog_sha256: sha(providerCatalogBytes),
    spec_source: plain(path.resolve(specFile)), spec_sha256: sha(specBytes),
    toolchain: { node_version: process.version, node_executable: process.execPath, node_sha256: sha(read(process.execPath, 256 * 1024 * 1024)), platform: process.platform, architecture: process.arch },
    limits, budget_preflight: cs1.budgetPreflight(profile, limits.slot_micros), runtime: pinned, grader, permission_review: permissionReview(pinned, grader), benefit_rule: benefitRule,
    blocks: candidates.ids.map(skill => ({ skill, runs: runs.filter(row => row.block === skill).map(row => row.id) })),
    limitations: ['Local build receipts record provenance; they are not cryptographic build attestation.', 'Structural, in-run and functional checks do not establish reader quality; two independent blind readers score every case after grading.', 'Every block needs the exact prepared plan hash. The envelope has no headroom: no retries, replays, reallocation or transfer of unused allocation.', 'Unknown or unresolved charges, identity drift, authority violations or runner interruption halt the campaign for read-only reconciliation.'],
    runs,
  };
}
function prepare(specFile, destination) {
  const directory = plain(path.resolve(destination));
  if (within(repository, directory) || within(directory, repository) || fs.existsSync(directory)) throw Error('New private directory outside repository required');
  noParentInstructions(path.dirname(directory));
  privateDirectory(directory);
  const plan = describe(specFile, directory, Date.now()), { items } = tasks();
  const { cases } = staged(directory, plan.runtime, order(items));
  // Every input validates before the fresh directory is claimed. A partial
  // preparation stays owned and can never be overwritten or replayed.
  fs.mkdirSync(directory, { mode: 0o700 });
  write(path.join(directory, 'preparation-owner.json'), { schema: plan.schema, spec_sha256: plan.spec_sha256 });
  for (const name of ['runtime', 'claims']) fs.mkdirSync(path.join(directory, name), { mode: 0o700 });
  fs.writeFileSync(plan.runtime.checker, read(plan.runtime.source_checker, 256 * 1024 * 1024), { flag: 'wx', mode: 0o600 });
  fs.writeFileSync(plan.runtime.cases_file, cases, { flag: 'wx', mode: 0o600 });
  if (sha(read(plan.runtime.checker, 256 * 1024 * 1024)) !== plan.runtime.checker_sha256 || sha(read(plan.runtime.cases_file)) !== plan.runtime.cases_sha256) throw Error('Checker runtime changed during staging');
  for (const row of plan.runs) {
    const item = items.find(entry => entry.task.id === row.case_id), base = safeChild(directory, row.id), workspace = path.join(base, 'workspace');
    fs.mkdirSync(workspace, { recursive: true, mode: 0o700 });
    fs.mkdirSync(path.join(base, 'data'), { mode: 0o700 });
    for (const relative of row.directories) fs.mkdirSync(safeChild(workspace, relative), { recursive: true, mode: 0o700 });
    for (const [relative, bytes] of item.files) fs.writeFileSync(safeChild(workspace, relative), bytes, { flag: 'wx', mode: 0o600 });
    // Final prompts live in the frozen fixtures; the harness never rewrites them.
    write(path.join(base, 'prompt.txt'), item.task.prompt);
    write(path.join(base, 'profile.json'), row.profile);
  }
  write(path.join(directory, 'plan.json'), plan);
  const planBytes = read(path.join(directory, 'plan.json'), 16 * 1024 * 1024);
  return { plan: path.join(directory, 'plan.json'), sha256: sha(planBytes), runs: plan.runs.length, write_runs: limits.write_runs, model_calls: 0, aggregate_cap_micros: limits.cap_micros, aggregate_call_ceiling: limits.requests, blocks: candidates.ids };
}
module.exports = { prepare, describe, tasks, order, scaffold, preparedFiles, preparedDirectories, derivedProfile, profileReasons, qualificationEnds, checkerRuntime, graderIdentity, staged, permissionReview, limits, arms, checkerTests, checkerArguments, marker, nodeSha256, checkerBuildScope, checkerCommand, graderFiles, sourceScope, benefitRule };
if (require.main === module) {
  try {
    const [command, spec, destination, ...extra] = process.argv.slice(2);
    if (command !== 'prepare' || !spec || !destination || extra.length) throw Error('Usage: developer-prepare.cjs prepare <spec.json> <new-private-directory>');
    console.log(JSON.stringify(prepare(spec, destination)));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
