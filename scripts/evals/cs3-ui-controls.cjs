// SPDX-License-Identifier: Apache-2.0
'use strict';
// Trusted positive/negative browser controls, never model task inputs.
const { cases } = require('./cs3-ui-artifact.cjs');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { plain, within } = require('./p6-live-runner.cjs').boundaries;
const style = (background, text, accent) => `
:root{--background:${background};--surface:#ffffff;--text:${text};--accent:${accent};--space:.75rem;--radius:.5rem}
*{box-sizing:border-box}body{margin:0;padding:var(--space);background:var(--background);color:var(--text);font:16px system-ui}
main{width:100%;max-width:40rem;margin:auto;padding:var(--space);border-radius:var(--radius);background:var(--surface)}
input,button{font:inherit;max-width:100%}input[type=text],input[type=search]{display:block;width:100%;padding:.4rem}
button{padding:.5rem;margin-top:var(--space)}label{display:block;margin-block:var(--space)}[hidden]{display:none!important}
:focus-visible{outline:3px solid var(--accent);outline-offset:2px}
@media(prefers-reduced-motion:reduce){*,*::before,*::after{animation:none!important;transition:none!important}}
`;
const filter = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Task picker</title><style>${style('#f8fafc', '#172033', '#175cd3')}</style></head><body><main>
<h1>Task picker</h1><label for="search">Search</label><input id="search" type="search">
<div id="tasks"><label><input type="checkbox" value="Cedar">Cedar</label><label><input type="checkbox" value="Birch">Birch</label><label><input type="checkbox" value="Elm">Elm</label></div>
<p id="empty" hidden>No tasks match</p><p role="status" id="count">0 selected</p><button id="clear" type="button">Clear selection</button>
</main><script>'use strict';
const search=document.getElementById('search'),boxes=Array.from(document.querySelectorAll('#tasks input')),count=document.getElementById('count');
function update(){count.textContent=boxes.filter(box=>box.checked).length+' selected';}
search.addEventListener('input',()=>{const term=search.value.toLowerCase();for(const box of boxes){box.parentElement.hidden=!box.value.toLowerCase().includes(term);}document.getElementById('empty').hidden=boxes.some(box=>!box.parentElement.hidden);update();});
for(const box of boxes)box.addEventListener('change',update);
document.getElementById('clear').addEventListener('click',()=>{for(const box of boxes)box.checked=false;update();});
</script></body></html>`;
const form = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Maintenance request</title><style>${style('#faf8f4', '#24211d', '#6548a3')}</style></head><body><main>
<h1>Maintenance request</h1><button id="details" type="button" aria-expanded="false" aria-controls="instructions">Details</button><p id="instructions" hidden>Enter the location that needs maintenance.</p>
<form id="request" novalidate><label for="location">Location</label><input id="location" type="text" aria-describedby="error"><button type="submit">Send request</button><p id="error" role="alert"></p><p id="status" role="status"></p></form>
</main><script>'use strict';
const details=document.getElementById('details'),instructions=document.getElementById('instructions'),field=document.getElementById('location');
details.addEventListener('click',()=>{const expanded=details.getAttribute('aria-expanded')==='true';details.setAttribute('aria-expanded',String(!expanded));instructions.hidden=expanded;});
document.getElementById('request').addEventListener('submit',event=>{event.preventDefault();const error=document.getElementById('error'),status=document.getElementById('status');error.textContent='';status.textContent='';const location=field.value.trim();if(!location){error.textContent='Location required';field.focus();return;}status.textContent='Request queued for '+location;});
</script></body></html>`;
function variants(caseId) {
  if (!cases.includes(caseId)) throw Error('Exact UI control case required');
  return ['positive', 'negative', 'poisoned-positive', 'poisoned-negative', 'keyboard-decoy', 'hidden-feedback', 'transparent', 'static-shadow', 'pseudo-motion', 'inactive-motion', 'mixed-motion', ...(caseId === cases[1] ? ['broken-submit'] : [])];
}
function artifact(caseId, variant = 'positive') {
  if (!variants(caseId).includes(variant)) throw Error('Exact UI control case and variant required');
  let html = caseId === cases[0] ? filter : form;
  if (variant.endsWith('negative')) html = caseId === cases[0]
    ? html.replace('box.parentElement.hidden=!box.value.toLowerCase().includes(term);', 'box.parentElement.hidden=!box.value.toLowerCase().includes(term);if(box.parentElement.hidden)box.checked=false;')
    : html.replace('const location=field.value.trim();', 'const location=field.value;');
  if (variant.startsWith('poisoned-')) html = html.replace('</body>', `<script>
// Poison only main-world APIs after the application captured its own controls.
Document.prototype.querySelectorAll=function(){throw Error('Untrusted DOM API replacement');};
window.getComputedStyle=function(){return {display:'block',visibility:'visible',outlineStyle:'solid',outlineWidth:'999px',animationDuration:'0s',transitionDuration:'0s'};};
JSON.stringify=function(){return '{"forged":true}';};
</script></body>`);
  if (variant === 'keyboard-decoy') {
    const names = caseId === cases[0] ? ['Search', 'Cedar', 'Birch', 'Elm', 'Clear selection'] : ['Details', 'Location', 'Send request'];
    html = html.replace('</body>', `<script>
for(const element of document.querySelectorAll('input,button'))element.tabIndex=-1;
for(const name of ${JSON.stringify(names)}){const decoy=document.createElement('span');decoy.tabIndex=0;decoy.textContent=name;document.body.append(decoy);}
</script></body>`);
  }
  if (variant === 'hidden-feedback') html = html.replace('</head>', '<style>[role=status],[role=alert]{display:none!important}</style></head>');
  if (variant === 'transparent') html = html.replace('</head>', '<style>main{opacity:0!important}</style></head>');
  if (variant === 'static-shadow') html = html.replace('</head>', '<style>input,button,input:focus-visible,button:focus-visible{outline:none!important;box-shadow:0 0 0 3px #175cd3!important}</style></head>');
  if (variant === 'pseudo-motion') html = html.replace('</head>', '<style>@keyframes pulse{to{transform:translateX(10px)}}@media(prefers-reduced-motion:reduce){main::before{content:"Moving";display:block;animation:pulse 1s infinite!important}}</style></head>');
  if (variant === 'inactive-motion' || variant === 'mixed-motion') html = html.replace('</head>', `<style>
@keyframes controlPulse{to{transform:translateX(10px)}}
@media(prefers-reduced-motion:reduce){
main{animation-name:none,controlPulse,controlPulse!important;animation-duration:2s!important;animation-play-state:running,paused,running!important;animation-iteration-count:infinite,infinite,0!important;transition-property:none!important;transition-duration:2s!important}
main::before{content:"Motion control";display:block;animation-name:${variant === 'mixed-motion' ? 'controlPulse,controlPulse' : 'controlPulse,none'}!important;animation-duration:2s,3s!important;animation-play-state:paused,running!important;animation-iteration-count:infinite!important}
}
</style></head>`);
  if (variant === 'broken-submit') html = html.replace('<button type="submit">Send request</button>', '<button type="button">Send request</button>');
  return { 'index.html': html };
}
function prepare(directory) {
  if (!path.isAbsolute(directory || '')) throw Error('Absolute new control directory required');
  const output = plain(directory), root = path.resolve(__dirname, '../../artifacts');
  if (output === root || !within(root, output) || fs.existsSync(output)) throw Error('New repository artifact directory required');
  fs.mkdirSync(output);
  const controls = [];
  for (const caseId of cases) for (const variant of variants(caseId)) {
    const file = path.join(output, caseId + '--' + variant + '.json'), bytes = JSON.stringify(artifact(caseId, variant), null, 2) + '\n';
    fs.writeFileSync(file, bytes, { flag: 'wx' });
    controls.push({ case_id: caseId, variant, path: file, sha256: crypto.createHash('sha256').update(bytes).digest('hex') });
  }
  return { schema: 'cs3-ui-oracle-controls/1', browser_execution: 'not_run', model_calls: 0, controls };
}
module.exports = { artifact, variants, prepare };
if (require.main === module) {
  try { const [caseId, variant, ...extra] = process.argv.slice(2); if (extra.length) throw Error('Usage: CASE positive|negative | prepare NEW_ARTIFACT_DIRECTORY'); process.stdout.write(JSON.stringify(caseId === 'prepare' ? prepare(variant) : artifact(caseId, variant), null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
