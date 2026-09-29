// SPDX-License-Identifier: Apache-2.0
// Adapted from Anthropic skill-creator/scripts/quick_validate.py, Apache-2.0.
// VCP changes: native JSON descriptor and content hashes; no YAML dependency.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
function validateSkill(directory) {
  const root = path.resolve(directory);
  if (fs.lstatSync(root).isSymbolicLink()) throw Error('Linked package root');
  const manifestPath = path.join(root, 'skill.json');
  if (!fs.lstatSync(manifestPath).isFile() || fs.lstatSync(manifestPath).isSymbolicLink()) throw Error('Invalid descriptor file');
  if (fs.statSync(manifestPath).size > 65536) throw Error('Descriptor too large');
  const descriptor = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const fields = ['schema_version', 'id', 'version', 'description', 'source', 'license', 'vcp_version', 'cues', 'environments', 'required_tools', 'body', 'resources'];
  if (!descriptor || Array.isArray(descriptor) || Object.keys(descriptor).some(k => !fields.includes(k)) || fields.some(k => !(k in descriptor))) throw Error('Unknown or missing descriptor field');
  if (descriptor.schema_version !== 1 || descriptor.vcp_version !== 1) throw Error('Unsupported VCP descriptor');
  if (typeof descriptor.id !== 'string' || !/^[a-z0-9._-]+$/.test(descriptor.id) || descriptor.id.length > 128) throw Error('Invalid skill name');
  function text(value, maximum) {
    if (typeof value !== 'string' || !value.trim() || Buffer.byteLength(value) > maximum || /[\x00-\x1f\x7f-\x9f]/.test(value)) throw Error('Invalid metadata text');
  }
  for (const [key, maximum] of Object.entries({ version: 128, description: 1024, source: 2048, license: 256 })) text(descriptor[key], maximum);
  for (const key of ['cues', 'environments', 'required_tools']) {
    if (!Array.isArray(descriptor[key]) || new Set(descriptor[key]).size > 32) throw Error('Invalid matching requirements');
    for (const value of descriptor[key]) text(value, 128);
  }
  if (!Array.isArray(descriptor.resources) || descriptor.resources.length > 32) throw Error('Invalid resource count');
  const seen = new Set(['skill.json']);
  let total = 0;
  for (const item of [descriptor.body, ...descriptor.resources]) {
    if (!item || typeof item.path !== 'string' || !/^[a-f0-9]{64}$/.test(item.sha256) || Object.keys(item).some(k => !['path', 'sha256'].includes(k))) throw Error('Invalid content reference');
    const parts = item.path.split('/');
    if (item.path.length > 512 || parts.some(p => !p || p === '.' || p === '..' || /[<>:"\\|?*\x00-\x1f]/.test(p) || /[. ]$/.test(p) || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(p))) throw Error('Invalid content path');
    const key = item.path.toLowerCase();
    if (seen.has(key)) throw Error('Duplicate content path');
    seen.add(key);
    let file = root;
    for (const part of parts) { file = path.join(file, part); if (fs.lstatSync(file).isSymbolicLink()) throw Error('Linked content'); }
    const stat = fs.statSync(file);
    if (!stat.isFile() || stat.size > 262144 || (total += stat.size) > 1048576) throw Error('Content size/type limit');
    const bytes = fs.readFileSync(file);
    if (crypto.createHash('sha256').update(bytes).digest('hex') !== item.sha256) throw Error('Content hash mismatch: ' + item.path);
  }
  return {id: descriptor.id, files: seen.size, bytes: total};
}
if (require.main === module) {
  try {
    if (process.argv.length !== 3) throw Error('Usage: node validate.cjs <package-directory>');
    console.log(JSON.stringify(validateSkill(process.argv[2])));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { validateSkill };
