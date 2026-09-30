// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { json, fileHash, channel, validateReceipt } = require('./provenance.cjs');
const { recordPair } = require('./pair.cjs');
const stages = Object.freeze([
  'source-gate', 'provision', 'portable-contracts', 'production-build', 'native-package',
  'setup-package', 'vsix-package', 'pair', 'native-boundaries', 'installed-native', 'installed-editor',
]);
const matrix = Object.freeze([
  ['production-identity', 'Production build and identity', 'none', 'Production target/features, stable source/cache, final inventories and unsigned artifact digests.'],
  ['clean-installation', 'Clean installation', 'both', 'Clean standard-user supported Windows, no developer state, Unicode/custom paths, prerequisite/space failures and retry.'],
  ['first-cli-task', 'First useful CLI task', 'both', 'Supported onboarding/renewal and bounded task; missing/wrong credential/tools, stale metadata and denied effects. Live calls require separate budget admission.'],
  ['first-editor-task', 'First useful editor task', 'both', 'Actual installed VSIX and engine: observer/controller task, pause/resume/reviewed edit, restricted/uninitialized and incompatible configuration.'],
  ['cross-client-configuration', 'Cross-client configuration', 'both', 'Installed CLI/editor restrictions and stale base/import/policy refusal, including resume/reconnect. Source-contract tests are supporting evidence only.'],
  ['editor-lifecycle', 'Editor lifecycle and buffers', 'both', 'Actual restart/reload, changed roots, uncertain commands, partial edits, typing, undo, dirty buffers and rejected update.'],
  ['upgrade-rollback', 'Upgrade, rollback, uninstall', 'both', 'Distinct compatible released builds with paused history/accounting, interruption/concurrency, locked binaries, unsupported state and preserved user bytes.'],
  ['skills-memory', 'Skills, memory and helpers', 'both', 'Installed helper smoke, explicit model verification, offline memory build/query and observed network denial where claimed.'],
  ['trust-recovery', 'Trust, privacy and recovery', 'both', 'Path/process, MCP/hooks, redaction/purge, ciphertext/tamper/wrong-key, full-volume/kill and independent-machine recovery where required.'],
  ['performance', 'Performance and usability', 'both', 'Declared hardware, distributions/sample limits, retained history including 130 versions, long checks and pause/termination.'],
  ['owner-acceptance', 'Owner/product acceptance', 'both', 'Current FR/I/U01–U09 dispositions and factual owner review; publication is a separate authorized decision.'],
]);
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function ordinary(file) {
  if (!fs.lstatSync(file).isFile()) throw Error('Evidence must be an ordinary file');
  return fs.readFileSync(file);
}
function secrets(environment = process.env) {
  return Object.entries(environment).filter(([name, value]) => /(?:TOKEN|PASSWORD|SECRET|API_KEY|CREDENTIAL)$/i.test(name) && value?.length >= 12).map(([, value]) => value);
}
function sanitize(text, values = secrets()) {
  for (const value of values) text = text.split(value).join('[REDACTED]');
  return text.replace(/\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|AKIA[A-Z0-9]{16})\b/g, '[REDACTED]')
    .replace(/(authorization\s*[:=]\s*(?:bearer|basic)\s+)[^\s"']+/gi, '$1[REDACTED]');
}
function validateRun(run) {
  if (run.schema !== 'vcp-candidate-run/1' || !/^[a-f0-9]{40}$/.test(run.reviewed_commit || '') ||
      !['pass', 'fail', 'running'].includes(run.status) || !Array.isArray(run.stages) || !run.environment?.os || !run.environment?.node) throw Error('Invalid candidate run');
  const seen = new Set();
  for (const row of run.stages) {
    if (!stages.includes(row.id) || seen.has(row.id) || !['pass', 'fail', 'not run', 'running'].includes(row.status) ||
        !Array.isArray(row.command) || row.command.some(value => typeof value !== 'string') ||
        (row.status === 'pass' && (row.exit_code !== 0 || !row.log)) ||
        (['pass', 'fail'].includes(row.status) && !row.expected)) throw Error('Invalid or duplicate candidate stage');
    seen.add(row.id);
  }
  return run;
}
function packet(runFile, output) {
  const run = validateRun(json(runFile));
  if (fs.existsSync(output)) throw Error('Evidence packet requires a new directory');
  fs.mkdirSync(output, { recursive: true });
  const entries = [];
  const logTransformations = [];
  const retainedLogs = new Set();
  function save(relative, bytes) {
    const target = path.join(output, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, bytes, { flag: 'wx' });
    entries.push({ path: relative, sha256: hash(bytes), bytes: Buffer.byteLength(bytes) });
  }
  function receipt(file, relative) {
    const bytes = ordinary(file);
    if (secrets().some(value => bytes.includes(Buffer.from(value)))) throw Error('Receipt contains an environment credential; retain privately for review');
    save(relative, bytes); return JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, ''));
  }
  function log(file, relative, expected, structured = false) {
    if (retainedLogs.has(path.resolve(file))) return;
    const bytes = ordinary(file);
    // Redact parsed string values in JSON: text replacement could consume an
    // escape before a quote and leave a retained manifest unparsable.
    const sanitized = structured
      ? JSON.stringify(JSON.parse(bytes.toString('utf8'), (_key, value) => typeof value === 'string' ? sanitize(value) : value), null, 2) + '\n'
      : sanitize(bytes.toString('utf8'));
    if (expected && hash(bytes) !== expected) validationFailures.push(`Retained log changed: ${relative}`);
    save(relative, sanitized);
    retainedLogs.add(path.resolve(file));
    logTransformations.push({ path: relative, original_sha256: hash(bytes), retained_sha256: hash(sanitized), sanitized: sanitized !== bytes.toString('utf8') });
  }
  const observations = stages.map(id => {
    const row = run.stages.find(value => value.id === id);
    if (!row) return { id, status: 'not run', reason: 'No execution receipt; prerequisite failed or run interrupted.' };
    const result = { ...row, command: row.command.map(value => sanitize(value)) };
    if (result.reason) result.reason = sanitize(result.reason);
    if (row.log && fs.existsSync(row.log)) {
      const bytes = ordinary(row.log);
      result.original_log_sha256 = hash(bytes);
      result.log = `logs/${id}.log`;
      const sanitized = sanitize(bytes.toString('utf8'));
      save(result.log, sanitized); result.log_sha256 = hash(sanitized);
      result.log_sanitized = sanitized !== bytes.toString('utf8');
    } else if (row.status === 'pass') throw Error('Passing stage lacks actual retained log');
    else delete result.log;
    if (result.status === 'running') { result.status = 'fail'; result.reason = 'Interrupted stage has no completion receipt.'; }
    return result;
  });
  let pair = null;
  const validationFailures = [];
  const receipts = run.receipts || {};
  if (receipts.delivery) receipt(receipts.delivery, 'receipts/delivery-check.json');
  if (receipts.native && receipts.vsix && receipts.setup) {
    try {
      const native = json(receipts.native), setup = json(receipts.setup);
      if (!receipts.build || native.manifest.build.receipt_sha256 !== fileHash(receipts.build) || setup.build_receipt_sha256 !== fileHash(receipts.build)) throw Error('Exact native/setup build receipt missing');
      if (native.manifest.release.reviewed_commit !== run.reviewed_commit) throw Error('Candidate source differs from reviewed run');
      pair = recordPair(receipts.native, receipts.vsix, receipts.setup, path.join(output, 'pair.json'));
      entries.push({ path: 'pair.json', sha256: fileHash(path.join(output, 'pair.json')), bytes: fs.statSync(path.join(output, 'pair.json')).size });
    } catch (error) { validationFailures.push(sanitize(error.message)); }
  }
  for (const name of ['native', 'vsix', 'setup']) {
    if (!receipts[name]) continue;
    try {
    const record = receipt(receipts[name], `receipts/${name}.json`);
    const directory = path.dirname(receipts[name]);
    // Only public compiler/package logs, never recursive editor/fixture folders.
    const names = name === 'setup' ? ['setup-build.log', 'compiler-extract.log', 'compiler-provision.log'] : name === 'vsix' ? ['sdk-ts-install.log', 'sdk-ts-compile.log', 'vscode-install.log', 'vscode-compile.log'] : [];
    for (const logName of names) {
      const expected = name === 'setup' && logName === 'setup-build.log' ? record.log_sha256 :
        record.build?.compilation?.packages?.flatMap(row => [row.install_log, row.compile_log]).find(row => row?.file === logName)?.sha256;
      const file = path.join(directory, logName);
      if (fs.existsSync(file)) log(file, `logs/${name}-${logName}`, expected);
      else if (expected) validationFailures.push(`Missing retained log: ${logName}`);
    }
    const archiveName = name === 'native' ? record.package : record.archive?.file;
    const digest = name === 'native' ? record.archive_sha256 : record.archive?.sha256;
    if (typeof archiveName !== 'string' || /[\\/:]/.test(archiveName) || path.basename(archiveName) !== archiveName) throw Error('Invalid artifact basename');
    const bytes = ordinary(path.join(path.dirname(receipts[name]), archiveName));
    if (hash(bytes) !== digest) throw Error('Artifact changed while collecting evidence');
    save(`artifacts/${archiveName}`, bytes);
    } catch (error) { validationFailures.push(`${name}: ${sanitize(error.message)}`); }
  }
  if (receipts.build) {
    try {
    const build = receipt(receipts.build, 'receipts/build.json');
    const directory = path.dirname(receipts.build);
    // Exact public build inputs/logs only: no target tree, compiler caches or runtime fixtures.
    const source = { schema: 'vcp-release-source/1', commit: build.source_commit, dirty: false,
      files: build.inputs, content_sha256: build.source_content_sha256 };
    if (!Array.isArray(build.inputs) || !build.inputs.length || build.source_commit !== run.reviewed_commit ||
        hash(JSON.stringify(build.inputs)) !== build.source_content_sha256) validationFailures.push('Invalid retained build source identity');
    try {
      const repository = path.resolve(__dirname, '../..'), selected = channel(repository);
      receipt(path.join(repository, 'release/internal-beta.json'), 'receipts/release-channel.json');
      if (source.files?.find(row => row.path === 'release/internal-beta.json')?.sha256 !== selected.config_sha256) throw Error('Selected channel differs from retained build source');
      const nativeExecutableHash = json(receipts.native).manifest.files.find(row => row.path === 'vcp.exe')?.sha256;
      const release = validateReceipt(build, selected, source, nativeExecutableHash);
      if (!pair || JSON.stringify(release) !== JSON.stringify(pair.release)) throw Error('Complete artifact pair differs from validated build release');
      for (const [name, expected] of [['vcp.exe', build.executable_sha256], ['vcp-launch.exe', build.launcher_sha256]]) {
        if (fileHash(path.join(directory, name)) !== expected) throw Error(`Original build artifact differs: ${name}`);
      }
    } catch (error) { validationFailures.push(`production build: ${sanitize(error.message)}`); }
    for (const name of ['source-before.json', 'source-after.json', 'dependencies-before.json', 'dependencies-after.json']) {
      const file = path.join(directory, name);
      if (!fs.existsSync(file)) { validationFailures.push(`Missing retained build input: ${name}`); continue; }
      try {
        const retained = receipt(file, `receipts/${name}`);
        if (name.startsWith('source-')) {
          if (JSON.stringify(retained) !== JSON.stringify(source)) throw Error('Source inventory differs from build receipt');
        } else {
          const expectedHash = build[name === 'dependencies-before.json' ? 'dependencies_before_sha256' : 'dependencies_after_sha256'];
          const copied = entries.find(row => row.path === `receipts/${name}`);
          if (copied.sha256 !== expectedHash || JSON.stringify(retained) !== JSON.stringify(build.dependency_source) ||
              retained.schema !== 'vcp-release-dependencies/1' || retained.status !== 'verified' ||
              retained.target !== build.target || retained.components < 1 ||
              retained.workspace_lock_sha256 !== build.inputs?.find(row => row.path === 'src/third_party/codex/codex-rs/Cargo.lock')?.sha256 ||
              build.dependencies_before_sha256 !== build.dependencies_after_sha256) throw Error('Dependency inventory differs from build receipt');
        }
      } catch (error) { validationFailures.push(`${name}: ${sanitize(error.message)}`); }
    }
    for (const name of ['build.log', 'upstream-verification.log', 'upstream-verification-after.log']) {
      const expected = build[{ 'build.log': 'log_sha256', 'upstream-verification.log': 'upstream_before_sha256', 'upstream-verification-after.log': 'upstream_after_sha256' }[name]];
      if (!/^[a-f0-9]{64}$/.test(expected || '')) validationFailures.push(`Missing build evidence hash: ${name}`);
      const file = path.join(directory, name);
      if (fs.existsSync(file)) log(file, `logs/${name}`, expected);
      else validationFailures.push(`Missing retained log: ${name}`);
    }
    try {
      const artifacts = ordinary(path.join(directory, 'build.log')).toString('utf8').split(/\r?\n/)
        .filter(line => line.startsWith('{')).map(line => JSON.parse(line)).filter(row => row.reason === 'compiler-artifact');
      for (const artifact of [build.compiler_artifact, build.launcher_compiler_artifact]) {
        if (!artifact || !artifacts.some(row => JSON.stringify(row) === JSON.stringify(artifact))) throw Error('Required compiler artifact absent from retained build log');
      }
      const before = json(path.join(directory, 'upstream-verification.log')), after = json(path.join(directory, 'upstream-verification-after.log'));
      if (before.status !== 'pass' || before.component !== 'codex' || !Number.isSafeInteger(before.files) || before.files < 1 ||
          !/^[a-f0-9]{64}$/.test(before.files_sha256 || '') || JSON.stringify(before) !== JSON.stringify(after)) throw Error('Retained upstream verification is not stable and successful');
    } catch (error) { validationFailures.push(`build evidence: ${sanitize(error.message)}`); }
    } catch (error) { validationFailures.push(`build: ${sanitize(error.message)}`); }
  }
  // Builders can fail before emitting a result. Preserve their narrowly named
  // public logs without traversing caches, staging trees or private fixtures.
  for (const [stage, names] of [
    ['build', ['build.log', 'upstream-verification.log', 'upstream-verification-after.log']],
    ['setup', ['setup-build.log', 'compiler-provision.log']],
  ]) {
    const root = path.join(path.dirname(path.resolve(runFile)), stage);
    if (!fs.existsSync(root)) continue;
    // build-production uses Guid.ToString(); build-setup uses ToString('N').
    const directoryName = stage === 'setup' ? /^[a-f0-9]{32}$/ : /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
    for (const child of fs.readdirSync(root, { withFileTypes: true })) {
      if (!child.isDirectory() || !directoryName.test(child.name)) continue;
      for (const name of names) {
        const file = path.join(root, child.name, name);
        if (fs.existsSync(file)) log(file, `logs/${stage}-${child.name}-${name}`);
      }
    }
  }
  for (const name of ['sdk-ts-install.log', 'sdk-ts-compile.log', 'vscode-install.log', 'vscode-compile.log']) {
    const file = path.join(path.dirname(path.resolve(runFile)), 'vsix', name);
    if (fs.existsSync(file)) log(file, `logs/vsix-${name}`);
  }
  // The delivery harness writes child diagnostics separately from its summary.
  // Retain only the manifest and exact attempt stdout/stderr names; never walk
  // test fixtures, private stores, browser profiles or arbitrary artifact paths.
  const contracts = path.join(path.dirname(path.resolve(runFile)), 'contracts');
  if (fs.existsSync(contracts)) {
    try {
      const plain = file => {
        for (let current = file;; current = path.dirname(current)) {
          if (fs.lstatSync(current).isSymbolicLink()) throw Error('Redirected contract evidence');
          if (current === path.dirname(current)) break;
        }
        return file;
      };
      const guid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
      for (const entry of fs.readdirSync(plain(contracts), { withFileTypes: true })) {
        if (!entry.isDirectory() || !guid.test(entry.name)) continue;
        const directory = plain(path.join(contracts, entry.name));
        const manifestFile = plain(path.join(directory, 'manifest.json'));
        const manifest = JSON.parse(ordinary(manifestFile));
        if (manifest.schema_version !== 1 || manifest.run_id !== entry.name || manifest.suite !== 'fast' ||
            !Array.isArray(manifest.attempts)) throw Error('Invalid contract manifest');
        log(manifestFile, `contracts/${entry.name}/manifest.json`, undefined, true);
        const seen = new Set();
        for (const attempt of manifest.attempts) {
          if (!guid.test(attempt.attempt_id) || seen.has(attempt.attempt_id)) throw Error('Invalid contract attempt identity');
          seen.add(attempt.attempt_id);
          for (const stream of ['stdout', 'stderr']) {
            const name = `${attempt.attempt_id}-${stream}.log`;
            const file = path.join(directory, name);
            const artifact = (attempt.artifacts || []).find(row => row.path === name);
            if (!fs.existsSync(file)) {
              if (artifact) validationFailures.push(`Missing contract log: ${name}`);
              continue;
            }
            if (artifact && !/^[a-f0-9]{64}$/.test(artifact.sha256 || '')) throw Error('Invalid contract log digest');
            // Interrupted attempts may have logs before their artifact hashes
            // are finalized. Keep those diagnostics without claiming success.
            log(plain(file), `contracts/${entry.name}/${name}`, artifact?.sha256);
          }
        }
      }
    } catch (error) { validationFailures.push(`contract evidence: ${sanitize(error.message)}`); }
  }
  const allPass = stages.every(id => observations.find(row => row.id === id)?.status === 'pass');
  const rows = matrix.map(([id, area, store, expected]) => ({ id, area, store, expected,
    status: 'not run', reason: 'Requires final-artifact qualification; pipeline or source-contract success alone does not satisfy this area.',
    source_commit: run.reviewed_commit, artifacts: pair?.artifacts || null, environment: run.environment, command: null, actual: null }));
  if (pair && !validationFailures.length && ['production-build', 'native-package', 'setup-package', 'vsix-package', 'pair'].every(id => observations.find(row => row.id === id)?.status === 'pass')) {
    rows[0].status = 'pass'; rows[0].reason = null;
    rows[0].command = observations.filter(row => ['production-build', 'native-package', 'setup-package', 'vsix-package', 'pair'].includes(row.id)).map(row => row.command);
    rows[0].actual = 'Strict builders and independent final-byte pairing passed; unsigned candidate only.';
    rows[0].evidence = ['pair.json', 'receipts/build.json', 'receipts/native.json', 'receipts/vsix.json', 'receipts/setup.json'];
  }
  const result = { schema: 'vcp-beta-evidence/1', status: 'qualification-required', reviewed_commit: run.reviewed_commit,
    pipeline_status: run.status === 'pass' && allPass && pair && !validationFailures.length ? 'pass' : 'fail', validation_failures: validationFailures,
    pair_id: pair?.pair_id || null, environment: run.environment, observations, matrix: rows, log_transformations: logTransformations,
    publication: { authorized: false, owner_acceptance: 'not run' },
    limitations: ['Hosted Windows build image has developer tools; it is not clean standard-user Windows qualification.',
      'Source and synthetic fixture tests do not qualify installed-product behavior.',
      'No automatic live provider calls, model acquisition, signing, publication or owner approval.',
      'GitHub artifacts expire after 90 days; export the complete hashed packet to approved durable storage before expiry.'],
    files: entries.sort((a,b) => a.path.localeCompare(b.path)) };
  fs.writeFileSync(path.join(output, 'evidence.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(path.join(output, 'SHA256SUMS'), [...result.files, { path: 'evidence.json', sha256: fileHash(path.join(output, 'evidence.json')) }].map(row => `${row.sha256}  ${row.path}`).join('\n') + '\n', { flag: 'wx' });
  return result;
}
if (require.main === module) {
  try { const [run, output, ...extra] = process.argv.slice(2); if (!run || !output || extra.length) throw Error('Use evidence.cjs <run.json> <new-packet-directory>'); const status = packet(run, output).pipeline_status; console.log(JSON.stringify({ status, output })); if (status !== 'pass') process.exitCode = 1; }
  catch (error) { console.error('Evidence collection failed: ' + error.message); process.exitCode = 1; }
}
module.exports = { stages, matrix, sanitize, validateRun, packet };
