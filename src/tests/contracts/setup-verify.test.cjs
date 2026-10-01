// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');

test('setup Verify decodes UTF-8 launcher selection without weakening refusal or changing caller encoding', { skip: process.platform !== 'win32' }, t => {
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-setup-verify-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const shell = path.resolve(__dirname, '../../../scripts/installer/shell.ps1');
  const app = path.join(root, 'Program Files café β'), data = path.join(root, 'Protected Data café β');
  fs.mkdirSync(data);
  // Prepare only owns this synthetic directory; it does not register or install
  // anything. The real Verify action then checks its actual pointer and launcher.
  execFileSync('pwsh', ['-NoProfile', '-File', shell, '-Action', 'Prepare', '-AppRoot', app, '-DataRoot', data], { windowsHide: true, timeout: 15000, stdio: 'pipe' });
  const archive = 'a'.repeat(64), candidate = 'b'.repeat(64);
  const engine = path.join(app, 'engine'), release = path.join(engine, 'releases', archive);
  fs.mkdirSync(release, { recursive: true });
  fs.writeFileSync(path.join(engine, 'active.json'), JSON.stringify({ schema: 'vcp-install-pointer/1', package_sha256: archive, release: archive, data_root: data }));
  fs.writeFileSync(path.join(release, 'manifest.json'), JSON.stringify({ release: { candidate_id: candidate } }));
  const source = path.join(root, 'launcher.cs');
  fs.writeFileSync(source, `using System;
using System.IO;
using System.Collections.Generic;
using System.Web.Script.Serialization;
class Fixture {
  static int Main(string[] args) {
    if(args.Length != 1 || args[0] != "--resolve-installation") return 2;
    string mode=Environment.GetEnvironmentVariable("VCP_VERIFY_FIXTURE_MODE");
    if(mode == "nonzero") return 7;
    var json=new JavaScriptSerializer();
    var engine=Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"engine");
    var pointer=json.Deserialize<Dictionary<string,object>>(File.ReadAllText(Path.Combine(engine,"active.json")));
    var selected=Path.Combine(engine,"releases",(string)pointer["release"],"vcp.exe");
    var data=(string)pointer["data_root"];
    if(mode == "wrong-engine") selected=Path.Combine(engine,"wrong.exe");
    if(mode == "wrong-data") data=Path.Combine(data,"wrong");
    Console.OutputEncoding=new System.Text.UTF8Encoding(false);
    Console.WriteLine(json.Serialize(new {schema="vcp-installed-engine/1",executable=selected,data_directory=data}));
    return 0;
  }
}
`);
  const compiler = path.join(process.env.SystemRoot, 'Microsoft.NET/Framework64/v4.0.30319/csc.exe');
  execFileSync(compiler, ['/nologo', '/target:exe', '/platform:x64', '/r:System.Web.Extensions.dll', `/out:${path.join(app, 'vcp.exe')}`, source], { windowsHide: true, timeout: 30000, stdio: 'pipe' });
  const child = path.join(root, 'verify.ps1');
  fs.writeFileSync(child, String.raw`param([string]$Shell,[string]$App,[string]$Data,[string]$Archive,[string]$Candidate,[string]$Mode,[string]$Result)
$ErrorActionPreference='Stop'
$env:VCP_VERIFY_FIXTURE_MODE=$Mode
[Console]::OutputEncoding=[Text.Encoding]::GetEncoding(437)
$before=[Console]::OutputEncoding.CodePage
$message=$null
try { & $Shell -Action Verify -AppRoot $App -DataRoot $Data -ExpectedArchive $Archive -CandidateId $Candidate }
catch { $message=$_.Exception.Message }
@{mode=$Mode;before=$before;after=[Console]::OutputEncoding.CodePage;error=$message} | ConvertTo-Json | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
`);
  const parent = path.join(root, 'run.ps1');
  fs.writeFileSync(parent, String.raw`param([string]$Child,[string]$Shell,[string]$App,[string]$Data,[string]$Archive,[string]$Candidate,[string]$Mode,[string]$Result)
$ErrorActionPreference='Stop'
$info=[Diagnostics.ProcessStartInfo]::new([Environment]::ProcessPath)
$info.UseShellExecute=$false;$info.CreateNoWindow=$true
$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
$info.Environment.Clear()
foreach($name in @('SystemRoot','WINDIR','TEMP','TMP')){if(Test-Path ('env:'+ $name)){$info.Environment[$name]=[Environment]::GetEnvironmentVariable($name)}}
$info.Environment['PATHEXT']='.EXE'
foreach($arg in @('-NoProfile','-NonInteractive','-File',$Child,'-Shell',$Shell,'-App',$App,'-Data',$Data,'-Archive',$Archive,'-Candidate',$Candidate,'-Mode',$Mode,'-Result',$Result)){$info.ArgumentList.Add($arg)}
$process=[Diagnostics.Process]::new();$process.StartInfo=$info
try {
  if(-not $process.Start()){throw 'Fixture PowerShell did not start'}
  $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
  if(-not $process.WaitForExit(10000)){$process.Kill($true);$null=$process.WaitForExit(5000);throw 'Fixture child exceeded its deadline'}
  $out=$stdout.GetAwaiter().GetResult();$err=$stderr.GetAwaiter().GetResult()
  if($process.ExitCode -ne 0 -or $out -or $err){throw 'Fixture child failed or emitted unexpected output'}
}finally{$process.Dispose()}
`);
  for (const [mode, expected] of [
    ['selected', null], ['wrong-engine', 'Launcher resolves a different engine or data root'],
    ['wrong-data', 'Launcher resolves a different engine or data root'], ['nonzero', 'Installed launcher failed validation'],
  ]) {
    const result = path.join(root, `${mode}.json`);
    execFileSync('pwsh', ['-NoProfile', '-File', parent, '-Child', child, '-Shell', shell, '-App', app, '-Data', data,
      '-Archive', archive, '-Candidate', candidate, '-Mode', mode, '-Result', result], { windowsHide: true, timeout: 20000, stdio: 'pipe' });
    const observation = JSON.parse(fs.readFileSync(result));
    assert.equal(observation.error, expected, mode);
    assert.equal(observation.before, 437);
    assert.equal(observation.after, 437, `${mode}: caller encoding must be restored`);
  }
});
