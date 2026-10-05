// SPDX-License-Identifier: Apache-2.0
// Offline compiler fixtures only; never reads or edits the running scenario application.
'use strict';
const test=require('node:test'), assert=require('node:assert/strict');
const fs=require('node:fs'), path=require('node:path'), os=require('node:os');
const {spawnSync}=require('node:child_process');
const repository=path.resolve(__dirname,'../../..');
const evaluator=path.join(repository,'scripts/evals/inventory-domain-probe/InventoryDomainProbe.csproj');
function dotnet(args) {
  const result=spawnSync('dotnet',args,{encoding:'utf8',timeout:120000,env:{...process.env,DOTNET_CLI_TELEMETRY_OPTOUT:'1',DOTNET_SKIP_FIRST_TIME_EXPERIENCE:'1'}});
  if(result.error) throw result.error;
  return result;
}
test('external compiled model probe catches weakened tests and rejects invalid execution evidence',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-domain-oracle-'));
  try {
    const artifacts=path.join(root,'evaluator');
    const build=dotnet(['build',evaluator,'--artifacts-path',artifacts,'--nologo','-v:q']);
    assert.equal(build.status,0,build.stdout+build.stderr);
    const executable=path.join(artifacts,'bin/InventoryDomainProbe/debug/InventoryDomainProbe.dll');
    const scope={workspace:'synthetic-domain-fixture',session:'fixture',task:'t1'};
    const terminal=path.join(root,'terminal.jsonl');
    const accepted={type:'accepted',correlation:'fixture-correlation',scope};
    const completed={type:'result',correlation:'fixture-correlation',scope,exit_code:0};
    const saveReceipt=rows=>fs.writeFileSync(terminal,rows.map(r=>JSON.stringify(r)).join('\n')+'\n');
    saveReceipt([accepted,completed]);
    const cases=[
      {name:'valid',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment',validation:'if (Quantity == 0) yield return new ValidationResult("nonzero", new[]{ nameof(Quantity) });',passed:true},
      {name:'valid-memberless',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment',validation:'if (Quantity == 0) yield return new ValidationResult("nonzero");',passed:true},
      {name:'weakened-zero-and-string',reason:'string',members:'Receipt, Sale, Adjustment',validation:'yield break;',failed:['reason-exact-enum','quantity-0-Receipt']},
      {name:'positive-only',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment',validation:'if (Quantity <= 0) yield return new ValidationResult("positive");',failed:['quantity--1-Sale','quantity--1-Adjustment']},
      {name:'extra-enum-member',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment, Other',validation:'if (Quantity == 0) yield return new ValidationResult("nonzero");',failed:['reason-exact-enum']},
      {name:'rejects-everything',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment',validation:'yield return new ValidationResult("always invalid");',failed:['quantity-1-Receipt','quantity--1-Sale']},
      {name:'source-mutates-during-probe',reason:'StockMovementReason',members:'Receipt, Sale, Adjustment',mutates:true},
    ];
    let valid;
    for(const item of cases) {
      const workspace=path.join(root,item.name); fs.mkdirSync(workspace);
      const validation=item.mutates ? `System.IO.File.AppendAllText(${JSON.stringify(path.join(workspace,'Marker.txt'))}, "x"); yield break;` : item.validation;
      fs.writeFileSync(path.join(workspace,'Model.csproj'),'<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>');
      fs.writeFileSync(path.join(workspace,'Model.cs'),`using System.ComponentModel.DataAnnotations;
namespace Inventory.Web.Data;
public enum StockMovementReason { ${item.members} }
public class StockMovement : IValidatableObject {
 public int Id {get;set;} public int ProductId {get;set;} public int Quantity {get;set;}
 public ${item.reason} Reason {get;set;} public DateTime OccurredAt {get;set;} public string Note {get;set;}
 public IEnumerable<ValidationResult> Validate(ValidationContext context) { ${validation} }
}
// A generated test's name or assertion is deliberately irrelevant to the external oracle.
public class DomainValidationTests { public bool StockMovement_requires_nonzero_quantity() => true; }
`);
      const compiled=dotnet(['build',path.join(workspace,'Model.csproj'),'--nologo','-v:q']);
      assert.equal(compiled.status,0,compiled.stdout+compiled.stderr);
      const assembly=path.join(workspace,'bin/Debug/net10.0/Model.dll');
      const report=path.join(root,item.name+'.json');
      const args=[executable,'--workspace',workspace,'--assembly',assembly,'--terminal-jsonl',terminal,'--out',report];
      const result=dotnet(args);
      if(item.mutates) {
        assert.equal(result.status,2,result.stdout+result.stderr);
        assert.equal(fs.existsSync(report),false,'Mutating model must not publish passing evidence');
        continue;
      }
      assert.equal(result.status,item.passed?0:1,result.stdout+result.stderr);
      const evidence=JSON.parse(fs.readFileSync(report));
      assert.equal(evidence.passed,!!item.passed);
      assert.match(evidence.source_and_binary_sha256,/^[a-f0-9]{64}$/);
      assert.match(evidence.assembly.sha256,/^[a-f0-9]{64}$/);
      for(const id of item.failed??[]) assert.ok(evidence.checks.some(check=>check.Id===id&&!check.Passed),id);
      if(item.passed) valid={workspace,assembly};
      assert.equal(dotnet(args).status,2,'Existing owner report must never be overwritten');
    }
    const run=(output)=>dotnet([executable,'--workspace',valid.workspace,'--assembly',valid.assembly,'--terminal-jsonl',terminal,'--out',output]);
    for(const [i,rows] of [[accepted],[accepted,{...completed,exit_code:7}],[accepted,{...completed,scope:{...scope,task:'foreign'}}],[accepted,completed,{type:'event'}]].entries()) {
      saveReceipt(rows); const output=path.join(root,`rejected-${i}.json`);
      assert.equal(run(output).status,2); assert.equal(fs.existsSync(output),false);
    }
    saveReceipt([accepted,completed]);
    assert.equal(run(path.join(valid.workspace,'model-controlled-report.json')).status,2);
  } finally {
    if(path.dirname(root)!==fs.realpathSync(os.tmpdir()) && path.dirname(root)!==path.resolve(os.tmpdir())) throw Error('Unexpected fixture cleanup path');
    fs.rmSync(root,{recursive:true,force:true});
  }
});
