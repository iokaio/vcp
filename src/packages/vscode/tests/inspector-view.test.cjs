// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const Module = require('node:module');
const { INSPECTOR_TABS, parseInspectorMessage, emptyInspectorState, boundInspectorState, inspectorPanelHtml } = require('../dist/inspector_view_model.js');
const opaque = '12345678-1234-1234-1234-123456789abc';
const state = extra => ({ ...emptyInspectorState('optimizer', 'current', 'authorized read'), ...extra });

test('inspector messages only select closed tabs and opaque action handles', () => {
  for (const action of ['ready', 'refresh']) assert.deepEqual(parseInspectorMessage({ action }), { action });
  for (const tab of INSPECTOR_TABS) assert.deepEqual(parseInspectorMessage({ action: 'tab', tab }), { action: 'tab', tab });
  assert.deepEqual(parseInspectorMessage({ action: 'invoke', id: opaque }), { action: 'invoke', id: opaque });
  for (const value of [null, [], {action:'invoke', id:'-'.repeat(36)}, {action:'tab',tab:'command:evil'}, {action:'refresh',cursor:'chosen'}, {action:'invoke',id:opaque,scope:{}}, {action:'invoke',id:opaque,url:'file:/private'}, Object.create({action:'ready'}), Object.defineProperty({},'action',{enumerable:true,get(){throw Error('getter executed')}}), {action:'ready',[Symbol()]:true}, new Proxy({}, {ownKeys(){throw Error('trap')}})]) assert.equal(parseInspectorMessage(value), undefined);
});
test('exact review display bounds fail closed without retaining any action or abbreviated policy', () => {
  const exact = state({sections:[{title:'Exact review',fields:[],text:'x'.repeat(70000)}],actions:[{id:opaque,label:'Apply exact policy'}]});
  assert.equal(boundInspectorState(exact), exact);
  const rejected = boundInspectorState({...exact,sections:[{title:'Exact review',fields:[],text:'☃'.repeat(100000)}]});
  assert.equal(rejected.phase,'partial'); assert.deepEqual(rejected.actions,[]); assert.deepEqual(rejected.sections,[]);
  assert.equal(boundInspectorState(state({sections:Array.from({length:65},()=>({title:'',fields:[]}))})).phase,'partial');
  const html=inspectorPanelHtml('local:', 'style" onload="bad', 'script', 'nonce');
  assert.ok(html.includes("default-src 'none'")); assert.ok(html.includes("script-src 'nonce-nonce'")); assert.ok(html.includes('style&quot; onload=&quot;bad')); assert.ok(!html.includes('unsafe-inline'));
});
function renderer() {
  const nodes=new Map(),posted=[]; let receive;
  const node=tag=>({tag,textContent:'',children:[],listeners:{},dataset:{},disabled:false,attributes:{},append(...values){this.children.push(...values)},replaceChildren(){this.children=[]},setAttribute(k,v){this.attributes[k]=v},addEventListener(k,v){this.listeners[k]=v},set innerHTML(_){throw Error('HTML sink')},set href(_){throw Error('link sink')},set src(_){throw Error('resource sink')}});
  for(const name of ['tabs','phase','message','scope','task','refresh','sections','actions','commands'])nodes.set(`inspector-${name}`,node(name));
  vm.runInNewContext(fs.readFileSync(require('node:path').join(__dirname,'../media/inspectors.js'),'utf8'),{TextEncoder,document:{getElementById:id=>nodes.get(id),createElement:tag=>{assert.ok(!['a','script','img','iframe'].includes(tag));return node(tag)}},window:{addEventListener:(_kind,listener)=>receive=listener},acquireVsCodeApi:()=>({postMessage:value=>posted.push(JSON.parse(JSON.stringify(value))),getState(){throw Error('no stored content')},setState(){throw Error('no stored content')}})});
  return {nodes,posted,receive:value=>receive({data:{type:'inspectors',state:value}})};
}
test('renderer preserves exact hostile review text, counters, reasons, and emits only opaque clicks', () => {
  const {nodes,posted,receive}=renderer(); assert.deepEqual(posted,[{action:'ready'}]);
  const hostile='<img src="file:/secret"><a href="command:run">javascript:steal()</a>';
  const exact=hostile.repeat(1000);
  receive(state({scopeLabel:hostile,sections:[{title:hostile,fields:[{label:'Cost',value:'90071992547409931234'}],text:exact,rows:[{title:'row',fields:[],actions:[{id:opaque,label:hostile,disabledReason:'Controller required'}]}]}],actions:[{id:opaque,label:'Apply exact review'}]}));
  const section=nodes.get('inspector-sections').children[0];
  assert.equal(section.children[2].textContent,exact); assert.equal(section.children[1].children[1].textContent,'90071992547409931234');
  const button=nodes.get('inspector-actions').children[0]; assert.equal(button.dataset.action,opaque); button.listeners.click();button.listeners.click();assert.deepEqual(posted.at(-1),{action:'invoke',id:opaque});assert.equal(posted.length,2);
  assert.equal(section.children[3].children[2].disabled,true);assert.equal(section.children[3].children[3].textContent,'Controller required');
  const tab=nodes.get('inspector-tabs').children[0];tab.listeners.click();assert.deepEqual(posted.at(-1),{action:'tab',tab:'history'});
  receive(state({sections:[{title:'overflow',fields:[],text:'z'.repeat(300000)}],actions:[{id:opaque,label:'Do not allow'}]}));assert.equal(nodes.get('inspector-actions').children.length,0);assert.equal(nodes.get('inspector-sections').children.length,0);assert.equal(nodes.get('inspector-phase').textContent,'partial');
});
test('panel clears on ready, hidden, reveal and recreation; late hidden publication is discarded', async () => {
  const old=Module._load;Module._load=function(request,parent,main){if(request==='vscode')return {Uri:{joinPath:(_uri,...parts)=>({toString:()=>parts.join('/')})}};return old.call(this,request,parent,main)};
  let InspectorPanel;try{({InspectorPanel}=require('../dist/inspector_panel.js'))}finally{Module._load=old}
  const posted=[],messages=[],visibility=[];let receive,changed,disposed;
  const disposable=()=>({dispose(){}});
  const view={visible:true,webview:{cspSource:'local:',asWebviewUri:x=>x,postMessage:x=>{posted.push(x);return Promise.resolve(true)},onDidReceiveMessage:fn=>{receive=fn;return disposable()}},onDidChangeVisibility:fn=>{changed=fn;return disposable()},onDidDispose:fn=>{disposed=fn;return disposable()}};
  const panel=new InspectorPanel({},async message=>{messages.push(message)},visible=>visibility.push(visible));panel.resolveWebviewView(view);
  panel.publish(state({sections:[{title:'private content',fields:[]}]}));receive({action:'ready'});assert.equal(posted.at(-1).state.sections.length,0);await Promise.resolve();assert.deepEqual(messages,[{action:'ready'}]);
  panel.publish(state({sections:[{title:'private again',fields:[]}]}));view.visible=false;changed();const count=posted.length;panel.publish(state({message:'late secret'}));receive({action:'invoke',id:opaque});assert.equal(posted.length,count);assert.equal(posted.at(-1).state.sections.length,0);
  view.visible=true;changed();assert.equal(posted.at(-1).state.sections.length,0);assert.deepEqual(visibility,[true,false,true]);
  receive({action:'invoke',id:opaque,scope:'forged'});assert.equal(messages.length,1);
  disposed();assert.equal(visibility.at(-1),false);const after=posted.length;panel.publish(state({message:'late disposed'}));assert.equal(posted.length,after);
  panel.resolveWebviewView(view);assert.equal(posted.at(-1).state.sections.length,0);panel.dispose();
});
