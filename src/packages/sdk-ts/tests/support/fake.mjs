// SPDX-License-Identifier: Apache-2.0
import { Client } from '../../dist/client.js';
export const scope = {workspace:'workspace',session:'session'};
export class Fake {
  writes=[]; closed=0;
  listen(frame,failed) {this.frame=frame;this.failed=failed;return()=>{this.frame=undefined;this.failed=undefined;};}
  setMaximum(n){this.maximum=n;}
  async send(bytes){this.writes.push(JSON.parse(bytes.toString()));this.onSend?.(this.writes.at(-1));}
  async close(){this.closed++;}
  reply(request,result){this.frame?.({jsonrpc:'2.0',id:request.id,result});}
}
export async function connect(methods=['session/read','session/create','command/read']) {
  const transport=new Fake();
  transport.onSend=request=>{
    if(request.method==='initialize') transport.reply(request,{protocol_version:'1.0',event_schema_version:'1.0',schema_version:'1.0',engine_build:'test',capabilities:request.params.capabilities,methods,limits:{maximum_frame_bytes:1048576,maximum_pending_requests:1,maximum_subscriptions:8,maximum_subscriber_queue_bytes:1048576},execution_host:{id:'host',platform:'test'},sandbox_capabilities:[]});
  };
  const client=await Client.connect(transport,scope,'controller');
  transport.onSend=undefined;transport.writes=[];
  return {client,transport};
}
export const session={kind:'session',value:{scope,revision:'0',configuration_revision:'0'}};
export const create={scope,mutation:{command_id:'original',expected_revision:'0',steering_revision:'0'},new_session:'new-session',configuration:'config'};
export const tick=()=>new Promise(resolve=>setImmediate(resolve));
