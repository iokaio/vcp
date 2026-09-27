// SPDX-License-Identifier: Apache-2.0
'use strict';
// The qualified fixture server will supply the bounded /ready and /poll responses.
async function load(){ const state=document.getElementById('state'), retry=document.getElementById('retry'); state.textContent='Loading'; retry.hidden=true; try { const response=await fetch('/poll'); if(!response.ok) throw Error('poll failed'); const rows=await response.json(); document.getElementById('items').replaceChildren(...rows.map(row=>Object.assign(document.createElement('li'),{textContent:row.label}))); state.textContent=`${rows.length} items`; } catch { state.textContent='Unable to load'; retry.hidden=false; } }
document.getElementById('retry').addEventListener('click',load); load();
