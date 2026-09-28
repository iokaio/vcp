// SPDX-License-Identifier: Apache-2.0
'use strict';
const form=document.getElementById('contact'), nameInput=document.getElementById('name');
form.addEventListener('submit',event=>{ event.preventDefault(); const error=document.getElementById('name-error'), status=document.getElementById('status'), name=nameInput.value.trim(); if(!name){ error.textContent='Name is required.'; status.textContent=''; nameInput.focus(); return; } error.textContent=''; status.textContent=`Saved ${name}.`; });
