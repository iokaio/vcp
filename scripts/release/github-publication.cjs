// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path');
const { hash, fileHash, json } = require('./provenance.cjs');
const repository = 'iokaio/vcp';
const check = (condition, message) => { if (!condition) throw Error(message); };
async function api(route, method = 'GET', body) {
  check(process.env.GH_TOKEN, 'GitHub workflow token required');
  const response = await fetch(`https://api.github.com/repos/${repository}/${route}`, {
    method, headers: { Authorization: `Bearer ${process.env.GH_TOKEN}`, Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28', ...(body ? { 'Content-Type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(60000),
  });
  if (response.status === 404 && method === 'GET') return null;
  check(response.ok, `GitHub ${method} ${route.split('?')[0]} returned ${response.status}`);
  return response.status === 204 ? null : response.json();
}
function validateRun(run, runId, attempt) {
  check(run && String(run.id) === runId && String(run.run_attempt) === attempt &&
    run.repository?.full_name === repository && run.head_repository?.full_name === repository &&
    run.path === '.github/workflows/beta-candidate.yml' && run.head_branch === 'main' &&
    run.event === 'workflow_dispatch' && run.status === 'completed' && run.conclusion === 'success' &&
    /^[a-f0-9]{40}$/.test(run.head_sha), 'Only an exact successful main candidate attempt is publishable');
  return run.head_sha;
}
async function select() {
  const runId = process.env.CANDIDATE_RUN_ID, attempt = process.env.CANDIDATE_ATTEMPT;
  check(/^[1-9]\d{0,14}$/.test(runId || '') && /^[1-9]\d{0,5}$/.test(attempt || '') &&
    /^[a-f0-9]{64}$/.test(process.env.EXPECTED_PAIR_ID || ''), 'Explicit run, attempt and full pair ID required');
  check(process.env.GITHUB_REPOSITORY === repository && process.env.GITHUB_REF === 'refs/heads/main', 'Publication runs only on this repository main');
  const run = await api(`actions/runs/${runId}/attempts/${attempt}`);
  const commit = validateRun(run, runId, attempt);
  const comparison = await api(`compare/${commit}...main`);
  check(['ahead', 'identical'].includes(comparison?.status), 'Candidate source is not retained on main');
  const checks = await api(`actions/workflows/ci.yml/runs?head_sha=${commit}&status=success&event=push&branch=main&per_page=100`);
  check(checks?.workflow_runs?.some(row => row.head_sha === commit && row.head_branch === 'main' &&
    row.event === 'push' && row.conclusion === 'success'), 'Candidate source lacks successful main Delivery checks');
  const name = `internal-beta-${commit}-${runId}-${attempt}`;
  const artifacts = await api(`actions/runs/${runId}/artifacts?name=${name}&per_page=100`);
  const selected = artifacts?.artifacts?.filter(row => row.name === name && !row.expired) || [];
  check(selected.length === 1 && Number.isSafeInteger(selected[0].id), 'Exactly one retained candidate packet required');
  fs.appendFileSync(process.env.GITHUB_OUTPUT, `commit=${commit}\nartifact_id=${selected[0].id}\n`);
  console.log(`Selected candidate ${runId}/${attempt}, source ${commit}.`);
}
function validateRemoteAssets(assets, expected, complete = true) {
  const names = new Set();
  for (const asset of assets) {
    const local = expected.find(row => row.name === asset.name);
    check(local && !names.has(asset.name) && asset.state === 'uploaded' && asset.size === local.bytes &&
      asset.digest === `sha256:${local.sha256}`, 'Existing release asset differs; refusing overwrite');
    names.add(asset.name);
  }
  if (complete) check(names.size === expected.length, 'Release asset set is incomplete');
}
async function upload(release, file, expected) {
  const url = new URL(release.upload_url.replace(/\{.*$/, ''));
  check(url.protocol === 'https:' && url.hostname === 'uploads.github.com', 'Unexpected GitHub upload origin');
  url.searchParams.set('name', expected.name);
  const bytes = fs.readFileSync(file);
  check(hash(bytes) === expected.sha256, 'Staged asset changed before upload');
  const response = await fetch(url, { method: 'POST', headers: {
    Authorization: `Bearer ${process.env.GH_TOKEN}`, 'Content-Type': 'application/octet-stream',
    'Content-Length': String(bytes.length), Accept: 'application/vnd.github+json',
  }, body: bytes, signal: AbortSignal.timeout(300000) });
  check(response.ok, `Release asset upload returned ${response.status}`);
  validateRemoteAssets([await response.json()], [expected]);
}
async function publish(directory) {
  check(process.env.GITHUB_REPOSITORY === repository && process.env.GITHUB_REF === 'refs/heads/main', 'Publication runs only on this repository main');
  const release = json(path.join(directory, 'assets/release.json'));
  check(/^v\d+\.\d+\.\d+-beta\.\d+-[a-f0-9]{12}$/.test(release.tag) &&
    /^[a-f0-9]{40}$/.test(release.commit) && /^[a-f0-9]{64}$/.test(release.pairId) &&
    release.commit === process.env.EXPECTED_COMMIT && release.pairId === process.env.EXPECTED_PAIR_ID &&
    release.tag === `v${release.version}-${release.pairId.slice(0, 12)}`, 'Invalid staged release identity');
  // Fetch all owned beta releases so a rerun of an old candidate cannot move the page backwards.
  const releases = [];
  for (let page = 1; ; page++) {
    check(page <= 100, 'Release history exceeds publication bound');
    const rows = await api(`releases?per_page=100&page=${page}`);
    check(Array.isArray(rows), 'Release list unavailable'); releases.push(...rows);
    if (rows.length < 100) break;
  }
  const expected = fs.readdirSync(path.join(directory, 'assets')).sort().map(name => {
    check(/^[a-zA-Z0-9._-]+$/.test(name), 'Invalid public asset name');
    const file = path.join(directory, 'assets', name);
    return { name, bytes: fs.statSync(file).size, sha256: fileHash(file) };
  });
  check(expected.length === 5, 'Only three binaries, release.json and SHA256SUMS may be published');
  check(Array.isArray(release.artifacts) && release.artifacts.length === 3 &&
    new Set(release.artifacts.map(row => row.kind)).size === 3 &&
    release.artifacts.every(row => ['setup', 'zip', 'vsix'].includes(row.kind)), 'Three named release artifacts required');
  const allowed = new Set(['release.json', 'SHA256SUMS', ...release.artifacts.map(row => row.name)]);
  check(allowed.size === 5 && expected.every(row => allowed.has(row.name)), 'Unexpected public asset');
  for (const row of release.artifacts) {
    const actual = expected.find(asset => asset.name === row.name);
    check(actual?.sha256 === row.sha256 && actual.bytes === row.bytes, 'Staged binary differs from verified manifest');
  }
  const sumNames = new Set();
  for (const line of fs.readFileSync(path.join(directory, 'assets/SHA256SUMS'), 'utf8').trimEnd().split(/\r?\n/)) {
    const match = /^([a-f0-9]{64})  ([a-zA-Z0-9._-]+)$/.exec(line);
    check(match && match[2] !== 'SHA256SUMS' && !sumNames.has(match[2]) &&
      expected.find(row => row.name === match[2])?.sha256 === match[1], 'Staged publication checksum mismatch');
    sumNames.add(match[2]);
  }
  check(sumNames.size === 4, 'All public binaries and metadata must be checksum-bound');
  let remote = releases.find(row => row.tag_name === release.tag);
  for (const prior of releases.filter(row => !row.draft && row.prerelease && row.tag_name !== release.tag &&
    /^v\d+\.\d+\.\d+-beta\.\d+-[a-f0-9]{12}$/.test(row.tag_name))) {
    // Tags are exact source commits: an older source may be republished only without promoting it.
    const comparison = await api(`compare/${prior.tag_name}...${release.commit}`);
    check(['ahead', 'identical'].includes(comparison?.status), 'A newer beta source is already published; refusing to move latest backwards');
    if (comparison.status === 'identical') {
      const manifest = prior.assets?.find(row => row.name === 'release.json');
      check(manifest && manifest.size < 65536 && /^sha256:[a-f0-9]{64}$/.test(manifest.digest || ''), 'Prior beta manifest unavailable');
      const response = await fetch(`https://github.com/${repository}/releases/download/${prior.tag_name}/release.json`,
        { signal: AbortSignal.timeout(60000) });
      check(response.ok, 'Prior beta manifest download failed');
      const bytes = Buffer.from(await response.arrayBuffer());
      check(bytes.length === manifest.size && `sha256:${hash(bytes)}` === manifest.digest, 'Prior beta manifest digest mismatch');
      const previous = JSON.parse(bytes);
      check(/^[1-9]\d*$/.test(previous.candidateRunId) && /^[1-9]\d*$/.test(previous.candidateAttempt) &&
        /^[1-9]\d*$/.test(release.candidateRunId) && /^[1-9]\d*$/.test(release.candidateAttempt), 'Missing candidate ordering metadata');
      check(BigInt(release.candidateRunId) > BigInt(previous.candidateRunId) ||
        (release.candidateRunId === previous.candidateRunId && BigInt(release.candidateAttempt) >= BigInt(previous.candidateAttempt)),
      'A newer candidate for this source is already published; refusing to move latest backwards');
    }
  }
  const ref = await api(`git/ref/tags/${release.tag}`);
  if (ref) check(ref.object?.type === 'commit' && ref.object.sha === release.commit, 'Release tag differs from candidate source');
  else await api('git/refs', 'POST', { ref: `refs/tags/${release.tag}`, sha: release.commit });
  if (!remote) remote = await api('releases', 'POST', { tag_name: release.tag, target_commitish: release.commit,
    name: `VCP ${release.version} beta (${release.pairId.slice(0, 12)})`, draft: true, prerelease: true,
    make_latest: 'false', body: fs.readFileSync(path.join(directory, 'notes.md'), 'utf8') });
  check(remote.prerelease === true, 'Existing release is not a beta prerelease');
  validateRemoteAssets(remote.assets, expected, !remote.draft);
  if (remote.draft) {
    for (const asset of expected.filter(row => !remote.assets.some(existing => existing.name === row.name))) {
      await upload(remote, path.join(directory, 'assets', asset.name), asset);
    }
    remote = await api(`releases/${remote.id}`);
    validateRemoteAssets(remote.assets, expected);
    remote = await api(`releases/${remote.id}`, 'PATCH', { draft: false, prerelease: true, make_latest: 'false' });
  }
  check(!remote.draft && remote.prerelease, 'Prerelease publication did not complete');
  validateRemoteAssets(remote.assets, expected);
  const { renderDownloadPage } = require('./download-page.cjs');
  const site = path.join(directory, 'site'); fs.mkdirSync(site, { recursive: true });
  fs.writeFileSync(path.join(site, 'index.html'), renderDownloadPage(release));
  fs.copyFileSync(path.join(directory, 'assets/release.json'), path.join(site, 'latest.json'));
  fs.writeFileSync(path.join(site, '.nojekyll'), '');
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY,
    `Published [${release.tag}](${remote.html_url}) from \`${release.commit}\`.\n\nPair: \`${release.pairId}\`.\n\n${release.qualification?.signing === 'signed' ? 'Signed' : 'Unsigned'} beta; full qualification remains incomplete.\n`);
  console.log(`Published verified prerelease ${release.tag}; Pages content prepared.`);
}
if (require.main === module) (async () => {
  const [command, directory, ...extra] = process.argv.slice(2);
  check(extra.length === 0, 'Unexpected arguments');
  if (command === 'select' && !directory) await select();
  else if (command === 'publish' && directory) await publish(directory);
  else throw Error('Use select or publish <prepared-directory>');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
module.exports = { select, publish, validateRun, validateRemoteAssets };
