// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {fixture}=require('./fixture.cjs'),{taskboard,inventory}=require('./acceptance.cjs'),{inspect}=require('./browser.cjs');
for(const kind of ['A','B']) test(`actual pinned Chromium exercises synthetic ${kind} UI oracle`,async()=>{
  const app=fixture(kind),state=await(kind==='A'?taskboard:inventory)(app.request,'full');
  const server=await app.listen();
  const root=path.resolve(__dirname,'../../../artifacts');
  const output=fs.mkdtempSync(path.join(root,`ee07-synthetic-browser-${kind}-`));
  try {
    const report=await inspect({kind,base:server.base,state,output,requireHelp:true});
    report.qualification='Synthetic oracle fixture only; not actual engagement/application acceptance.';
    fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));
    assert.equal(report.status,'passed',report.error);assert(report.checks.length>=4);assert.equal(report.screenshots.length,1);
  }finally{await server.close();}
});
