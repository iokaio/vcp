// SPDX-License-Identifier: Apache-2.0
(() => {
  const api = acquireVsCodeApi();
  const get = id => document.getElementById(id);
  const text = (value, limit = 4096) => typeof value === 'string' ? value.slice(0, limit) : 'unavailable';
  const projected = value => value ? text(value.text) + (value.truncated ? ' (truncated)' : '') : 'unavailable';
  const send = (action, id) => api.postMessage(id ? { action, id } : { action });
  const button = (label, action, id, disabled) => {
    const node = document.createElement('button'); node.type = 'button'; node.textContent = text(label, 512); node.disabled = !!disabled;
    node.addEventListener('click', () => { if (node.disabled) return; node.disabled = true; send(action, id); }); return node;
  };
  for (const action of ['previous', 'next', 'refresh']) get(`task-${action}`).addEventListener('click', () => send(action));
  window.addEventListener('message', event => {
    if (event.data?.type !== 'tasks' || !event.data.state) return;
    const state = event.data.state;
    get('task-phase').textContent = text(state.phase, 64);
    get('task-message').textContent = text(state.message);
    get('task-owner').textContent = `Controller: ${text(state.owner, 128)}`;
    get('task-count').textContent = `${Number.isSafeInteger(state.total) ? state.total : 0} tasks; page starts at ${Number.isSafeInteger(state.offset) ? state.offset + 1 : 1}`;
    if (state.stateCounts) for (const key of ['waiting_for_input', 'blocked', 'failed', 'paused']) {
      const count = state.stateCounts[key]; if (Number.isSafeInteger(count) && count > 0) get('task-count').textContent += `; ${count} ${key}`;
    }
    get('task-previous').disabled = !(state.offset > 0); get('task-next').disabled = !state.hasMore;
    get('task-rows').replaceChildren();
    for (const row of (state.rows ?? []).slice(0, 50)) {
      get('task-rows').append(button(`${row.parent ? 'Child' : 'Root'} ${text(row.task, 96)} — ${text(row.state, 64)}; ${row.pendingInputs?.length ?? 0} pending; effects ${text(row.effects, 64)}${row.dirty ? ' (refreshing)' : ''}`, 'select', row.actionId, state.phase !== 'current'));
    }
    const detail = get('task-detail'); detail.replaceChildren();
    const add = (label, value) => { const key = document.createElement('dt'); key.textContent = label; const node = document.createElement('dd'); node.textContent = text(value, 8192); detail.append(key, node); };
    const value = state.detail;
    if (value) {
      add('Task', value.task.task); add('Root', value.task.root); add('Parent', value.task.parent ?? 'root task'); add('State', value.task.state); add('Reason', value.task.reason); add('Objective', projected(value.objective)); add('Model', projected(value.model?.id)); add('Group', projected(value.model?.group)); add('Model policy', projected(value.model_policy)); add('Role', projected(value.role)); add('Steering revision', value.task.steering_revision); add('Effects', value.task.effects);
      add('Commentary', value.commentary === 'observed' ? 'retained evidence below' : 'No retained commentary is available.');
      add('History', value.complete ? 'Complete page' : value.next_cursor ? 'More history is available' : 'Some content is unavailable or requires an evidence read');
      if (state.usage) add('Root ledger cost (USD micros)', `known ${state.usage.settled_micros}; reserved ${state.usage.reserved_micros}; uncertain ${state.usage.unresolved_micros}; cap ${state.usage.cap_micros}`);
      else add('Cost', 'unavailable');
    }
    get('task-questions').replaceChildren();
    for (const question of (value?.questions ?? []).slice(0, 128)) {
      const node = document.createElement('p'); node.textContent = `Approval ${text(question.input.id, 96)} for effect ${text(question.effect, 96)}: ${projected(question.summary)}; operation ${text(question.input.operation_digest, 64)}; expires ${text(question.expires_at, 32)}; ${question.actionable ? 'pending' : 'not actionable'}`; get('task-questions').append(node);
    }
    get('task-actions').replaceChildren();
    for (const action of (state.actions ?? []).slice(0, 260)) get('task-actions').append(button(action.input ? `${text(action.label, 128)} ${text(action.input, 96)}` : action.label, 'task', action.id, !!action.disabledReason || state.phase !== 'current'));
    get('task-history').replaceChildren();
    for (const row of (value?.rows ?? []).slice(0, 128)) {
      const node = document.createElement('p'); node.textContent = row.kind === 'commentary' ? `Task ${text(row.task, 96)}: ${projected(row.text)}` : row.kind === 'effect' ? `Tool ${text(row.id, 96)} (${text(row.state, 64)}): ${projected(row.reason)}` : `Evidence ${text(row.id, 96)} (${text(row.schema, 128)})`; get('task-history').append(node);
    }
    get('task-evidence').replaceChildren();
    for (const link of (state.evidence ?? []).slice(0, 128)) get('task-evidence').append(button(link.label, 'evidence', link.id, state.phase !== 'current'));
    if (state.historyId) get('task-evidence').append(button('More history', 'history', state.historyId, state.phase !== 'current'));
    get('task-commands').replaceChildren();
    get('task-artifact').textContent = typeof state.artifact === 'string' ? state.artifact.slice(0, 65536) : '';
    for (const command of (state.commands ?? []).slice(-128)) { const node = document.createElement('li'); node.textContent = `${text(command.operation, 128)}: ${text(command.phase, 64)} — command ${text(command.commandId, 96)}`; get('task-commands').append(node); }
  });
  send('ready');
})();
