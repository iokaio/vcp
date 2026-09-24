// SPDX-License-Identifier: Apache-2.0
// Fresh Node process: only non-secret observer discovery state crosses reload.
import assert from 'node:assert/strict';
import {reconnectObserverLocal} from '../dist/index.js';
let input='';for await(const chunk of process.stdin){input+=chunk;if(input.length>65536)throw new Error('oversized input');}
const {executable,reference,initialize,task,pending}=JSON.parse(input);
const client=await reconnectObserverLocal({executable,reference,initialize});
try {
 assert.equal(client.role,'observer');
 assert.equal((await client.call('controller/read',{scope:client.scope})).value.ownership,'other_connection');
 assert.deepEqual((await client.call('task/read',{scope:client.scope,task})).value.pending_inputs,pending);
 await assert.rejects(client.call('controller/acquire',{scope:client.scope,command_id:'reloaded-observer-denied',expected_revision:null}));
 process.stdout.write('ok');
}finally{await client.dispose();}
