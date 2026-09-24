// SPDX-License-Identifier: Apache-2.0
(() => {
  'use strict';
  const vscode = acquireVsCodeApi();
  const element = id => document.getElementById(id);
  for (const action of ['connect', 'control', 'attach', 'reconcile', 'grant', 'revoke', 'disconnect', 'refresh']) element(action).addEventListener('click', () => vscode.postMessage({ action }));
  const text = value => typeof value === 'string' ? value.slice(0, 32768) : typeof value === 'number' && Number.isSafeInteger(value) ? String(value) : 'unavailable';
  window.addEventListener('message', event => {
    const message = event.data;
    if (!message || message.type !== 'connection' || !message.status || typeof message.status !== 'object') return;
    const status = message.status;
    element('phase').textContent = text(status.phase);
    element('message').textContent = text(status.message);
    const rows = [
      ['Engine', status.engineBuild], ['Executable', status.engineExecutable], ['Protocol', status.protocolVersion], ['Role', status.role === 'observer' ? 'observer (read-only)' : status.role === 'controller' ? 'controller' : undefined],
      ['Host', status.host ? `${text(status.host.platform)} · ${text(status.host.id)}` : undefined],
      ['Folder URI', status.workspaceUri], ['Canonical root', status.workspaceRoot],
      ['Root ID', status.rootId], ['Binding revision', status.bindingRevision],
      ['Workspace', status.scope?.workspace], ['Session', status.scope?.session],
      ['Editor trust', status.editorTrusted ? 'trusted' : 'restricted'], ['Engine trust', status.engineTrust],
      ['Tasks in snapshot', status.taskCount], ['Pending decisions', status.pendingInputs], ['Snapshot watermark', status.watermark],
    ];
    element('details').replaceChildren();
    for (const [label, value] of rows) {
      const term = document.createElement('dt'); term.textContent = label;
      const detail = document.createElement('dd'); detail.textContent = text(value);
      element('details').append(term, detail);
    }
    element('limitations').replaceChildren();
    for (const limitation of (Array.isArray(status.limitations) ? status.limitations : []).slice(0, 16)) {
      const row = document.createElement('li'); row.textContent = text(limitation); element('limitations').append(row);
    }
    element('connect').disabled = status.phase === 'connecting';
    element('refresh').disabled = status.phase !== 'connected';
    element('disconnect').disabled = status.phase === 'disconnected';
    for (const action of ['control', 'attach', 'reconcile']) element(action).disabled = status.phase === 'connecting';
    element('grant').disabled = status.phase !== 'connected' || status.role !== 'controller' || !status.editorTrusted;
    element('revoke').disabled = status.phase !== 'connected' || status.role !== 'controller';
  });
  vscode.postMessage({ action: 'ready' });
})();
