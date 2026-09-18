// SPDX-License-Identifier: Apache-2.0
'use strict';
function localClosure(text, policy, required, forbidden) {
  const packages = new Map();
  for (const [index, line] of text.replace(/^\uFEFF/, '').split(/\r?\n/).entries()) {
    if (!line.trim()) continue;
    const match = line.match(/^([A-Za-z0-9_-]+) v(\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?(?:\+[A-Za-z0-9.-]+)?)(?: \([^\r\n]*\))*$/);
    if (!match) throw Error('Unrecognized dependency record at line ' + (index + 1));
    const [, name, version] = match;
    if (forbidden.test(name)) {
      throw Error('Forbidden dependency in local library selection: ' + name);
    }
    packages.set(name + '@' + version, { name, version });
  }
  for (const name of required) {
    if (![...packages.values()].some(item => item.name === name)) throw Error('Missing required dependency: ' + name);
  }
  return { schema_version: 1, policy,
    scope: 'normal and build dependencies for the explicit native Windows target; not a network-effect proof',
    packages: [...packages.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([, value]) => value) };
}
function munariumClosure(text) {
  return localClosure(text, 'munarium-local-libraries-v1',
    ['munarium-core', 'munarium-store-mem', 'munarium-datastore', 'tantivy', 'diskann', 'diskann-vector'],
    /^(?:(?:axum|tonic|sqlx|reqwest|hyper)(?:-|$)|munarium-(?:server|providers|store-pg|retrieval-pg|retrieval|azure-auth|docintel-az|runbooks)$)/);
}
function embeddingClosure(text) {
  return localClosure(text, 'local-cpu-embedding-v1',
    ['vcp-embedding', 'candle-core', 'candle-nn', 'candle-transformers', 'tokenizers', 'safetensors'],
    /^(?:axum|tonic|sqlx|reqwest|hyper|ureq|hf-hub|cudarc|candle-kernels|candle-flash-attn|metal|intel-mkl|accelerate-src)(?:-|$)/);
}
function memoryClosure(text) {
  return localClosure(text, 'local-governed-corpus-v1',
    ['vcp-memory-spike', 'vcp-embedding', 'candle-core', 'candle-nn', 'candle-transformers', 'tokenizers', 'safetensors',
      'munarium-core', 'munarium-store-mem', 'munarium-datastore', 'tantivy', 'diskann', 'diskann-vector'],
    /^(?:(?:axum|tonic|sqlx|reqwest|hyper|ureq|hf-hub|cudarc|candle-kernels|candle-flash-attn|metal|intel-mkl|accelerate-src)(?:-|$)|munarium-(?:server|providers|store-pg|retrieval-pg|retrieval|azure-auth|docintel-az|runbooks)$)/);
}
function verifyReference(actual, reference, lockSha256) {
  const features = actual.policy === 'local-cpu-embedding-v1' ? [] : ['munarium-datastore/vector-diskann'];
  if (reference.schema_version !== 1 || reference.policy !== actual.policy ||
      !['munarium-local-libraries-v1', 'local-cpu-embedding-v1', 'local-governed-corpus-v1'].includes(actual.policy) ||
      reference.target !== 'x86_64-pc-windows-msvc' ||
      JSON.stringify(reference.features) !== JSON.stringify(features) ||
      reference.workspace_lock_sha256 !== lockSha256 || !Array.isArray(reference.packages)) {
    throw Error('Dependency reference identity or lockfile drift');
  }
  const expected = reference.packages.map(({ name, version }) => ({ name, version }));
  if (JSON.stringify(actual.packages) !== JSON.stringify(expected)) throw Error('Selected dependency graph drift');
}
module.exports = { munariumClosure, embeddingClosure, memoryClosure, verifyReference };
