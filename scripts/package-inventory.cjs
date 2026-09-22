// SPDX-License-Identifier: Apache-2.0
'use strict';

// Pure inventory and compatibility-manifest helpers for the P8-04 local
// distribution assembler. Archive creation remains in package.ps1 so the
// exact Windows ZIP implementation is exercised by the caller.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const MAX_FILES = 4096;
const MAX_FILE_BYTES = 1024 * 1024 * 1024;
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function portable(relative) {
  if (typeof relative !== 'string' || !relative || relative.length > 512 ||
      relative.startsWith('/') || relative.includes('\\') || relative.split('/').some(part =>
        !part || part === '.' || part === '..' || /[<>:"|?*\x00-\x1f]/.test(part) || /[. ]$/.test(part) ||
        /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part))) {
    throw Error('Invalid portable distribution path: ' + relative);
  }
  return relative;
}

function fileRecord(root, relative) {
  portable(relative);
  const absolute = path.join(root, ...relative.split('/'));
  const stat = fs.lstatSync(absolute);
  if (!stat.isFile() || stat.isSymbolicLink()) throw Error('Distribution entry must be an ordinary file: ' + relative);
  if (stat.size > MAX_FILE_BYTES) throw Error('Distribution entry exceeds size limit: ' + relative);
  const bytes = fs.readFileSync(absolute);
  return { path: relative, bytes: bytes.length, sha256: digest(bytes) };
}

function enumerate(root, maxFiles = MAX_FILES) {
  root = path.resolve(root);
  const found = [];
  function visit(relative) {
    const absolute = relative ? path.join(root, ...relative.split('/')) : root;
    const stat = fs.lstatSync(absolute);
    if (stat.isSymbolicLink()) throw Error('Linked distribution directory is not allowed');
    if (stat.isDirectory()) {
      for (const name of fs.readdirSync(absolute).sort()) visit(relative ? relative + '/' + name : name);
    } else if (stat.isFile()) {
      if (found.length >= maxFiles) throw Error('Distribution file count limit exceeded');
      found.push(fileRecord(root, relative));
    }
    else throw Error('Unsupported distribution entry: ' + relative);
  }
  visit('');
  return found.sort((a, b) => a.path.localeCompare(b.path));
}

function validateMetadata(metadata = {}) {
  const result = {
    schema: 'vcp-distribution-manifest/1',
    artifact: 'unsigned-local-candidate',
    source: metadata.source || { repository: 'vcp', git_commit: null, dirty: null },
    target: metadata.target || { os: 'Windows', architecture: 'unspecified' },
    build: metadata.build || { status: 'caller-supplied-unverified' },
    compatibility: metadata.compatibility || {
      cli: 'unversioned', worker: 'unversioned', canonical: 'unspecified', config: 'unspecified', index: 'unspecified'
    },
    runtime: metadata.runtime || [],
    skills: metadata.skills || null,
    model_provisioning: metadata.model_provisioning || { bundled: false, records: [] },
    signing: { status: 'unsigned', certificate: null },
    limitations: [
      'Unsigned local ZIP candidate; no signing or publication evidence.',
      'Compatibility is a source declaration; executable provenance and native qualification are recorded separately.',
      'No cross-format migration is attempted; incompatible versions require a validated restore.'
    ]
  };
  if (!Array.isArray(result.runtime) || !result.model_provisioning || !Array.isArray(result.model_provisioning.records)) {
    throw Error('Invalid runtime or model provisioning metadata');
  }
  return result;
}

function buildManifest(packageRoot, metadata, excluded = ['manifest.json']) {
  const manifest = validateMetadata(metadata);
  const excludedSet = new Set(excluded);
  manifest.files = enumerate(packageRoot).filter(file => !excludedSet.has(file.path));
  if (!manifest.files.length) throw Error('Distribution must contain at least one payload file');
  manifest.payload_sha256 = digest(Buffer.from(JSON.stringify(manifest.files)));
  return manifest;
}

function verifyManifest(packageRoot, manifest) {
  if (!manifest || manifest.schema !== 'vcp-distribution-manifest/1' || !Array.isArray(manifest.files)) {
    throw Error('Unsupported distribution manifest');
  }
  const actual = enumerate(packageRoot, MAX_FILES + 1).filter(file => file.path !== 'manifest.json');
  if (actual.length > MAX_FILES) throw Error('Distribution file count limit exceeded');
  if (JSON.stringify(actual) !== JSON.stringify(manifest.files)) throw Error('Distribution payload differs from manifest');
  if (manifest.model_provisioning?.bundled) throw Error('Bundled model assets are not permitted by this foundation');
  return { status: 'passed', entries: actual.length, payload_sha256: digest(Buffer.from(JSON.stringify(actual))) };
}

function compatibility(candidate, state) {
  if (!state) return { status: 'no-state-manifest' };
  const fields = ['canonical', 'config', 'index'];
  for (const field of fields) {
    if (!candidate.compatibility?.[field] || !state.compatibility?.[field] ||
        candidate.compatibility[field] !== state.compatibility[field]) {
      throw Error('Incompatible ' + field + ' format');
    }
  }
  return { status: 'compatible' };
}

module.exports = { MAX_FILES, MAX_FILE_BYTES, digest, portable, enumerate, buildManifest, verifyManifest, compatibility, validateMetadata };
