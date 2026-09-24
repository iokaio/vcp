// SPDX-License-Identifier: Apache-2.0
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { isAbsolute, win32 } from 'node:path';
import type { InitializeParams, Scope, Id } from '@vcp/protocol';
import { Client, type ClientTransport, type Role } from './client.js';
import { LineDecoder, encodeFrame } from './codec.js';
import { SdkError } from './errors.js';
import { validateWire } from './validation.js';

declare const opaque: unique symbol;
export type LocalAttachment = { readonly [opaque]: true };
type Pin = { pid: number; created: string; principal: { sid: number[]; session: number }; image: string; file: { volume: number; index: string } };
export type ObserverReconnectReference = { endpoint: string; server: Pin; scope: Scope };
export type ReconnectObserverOptions = { executable: string; reference: ObserverReconnectReference; initialize?: InitializeParams };
type Attachment = { endpoint: string; server: Pin; ticket: string };
type Ready = { schema: 'vcp-local-ready/1'; server: Pin; scope: Scope; role: Role; attachment?: Attachment; observer_attachment?: Attachment; observer_reconnect?: ObserverReconnectReference };
const handles = new WeakMap<LocalAttachment, { attachment: Attachment; role: Role; scope: Scope }>();
export type LaunchOptions = {
  executable: string; workspace: string; data?: string | null; role: Role;
  transport?: 'stdio' | 'windows_pipe'; rootTask?: Id;
  execution?: { profile: string; providerCredential: string; credentials?: Record<string, string> };
  initialize?: InitializeParams;
};
export type AttachOptions = { executable: string; attachment: LocalAttachment; initialize?: InitializeParams };
function object(value: unknown, required: string[], optional: string[] = []): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new SdkError('MALFORMED_PEER', 'invalid bootstrap object');
  const row = value as Record<string, unknown>;
  if (required.some(key => !Object.hasOwn(row, key)) || Object.keys(row).some(key => !required.includes(key) && !optional.includes(key))) throw new SdkError('MALFORMED_PEER', 'invalid bootstrap fields');
  return row;
}
function decimal(value: unknown): void {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > 18446744073709551615n) throw new SdkError('MALFORMED_PEER', 'invalid native counter');
}
function u32(value: unknown): void {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > 0xffffffff) throw new SdkError('MALFORMED_PEER', 'invalid native integer');
}
function pin(value: unknown): Pin {
  const row = object(value, ['pid', 'created', 'principal', 'image', 'file']); u32(row.pid); decimal(row.created);
  if (row.pid === 0 || typeof row.image !== 'string' || row.image.length > 32768 || !win32.isAbsolute(row.image) || row.image.includes('\0')) throw new SdkError('MALFORMED_PEER', 'invalid server image');
  const principal = object(row.principal, ['sid', 'session']); u32(principal.session);
  if (!Array.isArray(principal.sid) || principal.sid.length < 8 || principal.sid.length > 68 || principal.sid.some(b => !Number.isInteger(b) || b < 0 || b > 255)) throw new SdkError('MALFORMED_PEER', 'invalid server principal');
  const file = object(row.file, ['volume', 'index']); u32(file.volume); decimal(file.index);
  return value as Pin;
}
function attachment(value: unknown, server: Pin): Attachment {
  const row = object(value, ['endpoint', 'server', 'ticket']);
  pin(row.server);
  if (typeof row.endpoint !== 'string' || !/^\\\\\.\\pipe\\vcp-local-[a-fA-F0-9]{64}$/.test(row.endpoint) || typeof row.ticket !== 'string' || !/^[a-fA-F0-9]{64}$/.test(row.ticket) || JSON.stringify(row.server) !== JSON.stringify(server)) throw new SdkError('MALFORMED_PEER', 'invalid attachment');
  return value as Attachment;
}
/** Native authentication remains authoritative; this validates only the private bootstrap shape. */
export function validateReady(value: unknown, role: Role, expectedScope?: Scope): Ready {
  const row = object(value, ['schema', 'server', 'scope', 'role'], ['attachment', 'observer_attachment', 'observer_reconnect']);
  if (row.schema !== 'vcp-local-ready/1' || row.role !== role) throw new SdkError('MALFORMED_PEER', 'bootstrap version or role mismatch');
  const server = pin(row.server); validateWire('Scope', row.scope);
  const scope = row.scope as Scope;
  if (expectedScope && (scope.workspace !== expectedScope.workspace || scope.session !== expectedScope.session)) throw new SdkError('MALFORMED_PEER', 'attachment scope changed');
  if (row.attachment !== undefined) attachment(row.attachment, server);
  if (row.observer_attachment !== undefined) {
    if (role !== 'controller') throw new SdkError('MALFORMED_PEER', 'observer received elevated attachment fields');
    attachment(row.observer_attachment, server);
  }
  if (row.observer_reconnect !== undefined) {
    const reference = validateObserverReconnectReference(row.observer_reconnect);
    if (JSON.stringify(reference.server) !== JSON.stringify(server) || reference.scope.workspace !== scope.workspace || reference.scope.session !== scope.session) throw new SdkError('MALFORMED_PEER', 'observer reference identity mismatch');
  }
  return value as Ready;
}
/** A discovery reference contains no authority; the native bridge authenticates both peers. */
export function validateObserverReconnectReference(value: unknown): ObserverReconnectReference {
  const row = object(value, ['endpoint', 'server', 'scope']);
  pin(row.server); validateWire('Scope', row.scope);
  if (typeof row.endpoint !== 'string' || !/^\\\\\.\\pipe\\vcp-local-[a-fA-F0-9]{64}$/.test(row.endpoint)) throw new SdkError('MALFORMED_PEER', 'invalid observer endpoint');
  return structuredClone(value) as ObserverReconnectReference;
}
function handle(value: Attachment | undefined, role: Role, scope: Scope): LocalAttachment | undefined {
  if (!value) return undefined;
  const result = Object.freeze(Object.create(null)) as LocalAttachment;
  handles.set(result, { attachment: structuredClone(value), role, scope: structuredClone(scope) }); return result;
}
function absolute(value: string, label: string): void {
  if (typeof value !== 'string' || value.includes('\0') || !(isAbsolute(value) || win32.isAbsolute(value))) throw new SdkError('INVALID_ARGUMENT', `${label} must be an explicit absolute path`);
}
class Bridge implements ClientTransport {
  #child: ChildProcessWithoutNullStreams;
  #decoder = new LineDecoder(16 * 1024);
  #frame: ((value: unknown) => void) | undefined;
  #failure: (() => void) | undefined;
  #closed?: Promise<void>;
  #exited: Promise<void>;
  #dead = false;
  #ready = false;
  readonly ready: Promise<unknown>;
  constructor(executable: string, bootstrap: unknown) {
    const input = encodeFrame(bootstrap, 16 * 1024);
    this.#child = spawn(executable, ['local-bridge'], { shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    this.#exited = new Promise(resolve => this.#child.once('close', () => { this.#dead = true; resolve(); }));
    this.ready = new Promise((resolve, reject) => {
      const timer = setTimeout(() => { reject(new SdkError('TIMEOUT', 'local bootstrap deadline exceeded')); void this.close().catch(() => {}); }, 10_000);
      const fail = () => {
        clearTimeout(timer);
        if (!this.#ready) reject(new SdkError('TRANSPORT_INTERRUPTED', 'local bootstrap failed'));
        this.#failure?.();
      };
      this.#child.once('error', fail);
      this.#child.stdout.on('error', fail); this.#child.stdin.on('error', fail);
      // Drain diagnostics without retaining or exposing secret-bearing messages.
      this.#child.stderr.on('data', () => {}); this.#child.stderr.on('error', fail);
      this.#child.stdout.on('data', (data: Buffer) => {
        try {
          for (const frame of this.#decoder.push(data)) {
            if (!this.#ready) { this.#ready = true; clearTimeout(timer); this.#decoder.setMaximum(1024 * 1024); resolve(frame); }
            else if (this.#frame) this.#frame(frame);
            else throw new Error('unsolicited frame before initialize');
          }
        } catch { fail(); void this.close().catch(() => {}); }
      });
      this.#child.stdout.once('end', () => { try { this.#decoder.finish(); } catch { /* EOF is already a transport failure */ } fail(); });
      this.#child.once('close', fail);
      this.#child.stdin.write(input, error => { if (error) fail(); });
    });
  }
  listen(frame: (value: unknown) => void, failed: () => void): () => void {
    this.#frame = frame; this.#failure = failed;
    if (this.#dead) queueMicrotask(failed);
    return () => { this.#frame = undefined; this.#failure = undefined; };
  }
  setMaximum(bytes: number): void { this.#decoder.setMaximum(bytes); }
  send(frame: Buffer): Promise<void> {
    if (this.#closed || this.#dead) return Promise.reject(new SdkError('TRANSPORT_INTERRUPTED', 'bridge closed'));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { reject(new SdkError('TIMEOUT', 'bridge write deadline')); void this.close().catch(() => {}); }, 5000);
      this.#child.stdin.write(frame, error => { clearTimeout(timer); if (error) reject(new SdkError('TRANSPORT_INTERRUPTED', 'bridge write failed')); else resolve(); });
    });
  }
  close(): Promise<void> {
    if (this.#closed) return this.#closed;
    this.#closed = (async () => {
      this.#child.stdin.end();
      let timer: NodeJS.Timeout | undefined;
      await Promise.race([this.#exited, new Promise<void>(resolve => { timer = setTimeout(resolve, 5000); })]);
      clearTimeout(timer);
      if (!this.#dead) this.#child.kill(); // Only the spawned bridge, never a server pin PID.
      await Promise.race([this.#exited, new Promise<void>(resolve => { timer = setTimeout(resolve, 2000); })]);
      clearTimeout(timer);
      if (!this.#dead) throw new SdkError('TIMEOUT', 'owned bridge did not exit');
    })();
    return this.#closed;
  }
}
async function open(executable: string, bootstrap: unknown, role: Role, initialize?: InitializeParams, scope?: Scope): Promise<Client> {
  absolute(executable, 'executable');
  const bridge = new Bridge(executable, bootstrap);
  try {
    const ready = validateReady(await bridge.ready, role, scope);
    return await Client.connect(bridge, ready.scope, role, initialize, handle(ready.attachment, role, ready.scope), handle(ready.observer_attachment, 'observer', ready.scope), ready.observer_reconnect);
  } catch (error) { await bridge.close(); throw error; }
}
export function launchLocal(options: LaunchOptions): Promise<Client> {
  absolute(options.workspace, 'workspace'); if (options.data !== undefined && options.data !== null) absolute(options.data, 'data');
  if (options.role !== 'observer' && options.role !== 'controller') throw new SdkError('INVALID_ARGUMENT', 'invalid local role');
  if (options.execution) absolute(options.execution.profile, 'execution profile');
  return open(options.executable, {
    schema: 'vcp-local-bootstrap/1', observer_reconnect: true, workspace: options.workspace, data: options.data ?? null, role: options.role,
    ...(options.transport === undefined ? {} : { transport: options.transport }), ...(options.rootTask === undefined ? {} : { root_task: options.rootTask }),
    ...(options.execution === undefined ? {} : { execution: { profile: options.execution.profile, provider_credential: options.execution.providerCredential, ...(options.execution.credentials === undefined ? {} : { credentials: options.execution.credentials }) } }),
  }, options.role, options.initialize);
}
export function attachLocal(options: AttachOptions): Promise<Client> {
  const stored = handles.get(options.attachment);
  if (!stored) throw new SdkError('INVALID_ARGUMENT', 'attachment must originate from an authenticated local client');
  return open(options.executable, { schema: 'vcp-local-attach/1', observer_reconnect: true, attachment: stored.attachment, role: stored.role }, stored.role, options.initialize, stored.scope);
}

export function reconnectObserverLocal(options: ReconnectObserverOptions): Promise<Client> {
  const reference = validateObserverReconnectReference(options.reference);
  return open(options.executable, { schema: 'vcp-local-observer-reconnect/1', observer_reconnect: reference }, 'observer', options.initialize, reference.scope);
}
