// SPDX-License-Identifier: Apache-2.0
// Synthetic oracle qualification only; never used to claim an engagement passed.
'use strict';
const http = require('node:http'), { randomUUID } = require('node:crypto');
const { ordered } = require('./acceptance.cjs');
const copy = value => JSON.parse(JSON.stringify(value));
function fixture(kind, defect) {
  const tasks = new Map(), products = new Map(), operations = new Map(); let nextProduct = 1, nextAudit = 1;
  function response(status, json, headers = {}) { return { status, json: copy(json), headers }; }
  async function request(method, route, body) {
    route = route.split('?')[0];
    if (kind === 'A') {
      if (method === 'GET' && route === '/api/tasks/export') return response(200, {schemaVersion:1,tasks: defect === 'order' ? [...tasks.values()].reverse() : ordered([...tasks.values()])});
      if (method === 'GET' && tasks.has(route.split('/').at(-1))) return response(200,tasks.get(route.split('/').at(-1)));
      if (method === 'POST' && route === '/api/tasks') {
        const task = {id:randomUUID(),description:'',status:'todo',priority:'medium',dueDate:null,labels:[],createdAt:new Date().toISOString(),updatedAt:new Date().toISOString(),...body}; tasks.set(task.id,task); return response(201,task);
      }
      if (method === 'POST' && route === '/api/tasks/import') {
        const valid = body?.schemaVersion === 1 && Array.isArray(body.tasks) && new Set(body.tasks.map(t=>t.id)).size === body.tasks.length && body.tasks.every(t=>t.id && t.title?.trim() && ['todo','doing','done'].includes(t.status) && (!t.dueDate || new Date(t.dueDate).toISOString().slice(0,10)===t.dueDate));
        if (!valid) { if (defect === 'partial' && body?.tasks?.[0]?.id) tasks.set(body.tasks[0].id,body.tasks[0]); return response(400,{error:{code:'VALIDATION_ERROR'}}); }
        if (body.tasks.some(t=>tasks.has(t.id) && JSON.stringify(tasks.get(t.id))!==JSON.stringify(t))) return response(409,{error:{code:'IMPORT_CONFLICT'}});
        for (const task of body.tasks) tasks.set(task.id,task); return response(200,{imported:body.tasks.length});
      }
    } else {
      if (route === '/api/suppliers') return response(200,[{id:1,name:'Fixture supplier'}]);
      if (method === 'POST' && route === '/api/products') { const product={...body,id:nextProduct++,rowVersion:'1',stock:0,history:[]};products.set(product.id,product);return response(201,product); }
      const match = /^\/api\/products\/(\d+)(?:\/(stock|adjustments))?$/.exec(route);
      if (match) {
        const p = products.get(Number(match[1])); if (!p) return response(404,{});
        if (method === 'GET') return response(200,match[2]==='stock'?{onHand:p.stock}:match[2]==='adjustments'?p.history:p,{etag:`"${p.rowVersion}"`});
        if (method === 'POST' && match[2]==='adjustments') {
          const old = operations.get(body.operationId);
          if (old && defect !== 'duplicate') return JSON.stringify(old.body)===JSON.stringify(body) && old.productId===p.id ? response(200,old.receipt) : response(409,{code:'OPERATION_CONFLICT'});
          if (!body.rowVersion) return response(428,{});
          if (body.rowVersion!==p.rowVersion) return response(412,{});
          if (!Number.isInteger(body.delta)||body.delta===0||!body.reason?.trim()||p.stock+body.delta<0) return response(400,{});
          const row={id:nextAudit++,productId:p.id,delta:body.delta,beforeQuantity:p.stock,afterQuantity:p.stock+body.delta,reason:body.reason,timestamp:new Date().toISOString(),operationId:body.operationId};
          p.stock+=body.delta;p.rowVersion=String(Number(p.rowVersion)+1);p.history.push(row);
          const receipt={adjustmentId:row.id,product:{id:p.id,rowVersion:p.rowVersion}};
          operations.set(body.operationId,{body:copy(body),productId:p.id,receipt});return response(201,receipt);
        }
      }
    }
    return response(404,{});
  }
  function html(url) {
    if (kind === 'A') return `<!doctype html><p data-testid="transfer-format-help">JSON format version 1 preserves fields and rejects whole-document conflicts.</p><button id="export">Export tasks</button><label>Import tasks<input id="import" type="file"></label><p id="message"></p><script>
document.querySelector('#export').onclick=async()=>{const d=await(await fetch('/api/tasks/export')).json();const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([JSON.stringify(d)],{type:'application/json'}));a.download='tasks.json';a.click();};
document.querySelector('#import').onchange=async(e)=>{const m=document.querySelector('#message');m.removeAttribute('role');m.textContent='Loading';const r=await fetch('/api/tasks/import',{method:'POST',headers:{'content-type':'application/json'},body:await e.target.files[0].text()});m.setAttribute('role',r.ok?'status':'alert');m.textContent=r.ok?'Import complete, unchanged records retained':'Invalid import';};</script>`;
    const productId=Number(url.searchParams.get('id')),p=products.get(productId);
    if (url.pathname==='/Products/History') return `<!doctype html><table aria-label="Stock adjustment history">${p.history.map(r=>`<tr><td>${r.reason}</td></tr>`).join('')}</table>`;
    return `<!doctype html><p data-testid="adjustment-help">Stable operation IDs preserve idempotent retries and atomic audit updates.</p><form><label>Delta<input id="delta" type="number"></label><label>Reason<input id="reason"></label><button>Apply adjustment</button></form><p id="message"></p><script>
let rowVersion=${JSON.stringify(p.rowVersion)};document.querySelector('form').onsubmit=async(e)=>{e.preventDefault();const m=document.querySelector('#message');m.removeAttribute('role');m.textContent='Saving';const r=await fetch('/api/products/${productId}/adjustments',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({operationId:crypto.randomUUID(),delta:Number(document.querySelector('#delta').value),reason:document.querySelector('#reason').value,rowVersion})});const result=await r.json();m.setAttribute('role',r.ok?'status':'alert');m.textContent=r.ok?'Adjustment saved':'Invalid or stale adjustment';if(r.ok)rowVersion=result.product.rowVersion;};</script>`;
  }
  async function listen() {
    const server = http.createServer(async(req,res)=>{
      try { const url=new URL(req.url,'http://127.0.0.1');
        if (!url.pathname.startsWith('/api/')) {res.setHeader('content-type','text/html; charset=utf-8');res.end(html(url));return;}
        const chunks=[];for await(const part of req)chunks.push(part);const bytes=Buffer.concat(chunks);
        const result=await request(req.method,url.pathname,bytes.length?JSON.parse(bytes):undefined);
        res.writeHead(result.status,{'content-type':'application/json',...result.headers});res.end(JSON.stringify(result.json));
      }catch(error){res.writeHead(500);res.end(JSON.stringify({error:error.message}));}
    });
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    return {base:`http://127.0.0.1:${server.address().port}`,close:()=>new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()))};
  }
  return {request,listen,products,tasks};
}
module.exports={fixture};
