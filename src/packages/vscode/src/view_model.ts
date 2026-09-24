// SPDX-License-Identifier: Apache-2.0
export type ViewAction = 'connect' | 'control' | 'attach' | 'reconcile' | 'grant' | 'revoke' | 'disconnect' | 'refresh';
export type ViewMessage = ViewAction | 'ready';
/** Closed messages never carry paths, IDs, commands, credentials or HTML. */
export function parseViewMessage(value: unknown): ViewMessage | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.getPrototypeOf(value) !== Object.prototype) return undefined;
  if (Object.getOwnPropertyNames(value).length !== 1 || Object.getOwnPropertySymbols(value).length || !Object.hasOwn(value, 'action')) return undefined;
  const descriptor = Object.getOwnPropertyDescriptor(value, 'action');
  if (!descriptor?.enumerable || !('value' in descriptor)) return undefined;
  const action: unknown = descriptor.value;
  return action === 'connect' || action === 'control' || action === 'attach' || action === 'reconcile' || action === 'grant' || action === 'revoke' || action === 'disconnect' || action === 'refresh' || action === 'ready' ? action : undefined;
}

function escape(value: string): string {
  return value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
}

/** Dynamic engine values arrive separately and are rendered with textContent. */
export function connectionHtml(cspSource: string, styleUri: string, scriptUri: string, nonce: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${escape(cspSource)}; script-src 'nonce-${escape(nonce)}';"><link rel="stylesheet" href="${escape(styleUri)}"><title>VCP Workspace</title></head><body>
  <header><h1>Workspace</h1><span id="phase" class="badge">disconnected</span></header>
  <p id="message" role="status" aria-live="polite">Choose an initialized local workspace to inspect.</p>
  <dl id="details"></dl><h2>Connection limits</h2><ul id="limitations"></ul>
  <div class="actions"><button id="connect" type="button">Connect…</button><button id="refresh" type="button" disabled>Refresh</button><button id="disconnect" type="button" disabled>Disconnect</button></div>
  <div class="actions"><button id="control" type="button">Connect for trust changes…</button><button id="attach" type="button">Observe existing engine…</button><button id="reconcile" type="button">Reconcile moved root…</button></div>
  <div class="actions"><button id="grant" type="button" disabled>Trust engine workspace</button><button id="revoke" type="button" disabled>Revoke engine trust</button></div>
  <p class="note">Local Windows. Trust changes pause work and release control. Reload restores observation only. Tasks require deliberate resume.</p>
  <script nonce="${escape(nonce)}" src="${escape(scriptUri)}"></script></body></html>`;
}
