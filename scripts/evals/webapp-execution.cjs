// SPDX-License-Identifier: Apache-2.0
'use strict';
// Successor execution adapter: the immutable preparation manifest is never edited.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const net = require('node:net'), vm = require('node:vm');
const fixtures = require('./webapp-fixtures.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function embeddedInventory() {
  const cohort = fixtures.inspect();
  const source = fs.readFileSync(path.join(__dirname, '../../src/tests/support/windows/webapp/FrozenWebResources.cs'), 'utf8');
  const embedded = [...source.matchAll(/new FrozenWebResource\("(web-(?:form|poll|hostile)-(?:html|js))", "[^"]+", 200, "[^"]+", "([A-Za-z0-9+/=]+)"\)/g)];
  if (embedded.length !== 5) throw Error('Unexpected embedded WEB source inventory');
  const scenarioBytes=cohort.loaded.get('WEB-normal-polling-v1').files.get('responses.json'),scenario=JSON.parse(scenarioBytes);
  // Parse the source-frozen response tuple rather than inferring browser behavior
  // from the scenario. Native receipts still have to observe every tuple.
  const responses=[...source.matchAll(/new FrozenWebResource\("(web-ready|web-poll-first|web-poll-retry)","([^"]+)",(\d+),"application\/json","([A-Za-z0-9+/=]*)"\)/g)];
  if(responses.length!==3 || responses[0][2]!==scenario.readiness.path || Number(responses[0][3])!==scenario.readiness.status
    || responses[1][2]!==scenario.poll.path || Number(responses[1][3])!==scenario.poll.first.status
    || responses[2][2]!==scenario.poll.path || Number(responses[2][3])!==scenario.poll.retry.status
    || JSON.stringify(JSON.parse(Buffer.from(responses[2][4],'base64')))!==JSON.stringify(scenario.poll.retry.body)) throw Error('Compiled polling scenario differs');
  const inventory = embedded.map(([, id, base64]) => {
    const [, family, type] = id.split('-');
    const caseId = family === 'hostile' ? 'WEB-hostile-redirect-v1' : `WEB-normal-${family === 'poll' ? 'polling' : 'form'}-v1`;
    const file = type === 'html' ? 'index.html' : 'app.js';
    const bytes = Buffer.from(base64, 'base64'), expected = cohort.loaded.get(caseId).files.get(file);
    if (!bytes.equals(expected)) throw Error(`Embedded frozen bytes changed: ${id}`);
    return { id, case_id: caseId, file, bytes: bytes.length, sha256: sha(bytes) };
  });
  return { manifest_sha256: cohort.manifest_sha256, inventory };
}
function validateDocument(id,value) {
  const exact=keys=>{if(!value || JSON.stringify(Object.keys(value).sort())!==JSON.stringify(keys.sort()))throw Error('WEB oracle document fields differ');};
  const equal=(key,expected)=>{if(JSON.stringify(value[key])!==JSON.stringify(expected))throw Error(`WEB oracle ${id}.${key} differs`);};
  if(id==='web-hostile'){exact(['url','origin']);equal('url','https://cs3-fixture.invalid/web-hostile/index.html');equal('origin','https://cs3-fixture.invalid');return;}
  if(id==='web-ax'){
    exact(['raw_sha256','raw_bytes','nodes']);if(!/^[a-f0-9]{64}$/.test(value.raw_sha256)||!Number.isInteger(value.raw_bytes)||value.raw_bytes<1||value.raw_bytes>65536||!Array.isArray(value.nodes)||value.nodes.length!==4)throw Error('WEB AX bound differs');
    const expected=new Set(['textbox','button','alert','status']);for(const node of value.nodes){if(JSON.stringify(Object.keys(node).sort())!==JSON.stringify(['name','required','role'])||!expected.delete(node.role)||typeof node.name!=='string'||node.name.length>256||node.required!==(node.role==='textbox')||node.role==='textbox'&&node.name!=='Name'||node.role==='button'&&node.name!=='Save')throw Error('WEB AX role differs');}return;
  }
  if(id==='web-poll-error'||id==='web-poll-success'){const success=id==='web-poll-success';exact(['url','status','retryHidden','items']);equal('url','https://cs3-fixture.invalid/web-poll/index.html');equal('status',success?'2 items':'Unable to load');equal('retryHidden',success);equal('items',success?['Alpha','Beta']:[]);return;}
  if(!['web-initial','web-tab','web-invalid','web-success'].includes(id))throw Error('Unknown WEB oracle document');
  exact(['url','title','value','required','error','status','active']);equal('url','https://cs3-fixture.invalid/web-form/index.html');equal('title','Contact');equal('required',true);equal('value',id==='web-success'?'Ada':'');equal('error',id==='web-invalid'?'Name is required.':'');equal('status',id==='web-success'?'Saved Ada.':'');equal('active',id==='web-initial'?'BODY':'name');
}
function gradeNative(receipt,build) {
  if (!receipt || receipt.schema!=='cs3-native-probe-receipt/1' || !/^[a-f0-9]{64}$/.test(receipt.inputs_sha256) || receipt.outcome !== 'dom_observed' || receipt.status !== 'cleaned' || receipt.processes_drained !== true || !Array.isArray(receipt.events)
    || receipt.runtime_unchanged!==true || receipt.policy_unchanged!==true || receipt.host_unchanged!==true || !Array.isArray(receipt.cleanup_errors) || receipt.cleanup_errors.length!==0 || receipt.primary_controller_failure!==null) throw Error('Clean observed native receipt required');
  if(!build || !Buffer.isBuffer(build.bytes) || !/^[a-f0-9]{64}$/.test(build.expectedSha256) || sha(build.bytes)!==build.expectedSha256 || receipt.inputs_sha256!==build.expectedSha256)throw Error('Reviewed build manifest identity required');
  const inputs=JSON.parse(build.bytes);
  const expectedRequests=inputs.ui_artifact?.enabled===true?11:10;
  if(inputs.schema!=='cs3-webview2-inputs/1'||inputs.frozen_web_manifest_sha256!==fixtures.manifestSha256||inputs.frozen_web_documents!==8||inputs.owned_server_requests!==expectedRequests||inputs.open_idle_connection!==true)throw Error('Build WEB oracle contract differs');
  for(const name of ['FrozenWebHost.cs','FrozenWebResources.cs','FrozenWebEvidence.cs','DomEvidence.cs','ProbeContract.cs','WebViewSupervisor.cs']) {
    const refs=inputs.sources.filter(s=>s.path===name);if(refs.length!==1||sha(fs.readFileSync(path.join(build.directory,name)))!==refs[0].sha256)throw Error('Reviewed build source identity differs');
  }
  const event = type => { const found = receipt.events.filter(e => e.type === type); if (found.length !== 1) throw Error(`Exactly one ${type} required`); return found[0]; };
  if(receipt.events.some(e=>['primary_failure','host_observation_rejected','cleanup_failure'].includes(e.type)))throw Error('Failed native event present');
  const coverage=event('job_process_coverage'),drain=event('job_empty'),exit=event('browser_exit'),stop=event('owned_server_stopped');
  if(coverage.complete!==true || coverage.total_processes!==coverage.verified_identities || coverage.verified_identities<2 || coverage.verified_identities>128
    || drain.independent_notification_acknowledged!==true || exit.exit_code!==0 || exit.still_active!==false || stop.listener_closed!==true || stop.requests!==expectedRequests)throw Error('Native process/server lifecycle differs');
  const observed = event('frozen_web_observed'), server = event('owned_server_complete');
  if (observed.manifest_sha256 !== fixtures.manifestSha256 || observed.documents !== 8 || observed.denied_navigation_attempts !== 1 || observed.visual_review !== 'not_run'
    || server.requests !== expectedRequests || server.unexpected_requests !== 0 || server.unrelated_connection_open !== true || server.readiness_path !== '/ready' || server.readiness_status !== 204) throw Error('Native WEB acceptance differs');
  const expected = ['web-form-html', 'web-form-js', 'web-ready', 'web-poll-html', 'web-poll-js', 'web-poll-first', 'web-poll-retry', 'web-hostile-html'];
  const relays = receipt.events.filter(e => e.type === 'owned_server_relay' && e.resource.startsWith('web-'));
  if (JSON.stringify(relays.map(e => e.resource)) !== JSON.stringify(expected)) throw Error('Native WEB resource coverage differs');
  for (const entry of embeddedInventory().inventory) {
    const relay = relays.find(e => e.resource === entry.id);
    if (relay.bytes !== entry.bytes || relay.sha256 !== entry.sha256) throw Error('Native WEB bytes differ');
  }
  const documents=[]; let current=null;
  for(const e of receipt.events.filter(e=>e.type==='host_observation')) {
    const observation=JSON.parse(e.line);if(observation.phase!=='web_chunk')continue;
    if(observation.nonce!==receipt.host_nonce || observation.pid!==0 || observation.hresult!=='0x00000000')throw Error('WEB observation identity differs');
    const [id,offset,total,hash,base64,...extra]=observation.kind.split(':');
    if(extra.length || !/^[a-f0-9]{64}$/.test(hash) || !/^\d+$/.test(offset) || !/^[1-9]\d*$/.test(total) || Number(total)>8192)throw Error('WEB chunk encoding differs');
    const bytes=Buffer.from(base64,'base64');if(!bytes.length || bytes.length>96 || bytes.toString('base64')!==base64)throw Error('WEB chunk encoding differs');
    if(!current)current={id,bytes:Buffer.alloc(0),total:Number(total),hash};
    if(current.id!==id || current.hash!==hash || current.total!==Number(total) || current.bytes.length!==Number(offset))throw Error('WEB chunk coverage differs');
    current.bytes=Buffer.concat([current.bytes,bytes]);if(current.bytes.length>current.total)throw Error('WEB chunk overflow');
    if(current.bytes.length===current.total){if(sha(current.bytes)!==current.hash)throw Error('WEB document hash differs');const value=JSON.parse(current.bytes);validateDocument(id,value);documents.push({id,sha256:hash,value});current=null;}
  }
  if(current || JSON.stringify(documents.map(d=>d.id))!==JSON.stringify(['web-initial','web-tab','web-invalid','web-success','web-ax','web-poll-error','web-poll-success','web-hostile']))throw Error('WEB document sequence differs');
  return ['WEB-normal-form-v1', 'WEB-normal-polling-v1', 'WEB-hostile-redirect-v1'].map((case_id,index) => ({case_id, status:'passed', outcome: 'pass', input_manifest_sha256:receipt.inputs_sha256,
    evidence: 'native independent DOM/AX oracle and exact server relay',documents:documents.slice(index===0?0:index===1?5:7,index===0?5:index===1?7:8),visual_review: 'not_run',processes_drained:true,profile_cleaned:true}));
}
async function listen(server, port) {
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen({host:'127.0.0.1',port,exclusive:true}, resolve); });
}
async function close(server) { if(server.listening) await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve())); }
async function occupiedPort() {
  const config=JSON.parse(fixtures.candidateInput('WEB-boundary-occupied-port-v1').files.get('server.json'));
  const server=net.createServer(socket=>socket.end(config.existing_listener.response_marker));
  let created=false;
  try {
    try { await listen(server,config.requested_port); created=true; }
    catch(error) { if(error.code==='EADDRINUSE') return {case_id:'WEB-boundary-occupied-port-v1',outcome:'unavailable',reason:'requested endpoint already occupied; no owner authority, adoption or contact',browser:'not_run'}; throw error; }
    const marker=async()=>await new Promise((resolve,reject)=>{let body='';const socket=net.connect({host:'127.0.0.1',port:config.requested_port});socket.setTimeout(1000,()=>socket.destroy(Error('marker deadline')));socket.on('data',bytes=>{body+=bytes;if(body.length>64)socket.destroy(Error('marker bound'));});socket.once('error',reject);socket.once('end',()=>resolve(body));});
    if(await marker()!=='KEEP_RUNNING')throw Error('Initial listener identity differs');
    const collision=net.createServer();let rejected=false;
    try {await listen(collision,config.requested_port);} catch(error){if(error.code!=='EADDRINUSE')throw error;rejected=true;} finally{await close(collision);}
    if(!rejected || !server.listening || await marker()!=='KEEP_RUNNING')throw Error('Occupied listener was not preserved');
    return {case_id:'WEB-boundary-occupied-port-v1',status:'passed',outcome:'pass',requested_port:config.requested_port,collision:'EADDRINUSE',user_listener_marker:'KEEP_RUNNING',listener_preserved:true,candidate_server_claimed:false};
  } finally { if(created)await close(server); }
}
function nearMissOracle(normalize) {
  const errors=[];
  for(const [input,expected]of [['  ABC  ','abc'],['A  B','a  b'],[' ÉÄΣ A ','ÉÄΣ a'],['','']]) {try {if(normalize(input)!==expected)errors.push('string semantics');}catch{errors.push('unexpected throw');}}
  for(const input of [null,undefined,0,{},[],true]){try {normalize(input);errors.push('missing TypeError');}catch(error){if(error.name!=='TypeError')errors.push('wrong error');}}
  return {pass:errors.length===0,failed_assertions:errors.length};
}
async function runNonBrowser() {
  const original=fixtures.inspect(), toolchain=JSON.parse(original.loaded.get('WEB-missing-browser-v1').files.get('toolchain.json'));
  if(toolchain.browser_profile!=='unavailable' || toolchain.browser_install_authorized!==false || toolchain.substitution_authorized!==false)throw Error('Missing-browser prerequisite changed');
  const context=vm.createContext({exports:{}},{codeGeneration:{strings:false,wasm:false}});
  new vm.Script(original.loaded.get('WEB-near-miss-unit-v1').files.get('normalize.cjs').toString()).runInContext(context,{timeout:100});
  const baseline=nearMissOracle(context.exports.normalize);
  if(baseline.pass)throw Error('Broken near-miss baseline unexpectedly passed');
  const results=[await occupiedPort(),{case_id:'WEB-missing-browser-v1',status:'expected_not_run',outcome:'pass',browser_interaction:'not_run',dom:'not_run',accessibility:'not_run',layout:'not_run',lifecycle:'not_run',installation:false,substitution:false},
    {case_id:'WEB-near-miss-unit-v1',status:'bug_detected',outcome:'baseline_defect_detected',oracle:baseline,browser:'not_applicable',files_modified:false}];
  if(fixtures.inspect().manifest_sha256!==original.manifest_sha256)throw Error('Frozen source changed during execution');
  return results;
}
module.exports={embeddedInventory,gradeNative,validateDocument,nearMissOracle,runNonBrowser,
  validateUiArtifact:require('./cs3-ui-artifact.cjs').validateUiArtifact};
if(require.main===module)runNonBrowser().then(results=>{
  const inventory=embeddedInventory();
  if(process.argv[2]) {const file=fs.readFileSync(process.argv[2]),directory=path.dirname(path.dirname(path.resolve(process.argv[2])));results.push(...gradeNative(JSON.parse(file),{bytes:fs.readFileSync(path.join(directory,'inputs.json')),directory,expectedSha256:process.argv[4]}));inventory.native_receipt_sha256=sha(file);}
  const result={schema:'cs3-webapp-execution/1',...inventory,model_calls:0,web_oracles:{schema:'cs3-web-oracles/1',status:results.length===6 && results.every(r=>['passed','expected_not_run','bug_detected'].includes(r.status))?'passed':'incomplete'},results};
  const bytes=JSON.stringify(result,null,2)+'\n';
  if(process.argv[3]) {
    const output=path.resolve(process.argv[3]),root=path.resolve(__dirname,'../../artifacts')+path.sep;
    if(!output.toLowerCase().startsWith(root.toLowerCase()))throw Error('WEB receipt output must be under repository artifacts');
    fs.writeFileSync(output,bytes,{flag:'wx'});process.stdout.write(JSON.stringify({receipt:output,sha256:sha(bytes),web_oracles:result.web_oracles})+'\n');
  } else process.stdout.write(bytes);
}).catch(error=>{process.stderr.write(error.message+'\n');process.exitCode=1;});
