// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { createVSIX } = require('@vscode/vsce');
const { inventory, sha256, fileHash } = require('./inventory.cjs');
const { prepareOutput } = require('./output.cjs');
const root = path.resolve(__dirname, '..');
const repo = path.resolve(root, '../../..');
async function main() {
  const options = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    const key = process.argv[i];
    if (!['--engine', '--engine-manifest', '--output', '--version'].includes(key) || options[key] || !process.argv[i + 1]) throw new Error('Invalid packaging arguments');
    options[key] = process.argv[i + 1];
  }
  if (!options['--engine'] || !path.isAbsolute(options['--engine']) || !options['--engine-manifest']) throw new Error('Explicit absolute engine and native distribution result required');
  if (options['--version'] && !/^0\.1\.[1-9][0-9]*$/.test(options['--version'])) throw new Error('Qualification successor must use 0.1.patch');
  const nativeBytes = fs.readFileSync(options['--engine-manifest']);
  const native = JSON.parse(nativeBytes);
  const engineHash = await fileHash(options['--engine']);
  const nativeExecutable = native.manifest?.files?.filter(row => row.path === 'vcp.exe');
  if (native.schema !== 'vcp-distribution-result/1' || nativeExecutable?.length !== 1 || nativeExecutable[0].sha256 !== engineHash || !/^[a-f0-9]{40}$/.test(native.manifest?.source?.git_commit || '') || !['caller-supplied-unverified', 'recorded-local-build'].includes(native.manifest?.build?.status)) throw new Error('Engine provenance does not match native distribution result');
  if (native.manifest.build.status === 'caller-supplied-unverified' && native.manifest.build.executable_sha256 !== engineHash) throw new Error('Engine build hash mismatch');
  if (native.manifest.build.status === 'recorded-local-build') {
    const receiptRows = native.manifest.files.filter(row => row.path === 'build-receipt.json');
    if (native.manifest.build.receipt !== 'build-receipt.json' || receiptRows.length !== 1 || !/^[a-f0-9]{64}$/.test(native.manifest.build.receipt_sha256) || receiptRows[0].sha256 !== native.manifest.build.receipt_sha256) throw new Error('Native build receipt provenance mismatch');
    const receiptBytes = fs.readFileSync(path.join(path.dirname(path.resolve(options['--engine-manifest'])), 'package/build-receipt.json'));
    const receipt = JSON.parse(receiptBytes);
    if (sha256(receiptBytes) !== native.manifest.build.receipt_sha256 || receipt.schema !== 'vcp-local-build/1' || receipt.exit_code !== 0 || receipt.executable_sha256 !== engineHash) throw new Error('Native build receipt does not bind engine');
  }
  const git = args => execFileSync('git', ['-c', `safe.directory=${repo.replaceAll('\\', '/')}`, ...args], { cwd: repo, encoding: 'utf8', windowsHide: true }).trim();
  const source = { git_commit: git(['rev-parse', 'HEAD']), dirty: git(['status', '--porcelain']).length > 0 };
  const output = prepareOutput(options['--output'] || path.join(repo, 'artifacts/p4-vscode-package'), 'vcp-vsix-output/1');
  const stage = path.join(output, 'stage');
  execFileSync(process.execPath, [path.join(__dirname, 'stage.cjs'), stage], { stdio: 'pipe', windowsHide: true });
  const packagePath = path.join(stage, 'package.json');
  const manifest = JSON.parse(fs.readFileSync(packagePath));
  if (options['--version']) {
    manifest.version = options['--version'];
  }
  manifest.repository = { type: 'git', url: 'https://github.com/iokaio/vcp.git' };
  fs.writeFileSync(packagePath, JSON.stringify(manifest, null, 2) + '\n');
  fs.writeFileSync(path.join(stage, '.vscodeignore'), '.vcp-stage.json\n.vscodeignore\n');
  const file = `vcp-local-${manifest.version}.vsix`;
  await createVSIX({ cwd: stage, packagePath: path.join(output, file), target: 'win32-x64', useYarn: false, rewriteRelativeLinks: false });
  const files = await inventory(path.join(output, file));
  const expected = new Map();
  function walk(directory, prefix = '') {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.name.startsWith('.')) continue;
      const relative = prefix + entry.name;
      if (entry.isDirectory()) walk(path.join(directory, entry.name), relative + '/');
      else {
        // Official VSCE normalizes these two top-level document names.
        const archived = relative === 'README.md' ? 'readme.md' : relative === 'LICENSE' ? 'LICENSE.txt' : relative;
        expected.set('extension/' + archived, sha256(fs.readFileSync(path.join(directory, entry.name))));
      }
    }
  }
  walk(stage);
  for (const entry of files) {
    if (['[Content_Types].xml', 'extension.vsixmanifest'].includes(entry.path)) continue;
    if (expected.get(entry.path) !== entry.sha256) throw new Error('Archive contains missing, modified or unexpected payload: ' + entry.path);
    expected.delete(entry.path);
  }
  if (expected.size) throw new Error('Archive omits staged runtime payload');
  const sdk = JSON.parse(fs.readFileSync(path.resolve(root, '../sdk-ts/package.json')));
  const schema = fs.readFileSync(path.resolve(root, '../protocol-ts/schema.json'));
  const record = {
    schema: 'vcp-vsix-package/1', archive: { file, sha256: await fileHash(path.join(output, file)) },
    extension: { id: 'vcp.vcp-local', version: manifest.version, qualification_successor: !!options['--version'], source },
    engine: { executable_sha256: engineHash, source_commit: native.manifest.source.git_commit, source_dirty: native.manifest.source.dirty, build_status: native.manifest.build.status, build_receipt_sha256: native.manifest.build.receipt_sha256 ?? null, native_archive_sha256: native.archive_sha256, native_manifest_sha256: sha256(nativeBytes) },
    compatibility: { vscode: '1.138.0', platform: 'win32-x64', status: 'qualification-required' },
    build: { node: process.version, vsce: '4.0.0', typescript: require('typescript/package.json').version, lock_sha256: sha256(fs.readFileSync(path.join(root, 'package-lock.json'))) },
    sdk: { version: sdk.version, schema_sha256: sha256(schema) }, files,
  };
  fs.writeFileSync(path.join(output, 'manifest.json'), JSON.stringify(record, null, 2) + '\n');
  console.log(JSON.stringify({ archive: path.join(output, file), manifest: path.join(output, 'manifest.json'), files: files.length }));
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
