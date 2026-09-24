// SPDX-License-Identifier: Apache-2.0
(() => {
  const api = acquireVsCodeApi();
  const tabs = ['history', 'memory', 'evidence', 'cost', 'policy', 'routing', 'optimizer', 'pruning', 'publisher'];
  const phases = ['disconnected', 'loading', 'current', 'stale', 'unavailable', 'unsupported', 'partial'];
  const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
  const get = id => document.getElementById(`inspector-${id}`);
  const send = value => api.postMessage(value);
  const node = (tag, text) => { const element = document.createElement(tag); if (text !== undefined) element.textContent = text; return element; };
  const tabButtons = new Map();
  for (const tab of tabs) {
    const button = node('button', tab[0].toUpperCase() + tab.slice(1)); button.type = 'button'; button.dataset.tab = tab;
    button.addEventListener('click', () => { if (button.disabled) return; button.disabled = true; send({ action: 'tab', tab }); });
    tabButtons.set(tab, button); get('tabs').append(button);
  }
  get('refresh').addEventListener('click', () => { const button = get('refresh'); if (button.disabled) return; button.disabled = true; send({ action: 'refresh' }); });
  const valid = state => {
    if (!state || !tabs.includes(state.tab) || !phases.includes(state.phase) || !['message', 'scopeLabel', 'taskLabel'].every(key => typeof state[key] === 'string')) return false;
    let rows = 0, fields = 0, actions = 0;
    const checkFields = values => Array.isArray(values) && (fields += values.length) <= 4096 && values.every(value => value && typeof value.label === 'string' && typeof value.value === 'string');
    const checkActions = values => Array.isArray(values) && (actions += values.length) <= 512 && values.every(value => value && typeof value.id === 'string' && uuid.test(value.id) && typeof value.label === 'string' && (value.disabledReason === undefined || typeof value.disabledReason === 'string'));
    return Array.isArray(state.sections) && state.sections.length <= 64 && checkActions(state.actions) && state.sections.every(section => section && typeof section.title === 'string' && checkFields(section.fields) && (section.text === undefined || typeof section.text === 'string') && (section.rows === undefined || (Array.isArray(section.rows) && (rows += section.rows.length) <= 512 && section.rows.every(row => row && typeof row.title === 'string' && checkFields(row.fields) && (row.actions === undefined || checkActions(row.actions)))))) && Array.isArray(state.commands) && state.commands.length <= 128 && state.commands.every(command => command && ['commandId', 'method', 'phase'].every(key => typeof command[key] === 'string'));
  };
  const fields = values => { const list = node('dl'); for (const value of values) list.append(node('dt', value.label), node('dd', value.value)); return list; };
  const actions = (target, values, current) => {
    for (const value of values) {
      const button = node('button', value.label); button.type = 'button'; button.dataset.action = value.id;
      button.disabled = !current || value.disabledReason !== undefined;
      if (value.disabledReason !== undefined) button.title = value.disabledReason;
      button.addEventListener('click', () => { if (button.disabled) return; button.disabled = true; send({ action: 'invoke', id: value.id }); });
      target.append(button);
      if (value.disabledReason !== undefined) target.append(node('p', value.disabledReason));
    }
  };
  window.addEventListener('message', event => {
    if (event.data?.type !== 'inspectors') return;
    let state = event.data.state;
    try {
      if (new TextEncoder().encode(JSON.stringify(event.data)).byteLength > 256 * 1024 || !valid(state)) throw Error('invalid display');
    } catch {
      state = { phase: 'partial', tab: tabs.includes(state?.tab) ? state.tab : 'history', message: 'Inspector presentation is unavailable within its display bounds. Refresh a narrower selection; no review actions are available.', scopeLabel: '', taskLabel: '', sections: [], actions: [], commands: [] };
    }
    get('phase').textContent = state.phase; get('message').textContent = state.message;
    get('scope').textContent = state.scopeLabel; get('task').textContent = state.taskLabel;
    for (const [tab, button] of tabButtons) { button.disabled = tab === state.tab; button.setAttribute('aria-pressed', String(tab === state.tab)); }
    get('refresh').disabled = state.phase === 'loading';
    for (const group of ['sections', 'actions', 'commands']) get(group).replaceChildren();
    const current = state.phase === 'current';
    actions(get('actions'), state.actions, current);
    for (const section of state.sections) {
      const wrapper = node('section'); wrapper.append(node('h2', section.title), fields(section.fields));
      if (section.text !== undefined) wrapper.append(node('pre', section.text));
      for (const row of section.rows ?? []) {
        const item = node('article'); item.append(node('h3', row.title), fields(row.fields)); actions(item, row.actions ?? [], current); wrapper.append(item);
      }
      get('sections').append(wrapper);
    }
    for (const command of state.commands) get('commands').append(node('li', `${command.method}: ${command.phase} — command ${command.commandId}`));
  });
  send({ action: 'ready' });
})();
