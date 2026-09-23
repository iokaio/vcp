// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
import type { ConnectionStatus } from './engine_connection.js';
import { connectionHtml, parseViewMessage, type ViewAction } from './view_model.js';

export class ConnectionView implements vscode.WebviewViewProvider, vscode.Disposable {
  #view: vscode.WebviewView | undefined;
  #status: ConnectionStatus;
  #messages: vscode.Disposable | undefined;
  #viewListeners: vscode.Disposable[] = [];
  constructor(private readonly extensionUri: vscode.Uri, initial: ConnectionStatus, private readonly action: (action: ViewAction) => Promise<unknown>) { this.#status = initial; }
  resolveWebviewView(view: vscode.WebviewView): void {
    this.#messages?.dispose();
    for (const listener of this.#viewListeners) listener.dispose();
    this.#viewListeners = [];
    this.#view = view;
    const media = vscode.Uri.joinPath(this.extensionUri, 'media');
    view.webview.options = { enableScripts: true, localResourceRoots: [media] };
    const nonce = randomBytes(24).toString('hex');
    view.webview.html = connectionHtml(view.webview.cspSource, view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'connection.css')).toString(), view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'connection.js')).toString(), nonce);
    this.#messages = view.webview.onDidReceiveMessage((message: unknown) => {
      const action = parseViewMessage(message);
      if (action === 'ready') this.publish(this.#status);
      else if (action) void this.action(action).catch(() => {});
    });
    this.#viewListeners.push(view.onDidChangeVisibility(() => { if (view.visible) this.publish(this.#status); }));
    this.#viewListeners.push(view.onDidDispose(() => { if (this.#view === view) { this.#messages?.dispose(); this.#messages = undefined; this.#view = undefined; } }));
    this.publish(this.#status);
  }
  publish(status: ConnectionStatus): void {
    this.#status = status;
    if (this.#view) void this.#view.webview.postMessage({ type: 'connection', status });
  }
  dispose(): void {
    this.#messages?.dispose(); this.#messages = undefined; this.#view = undefined;
    for (const listener of this.#viewListeners) listener.dispose();
    this.#viewListeners = [];
  }
}
