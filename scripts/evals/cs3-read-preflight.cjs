// SPDX-License-Identifier: Apache-2.0
'use strict';
// One separately budgeted, read-only native CLI compatibility observation.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), capture = require('./developer-runner.cjs');
const { fixedProfileReasons } = require('./builtin-live-runner.cjs');
const { plain, read, write, within, filesUnder, noParentInstructions, privateDirectory, frames, inspection, invoke } = prior.boundaries;
const root = path.resolve(__dirname, '../..');
const MODEL = 'deepseek/deepseek-v3.2', ENDPOINT = 'deepinfra/fp4';
const EXECUTABLE = 'd08ff1069d6700a8aebc7ba668b510ce98867dc6ec2f68b7fed079b34bc5312e';
const CONTENT = 'CS3_READ_FIRST_71C9\nCS3_READ_MIDDLE_A25E\nCS3_READ_LAST_8D04\n';
const ANSWER = { whole: ['CS3_READ_FIRST_71C9', 'CS3_READ_MIDDLE_A25E', 'CS3_READ_LAST_8D04'], range: 'CS3_READ_MIDDLE_A25E' };
const WHOLE = { path: 'status.txt', max_bytes: 4096, start_line: null, end_line: null };
const RANGE = { path: 'status.txt', max_bytes: 4096, start_line: 2, end_line: 2 };
const PROMPT = 'Read status.txt with vcp_read using exactly path="status.txt", max_bytes=4096, start_line=null, end_line=null (JSON integers and null, not strings). Wait for that tool result, then read only its second line with vcp_read using path="status.txt", max_bytes=4096, start_line=2, end_line=2. Do not batch the two reads together. Both calls are required even though the whole-file read already contains line two. After receiving the second result, run vcp_verify citing the actual returned evidence artifact IDs. Do not select skills or change files. After verification returns, return only a JSON object with whole (array of the three line strings, without newlines) and range (the second line string, without newline). No Markdown fences.\n';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const sourcePaths = ['scripts/evals/cs3-read-preflight.cjs', 'scripts/evals/p6-live-runner.cjs', 'scripts/evals/developer-runner.cjs', 'scripts/evals/builtin-live-runner.cjs'];
const probeSources = { binary: 'src/crates/vcp-cli/src/bin/vcp-provider-conformance.rs', lease: 'src/crates/vcp-lifecycle/src/foundation/conformance.rs', settlement: 'src/crates/vcp-lifecycle/src/foundation/worker/conformance.rs', catalog: 'src/crates/vcp-models/src/catalog.rs' };
function requireThat(value, reason) { if (!value) throw Error(reason); }
function bound(ref, maximum = 16 * 1024 * 1024) {
  requireThat(ref && path.isAbsolute(ref.path || '') && /^[a-f0-9]{64}$/.test(ref.sha256), 'Absolute hash-bound evidence required');
  const bytes = read(ref.path, maximum); requireThat(sha(bytes) === ref.sha256, 'Bound evidence changed'); return bytes;
}
const json = ref => JSON.parse(bound(ref));
const reference = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) });
function count(value) { requireThat(typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) && Number.isSafeInteger(Number(value)), 'Canonical counter required'); return Number(value); }
function dollarMicros(value) {
  requireThat(typeof value === 'number' && Number.isFinite(value) && value >= 0, 'Finite observed USD amount required');
  const match = String(value).match(/^(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$/), fraction = match?.[2] || '';
  requireThat(match, 'Observed USD encoding rejected'); const shift = 6 + Number(match[3] || 0) - fraction.length;
  requireThat(Math.abs(shift) <= 100, 'Observed USD exponent bound'); const numerator = BigInt(match[1] + fraction);
  const result = shift >= 0 ? numerator * 10n ** BigInt(shift) : (numerator + 10n ** BigInt(-shift) - 1n) / 10n ** BigInt(-shift);
  requireThat(result <= BigInt(Number.MAX_SAFE_INTEGER), 'Observed USD bound'); return Number(result);
}
function sources() {
  // Literal local imports are source-derived, never the mutable require.cache
  // graph. Dynamic fixture imports in unused legacy campaign functions are not
  // part of the four shared helpers used here (read/inspect/retain/answer).
  const files = new Set();
  function visit(file) {
    if(files.has(file))return;files.add(file);const absolute=path.join(root,file),text=read(absolute).toString();
    if(!/\.(?:cjs|js)$/.test(file))return;
    for(const match of text.matchAll(/require\(\s*(['"])(\.[^'"]+)\1\s*\)/g)){
      const dependency=require.resolve(path.resolve(path.dirname(absolute),match[2]));
      requireThat(within(root,dependency),'Shared helper import escapes repository');visit(path.relative(root,dependency).split(path.sep).join('/'));
    }
  }
  for (const file of sourcePaths) visit(file);
  return Object.fromEntries([...files].sort().map(file => [file, sha(read(path.join(root, file)))]));
}
function validateQualification(ref, spec) {
  const wrapper = json(ref), profile = json(spec.profile), catalog = json(spec.catalog);
  requireThat(wrapper.schema === 'cs3-successor-provider-qualification/1' && wrapper.model === MODEL && wrapper.endpoint === ENDPOINT, 'Successor qualification identity required');
  requireThat(equal(Object.keys(wrapper).sort(),['binary','catalog','endpoint','model','probe_claim','profile','qualification_claim','schema','snapshot','sources']),'Qualification wrapper fields differ');
  requireThat(equal(wrapper.profile, spec.profile) && equal(wrapper.catalog, spec.catalog), 'Qualification profile/catalog reference differs');
  const input = json(wrapper.sources), claim = json(wrapper.probe_claim), qualified = json(wrapper.qualification_claim), snapshot = json(wrapper.snapshot);
  const probe = json(input.probe_spec), report = json(input.report), original = json({ path: probe.catalog, sha256: probe.catalog_sha256 });
  bound(wrapper.binary, 1024 * 1024 * 1024);
  requireThat(claim.binary_sha256 === wrapper.binary.sha256 && equal(claim.spec, probe) && claim.spec_sha256 === input.probe_spec.sha256, 'Conformance executable/spec claim differs');
  requireThat(equal(claim.source_sha256, Object.fromEntries(Object.entries(probeSources).map(([key, file]) => [key, sha(read(path.join(root, file)))]))), 'Conformance embedded source closure changed');
  requireThat(probe.model === MODEL && probe.endpoint === ENDPOINT && probe.cap_usd === '0.250000' && probe.max_output_tokens === 2048, 'Conformance limits or endpoint differ');
  requireThat(equal(input.catalog, spec.catalog) && qualified.schema === 'p6-provider-qualification/1' && qualified.authorized_sources_sha256 === wrapper.sources.sha256, 'Qualification source authorization differs');
  requireThat(equal(snapshot, profile.provider) && snapshot.raw_sha256 === spec.catalog.sha256 && snapshot.compatibility?.model === MODEL && snapshot.compatibility?.endpoint === ENDPOINT && snapshot.compatibility?.id === 'p6-generation-qualified/' + wrapper.sources.sha256, 'Exact qualified snapshot required');
  requireThat(snapshot.compatibility.responses_text_tools === true && snapshot.compatibility.provider_preferences_qualified === true && snapshot.compatibility.deny_data_collection === true && snapshot.compatibility.require_zdr === false, 'Qualified compatibility policy differs');
  requireThat(snapshot.observed_at === input.observed_at && snapshot.valid_until === input.valid_until && count(input.valid_until) > count(input.observed_at) && count(input.valid_until) <= count(probe.observed_at) + 86400000, 'Qualification dated window differs');
  const endpoint = data => { requireThat(data.data?.id === MODEL, 'Catalog model differs'); const rows = data.data.endpoints.filter(row => row.tag === ENDPOINT); requireThat(rows.length === 1 && rows[0].status === 0 && ['tools', 'tool_choice', 'max_tokens'].every(p => rows[0].supported_parameters.includes(p)), 'Exact available tool endpoint required'); return rows[0]; };
  const selected = endpoint(catalog), previous = endpoint(original);
  requireThat(selected.context_length === previous.context_length && selected.max_completion_tokens === previous.max_completion_tokens && equal(selected.pricing, previous.pricing), 'Endpoint capabilities or tariffs changed');
  requireThat(report.schema === 'p6-provider-conformance/1' && report.status === 'observed' && report.responses_text_tools === true && report.candidate?.raw_sha256 === probe.catalog_sha256 && report.candidate.price?.model === MODEL && report.candidate.price?.provider === ENDPOINT, 'Successful exact conformance observation required');
  requireThat(['context','max_input','max_output'].every(key=>snapshot[key]===report.candidate[key]) && count(snapshot.context)===selected.context_length && count(snapshot.max_input)===(selected.max_prompt_tokens??selected.context_length) && count(snapshot.max_output)===Math.min(selected.context_length,selected.max_completion_tokens) && equal(snapshot.price.rates,report.candidate.price.rates) && snapshot.price.currency==='USD' && snapshot.price.model===MODEL && snapshot.price.provider===ENDPOINT && snapshot.price.valid_until===input.valid_until,'Qualified endpoint bounds or tariff differs from actual probe');
  const ledger = report.ledger;
  requireThat(ledger?.currency === 'USD' && ledger.cap === '250000' && ledger.active === '0' && ledger.unresolved === '0' && ledger.overrun === false, 'Known bounded qualification accounting required');
  requireThat(report.responses?.length === 2 && input.generations?.length === 2 && qualified.attribution?.length === 2, 'Exactly two conformance responses required');
  const first = report.responses[0], second = report.responses[1];
  requireThat(first.calls?.length === 1 && first.calls[0].name === 'vcp_conformance_echo' && equal(first.calls[0].arguments, { marker: 'VCP_CONFORMANCE_☃' }) && second.calls?.length === 0 && Object.values(second.completed_messages).join('').trim() === 'VCP_CONFORMANCE_OK', 'Exact echo and continuation proof required');
  let cost = 0;
  for (let index = 0; index < 2; index++) {
    const response = report.responses[index], generation = json(input.generations[index]).data, attribution = qualified.attribution[index];
    requireThat(response.status === 'Completed' && response.served_model === MODEL && response.usage?.cost?.currency === 'USD', 'Completed, charged model response required');
    cost += count(response.usage.cost.micros);
    requireThat(generation?.id === response.response_id && generation.cancelled === false && generation.streamed === true && generation.is_byok === false && generation.provider_name === selected.provider_name && generation.provider_responses?.length === 1 && catalog.data.endpoints.filter(e=>e.provider_name===selected.provider_name).length===1 && !catalog.data.endpoints.some(e=>e.tag.startsWith(ENDPOINT+'/')), 'Single served provider attribution required');
    const served = generation.provider_responses[0];
    requireThat(served.status === 200 && served.is_byok === false && served.provider_name === selected.provider_name && served.model_permaslug === generation.model && selected.name === selected.provider_name + ' | ' + generation.model && (!response.served_provider || [ENDPOINT,selected.provider_name].includes(response.served_provider)), 'Served model revision differs from catalog');
    requireThat(dollarMicros(generation.total_cost)===count(response.usage.cost.micros), 'Generation charge differs from response');
    requireThat(attribution.method === 'generation-single-attempt-exact-catalog-model-provider/2' && attribution.generation_sha256 === input.generations[index].sha256 && attribution.response_id === response.response_id && attribution.requested_model === MODEL && attribution.catalog_endpoint === ENDPOINT && attribution.provider_name === selected.provider_name && attribution.observed_endpoint_id === served.endpoint_id && attribution.observed_model_revision === generation.model, 'Attribution does not join actual generation');
  }
  requireThat(qualified.attribution[0].observed_endpoint_id === qualified.attribution[1].observed_endpoint_id && qualified.attribution[0].observed_model_revision === qualified.attribution[1].observed_model_revision && cost === count(report.actual_cost_micros) && cost === count(ledger.settled) && cost <= 250000, 'Qualification charges/endpoint do not reconcile');
  return { status: 'passed', actual_cost_micros: cost, observed_attempts: 2, model: MODEL, endpoint: ENDPOINT };
}
function sourceProfile(spec, current = true) {
  bound(spec.executable, 1024 * 1024 * 1024); requireThat(spec.executable.sha256 === EXECUTABLE, 'Exact installed CLI required');
  const profile = json(spec.profile); bound(spec.catalog);
  requireThat(!fixedProfileReasons(profile, current ? Date.now() : 0).length && profile.max_requests === 16 && profile.output_tokens === '2048' && profile.deadline_seconds === 180 && profile.provider_timeout_seconds === 60 && profile.max_transport_retries === 0 && profile.maximum_autonomy === 'plan' && equal(profile.automatic_effects, []) && profile.provider.raw_sha256 === spec.catalog.sha256 && profile.provider.compatibility.model === MODEL && profile.provider.compatibility.endpoint === ENDPOINT, 'Fixed read-only successor profile required');
  const allowed = ['version','workspace','trust_workspace','sync_roots','maximum_autonomy','automatic_effects','budget_usd','provider','catalog','affected_paths','canonical_tools','max_requests','output_tokens','provider_timeout_seconds','max_transport_retries','deadline_seconds','processes','checks','mcp','mcp_http'];
  requireThat(Object.keys(profile).every(key => allowed.includes(key)), 'Unexpected source profile extension'); return profile;
}
function derived(spec, directory, current) { return { ...sourceProfile(spec, current), workspace: path.join(directory, 'workspace'), catalog: spec.catalog.path, budget_usd: '0.600000', affected_paths: ['status.txt'], canonical_tools: ['vcp_read', 'vcp_verify'] }; }
function claimFile() { const common = execFileSync('git', ['rev-parse', '--git-common-dir'], { cwd: root, encoding: 'utf8', windowsHide: true }).trim(); return path.join(path.resolve(root, common), 'vcp-cs3-deepinfra-read-preflight-20260928.json'); }
function prepare(specFile, destination) {
  const spec = JSON.parse(read(specFile)), directory = plain(path.resolve(destination));
  requireThat(equal(Object.keys(spec).sort(), ['catalog','executable','profile','qualification']), 'Preflight spec fields differ');
  validateQualification(spec.qualification, spec); const profile = derived(spec, directory, true);
  requireThat(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory), 'New private directory outside repository required'); noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'workspace')); fs.mkdirSync(path.join(directory, 'data'));
  write(path.join(directory, 'workspace/status.txt'), CONTENT); write(path.join(directory, 'profile.json'), profile); write(path.join(directory, 'prompt.txt'), PROMPT);
  const plan = { schema: 'cs3-read-preflight-plan/1', directory, spec, source: sources(), node: reference(fs.realpathSync(process.execPath)), profile_sha256: sha(read(path.join(directory, 'profile.json'))), prompt_sha256: sha(PROMPT), cap_micros: 600000, request_ceiling: 16 };
  write(path.join(directory, 'plan.json'), plan); return reference(path.join(directory, 'plan.json'));
}
function checkPlan(ref, current) {
  const plan = json(ref); requireThat(plan.schema === 'cs3-read-preflight-plan/1' && plain(path.dirname(ref.path)) === plan.directory && equal(plan.source, sources()) && equal(plan.node, reference(fs.realpathSync(process.execPath))), 'Preflight plan/source/runtime identity changed');
  privateDirectory(plan.directory); noParentInstructions(plan.directory); validateQualification(plan.spec.qualification, plan.spec);
  requireThat(equal(JSON.parse(read(path.join(plan.directory, 'profile.json'))), derived(plan.spec, plan.directory, current)) && sha(read(path.join(plan.directory, 'profile.json'))) === plan.profile_sha256 && read(path.join(plan.directory, 'prompt.txt')).toString() === PROMPT && plan.prompt_sha256 === sha(PROMPT) && plan.cap_micros === 600000 && plan.request_ceiling === 16, 'Preflight inputs changed');
  requireThat(equal(filesUnder(path.join(plan.directory, 'workspace')), ['status.txt']) && read(path.join(plan.directory, 'workspace/status.txt')).toString() === CONTENT, 'Read-only workspace changed'); return plan;
}
function invocation(plan) { const base=plan.directory; return ['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd','0.600000','--autonomy','plan']; }
function gapAllowed(view,gap) {
  return prior.privacyGap(gap,gap.artifact) || view==='routing' && equal(gap,{reason:'no automatic routing decision retained for this task; fixed provider or no admitted routed request',requested_model:'attempt.quote.price.model',served_model:'captured response bytes when observed; never inferred from requested model',visibility:'unavailable'});
}
function completedResponses(responses, attempts, scope) {
  const result = [];
  for (const { item,bytes } of responses) for (const block of bytes.toString().split(/\r?\n\r?\n/)) {
    const data = block.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n'); if (!data || data === '[DONE]') continue;
    const event = JSON.parse(data); if (event.type !== 'response.completed') continue;
    const attempt=attempts.find(a=>a.phase==='settled'&&a.provider_request===event.response?.id);
    requireThat(event.response?.status === 'completed' && event.response.model===MODEL && attempt && item.record.spec.source==='retained-codex-attempt:'+attempt.id && equal(item.record.spec.scope,scope), 'Unaccounted, foreign-task, wrong-model or incomplete response'); result.push(event.response);
  }
  requireThat(result.length === attempts.filter(a => a.phase === 'settled').length && new Set(result.map(r => r.id)).size === result.length, 'Response/attempt coverage differs'); return result;
}
function oracle(evidence, artifacts, stdout, exit) {
  const output = frames(stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
  requireThat(exit.status === 0 && !exit.error && accepted?.scope?.task && final?.conditions?.completed === true, 'Native preflight did not complete');
  requireThat(output.filter(f=>f.type==='accepted').length===1&&output.filter(f=>f.type==='result').length===1&&equal(final.scope,accepted.scope)&&['workspace','session','task'].every(k=>typeof accepted.scope[k]==='string'&&accepted.scope[k].length>0),'Exact accepted/final native scope required');
  const money = prior.accounting(evidence.costs, 600000); requireThat(money.attempts.length > 0 && money.attempts.length <= 16, 'Preflight request ceiling');
  requireThat(evidence.costs.flatMap(p=>p.items).filter(i=>i.collection==='ledger').every(i=>equal(i.record.scope,accepted.scope))&&money.attempts.every(a=>equal(a.scope,accepted.scope)&&a.root===accepted.scope.task),'Canonical accounting belongs to another task');
  requireThat(artifacts.every(a=>equal(a.item.record.spec.scope,accepted.scope)),'Retained artifact belongs to another task');
  const responses = artifacts.filter(a => a.item.record.spec.channel === 'response'), completed = completedResponses(responses, money.attempts,accepted.scope);
  const calls = completed.flatMap(r => (r.output || []).filter(o => o.type === 'function_call').map(call=>({...call,response_id:r.id}))).map(call => ({ ...call, decoded: JSON.parse(call.arguments) }));
  requireThat(calls.every(c=>typeof c.call_id==='string' && c.call_id.length>0) && new Set(calls.map(c=>c.call_id)).size===calls.length,'Unique completed tool-call identities required');
  const wholeCalls=calls.filter(c=>c.name==='vcp_read'&&equal(c.decoded,WHOLE)),rangeCalls=calls.filter(c=>c.name==='vcp_read'&&equal(c.decoded,RANGE)),verifyCalls=calls.filter(c=>c.name==='vcp_verify');
  requireThat(calls.length===3&&wholeCalls.length===1&&rangeCalls.length===1&&verifyCalls.length===1&&equal(Object.keys(verifyCalls[0].decoded),['citations'])&&Array.isArray(verifyCalls[0].decoded.citations)&&verifyCalls[0].decoded.citations.length>0,'Exact whole/null, range/integer, verify call sequence required');
  const wholeCall=wholeCalls[0],rangeCall=rangeCalls[0],verifyCall=verifyCalls[0];
  const results = artifacts.filter(a => a.item.record.spec.schema === 'vcp-tool-result-v1').map(a => ({ id: a.item.id, body: JSON.parse(a.bytes) }));
  const whole = results.filter(r => r.body.text === CONTENT && r.body.complete === true), range = results.filter(r => r.body.text === 'CS3_READ_MIDDLE_A25E\n' && equal(r.body.returned_range, { start_line: 2, end_line: 2 }));
  requireThat(whole.length === 1 && range.length === 1 && verifyCall.decoded.citations.every(id => results.some(r => r.id === id)), 'Actual canonical read results and verification citations required');
  const requests=artifacts.filter(a=>a.item.record.spec.schema==='responses-request/1').map(a=>({digest:sha(a.bytes),body:JSON.parse(a.bytes)}));
  function requestFor(responseId){const attempt=money.attempts.find(a=>a.provider_request===responseId),found=requests.filter(r=>r.digest===attempt?.request_digest);requireThat(found.length===1&&Array.isArray(found[0].body.input)&&found[0].body.model===MODEL,'Exact captured dispatched request required');return found[0].body;}
  for(const response of completed)requestFor(response.id);
  function continued(previous,responseId,observed){const input=requestFor(responseId).input,previousCalls=input.filter(i=>i.type==='function_call'&&i.call_id===previous.call_id),outputs=input.filter(i=>i.type==='function_call_output'&&i.call_id===previous.call_id);requireThat(previousCalls.length===1&&previousCalls[0].name===previous.name&&equal(JSON.parse(previousCalls[0].arguments),previous.decoded)&&outputs.length===1,'Actual tool-result continuation and causal call order required');const body=JSON.parse(outputs[0].output);if(observed)requireThat(body.evidence===observed.id&&equal(body.result,observed.body),'Continuation differs from actual read evidence');return body;}
  continued(wholeCall,rangeCall.response_id,whole[0]);continued(rangeCall,verifyCall.response_id,range[0]);
  const manifests = artifacts.filter(a => a.item.record.spec.schema === 'context-manifest/1').map(a => JSON.parse(a.bytes));
  for (const attempt of money.attempts.filter(a => a.phase === 'settled')) { const found = manifests.filter(m => m.request_sha256 === attempt.request_digest); requireThat(found.length > 0 && found.every(m => Array.isArray(m.included) && !m.included.some(p => p.kind === 'skill')), 'No-skill canonical context required for every request'); }
  const verifications=evidence.verification.flatMap(page=>page.items).filter(item=>item.collection==='verification').map(item=>item.record);
  requireThat(verifications.some(record => equal(record?.scope,accepted.scope) && record.outputs?.length>0 && equal(record.outstanding_issues,[]) && equal(record.unresolved_effects,[])), 'Satisfied canonical verification observation required');
  const answer = capture.responseAnswer(responses, money.attempts); requireThat(equal(answer.answer, ANSWER), 'Exact observed marker answer required');const verified=continued(verifyCall,answer.provider_request).verification;
  requireThat(verified&&verifications.filter(record=>equal(record,verified)).length===1&&equal(verified.scope,accepted.scope)&&equal(verified.outstanding_issues,[])&&equal(verified.unresolved_effects,[])&&verifyCall.decoded.citations.every(id=>verified.outputs.includes(id)),'Final continuation does not join actual satisfied verification');
  return { status: 'passed', actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length, scope: accepted.scope, reads: 2, selected_skills: 0, preserved: true };
}
function run(planFile, authorization, call = invoke) {
  const planRef = { path: plain(path.resolve(planFile)), sha256: authorization }, plan = checkPlan(planRef, true), base = plan.directory;
  requireThat(fs.readdirSync(path.join(base, 'data')).length === 0, 'Preflight data directory already used');
  const claim = { schema: 'cs3-read-preflight-claim/1', plan: planRef, cap_micros: 600000, request_ceiling: 16 };
  write(claimFile(), claim); write(path.join(base, 'claim.json'), claim);
  const args = invocation(plan);
  write(path.join(base, 'attempted.json'), { executable: plan.spec.executable, args });
  const report = { schema: 'cs3-read-preflight/1', plan: planRef, status: 'failed', actual_cost_micros: null, raw: {}, artifacts: [] };
  try {
    const execution = call(plan.spec.executable.path, args, 360000); write(path.join(base,'stdout.jsonl'),execution.stdout); write(path.join(base,'stderr.txt'),execution.stderr); write(path.join(base,'exit.json'),{status:execution.status,error:execution.error});
    for (const name of ['stdout.jsonl','stderr.txt','exit.json']) report.raw[name] = reference(path.join(base,name));
    const accepted = frames(execution.stdout).find(f => f.type === 'accepted'); requireThat(accepted?.scope?.task, 'No durable preflight task');
    const evidence = {}, artifacts = [], seen = new Set(), native = { executable: plan.spec.executable.path };
    for (const view of ['costs','routing','outputs','context','tools','verification']) {
      evidence[view] = inspection(native,base,accepted.scope.task,view,call); write(path.join(base,view+'.json'),evidence[view]); report.raw[view] = reference(path.join(base,view+'.json'));
      requireThat(!evidence[view].some(p => p.gaps.some(g => !gapAllowed(view,g))), 'Incomplete raw inspection');
      if(view==='costs'){const money=prior.accounting(evidence.costs,600000);report.actual_cost_micros=money.actual_cost_micros;report.observed_attempts=money.attempts.length;}
      if(view==='routing')continue; // explanatory duplicate view; canonical bytes come from outputs/context/tools
      for (const item of evidence[view].flatMap(p => p.items).filter(i => i.collection === 'artifact')) {
        if (seen.has(item.id)) continue; seen.add(item.id); const bytes = capture.retained(native,base,item,view,call), file = path.join(base,'artifact-'+sha(item.id)+'.bin'); fs.writeFileSync(file,bytes,{flag:'wx',mode:0o600});
        report.artifacts.push({ id:item.id,view,ref:reference(file) }); artifacts.push({ item,bytes });
      }
    }
    const money=prior.accounting(evidence.costs,600000);report.actual_cost_micros=money.actual_cost_micros;report.observed_attempts=money.attempts.length;
    checkPlan(planRef,false); Object.assign(report,oracle(evidence,artifacts,execution.stdout,execution));
  } catch (error) { report.error = error.message; }
  write(path.join(base,'result.json'),report); return { ...reference(path.join(base,'result.json')), status:report.status, actual_cost_micros:report.actual_cost_micros };
}
function validate(ref, spec) {
  const report = json(ref), plan = checkPlan(report.plan,false), base = plan.directory;
  requireThat(report.schema === 'cs3-read-preflight/1' && plain(path.dirname(ref.path)) === base && equal(plan.spec.profile,spec.profile) && equal(plan.spec.catalog,spec.catalog) && equal(plan.spec.executable,spec.executable), 'Preflight does not bind campaign inputs');
  if(spec.successor)requireThat(equal(plan.spec.qualification,spec.successor.qualification),'Preflight qualification wrapper differs');
  requireThat(equal(json(reference(claimFile())),json(reference(path.join(base,'claim.json')))) && equal(json(reference(path.join(base,'claim.json'))).plan,report.plan), 'One-shot preflight ownership differs');
  requireThat(equal(JSON.parse(read(path.join(base,'attempted.json'))),{executable:plan.spec.executable,args:invocation(plan)}),'Exact native invocation differs');
  for(const [name,raw]of Object.entries(report.raw)){const filename=['stdout.jsonl','stderr.txt','exit.json'].includes(name)?name:name+'.json';requireThat(plain(raw.path)===path.join(base,filename),'Raw evidence must belong to the exact preflight directory');}
  const evidence = Object.fromEntries(['costs','routing','outputs','context','tools','verification'].map(view => [view,json(report.raw[view])]));
  const descriptors = new Map(); for (const [view,pages] of Object.entries(evidence)) { requireThat(!pages.some(p => p.gaps.some(g => !gapAllowed(view,g))), 'Incomplete retained inspection'); if(view==='routing')continue;for (const item of pages.flatMap(p => p.items).filter(i => i.collection === 'artifact')) {requireThat(!descriptors.has(item.id)||equal(descriptors.get(item.id),item),'Repeated artifact descriptor differs');descriptors.set(item.id,item);} }
  requireThat(report.artifacts.length === descriptors.size && new Set(report.artifacts.map(a=>a.id)).size === descriptors.size,'Retained artifact coverage differs');
  const artifacts = report.artifacts.map(row => { const item=descriptors.get(row.id),bytes=bound(row.ref); requireThat(item && item.record.state==='complete' && Number(item.record.length)===bytes.length && item.record.sha256===sha(bytes),'Retained artifact descriptor differs'); return {item,bytes}; });
  const observed=oracle(evidence,artifacts,bound(report.raw['stdout.jsonl']).toString(),json(report.raw['exit.json'])); bound(report.raw['stderr.txt']);
  requireThat(Object.entries(observed).every(([key,value])=>equal(report[key],value)),'Preflight claimed outcome differs from raw evidence'); return observed;
}
module.exports={prepare,run,validate,validateQualification,oracle,completedResponses,bound,sources,dollarMicros,gapAllowed,CONTENT,ANSWER,WHOLE,RANGE,PROMPT};
if(require.main===module){try{const [command,...args]=process.argv.slice(2);let result;if(command==='prepare')result=prepare(...args);else if(command==='run')result=run(...args);else throw Error('Usage: prepare SPEC NEW_PRIVATE_DIRECTORY | run PLAN SHA256');process.stdout.write(JSON.stringify(result,null,2)+'\n');if(result.status==='failed')process.exitCode=1;}catch(error){process.stderr.write(error.message+'\n');process.exitCode=1;}}
