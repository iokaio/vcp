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
  if (!fs.existsSync(manifestPath)) throw Error('Missing skill.json in ' + root);
  if (!fs.lstatSync(manifestPath).isFile() || fs.lstatSync(manifestPath).isSymbolicLink()) throw Error('Invalid descriptor file');
  if (fs.statSync(manifestPath).size > 65536) throw Error('Descriptor too large');
  const descriptor = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const fields = ['schema_version', 'id', 'version', 'description', 'source', 'license', 'vcp_version', 'cues', 'environments', 'required_tools', 'body', 'resources'];
  if (!descriptor || Array.isArray(descriptor) || Object.keys(descriptor).some(k => !fields.includes(k)) || fields.some(k => !(k in descriptor))) throw Error('Unknown or missing descriptor field');
  if (descriptor.schema_version !== 1 || descriptor.vcp_version !== 1) throw Error('Unsupported VCP descriptor');
  // Stricter than the loader: start with a letter or digit, so no dot-only names.
  if (typeof descriptor.id !== 'string' || !/^[a-z0-9][a-z0-9._-]*$/.test(descriptor.id) || descriptor.id.length > 128) throw Error('Invalid skill name');
  const warnings = [];
  if (path.basename(root) !== descriptor.id) warnings.push('Package directory name differs from id');
  function text(value, maximum) {
    if (typeof value !== 'string' || !value.trim() || Buffer.byteLength(value) > maximum || /[\x00-\x1f\x7f-\x9f]/.test(value)) throw Error('Invalid metadata text');
  }
  for (const [key, maximum] of Object.entries({ version: 128, description: 1024, source: 2048, license: 256 })) text(descriptor[key], maximum);
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(descriptor.version)) warnings.push('Version is not semantic (MAJOR.MINOR.PATCH)');
  if (!/\bUse when\b/.test(descriptor.description)) warnings.push('Description does not say when to use the skill ("Use when ...")');
  for (const key of ['cues', 'environments', 'required_tools']) {
    if (!Array.isArray(descriptor[key]) || descriptor[key].length > 32 || new Set(descriptor[key]).size !== descriptor[key].length) throw Error('Invalid or duplicate matching requirements');
    for (const value of descriptor[key]) text(value, 128);
  }
  if (!Array.isArray(descriptor.resources) || descriptor.resources.length > 32) throw Error('Invalid resource count');
  const seen = new Set(['skill.json']);
  let total = 0;
  for (const item of [descriptor.body, ...descriptor.resources]) {
    if (!item || typeof item.path !== 'string' || !/^[a-f0-9]{64}$/.test(item.sha256) || Object.keys(item).some(k => !['path', 'sha256', 'use'].includes(k))) throw Error('Invalid content reference');
    // ADR-070 roles: omitted means context; file resources are verified but never context.
    if ('use' in item && !['context', 'file'].includes(item.use)) throw Error('Invalid resource role');
    if (item === descriptor.body && (item.use ?? 'context') !== 'context') throw Error('Skill body must be context');
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
  // Undeclared files are neither verified nor installed by the builtin stager.
  const stack = [''];
  let entries = 0;
  while (stack.length && entries <= 512) {
    const relative = stack.pop();
    for (const entry of fs.readdirSync(path.join(root, relative), {withFileTypes: true})) {
      if (++entries > 512) { warnings.push('Package has more than 512 entries; undeclared-file check incomplete'); break; }
      const child = relative ? relative + '/' + entry.name : entry.name;
      if (entry.isDirectory()) stack.push(child);
      else if (!seen.has(child.toLowerCase())) warnings.push('Undeclared file: ' + child);
    }
  }
  return {id: descriptor.id, files: seen.size, bytes: total, warnings};
}
if (require.main === module) {
  try {
    if (process.argv.length !== 3) throw Error('Usage: node validate.cjs <package-directory>');
    console.log(JSON.stringify(validateSkill(process.argv[2])));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { validateSkill };
