// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const TOML = require('../../src/tests/node_modules/@iarna/toml');
const { embeddingClosure } = require('../../src/tests/support/dependency-closure.cjs');
const { digest } = require('../../src/tests/support/harness.cjs');
try {
  if (process.argv.length !== 4) throw Error('Required: <native-dependency-log> <new-reference.json>');
  const repository = path.resolve(__dirname, '../..');
  const lock = fs.readFileSync(path.join(repository, 'src/third_party/codex/codex-rs/Cargo.lock'));
  const packages = TOML.parse(lock.toString()).package;
  const graph = embeddingClosure(fs.readFileSync(process.argv[2], 'utf8'));
  const cache = path.join(process.env.CARGO_HOME || path.join(os.homedir(), '.cargo'), 'registry/src');
  const registries = fs.readdirSync(cache).map(name => path.join(cache, name));
  const records = graph.packages.map(p => {
    const matches = packages.filter(entry => entry.name === p.name && entry.version === p.version);
    if (matches.length !== 1) throw Error('Ambiguous package identity: ' + p.name);
    const entry = matches[0];
    if (p.name === 'vcp-embedding') {
      const source = 'src/crates/vcp-embedding';
      const manifest = TOML.parse(fs.readFileSync(path.join(repository, source, 'Cargo.toml'), 'utf8'));
      if (entry.source || manifest.package.version !== p.version || manifest.package.license !== 'Apache-2.0') throw Error('Unexpected local package identity');
      return { ...p, source, license: manifest.package.license };
    }
    if (entry.source !== 'registry+https://github.com/rust-lang/crates.io-index' || !/^[a-f0-9]{64}$/.test(entry.checksum)) throw Error('Unexpected registry source');
    const manifests = registries.map(root => path.join(root, p.name + '-' + p.version, 'Cargo.toml')).filter(file => fs.existsSync(file));
    if (!manifests.length) throw Error('Package source not provisioned: ' + p.name);
    const licenses = manifests.map(file => TOML.parse(fs.readFileSync(file, 'utf8')).package.license);
    if (!licenses[0] || licenses.some(value => value !== licenses[0])) throw Error('License requires individual review: ' + p.name);
    return { ...p, source: entry.source, checksum: entry.checksum, license: licenses[0] };
  });
  const result = { schema_version: 1, policy: graph.policy, target: 'x86_64-pc-windows-msvc', features: [],
    workspace_lock_sha256: digest(lock), scope: 'Native normal/build graph; package license declarations, not a release notice bundle or OS network-denial proof.', packages: records };
  fs.writeFileSync(process.argv[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'pass', packages: records.length, lock_sha256: result.workspace_lock_sha256 }));
} catch (error) { console.error('Embedding dependency inventory: ' + error.message); process.exitCode = 1; }
