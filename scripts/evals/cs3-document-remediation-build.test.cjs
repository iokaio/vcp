// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), vm = require('node:vm'), crypto = require('node:crypto');
const file = path.join(__dirname, 'cs3-document-remediation-build.ps1');
const source = fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
const historicalBytes = fs.readFileSync(path.join(__dirname, 'cs3-comparison-build.ps1'));
const historical = historicalBytes.toString('utf8').replace(/\r\n/g, '\n');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const captureScript = value => value.match(/\$capture = @'\n([\s\S]*?)\n'@/)[1];
test('prospective schema records genuine observed output without rewriting the historical builder', () => {
  assert.equal(sha(historicalBytes), '2760883d63dd9934bc19cd85f5bcd8cc73eca94964b90d1b5d8780c07d53a460');
  assert(source.includes("schema='cs3-document-remediation-build/1'"));
  assert(!source.includes('expected_executable_sha256')); assert(!source.includes('expected_executable_matches'));
  assert(!source.includes('d08ff1069d6700a8aebc7ba668b510ce98867dc6ec2f68b7fed079b34bc5312e'));
  assert(source.includes('$receipt.executable_sha256 = Hash $executable'));
  assert(source.includes("provider_calls=0;tests_executed=0;qualification_build=$true;production_release=$false"));
});
test('source freeze is a separate no-build mode and exact manifest identity is checked before Cargo', () => {
  const capture = source.indexOf('if ($CaptureOnly) {\n    $captureDirectory');
  const build = source.indexOf('    & $cargoLauncher @arguments');
  assert(capture > 0 && source.indexOf('    return\n}', capture) < build);
  assert(source.includes('build_executed=$false')); assert(source.includes('SourceManifestSha256 -cnotmatch'));
  assert(source.includes('(Get-Item -LiteralPath $SourceManifestPath).Length -gt 8MB'));
  assert(source.indexOf("if ($before -cne $sourceManifest) { throw 'Current source differs", capture) < build);
  assert(source.includes('source_manifest=[ordered]@{path=$SourceManifestPath;sha256=$SourceManifestSha256}'));
});
test('actual bounded capture code preserves sorted identity and detects source changes', t => {
  assert.equal(captureScript(source), captureScript(historical));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-build-source-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src')); fs.writeFileSync(path.join(root, 'src/b.txt'), 'b'); fs.writeFileSync(path.join(root, 'src/a.txt'), 'a');
  function capture() { const lines = []; vm.runInNewContext(captureScript(source), { require, process: { cwd: () => root, argv: ['node', 'src'] }, console: { log: value => lines.push(value) } }); assert.equal(lines.length, 1); return JSON.parse(lines[0]); }
  const before = capture(); assert.deepEqual(before.files.map(f => f.path), ['src/a.txt', 'src/b.txt']); assert.deepEqual(capture(), before);
  fs.writeFileSync(path.join(root, 'src/a.txt'), 'changed'); assert.notEqual(capture().content_sha256, before.content_sha256);
  assert(captureScript(source).includes('count>20000||depth>24')); assert(captureScript(source).includes('256*1024*1024')); assert(captureScript(source).includes('s.isSymbolicLink()'));
});
test('exact offline compiler artifact, staging and embedded catalog guards are retained', () => {
  assert(source.includes("'--locked','--offline','-p','vcp-cli','--features','qualification','--test','executable','--no-run'"));
  assert(source.includes("'-j2','--message-format=json-render-diagnostics'"));
  assert(source.includes("$_.target.name -ceq 'vcp' -and $_.executable -and -not $_.profile.test"));
  assert(source.includes('$compilerArtifacts.Count -ne 1')); assert(source.includes('[IO.Path]::GetFullPath($compilerArtifacts[0].executable) -ine $built'));
  assert(source.includes("$compilerArtifacts[0].features[0] -cne 'qualification'"));
  assert(source.includes('[IO.File]::Copy($built,$executable,$false)'));
  assert(source.includes('$receipt.executable_sha256 -cne $compiledHash -or (Hash $built) -cne $compiledHash'));
  assert(source.includes('requireEmbeddedCatalog')); assert(source.includes('CARGO_BUILD_TARGET'));
  assert(source.includes("'artifacts/codex-target'"));
});
test('failed builds still check source/toolchain identity and always restore the stack environment', () => {
  const cleanup = source.slice(source.indexOf('    # Final source/toolchain observations'));
  assert(cleanup.includes('$after = Capture-Source')); assert(cleanup.includes('$toolsAfter = Capture-Toolchain'));
  assert(cleanup.includes('$before -ceq $after -and $after -ceq $sourceManifest -and (Hash $SourceManifestPath) -ceq $SourceManifestSha256'));
  assert(cleanup.includes('$receipt.toolchain_unchanged = $toolsBefore -ceq $toolsAfter'));
  assert(cleanup.includes('-not $receipt.failure -and $receipt.exit_code -eq 0 -and $receipt.executable_sha256'));
  assert(cleanup.includes("$receipt.status = 'failed'")); assert(cleanup.includes('$receipt.final_integrity_failure = $_.Exception.Message'));
  assert(cleanup.includes('} finally {\n        $env:RUST_MIN_STACK = $oldStack\n        Pop-Location'));
});
