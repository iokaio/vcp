// SPDX-License-Identifier: Apache-2.0
'use strict';
// Explicit maintenance after a selected native run; never fetches packages.
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const TOML = require('../../src/tests/node_modules/@iarna/toml');
const { sha256 } = require('../../src/tests/support/upstream-selection.cjs');
const { munariumClosure } = require('../../src/tests/support/dependency-closure.cjs');
try {
  if (process.argv.length !== 4) throw Error('Required: <native-dependencies.json> <new-output.json>');
  const root = path.resolve(__dirname, '../..');
  const lock = fs.readFileSync(path.join(root, 'src/third_party/codex/codex-rs/Cargo.lock'));
  const packages = TOML.parse(lock.toString()).package;
  const graph = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  if (graph.schema_version !== 1 || graph.policy !== 'munarium-local-libraries-v1') throw Error('Unexpected dependency record');
  const normalized = munariumClosure(graph.packages.map(p => `${p.name} v${p.version}`).join('\n'));
  const registry = path.join(process.env.CARGO_HOME || path.join(os.homedir(), '.cargo'), 'registry/src');
  const registryRoots = fs.readdirSync(registry).map(name => path.join(registry, name));
  const records = normalized.packages.map(p => {
    const matches = packages.filter(entry => entry.name === p.name && entry.version === p.version);
    if (matches.length !== 1) throw Error('Ambiguous lock identity: ' + p.name);
    const entry = matches[0];
    if (['munarium-core', 'munarium-store-mem', 'munarium-datastore'].includes(p.name)) {
      const source = 'src/third_party/munarium/server/src/' + p.name;
      const manifest = TOML.parse(fs.readFileSync(path.join(root, source, 'Cargo.toml'), 'utf8'));
      if (entry.source || manifest.package.version !== p.version || manifest.package.license !== 'Apache-2.0') throw Error('Unexpected local package identity');
      return { ...p, source, license: manifest.package.license };
    }
    if (entry.source !== 'registry+https://github.com/rust-lang/crates.io-index' || !/^[a-f0-9]{64}$/.test(entry.checksum)) throw Error('Unexpected registry source');
    const manifests = registryRoots.map(directory => path.join(directory, p.name + '-' + p.version, 'Cargo.toml')).filter(file => fs.existsSync(file));
    if (!manifests.length) throw Error('Package source not provisioned: ' + p.name + '@' + p.version);
    const licenses = manifests.map(file => TOML.parse(fs.readFileSync(file, 'utf8')).package.license);
    if (!licenses[0] || licenses.some(license => license !== licenses[0])) throw Error('License requires individual review: ' + p.name);
    return { ...p, source: entry.source, checksum: entry.checksum, license: licenses[0] };
  });
  const result = { schema_version: 1, policy: graph.policy, target: 'x86_64-pc-windows-msvc',
    features: ['munarium-datastore/vector-diskann'], workspace_lock_sha256: sha256(lock),
    scope: 'Selected normal/build graph; license declarations read from provisioned package manifests, not a release notice bundle.', packages: records };
  fs.writeFileSync(process.argv[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'pass', packages: records.length, lock_sha256: result.workspace_lock_sha256 }));
} catch (error) { console.error('Dependency inventory failed: ' + error.message); process.exitCode = 1; }
