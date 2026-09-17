// SPDX-License-Identifier: Apache-2.0
'use strict';
function munariumClosure(text) {
  const packages = new Map();
  for (const [index, line] of text.replace(/^\uFEFF/, '').split(/\r?\n/).entries()) {
    if (!line.trim()) continue;
    const match = line.match(/^([A-Za-z0-9_-]+) v(\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?(?:\+[A-Za-z0-9.-]+)?)(?: \([^\r\n]*\))*$/);
    if (!match) throw Error('Unrecognized dependency record at line ' + (index + 1));
    const [, name, version] = match;
    if (/^(?:axum|tonic|sqlx|reqwest|hyper)(?:-|$)/.test(name) ||
        /^munarium-(?:server|providers|store-pg|retrieval-pg|retrieval|azure-auth|docintel-az|runbooks)$/.test(name)) {
      throw Error('Forbidden dependency in local library selection: ' + name);
    }
    packages.set(name + '@' + version, { name, version });
  }
  for (const name of ['munarium-core', 'munarium-store-mem', 'munarium-datastore', 'tantivy', 'diskann', 'diskann-vector']) {
    if (![...packages.values()].some(item => item.name === name)) throw Error('Missing required dependency: ' + name);
  }
  return { schema_version: 1, policy: 'munarium-local-libraries-v1',
    scope: 'normal and build dependencies for the explicit native Windows target; not a network-effect proof',
    packages: [...packages.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([, value]) => value) };
}
module.exports = { munariumClosure };
