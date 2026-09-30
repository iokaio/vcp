// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const provenance = require('../../../../scripts/release/provenance.cjs');
const { portable, verifyManifest, MAX_FILES, MAX_FILE_BYTES } = require('../../../../scripts/package-inventory.cjs');
const { fileHash } = require('./inventory.cjs');
const check = (value, message) => { if (!value) throw Error(message); };

function releaseMode(native, options) {
  const strict = native.status === 'release-candidate' || native.manifest?.build?.status === 'verified-release-build' || !!native.manifest?.release;
  check(!(options['--reviewed-commit'] || options['--build-receipt']) || strict, 'Reviewed commit and build receipt require a verified native release candidate');
  if (strict) {
    check(/^[a-f0-9]{40}$/.test(options['--reviewed-commit'] || ''), 'Beta VSIX requires --reviewed-commit');
    check(!options['--version'], 'Beta VSIX version is selected by the release channel');
    check(path.isAbsolute(options['--build-receipt'] || ''), 'Beta VSIX requires an absolute --build-receipt with retained original build evidence');
    check(native.status === 'release-candidate' && native.manifest?.build?.status === 'verified-release-build', 'Unverified native release candidate');
  }
  return strict;
}

function validateNativeIdentity(native, selected, source, engineHash, receipt) {
  const manifest = native.manifest;
  check(native.schema === 'vcp-distribution-result/1' && native.status === 'release-candidate' &&
    manifest?.build?.status === 'verified-release-build' && manifest.source?.dirty === false &&
    manifest.source.git_commit === source.commit, 'Native release source differs from reviewed checkout');
  const expected = provenance.releaseIdentity(selected, source, engineHash);
  check(JSON.stringify(manifest.release) === JSON.stringify(expected), 'Native release identity mismatch');
  check(manifest.build.receipt === 'build-receipt.json' && /^[a-f0-9]{64}$/.test(manifest.build.receipt_sha256 || ''), 'Native release receipt required');
  for (const [name, sha256] of [['vcp.exe', engineHash], ['vcp-launch.exe', receipt.launcher_sha256], ['build-receipt.json', manifest.build.receipt_sha256]]) {
    const rows = manifest.files?.filter(row => row.path === name);
    check(rows?.length === 1 && rows[0].sha256 === sha256, 'Native release file mismatch: ' + name);
  }
  provenance.validateReceipt(receipt, selected, source, engineHash);
  return expected;
}

// Stream the native archive (the engine is much larger than a VSIX entry).
// Match every entry to the separately verified staged payload, including its manifest.
function verifyNativeArchive(filename, expected) {
  const yauzl = require('yauzl');
  return new Promise((resolve, reject) => {
    yauzl.open(filename, { lazyEntries: true, strictFileNames: true }, (error, zip) => {
      if (error) return reject(error);
      const remaining = new Map(expected.map(row => [row.path, row]));
      const seen = new Set(); let count = 0;
      const fail = error => { zip.close(); reject(error); };
      zip.on('error', fail);
      zip.on('end', () => remaining.size ? reject(Error('Native archive omits release payload')) : resolve());
      zip.on('entry', entry => {
        let name;
        try { name = portable(entry.fileName); } catch (error) { return fail(error); }
        const expected = remaining.get(name);
        if (!expected || seen.has(name.toLowerCase()) || ++count > MAX_FILES + 1 ||
          entry.uncompressedSize > MAX_FILE_BYTES || entry.uncompressedSize !== expected.bytes ||
          ((entry.externalFileAttributes >>> 16) & 0xf000) === 0xa000) return fail(Error('Invalid native archive entry'));
        seen.add(name.toLowerCase());
        zip.openReadStream(entry, (error, stream) => {
          if (error) return fail(error);
          const hash = crypto.createHash('sha256'); let length = 0;
          stream.on('error', fail);
          stream.on('data', bytes => {
            length += bytes.length;
            if (length > expected.bytes) { stream.destroy(); fail(Error('Native archive entry exceeds bound')); }
            else hash.update(bytes);
          });
          stream.on('end', () => {
            if (length !== expected.bytes || hash.digest('hex') !== expected.sha256) return fail(Error('Native archive payload mismatch'));
            remaining.delete(name); zip.readEntry();
          });
        });
      });
      zip.readEntry();
    });
  });
}

async function verifyNativeRelease(repo, nativeFile, engine, reviewedCommit, buildReceipt) {
  const native = provenance.json(nativeFile), selected = provenance.channel(repo);
  provenance.verifyVersions(repo, selected);
  const source = provenance.captureSource(repo, reviewedCommit);
  const engineHash = provenance.fileHash(engine);
  const directory = path.dirname(nativeFile), packageRoot = path.join(directory, 'package');
  const receiptFile = path.join(packageRoot, 'build-receipt.json'), receipt = provenance.json(receiptFile);
  const release = validateNativeIdentity(native, selected, source, engineHash, receipt);
  check(provenance.fileHash(receiptFile) === native.manifest.build.receipt_sha256, 'Native receipt bytes changed');
  check(provenance.fileHash(buildReceipt) === native.manifest.build.receipt_sha256, 'Original build receipt differs from native package');
  const manifestFile = path.join(packageRoot, 'manifest.json');
  check(JSON.stringify(provenance.json(manifestFile)) === JSON.stringify(native.manifest), 'Staged native manifest changed');
  verifyManifest(packageRoot, native.manifest);
  check(native.manifest.notices?.schema === 'vcp-notice-bundle/1' && /^[a-f0-9]{64}$/.test(native.manifest.notices.inventory_sha256 || ''), 'Native release notices required');
  provenance.verifyPayloadSources(packageRoot, receipt, native.manifest.notices.inventory_sha256);
  check(typeof native.package === 'string' && portable(native.package) === path.basename(native.package), 'Invalid native archive filename');
  const archive = path.join(directory, native.package);
  check(await fileHash(archive) === native.archive_sha256, 'Native archive bytes changed');
  await verifyNativeArchive(archive, [...native.manifest.files, { path: 'manifest.json', bytes: fs.statSync(manifestFile).size, sha256: provenance.fileHash(manifestFile) }]);
  // Execute only after the selected receipt and every payload/archive hash bind it.
  const original = provenance.verifyBuild(repo, buildReceipt, engine, reviewedCommit);
  check(JSON.stringify(original) === JSON.stringify(release) && provenance.fileHash(engine) === engineHash, 'Original build identity or engine changed during release verification');
  return { release, source, engineHash, nativeHash: provenance.fileHash(nativeFile) };
}

function toolHash(directory) {
  const rows = [];
  function visit(relative) {
    const filename = path.join(directory, relative), stat = fs.lstatSync(filename);
    check(!stat.isSymbolicLink(), 'Release tool directory contains a link');
    if (stat.isDirectory()) for (const name of fs.readdirSync(filename).sort()) visit(relative ? relative + '/' + name : name);
    else {
      check(rows.length < 20000, 'Release tool inventory exceeds bound');
      rows.push({ path: relative, sha256: provenance.fileHash(filename) });
    }
  }
  visit('');
  return provenance.hash(JSON.stringify(rows));
}

function checkEnvironment(environment = process.env) {
  check(!Object.entries(environment).some(([name, value]) => /^NODE_(OPTIONS|PATH)$/i.test(name) && value), 'Strict VSIX packaging refuses Node preload/options or module path overrides');
}

function checkBuildDirectories(directory) {
  // npm and tsc write ignored build trees. Refuse redirects before either tool
  // starts, including hard-linked output files that could alias unrelated data.
  for (const name of ['node_modules', 'dist']) {
    const target = path.join(directory, name);
    let stat;
    try { stat = fs.lstatSync(target); } catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    check(stat.isDirectory() && !stat.isSymbolicLink(), 'Redirected build directory refused: ' + name);
    if (name === 'dist') {
      const pending = [target]; let count = 0;
      while (pending.length) for (const entry of fs.readdirSync(pending.pop(), { withFileTypes: true })) {
        const filename = path.join(entry.parentPath, entry.name), info = fs.lstatSync(filename);
        check(++count <= 20000 && !info.isSymbolicLink() && (info.isDirectory() || (info.isFile() && info.nlink === 1)), 'Redirected or multiply linked compiler output refused');
        if (info.isDirectory()) pending.push(filename);
      }
    }
  }
}

function compileRelease(repo, reviewedCommit, output) {
  checkEnvironment();
  provenance.verifyVersions(repo, provenance.channel(repo));
  provenance.captureSource(repo, reviewedCommit);
  for (const name of ['sdk-ts', 'vscode']) checkBuildDirectories(path.join(repo, 'src/packages', name));
  // Official Windows Node places npm beside node.exe. Do not discover a script
  // through PATH or accept npm configuration that can run a different toolchain.
  const npmRoot = path.join(path.dirname(process.execPath), 'node_modules/npm');
  const npmCli = path.join(npmRoot, 'bin/npm-cli.js');
  const nodeHash = provenance.fileHash(process.execPath), npmHash = toolHash(npmRoot);
  const environment = Object.fromEntries(Object.entries(process.env).filter(([name]) => !/^npm_config_/i.test(name)));
  const userConfig = path.join(output, 'npm-user.conf'), globalConfig = path.join(output, 'npm-global.conf');
  fs.writeFileSync(userConfig, '', { flag: 'wx' }); fs.writeFileSync(globalConfig, '', { flag: 'wx' });
  const builds = [];
  const run = (name, operation, args, directory, timeout) => {
    const file = name + '-' + operation + '.log';
    const result = spawnSync(process.execPath, args, { cwd: directory, env: environment, windowsHide: true,
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout, maxBuffer: 16 * 1024 * 1024 });
    const log = 'node ' + JSON.stringify(args) + '\n' + (result.stdout || '') + (result.stderr || '') +
      '\nexit=' + result.status + '; error=' + (result.error?.code || 'none') + '\n';
    fs.writeFileSync(path.join(output, file), log, { flag: 'wx' });
    check(!result.error && result.status === 0, 'Release ' + name + ' ' + operation + ' failed; inspect ' + file);
    return { file, sha256: provenance.hash(log) };
  };
  for (const name of ['sdk-ts', 'vscode']) {
    const directory = path.join(repo, 'src/packages', name);
    const command = [npmCli, 'ci', '--offline', '--include=dev', '--ignore-scripts', '--no-audit', '--no-fund', '--userconfig', userConfig, '--globalconfig', globalConfig];
    const installLog = run(name, 'install', command, directory, 180000);
    // Never let package resolution fall back to an unrelated ancestor compiler.
    const compiler = path.join(directory, 'node_modules/typescript/bin/tsc');
    const manifest = provenance.json(path.join(directory, 'package.json'));
    const version = provenance.json(path.join(path.dirname(compiler), '../package.json')).version;
    check(version === manifest.devDependencies.typescript, 'Installed TypeScript differs from pinned package');
    const compilerRoot = path.dirname(path.dirname(compiler)), before = toolHash(compilerRoot);
    const compileLog = run(name, 'compile', [compiler, '-p', 'tsconfig.json'], directory, 120000);
    check(toolHash(compilerRoot) === before, 'TypeScript compiler changed during build');
    builds.push({ package: name, install: ['npm', 'ci', '--offline', '--include=dev', '--ignore-scripts', '--no-audit', '--no-fund'], command: ['node', 'typescript/bin/tsc', '-p', 'tsconfig.json'],
      lock_sha256: provenance.fileHash(path.join(directory, 'package-lock.json')), typescript: version, compiler_tree_sha256: before,
      install_log: installLog, compile_log: compileLog });
  }
  check(nodeHash === provenance.fileHash(process.execPath) && npmHash === toolHash(npmRoot), 'Release toolchain changed during build');
  return { node_sha256: nodeHash, npm_version: provenance.json(path.join(npmRoot, 'package.json')).version, npm_tree_sha256: npmHash, packages: builds };
}

module.exports = { releaseMode, validateNativeIdentity, verifyNativeArchive, verifyNativeRelease, compileRelease, checkEnvironment };
