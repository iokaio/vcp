// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
import { parseTaskPanelMessage, taskPanelHtml, type TaskPanelMessage, type TaskPanelState } from './task_view_model.js';

export class TaskPanel implements vscode.WebviewViewProvider, vscode.Disposable {
  #view: vscode.WebviewView | undefined;
  #listeners: vscode.Disposable[] = [];
  #state: TaskPanelState = { phase: 'disconnected', message: 'Connect to inspect task state.', owner: 'unknown', total: 0, offset: 0, hasMore: false, rows: [], actions: [], commands: [] };
  constructor(private readonly uri: vscode.Uri, private readonly action: (message: TaskPanelMessage) => Promise<void>) {}
  resolveWebviewView(view: vscode.WebviewView): void {
    this.dispose(); this.#view = view;
    const media = vscode.Uri.joinPath(this.uri, 'media');
    const nonce = randomBytes(24).toString('hex');
    view.webview.options = { enableScripts: true, localResourceRoots: [media] };
    view.webview.html = taskPanelHtml(view.webview.cspSource,
      view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'connection.css')).toString(),
      view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'tasks.js')).toString(), nonce);
    this.#listeners.push(view.webview.onDidReceiveMessage((raw: unknown) => {
      const message = parseTaskPanelMessage(raw);
      if (message?.action === 'ready') this.publish(this.#state);
      else if (message) void this.action(message).catch(() => {});
    }), view.onDidChangeVisibility(() => { if (view.visible) this.publish(this.#state); }), view.onDidDispose(() => this.dispose()));
    this.publish(this.#state);
  }
  publish(state: TaskPanelState): void {
    // Renderer limits are a second boundary, not a substitute for transport bounds.
    this.#state = Buffer.byteLength(JSON.stringify(state), 'utf8') <= 512 * 1024 ? state
      : { phase: 'partial', message: 'Task presentation exceeded its display bound. Select a smaller page or refresh.', owner: 'unknown', total: 0, offset: 0, hasMore: false, rows: [], actions: [], commands: [] };
    if (this.#view) void this.#view.webview.postMessage({ type: 'tasks', state: this.#state });
  }
  dispose(): void { this.#view = undefined; for (const listener of this.#listeners) listener.dispose(); this.#listeners = []; }
}
