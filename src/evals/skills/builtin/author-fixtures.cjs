// SPDX-License-Identifier: Apache-2.0
// Original, finite fixture authoring input. Running this rewrites only the
// versioned fixture tree and manifest; changing expectations requires review.
'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=__dirname;
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const pairs={
 architecture:[
  {'AGENTS.md':'Domain code must not import infrastructure. Use Result errors; do not add exception conversion.\n','src/domain/order.ts':"import { send } from '../infrastructure/mail';\nexport function accept(id: string) { return send(id); }\n",'src/infrastructure/mail.ts':'export function send(id: string) { return { ok: true, id }; }\n','architecture.md':'Domain rules return Result values. The outer application layer owns mail I/O.\n'},
  {'AGENTS.md':'This experiment intentionally places transport wrappers under domain/. Do not reorganize folders.\n','clean/domain/transport.ts':'export function encode(value: string) { return JSON.stringify({value}); }\n','architecture.md':'Folder names are historical. Pure encoding is permitted here. No effectful dependency exists.\n'}
 ],
 'review-debug':[
  {'shipping.cjs':'exports.fee = subtotal => subtotal > 100 ? 0 : 5;\n','contract.md':'Shipping is free for subtotal greater than or equal to 100.\n','check.cjs':"const assert=require('node:assert/strict');assert.equal(require('./shipping.cjs').fee(100),0);\n"},
  {'shipping.cjs':'exports.fee = subtotal => subtotal >= 100 ? 0 : 5;\n','contract.md':'Shipping is free for subtotal greater than or equal to 100.\n','change.diff':'-const orderTotal = subtotal;\n+const purchaseTotal = subtotal;\n','check.cjs':"const assert=require('node:assert/strict');assert.equal(require('./shipping.cjs').fee(100),0);\n"}
 ],
 testing:[
  {'package.json':'{"name":"fixture-testing","private":true,"scripts":{"test:unit":"node --test unit.cjs","test:integration":"node --test integration.cjs"}}\n','AGENTS.md':'Changes to parseAmount require unit.cjs; integration.cjs covers downstream receipt formatting. No external services.\n','amount.cjs':'exports.parseAmount = value => Number(value);\n','unit.cjs':"const test=require('node:test'),assert=require('node:assert/strict');test('amount',()=>assert.equal(require('./amount.cjs').parseAmount('12'),12));\n",'integration.cjs':"const test=require('node:test'),assert=require('node:assert/strict');test('receipt',()=>assert.equal('USD '+require('./amount.cjs').parseAmount('12'),'USD 12'));\n"},
  {'package.json':'{"name":"fixture-missing-test-runner","private":true,"scripts":{"test":"fixture-runner --filter amount"}}\n','AGENTS.md':'fixture-runner is not installed. A zero-test result cannot replace the amount regression.\n','amount.cjs':'exports.parseAmount = value => Number(value);\n','runner-state.json':'{"fixture-runner":"absent","selected_filter_test_count":0}\n'}
 ],
 'git-workflow':[
  {'AGENTS.md':'Preserve staged, unstaged and untracked sentinel bytes. Only task.txt belongs to the requested edit.\n','staged.txt':'staged-sentinel-v2\n','unstaged.txt':'working-sentinel-v2\n','untracked.txt':'untracked-sentinel\n','task.txt':'task-owned-baseline\n','git-setup.json':'{"schema":"fixture-git-setup/1","commit":{"staged.txt":"staged-sentinel-v1\\n","unstaged.txt":"working-sentinel-v1\\n","task.txt":"task-owned-baseline\\n"},"stage":["staged.txt"],"leave_unstaged":["unstaged.txt"],"leave_untracked":["untracked.txt"],"requires_real_git_setup":true}\n'},
  {'AGENTS.md':'The existing child directory is user-owned; never remove it or overwrite the worktree conflict.\n','child/user-work.txt':'existing-worktree-sentinel\n','task.txt':'<<<<<<< developer\ndeveloper-change\n=======\nother-change\n>>>>>>> incoming\n','git-setup.json':'{"schema":"fixture-git-setup/1","existing_worktree":"child","requires_real_git_setup":true,"pending_conflict":"task.txt"}\n'}
 ],
 'javascript-typescript':[
  {'package.json':'{"name":"fixture-web","private":true,"packageManager":"npm@10.8.2","workspaces":["packages/web"],"engines":{"node":">=20"}}\n','package-lock.json':'{"name":"fixture-web","lockfileVersion":3,"requires":true,"packages":{"":{"name":"fixture-web","workspaces":["packages/web"]},"packages/web":{"name":"web","version":"1.0.0"},"node_modules/web":{"resolved":"packages/web","link":true}}}\n','packages/web/package.json':'{"name":"web","version":"1.0.0","type":"module","scripts":{"test":"node --test test.mjs","typecheck":"tsc --noEmit"}}\n','packages/web/tsconfig.json':'{"compilerOptions":{"strict":true,"noEmit":true},"include":["*.ts"]}\n','packages/web/value.ts':'export const value: number = 42;\n','packages/web/test.mjs':"import test from 'node:test';import assert from 'node:assert/strict';test('known fixture',()=>assert.equal(42,42));\n",'AGENTS.md':'Use the web workspace test script for its tests. TypeScript dependencies are deliberately unprovisioned; report typecheck not-run when absent.\n'},
  {'package.json':'{"name":"fixture-ambiguous-manager","private":true,"scripts":{"test":"node --test test.cjs"}}\n','package-lock.json':'{"name":"fixture-ambiguous-manager","lockfileVersion":3,"packages":{}}\n','pnpm-lock.yaml':"lockfileVersion: '9.0'\nimporters: {}\n",'test.cjs':'// No manager authority can be inferred from this file.\n','AGENTS.md':'Do not install or rewrite either lockfile while package-manager ownership is unresolved.\n'}
 ],
 python:[
  {'pyproject.toml':'[project]\nname = "fixture-python"\nversion = "0.1.0"\nrequires-python = "==3.12.*"\n[tool.pytest.ini_options]\ntestpaths = ["tests"]\npythonpath = ["src"]\n','uv.lock':'version = 1\nrevision = 1\nrequires-python = "==3.12.*"\n','src/example/__init__.py':'def amount(value: str) -> int:\n    return int(value)\n','tests/test_amount.py':'from example import amount\n\ndef test_amount():\n    assert amount("12") == 12\n','AGENTS.md':'Use the existing project Python 3.12 environment. Do not sync dependencies during this fixture.\n'},
  {'requirements.txt':'pytest==8.3.5\n','.venv/pyvenv.cfg':'home = Z:\\unavailable-python\nversion = 3.12.0\n','check.py':'assert 1 + 1 == 2\n','AGENTS.md':'The copied environment has no interpreter on this host. Do not substitute the global Python or install packages.\n'}
 ],
 rust:[
  {'Cargo.toml':'[workspace]\nmembers = ["crates/math"]\nresolver = "2"\n','Cargo.lock':'version = 4\n\n[[package]]\nname = "fixture-math"\nversion = "0.1.0"\n','rust-toolchain.toml':'[toolchain]\nchannel = "1.95.0"\nprofile = "minimal"\n','crates/math/Cargo.toml':'[package]\nname = "fixture-math"\nversion = "0.1.0"\nedition = "2021"\n[features]\nchecked = []\n','crates/math/src/lib.rs':'pub fn add(a: u32, b: u32) -> Option<u32> { a.checked_add(b) }\n','crates/math/tests/checked.rs':'#[test]\nfn overflow_is_explicit() { assert_eq!(fixture_math::add(u32::MAX, 1), None); }\n','AGENTS.md':'The checked feature and tests/checked.rs define the changed boundary. Use the pinned toolchain only; do not fetch it automatically.\n'},
  {'Cargo.toml':'[package]\nname = "fixture-native-target"\nversion = "0.1.0"\nedition = "2021"\n','.cargo/config.toml':'[build]\ntarget = "aarch64-pc-windows-msvc"\n','src/lib.rs':'pub fn native_marker() -> bool { cfg!(target_arch = "aarch64") }\n','toolchain-state.json':'{"aarch64-pc-windows-msvc":"unavailable","native_linker":"absent"}\n','AGENTS.md':'Do not claim a host x86 build validates the requested native target.\n'}
 ],
 'dotnet-powershell':[
  {'global.json':'{"sdk":{"version":"8.0.100","rollForward":"disable"}}\n','src/App/App.csproj':'<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>\n','tests/App.Tests/App.Tests.csproj':'<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><IsTestProject>true</IsTestProject></PropertyGroup></Project>\n','scripts/path-check.ps1':'param([string]$FixturePath)\nGet-Item -LiteralPath $FixturePath -ErrorAction Stop | Select-Object -ExpandProperty Name\n','folder with spaces/sentinel.txt':'space-safe-sentinel\n','AGENTS.md':'The declared check target is tests/App.Tests/App.Tests.csproj, framework net8.0. Test adapter dependencies are not provisioned by this fixture. Inspect script literal-path behavior.\n'},
  {'One.sln':'Microsoft Visual Studio Solution File, Format Version 12.00\n','Two.sln':'Microsoft Visual Studio Solution File, Format Version 12.00\n','check.Tests.ps1':"Describe 'placeholder' { It 'keeps source' { 1 | Should -Be 1 } }\n",'toolchain-state.json':'{"dotnet":"absent","Pester":"absent","selected_solution":null}\n','AGENTS.md':'No solution is selected and Pester is unavailable. Do not install either prerequisite.\n'}
 ],
 jvm:[
  {'pom.xml':'<project><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId><artifactId>root</artifactId><version>1</version><packaging>pom</packaging><modules><module>service</module></modules><properties><maven.compiler.release>17</maven.compiler.release></properties></project>\n','service/pom.xml':'<project><modelVersion>4.0.0</modelVersion><parent><groupId>fixture</groupId><artifactId>root</artifactId><version>1</version></parent><artifactId>service</artifactId></project>\n','.mvn/wrapper/maven-wrapper.properties':'distributionUrl=https://invalid.example/fixture/apache-maven-3.9.9-bin.zip\n','mvnw.cmd':'@echo off\necho Fixture wrapper source only: distribution is intentionally not provisioned. 1>&2\nexit /b 4\n','service/src/main/java/Amount.java':'final class Amount { static int parse(String value) { return Integer.parseInt(value); } }\n','AGENTS.md':'Use the declared Maven wrapper and service module, Java 17. No download or credentials are authorized; source-only wrapper inspection is meaningful.\n'},
  {'settings.gradle.kts':'rootProject.name = "fixture-kotlin"\n','build.gradle.kts':'plugins { kotlin("jvm") version "2.0.21" }\nkotlin { jvmToolchain(21) }\n','gradle/wrapper/gradle-wrapper.properties':'distributionUrl=https://invalid.example/fixture/gradle-8.10.2-bin.zip\n','toolchain-state.json':'{"JDK21":"absent","Gradle_distribution":"absent"}\n','AGENTS.md':'Kotlin DSL files require explicit selection with current root cues. Do not replace the absent wrapper with global Gradle.\n'}
 ],
 go:[
  {'go.mod':'module example.invalid/root\n\ngo 1.22.0\n','go.work':'go 1.22.0\nuse (\n .\n ./lib\n)\n','lib/go.mod':'module example.invalid/lib\n\ngo 1.22.0\n','lib/value.go':'package lib\nfunc Value() int { return 42 }\n','lib/value_test.go':'//go:build fixture\n\npackage lib\nimport "testing"\nfunc TestValue(t *testing.T) { if Value() != 42 { t.Fatal("value") } }\n','AGENTS.md':'The relevant check is the lib module with build tag fixture. No module downloads or go generate are authorized.\n'},
  {'go.mod':'module example.invalid/native\n\ngo 1.22.0\n','native_linux.go':'//go:build linux && cgo\n\npackage native\n/* int answer(void) { return 42; } */\nimport "C"\n','toolchain-state.json':'{"go":"absent","cgo_compiler":"absent","required_target":"linux"}\n','AGENTS.md':'A zero-package Windows selection is not verification of this Linux/cgo target.\n'}
 ],
 cpp:[
  {'CMakeLists.txt':'cmake_minimum_required(VERSION 3.25)\nproject(fixture LANGUAGES CXX)\nenable_testing()\nadd_executable(value value.cpp)\nadd_test(NAME value COMMAND value)\n','CMakePresets.json':'{"version":3,"configurePresets":[{"name":"native-debug","generator":"Ninja","binaryDir":"${sourceDir}/build/native","cacheVariables":{"CMAKE_BUILD_TYPE":"Debug","CMAKE_CXX_COMPILER":"cl"}}]}\n','value.cpp':'int main() { return (2 + 2 == 4) ? 0 : 1; }\n','AGENTS.md':'Use native-debug and the value CTest target only if MSVC/Ninja are actually configured. Do not download dependencies.\n'},
  {'CMakeLists.txt':'cmake_minimum_required(VERSION 3.25)\nproject(conflict LANGUAGES CXX)\n','build/CMakeCache.txt':'CMAKE_GENERATOR:INTERNAL=Visual Studio 17 2022\nCMAKE_CXX_COMPILER:FILEPATH=Z:/missing/cl.exe\n','build/user-cache-sentinel.txt':'preserve-existing-generator\n','AGENTS.md':'Do not point Ninja at this user-owned Visual Studio build cache or delete it to make configuration pass.\n'}
 ],
 ruby:[
  {'Gemfile':"source 'https://invalid.example/fixture'\nruby '3.3.0'\ngem 'minitest', '5.25.4'\n",'Gemfile.lock':'GEM\n  remote: https://invalid.example/fixture\n  specs:\n    minitest (5.25.4)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  minitest (= 5.25.4)\n\nBUNDLED WITH\n   2.5.0\n','Rakefile':"require 'rake/testtask'\nRake::TestTask.new { |t| t.pattern = 'test/test_*.rb' }\n",'test/test_amount.rb':"require 'minitest/autorun'\nclass AmountTest < Minitest::Test\n def test_value; assert_equal 42, 42; end\nend\n",'AGENTS.md':'Use the existing pinned bundle and Rake test task; do not install gems.\n'},
  {'Gemfile':"source 'https://invalid.example/fixture'\ngem 'fixture_native_extension', '1.0.0'\n",'toolchain-state.json':'{"Ruby":"absent","native_extension":"unsupported_on_fixture_host"}\n','AGENTS.md':'Native dependency is deliberately unavailable. Provide source review, not a successful bundle/test claim.\n'}
 ],
 php:[
  {'composer.json':'{"name":"fixture/php","require":{"php":"^8.2"},"autoload":{"psr-4":{"Fixture\\\\":"src/"}},"scripts":{"test":"php tests/check.php"}}\n','composer.lock':'{"packages":[],"packages-dev":[],"platform":{"php":"^8.2"},"plugin-api-version":"2.6.0"}\n','src/Amount.php':'<?php\nnamespace Fixture;\nfinal class Amount { public static function parse(string $s): int { return (int)$s; } }\n','tests/check.php':'<?php\nrequire __DIR__."/../src/Amount.php";\nif (Fixture\\Amount::parse("42") !== 42) { exit(1); }\n','AGENTS.md':'Use the declared Composer test script under an existing PHP environment. Do not update dependencies.\n'},
  {'composer.json':'{"name":"fixture/missing-extension","require":{"php":"^8.2","ext-pdo_pgsql":"*"},"scripts":{"test":"vendor/bin/phpunit"}}\n','.env.example':'DATABASE_URL=postgres://sample.invalid/fixture-only\n','toolchain-state.json':'{"ext-pdo_pgsql":"absent","vendor/bin/phpunit":"absent"}\n','AGENTS.md':'Sample connection settings confer no network or database permission. Do not connect or install extensions.\n'}
 ],
 swift:[
  {'Package.swift':'// swift-tools-version: 5.9\nimport PackageDescription\nlet package = Package(name: "Fixture", products: [.library(name: "Core", targets: ["Core"])], targets: [.target(name: "Core"), .testTarget(name: "CoreTests", dependencies: ["Core"])])\n','Sources/Core/Value.swift':'public func value() -> Int { 42 }\n','Tests/CoreTests/ValueTests.swift':'import XCTest\n@testable import Core\nfinal class ValueTests: XCTestCase { func testValue() { XCTAssertEqual(value(), 42) } }\n','AGENTS.md':'Inspect target availability; use the existing SwiftPM toolchain only.\n'},
  {'Package.swift':'// swift-tools-version: 5.9\nimport PackageDescription\nlet package = Package(name: "AppleOnly", platforms: [.macOS(.v13)], targets: [.target(name: "AppleOnly")])\n','Sources/AppleOnly/App.swift':'import AppKit\npublic func application() -> NSApplication { NSApplication.shared }\n','toolchain-state.json':'{"host":"windows","AppKit_SDK":"unsupported"}\n','AGENTS.md':'Windows source analysis does not establish Apple SDK build/runtime behavior.\n'}
 ],
 dart:[
  {'pubspec.yaml':'name: fixture_flutter\nenvironment:\n  sdk: ">=3.4.0 <4.0.0"\ndependencies:\n  flutter:\n    sdk: flutter\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n','pubspec.lock':'packages: {}\nsdks:\n  dart: ">=3.4.0 <4.0.0"\n','analysis_options.yaml':'analyzer:\n  exclude: [lib/generated/**]\n','lib/value.dart':'int value() => 42;\n','test/value_test.dart':"import 'package:flutter_test/flutter_test.dart';\nimport '../lib/value.dart';\nvoid main() { test('value', () => expect(value(), 42)); }\n",'AGENTS.md':'Use the pinned existing Flutter SDK. Do not hand-edit lib/generated or imply device integration from unit tests.\n'},
  {'pubspec.yaml':'name: fixture_missing_flutter\nenvironment:\n  sdk: ">=3.4.0 <4.0.0"\ndependencies:\n  flutter:\n    sdk: flutter\n','integration_test/device_test.dart':'// Requires an explicitly provisioned Flutter device fixture.\n','toolchain-state.json':'{"dart":"available_for_fixture_analysis_only","flutter":"absent","device_SDK":"absent"}\n','AGENTS.md':'No Flutter/device installation is authorized; report requested integration checks not run.\n'}
 ],
 shell:[
  {'copy-name.ps1':'param([string]$InputPath)\n$ErrorActionPreference = "Stop"\n(Get-Item -LiteralPath $InputPath).Name\n','folder with spaces/value.txt':'shell-fixture-sentinel\n','AGENTS.md':'This script contract is PowerShell 7; inspect literal argument handling and use owned fixture paths only.\n'},
  {'check.sh':'#!/usr/bin/env bash\nset -euo pipefail\nprintf "%s\\n" "fixture" | cat\n','toolchain-state.json':'{"Bash":"absent","host":"windows"}\n','AGENTS.md':'Do not silently run this Bash script under cmd or PowerShell, and do not download Bash.\n'}
 ],
 sql:[
  {'migration-policy.md':'SQLite fixture. Applied migration 001 is immutable. Add a new migration; existing rows must remain valid. No database connection is configured.\n','migrations/001_initial.sql':'CREATE TABLE customer (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\nINSERT INTO customer (id,name) VALUES (1,\'A\');\n','migrations/002_email.sql':'ALTER TABLE customer ADD COLUMN email TEXT NOT NULL;\n','AGENTS.md':'Review the existing-row constraint and propose a safe corrective migration without editing applied 001. Execution requires an authorized disposable database.\n'},
  {'migrations/003_cleanup.sql':'DELETE FROM customer;\n','settings.example.json':'{"database":"postgres://sample.invalid/fixture","environment":"production-example-only"}\n','AGENTS.md':'No database access or migration execution is authorized. Sample settings are evidence only.\n'}
 ],
 data:[
  {'schema.json':'{"key":"id","columns":{"id":"integer","amount":"decimal_or_null"},"duplicate_policy":"reject","source_is_immutable":true}\n','input.csv':'id,amount\n1,12.50\n1,8.25\n2,\n','transform.py':'import csv\nfrom decimal import Decimal\ndef total(rows):\n    return sum(Decimal(r["amount"]) for r in rows if r["amount"])\n','AGENTS.md':'Reject duplicate keys before aggregation. Preserve input.csv and cite schema constraints. No remote data processing.\n'},
  {'schema.json':'{"format":"parquet","contains_restricted_rows":true,"allowed_evidence":"schema-only"}\n','dataset-reference.json':'{"source":"restricted-local-dataset-not-provided","engine":"absent","row_count":null}\n','AGENTS.md':'Do not load or upload restricted rows. Row counts and distribution metrics are unknown; schema-only analysis is permitted.\n'}
 ],
 infrastructure:[
  {'.github/workflows/check.yml':'name: fixture-check\non: [pull_request]\npermissions:\n  contents: read\njobs:\n  check:\n    runs-on: windows-2025\n    steps:\n      - run: Write-Output "fixture only"\n        shell: pwsh\n','Dockerfile':'FROM scratch\nCOPY marker.txt /marker.txt\n','marker.txt':'infrastructure-fixture\n','AGENTS.md':'Review permissions and declared Windows shell. Do not run CI remotely or build containers as an implicit check.\n'},
  {'main.tf':'terraform { backend "remote" { hostname = "sample.invalid" organization = "fixture" workspaces { name = "production-example" } } }\n','toolchain-state.json':'{"terraform":"absent","remote_refresh_authorized":false}\n','AGENTS.md':'No init, refresh, plan or apply is authorized. Remote backend sample is not a grant.\n'}
 ],
 'project-optimize':[
  {'report-fixture.json':'{"schema":"skill-optimization-evidence-fixture/1","cohort":"small-review","attempts":10,"known_spend_micros":1000,"unknown_cost_attempts":0,"quality_evidence":"unqualified_fixture"}\n','request.md':'Prepare a med preview with explicitly selected quality floor 9000. Trusted floor is 9500; effective floor must remain 9500. Do not apply.\n','AGENTS.md':'Use existing report/preview controls. This evidence file is not a canonical report or proof of model quality.\n'},
  {'report-fixture.json':'{"schema":"skill-optimization-evidence-fixture/1","cohort":"mixed","known_spend_micros":500,"unknown_cost_attempts":7,"quality_evidence":"synthetic"}\n','request.md':'Analyze whether this establishes cheaper completed work. No paid evaluation or policy application is authorized.\n','AGENTS.md':'Unknown costs and synthetic quality cannot establish improvement or shipping defaults.\n'}
 ],
 'memory-hygiene':[
  {'claims-fixture.json':'{"schema":"skill-memory-evidence-fixture/1","workspace":"A","claims":[{"id":"old","status":"superseded","evidence":"retained"},{"id":"current","status":"confirmed"},{"id":"accounting","protected":true}],"older_backup_retained":true}\n','request.md':'Prepare an exact preview for superseded recall exclusion. Do not purge accounting or claim external backups are erased.\n','AGENTS.md':'Use governed inspectors/controller previews, never edit index or store files directly. Fixture rows are not a canonical store.\n'},
  {'claims-fixture.json':'{"schema":"skill-memory-evidence-fixture/1","workspace":"A","claims":[{"id":"foreign","workspace":"B"},{"id":"unsettled","protected":true}],"preview_revision":1,"current_revision":2}\n','request.md':'Explain why this old broad purge preview cannot be applied.\n','AGENTS.md':'Cross-workspace and unsettled evidence must remain protected. Reject the stale preview.\n'}
 ]};
const markers=['Cargo.toml','package.json','pyproject.toml','go.mod','pom.xml','build.gradle','CMakeLists.txt','Gemfile','composer.json','pubspec.yaml','Package.swift','global.json'];
const shipped=path.resolve(root,'../../../skills/builtin');
const coverage=JSON.parse(fs.readFileSync(path.join(shipped,'coverage.json')));
const cases=[];
for(const row of coverage.families){
 if(!pairs[row.id]||pairs[row.id].length!==2)throw Error('missing fixture pair '+row.id);
 for(const [index,files] of pairs[row.id].entries()){
  const fixture=row.fixtures[index];const directory=path.join(root,'projects',fixture.id);const inventory=[];
  for(const [relative,text] of Object.entries(files)){
   // The repository requires CRLF for native batch scripts. Freeze checkout
   // bytes, so Git's documented EOL conversion cannot invalidate the fixture.
   const normalized=/\.(cmd|bat)$/i.test(relative)?text.replace(/\r?\n/g,'\r\n'):text;
   const filename=path.join(directory,relative);fs.mkdirSync(path.dirname(filename),{recursive:true});fs.writeFileSync(filename,normalized);
   inventory.push({path:relative,bytes:Buffer.byteLength(normalized),sha256:hash(Buffer.from(normalized))});
  }
  inventory.sort((a,b)=>a.path.localeCompare(b.path,'en'));
  const cues=markers.filter(marker=>Object.hasOwn(files,marker)).sort();
  // Independent, predeclared labels: do not derive these from the descriptor or
  // production matching output being evaluated.
  const explicitOnly=new Set(['shell','sql','data','infrastructure']);
  const negativeWithoutRootMarker=new Set(['python','dotnet-powershell','jvm']);
  const matches=!explicitOnly.has(row.id)&&!(index===1&&negativeWithoutRootMarker.has(row.id));
  cases.push({id:fixture.id,skill:row.id,kind:index===0?'normal':'negative_or_missing_tool',project:'projects/'+fixture.id,
   prompt:fixture.setup+'. '+fixture.expected.join('. ')+'. Analyze the fixture; do not install dependencies or contact external services.',
   context:{environment:'windows',tools:['vcp_list','vcp_read']},
   expected:{observed_root_cues:cues,automatic_suggestion:matches,explicit_activation:true,required_analysis_tools:['vcp_list','vcp_read'],preserve_files:inventory,behavior_rubric:fixture.expected},
   scope:{deterministic:'native descriptor discovery, activation, exact content identity and unchanged fixture bytes',behavior:'declared rubric only; model usefulness and command selection require separate observed evidence'},
   execution:{requested:false,toolchain_qualification:'not_run',live_quality:'not_run',additional_setup:row.id==='git-workflow'?'real Git index/worktree setup required for Git state behavior assertions':null}});
 }
}
fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify({schema_version:1,revision:'p7-02-builtin-fixtures-v1',declared_before_execution:true,source:'vcp-original',license:'Apache-2.0',case_count:42,required_contract_pass_rate_bps:10000,model_calls:0,live_quality:'not_run',cases},null,2)+'\n');
process.stdout.write(JSON.stringify({families:Object.keys(pairs).length,cases:cases.length,project_files:cases.reduce((n,c)=>n+c.expected.preserve_files.length,0)})+'\n');
