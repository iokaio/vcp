// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { validatePath } = require('./upstream-inventory.cjs');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function specification(repository) {
  const bytes = fs.readFileSync(path.join(repository, 'src/third_party/components/minilm-assets.json'));
  const spec = JSON.parse(bytes);
  if (spec.schema_version !== 1 || spec.repository !== 'https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2' ||
      !/^[a-f0-9]{40}$/.test(spec.revision) || spec.dimensions !== 384 || spec.max_tokens !== 256 ||
      spec.pooling !== 'attention-mask-mean' || spec.normalization !== 'l2' || !Array.isArray(spec.files) || spec.files.length !== 10) throw Error('Invalid model specification');
  const seen = new Set();
  for (const file of spec.files) {
    validatePath(file.path);
    if (seen.has(file.path.toLowerCase()) || !Number.isSafeInteger(file.bytes) || file.bytes <= 0 || file.bytes > 100 * 1024 * 1024 ||
        !/^[a-f0-9]{64}$/.test(file.sha256)) throw Error('Invalid model file identity');
    seen.add(file.path.toLowerCase());
  }
  return { spec, sha256: sha256(bytes) };
}
function canonical(file) {
  let existing = path.resolve(file); const suffix = [];
  while (!fs.existsSync(existing)) {
    suffix.unshift(path.basename(existing));
    const parent = path.dirname(existing);
    if (parent === existing) throw Error('No existing model destination ancestor');
    existing = parent;
  }
  return path.join(fs.realpathSync(existing), ...suffix);
}
function outside(file, roots) {
  const result = canonical(file), fold = value => process.platform === 'win32' ? value.toLowerCase() : value;
  for (const root of roots.map(canonical)) {
    if (fold(result) === fold(root) || fold(result).startsWith(fold(root) + path.sep)) throw Error('Model assets and evidence must remain outside their protected source roots');
  }
  return result;
}
async function verify(root, spec) {
  let total = 0;
  for (const file of spec.files) {
    const target = path.join(root, file.path);
    const stat = fs.statSync(target);
    if (!stat.isFile() || stat.size !== file.bytes) throw Error('Model asset size mismatch: ' + file.path);
    const hash = crypto.createHash('sha256'); let bytes = 0;
    for await (const chunk of fs.createReadStream(target)) {
      bytes += chunk.length;
      if (bytes > file.bytes) throw Error('Model asset grew during verification: ' + file.path);
      hash.update(chunk);
    }
    if (bytes !== file.bytes || hash.digest('hex') !== file.sha256) throw Error('Model asset digest mismatch: ' + file.path);
    total += bytes;
  }
  return { files: spec.files.length, bytes: total };
}
async function acquire(root, spec) {
  if (fs.existsSync(root)) throw Error('Model acquisition requires a new destination');
  fs.mkdirSync(path.dirname(root), { recursive: true }); fs.mkdirSync(root);
  const record = { schema_version: 1, owner: crypto.randomUUID(), revision: spec.revision, status: 'acquiring', files: [] };
  const save = () => fs.writeFileSync(path.join(root, '.vcp-acquisition.json'), JSON.stringify(record, null, 2) + '\n');
  save();
  try {
    for (const file of spec.files) {
      const target = path.join(root, file.path), partial = target + '.partial';
      fs.mkdirSync(path.dirname(target), { recursive: true });
      const response = await fetch(spec.repository + '/resolve/' + spec.revision + '/' + file.path,
        { signal: AbortSignal.timeout(300000), headers: { 'User-Agent': 'VCP-model-qualification' } });
      if (!response.ok || !response.body) throw Error('Model acquisition HTTP ' + response.status);
      const fd = fs.openSync(partial, 'wx'), hash = crypto.createHash('sha256'); let bytes = 0;
      try {
        for await (const chunk of response.body) {
          bytes += chunk.length;
          if (bytes > file.bytes) throw Error('Model download exceeded expected size: ' + file.path);
          hash.update(chunk);
          let offset = 0;
          while (offset < chunk.length) offset += fs.writeSync(fd, chunk, offset);
        }
        if (bytes !== file.bytes || hash.digest('hex') !== file.sha256) throw Error('Model download digest mismatch: ' + file.path);
        fs.fsyncSync(fd);
      } finally { fs.closeSync(fd); }
      fs.renameSync(partial, target);
      record.files.push(file.path); save();
    }
    const result = await verify(root, spec); record.status = 'verified'; save(); return result;
  } catch (error) { record.status = 'failed'; record.reason = error.message; save(); throw error; }
}
module.exports = { specification, canonical, outside, verify, acquire };
