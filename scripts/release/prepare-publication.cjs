// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path');
const { fileHash, pairIdentity, releaseIdentity } = require('./provenance.cjs');
const { stages } = require('./evidence.cjs');
const repository = 'https://github.com/iokaio/vcp';
const sha = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const check = (condition, message) => { if (!condition) throw Error(message); };
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function relative(file) {
  check(typeof file === 'string' && file.length <= 1024 && !/[\\:\x00-\x1f\x7f]/.test(file) &&
    file.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part) &&
      !/[<>"|?*]/.test(part) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part)),
  'Noncanonical packet path');
  return file;
}
function ordinary(file, directory = false) {
  const absolute = path.resolve(file), parsed = path.parse(absolute);
  let current = parsed.root;
  const parts = absolute.slice(parsed.root.length).split(path.sep).filter(Boolean);
  for (let i = 0; i < parts.length; i++) {
    current = path.join(current, parts[i]);
    const stat = fs.lstatSync(current);
    check(!stat.isSymbolicLink(), 'Linked publication input or output path');
    check(i === parts.length - 1 && !directory ? stat.isFile() : stat.isDirectory(),
      'Publication path has the wrong file type');
  }
  check(directory || parts.length > 0, 'Publication file required');
  return absolute;
}
function readJson(file) {
  check(fs.statSync(file).size <= 32 * 1024 * 1024, 'Oversized publication receipt');
  return JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
}
function decimal(value, label) {
  check(typeof value === 'string' || Number.isSafeInteger(value), 'Invalid ' + label);
  const text = String(value);
  check(/^[1-9][0-9]{0,19}$/.test(text), 'Invalid ' + label);
  return text;
}
function preparePublication(options) {
  const { expectedPair, expectedCommit } = options;
  check(sha(expectedPair) && typeof expectedCommit === 'string' && /^[a-f0-9]{40}$/.test(expectedCommit),
    'Explicit expected pair and reviewed commit required');
  const runId = decimal(options.runId, 'candidate run'), attempt = decimal(options.attempt, 'candidate attempt');
  const packet = ordinary(options.packet, true), output = path.resolve(options.output);
  ordinary(path.dirname(output), true);
  check(!fs.existsSync(output), 'Publication output must be a new directory');
  const inside = path.relative(packet, output);
  check(inside && (inside === '..' || inside.startsWith('..' + path.sep) || path.isAbsolute(inside)),
    'Publication output must be outside the packet');

  const sumsFile = ordinary(path.join(packet, 'SHA256SUMS'));
  check(fs.statSync(sumsFile).size <= 1024 * 1024, 'Oversized packet checksum list');
  const lines = fs.readFileSync(sumsFile, 'utf8').replace(/\r?\n$/, '').split(/\r?\n/), sums = new Map(), names = new Set();
  for (const line of lines) {
    const match = /^([a-f0-9]{64})  (.+)$/.exec(line);
    check(match, 'Malformed packet checksum row');
    const name = relative(match[2]);
    check(name !== 'SHA256SUMS' && !names.has(name.toLowerCase()), 'Duplicate or recursive packet checksum path');
    names.add(name.toLowerCase());
    const file = ordinary(path.join(packet, ...name.split('/')));
    check(fileHash(file) === match[1], 'Packet checksum mismatch: ' + name);
    sums.set(name, { file, sha256: match[1] });
  }
  function input(name) {
    check(sums.has(name), 'Required packet input is not checksum-bound: ' + name);
    return sums.get(name);
  }
  const evidence = readJson(input('evidence.json').file), recorded = readJson(input('pair.json').file);
  check(evidence.schema === 'vcp-beta-evidence/1' && evidence.status === 'qualification-required' &&
    evidence.selection_status === 'pass' && ['incomplete', 'pass'].includes(evidence.pipeline_status) &&
    Array.isArray(evidence.validation_failures) && evidence.validation_failures.length === 0 &&
    evidence.reviewed_commit === expectedCommit && evidence.pair_id === expectedPair,
  'Candidate evidence is not an admitted selected pair');
  const full = evidence.pipeline_status === 'pass', end = full ? stages.length - 1 : stages.indexOf('pair');
  check(evidence.stop_after === (full ? 'installed-editor' : 'pair') &&
    Array.isArray(evidence.observations) && evidence.observations.length === stages.length,
  'Candidate stage selection is inconsistent');
  for (const [i, row] of evidence.observations.entries()) {
    check(row.id === stages[i] && row.status === (i <= end ? 'pass' : 'not run'),
      'Candidate stages are failed, missing or out of order');
    if (i <= end) {
      check(row.exit_code === 0 && typeof row.log === 'string', 'Successful candidate stage lacks exit/log evidence');
      check(row.log.startsWith('logs/'), 'Candidate stage log is outside packet logs');
      input(row.log);
    }
  }
  const paired = evidence.observations[stages.indexOf('pair')];
  check(paired.verified_pair_sha256 === input('pair.json').sha256, 'Candidate stage pair receipt mismatch');
  check(typeof paired.ended_at === 'string' && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,7})?Z$/.test(paired.ended_at) &&
    Number.isFinite(Date.parse(paired.ended_at)), 'Candidate pair completion date required');

  const nativeInput = input('receipts/native.json'), vsixInput = input('receipts/vsix.json');
  const setupInput = input('receipts/setup.json'), buildInput = input('receipts/build.json');
  const native = readJson(nativeInput.file), vsix = readJson(vsixInput.file);
  const setup = readJson(setupInput.file), build = readJson(buildInput.file);
  const pair = pairIdentity(native, vsix, setup), release = pair.release;
  const receipts = { native_sha256: nativeInput.sha256, vsix_sha256: vsixInput.sha256, setup_sha256: setupInput.sha256 };
  check(pair.pair_id === expectedPair && recorded.schema === pair.schema && recorded.status === pair.status &&
    recorded.pair_id === pair.pair_id && same(recorded.release, release) && same(recorded.artifacts, pair.artifacts) &&
    same(recorded.signing, pair.signing) && same(recorded.receipts, receipts), 'Recorded release pair or receipt binding mismatch');
  check(release.reviewed_commit === expectedCommit && native.manifest.source.git_commit === expectedCommit &&
    release.channel === 'internal-beta' && release.target === 'x86_64-pc-windows-msvc' && ['unsigned', 'signed'].includes(release.signing?.status) &&
    sha(release.candidate_id) && sha(release.source_content_sha256) && sha(release.config_sha256), 'Release source/channel mismatch');
  check(same(release, releaseIdentity(release, { commit: expectedCommit, content_sha256: release.source_content_sha256 },
    build.executable_sha256)), 'Release candidate identity mismatch');
  const signing = require('./signing.cjs');
  const transformation = signing.validateBinding(native.manifest.signing, release, buildInput.sha256, 'native',
    { engine: build.executable_sha256, launcher: build.launcher_sha256 });
  if (transformation) {
    for (const [name, binding] of [['native', native.manifest.signing], ['setup', setup.signing]]) {
      const retained = input(`receipts/${name}-signing.json`);
      check(retained.sha256 === binding.receipt_sha256 && same(readJson(retained.file), binding.transformation), 'Retained signing receipt mismatch');
      for (const row of binding.transformation.files) {
        const nameInPacket = `logs/${name}-${row.role}-signature.log`, retainedLog = input(nameInPacket);
        const logs = evidence.log_transformations?.filter(value => value.path === nameInPacket);
        check(logs?.length === 1 && logs[0].original_sha256 === row.verification.log_sha256 &&
          logs[0].retained_sha256 === retainedLog.sha256, 'Signing verification log binding mismatch');
        if (name === 'setup') {
          const original = input(`signing/${row.role}/unsigned.exe`), signed = input(`signing/${row.role}/signed.exe`);
          check(original.sha256 === row.input_sha256 && signed.sha256 === row.output_sha256, 'Setup signing bytes mismatch');
          signing.verifyPeTransformation(original.file, signed.file);
        }
      }
    }
    for (const [name, role] of [['vcp.exe', 'engine'], ['vcp-launch.exe', 'launcher']]) {
      const original = input('build-output/' + name), signed = input('signed/' + name);
      const row = transformation.files.find(row => row.role === role);
      check(original.sha256 === row.input_sha256 && signed.sha256 === row.output_sha256, 'Retained signing bytes mismatch');
      signing.verifyPeTransformation(original.file, signed.file);
    }
  }
  check(native.manifest.build.receipt_sha256 === buildInput.sha256 && setup.build_receipt_sha256 === buildInput.sha256 &&
    vsix.engine.native_manifest_sha256 === nativeInput.sha256 && build.schema === 'vcp-local-build/1' &&
    build.exit_code === 0 && build.cargo_exit_code === 0 && build.qualification_build === false && build.profile === 'release' &&
    build.source_dirty === false && build.source_stable === true && build.source_commit === expectedCommit &&
    build.source_content_sha256 === release.source_content_sha256 && build.target === release.target &&
    signing.payloadHashes(build, transformation).executable_sha256 === vsix.engine.executable_sha256 && same(build.release, release),
  'Production build/receipt binding mismatch');
  check(typeof release.native_version === 'string' && /^\d+\.\d+\.\d+-beta\.\d+$/.test(release.native_version) &&
    typeof release.vsix_version === 'string' && /^\d+\.\d+\.\d+$/.test(release.vsix_version), 'Invalid beta release version');
  const tag = `v${release.native_version}-${pair.pair_id.slice(0, 12)}`;
  const href = name => `${repository}/releases/download/${tag}/${encodeURIComponent(name)}`;
  const artifacts = [
    ['setup', setup.archive.file, setup.archive.sha256, '.exe'],
    ['zip', native.package, native.archive_sha256, '.zip'],
    ['vsix', vsix.archive.file, vsix.archive.sha256, '.vsix'],
  ].map(([kind, name, digest, extension]) => {
    check(typeof name === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(name) && name.endsWith(extension),
      'Invalid release artifact filename');
    relative(name);
    const entry = input('artifacts/' + name);
    check(entry.sha256 === digest, 'Release artifact differs from selected pair');
    return { kind, name, sha256: digest, bytes: fs.statSync(entry.file).size, href: href(name) };
  });
  const result = {
    version: release.native_version, vsixVersion: release.vsix_version, extensionId: vsix.extension.id, pairId: pair.pair_id,
    commit: expectedCommit, tag, candidateRunId: runId, candidateAttempt: attempt,
    runUrl: `${repository}/actions/runs/${runId}/attempts/${attempt}`,
    candidateAt: new Date(paired.ended_at).toISOString(), manifestHref: href('release.json'), checksumHref: href('SHA256SUMS'),
    qualification: { selectionStatus: 'pass', pipelineStatus: evidence.pipeline_status, status: 'qualification-required',
      scope: 'limited-internal-beta', signing: release.signing.status }, artifacts,
  };
  const source = `${repository}/blob/${expectedCommit}`;
  const notes = `# VCP ${result.version} internal beta\n\n` +
    `${transformation ? 'Signed' : 'Unsigned'} Windows x64 beta for a limited developer-host and manual-testing handoff. Full qualification remains incomplete; this is not a clean-host support or production-readiness claim.\n\n` +
    `Candidate pair completed: ${result.candidateAt}. This is the build observation date, not the publication date.\n\n` +
    `- Source: [${expectedCommit}](${repository}/commit/${expectedCommit})\n` +
    `- Candidate run: [${runId}, attempt ${attempt}](${result.runUrl})\n` +
    `- Pair: \`${result.pairId}\`\n` +
    `- Automated selection: pass; full pipeline: ${evidence.pipeline_status}; manual qualification: required.\n` +
    `- [Installation guide](${source}/docs/usage/beta-installation.md)\n` +
    `- [Onboarding guide](${source}/docs/usage/beta-onboarding.md)\n` +
    `- [Manual testing checklist](${source}/docs/release-planning/04-manual-testing-checklist.md)\n` +
    `- [Known limitations](${source}/docs/usage/beta-known-issues.md)\n\n` +
    `Verify the downloaded assets against SHA256SUMS. Only the selected installer, portable ZIP, VSIX, public manifest and checksums are distributed; private qualification receipts and logs are excluded.\n`;

  // Nothing is written until the complete selected packet and pair pass admission.
  fs.mkdirSync(output);
  const assets = path.join(output, 'assets'); fs.mkdirSync(assets);
  for (const row of artifacts) {
    const destination = path.join(assets, row.name);
    fs.copyFileSync(input('artifacts/' + row.name).file, destination, fs.constants.COPYFILE_EXCL);
    check(fileHash(destination) === row.sha256, 'Copied publication artifact changed');
  }
  fs.writeFileSync(path.join(assets, 'release.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  const published = [...artifacts.map(row => ({ name: row.name, sha256: row.sha256 })),
    { name: 'release.json', sha256: fileHash(path.join(assets, 'release.json')) }].sort((a, b) => a.name.localeCompare(b.name));
  fs.writeFileSync(path.join(assets, 'SHA256SUMS'), published.map(row => `${row.sha256}  ${row.name}`).join('\n') + '\n', { flag: 'wx' });
  fs.writeFileSync(path.join(output, 'notes.md'), notes, { flag: 'wx' });
  return result;
}
if (require.main === module) {
  try {
    const names = { '--packet': 'packet', '--output': 'output', '--run-id': 'runId', '--attempt': 'attempt',
      '--expected-pair': 'expectedPair', '--expected-commit': 'expectedCommit' }, options = {};
    const args = process.argv.slice(2);
    for (let i = 0; i < args.length; i += 2) {
      const key = names[args[i]];
      check(key && args[i + 1] && !(key in options), 'Use each required publication option exactly once');
      options[key] = args[i + 1];
    }
    check(Object.keys(options).length === Object.keys(names).length, 'All six publication options are required');
    console.log(JSON.stringify(preparePublication(options)));
  } catch (error) { console.error('Publication preparation failed: ' + error.message); process.exitCode = 1; }
}
module.exports = { preparePublication };
