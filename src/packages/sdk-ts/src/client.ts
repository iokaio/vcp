// SPDX-License-Identifier: Apache-2.0
import type { CommandRead, Id, InitializeParams, InitializeResult, Scope, SessionSnapshotRead } from '@vcp/protocol';
import { encodeFrame } from './codec.js';
import { SdkError, RpcFailure } from './errors.js';
import { RESULT_KINDS, REQUIRED_PROFILES, type Method, type Params, type Reply } from './method-results.js';
import { validateWire } from './validation.js';
import { EventStream, snapshotPages, type StreamOptions } from './subscriptions.js';
import type { EventsSubscribe } from '@vcp/protocol';
import type { LocalAttachment } from './local.js';

export type Role = 'observer' | 'controller';
export type CallOptions = { signal?: AbortSignal; timeoutMs?: number };
export class OperationError extends SdkError {
  constructor(code: string, message: string, readonly commandId?: Id) { super(code, message); }
}
/** Internal transport seam. The public entry points use the native bridge. */
export interface ClientTransport {
  send(frame: Buffer): Promise<void>;
  listen(frame: (value: unknown) => void, failed: () => void): () => void;
  setMaximum(bytes: number): void;
  close(): Promise<void>;
}
type Pending = {
  id: string; method: Method | 'initialize'; bytes: Buffer; timeoutMs: number;
  commandId: Id | undefined; resolve: (value: unknown) => void; reject: (error: unknown) => void;
  signal: AbortSignal | undefined; abort?: () => void; timer?: NodeJS.Timeout; settled: boolean;
  snapshotScope?: Scope;
};
const LOCAL_FRAME = 1024 * 1024;
const MAX_QUEUE = 32;
const MAX_QUEUE_BYTES = 2 * 1024 * 1024;
const DEFAULT_TIMEOUT = 30_000;
function timeout(value: number | undefined): number {
  const n = value ?? DEFAULT_TIMEOUT;
  if (!Number.isSafeInteger(n) || n < 1 || n > 120_000) throw new SdkError('INVALID_ARGUMENT', 'timeout must be 1..120000 milliseconds');
  return n;
}
function command(params: unknown): Id | undefined {
  if (!params || typeof params !== 'object') return undefined;
  const value = params as { command_id?: unknown; mutation?: { command_id?: unknown } };
  const candidate = value.mutation?.command_id ?? value.command_id;
  return typeof candidate === 'string' ? candidate : undefined;
}
function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
export class Client {
  readonly scope: Readonly<Scope>;
  readonly role: Role;
  #initialized!: InitializeResult;
  #transport: ClientTransport;
  #remove: () => void;
  #queue: Pending[] = [];
  #queuedBytes = 0;
  #active: Pending | undefined;
  #sequence = 0;
  #maximum = LOCAL_FRAME;
  #closed = false;
  #stopping = false;
  #disposing?: Promise<void>;
  #streams = new Set<EventStream>();
  #attachment: LocalAttachment | undefined;
  #observer: LocalAttachment | undefined;
  private constructor(transport: ClientTransport, scope: Scope, role: Role, attachment?: LocalAttachment, observer?: LocalAttachment) {
    validateWire('Scope', scope);
    this.scope = freeze(structuredClone(scope)); this.role = role;
    this.#transport = transport; this.#attachment = attachment; this.#observer = observer;
    this.#remove = transport.listen(value => this.#receive(value), () => this.#fail('TRANSPORT_INTERRUPTED'));
  }
  /** Internal factory; launchLocal and attachLocal establish authenticated bootstrap first. */
  static async connect(transport: ClientTransport, scope: Scope, role: Role, params?: InitializeParams, attachment?: LocalAttachment, observer?: LocalAttachment): Promise<Client> {
    const client = new Client(transport, scope, role, attachment, observer);
    try {
      const request = params ?? {
        protocol_version: '1.0', client: { name: '@vcp/sdk', version: '0.1.0' },
        capabilities: [...new Set([...Object.keys(RESULT_KINDS), ...Object.values(REQUIRED_PROFILES).flat(), 'jsonrpc/2.0', 'durable-command/1', 'approval/source-revisions/1', 'memory/inspection-state/1'])], required_capabilities: [],
      };
      validateWire('InitializeParams', request);
      const result = await client.#enqueue('initialize', request, { timeoutMs: 10_000 }) as InitializeResult;
      validateWire('InitializeResult', result);
      if (result.protocol_version !== '1.0' || result.event_schema_version !== '1.0' || result.schema_version !== '1.0') throw new SdkError('UNSUPPORTED_VERSION', 'unsupported server protocol version');
      const requested = new Set([...request.capabilities, ...(request.required_capabilities ?? [])]);
      if (result.capabilities.some(c => !requested.has(c)) || (request.required_capabilities ?? []).some(c => !result.capabilities.includes(c))) throw new SdkError('CAPABILITY_UNAVAILABLE', 'server capability negotiation mismatch');
      client.#maximum = Math.min(LOCAL_FRAME, result.limits.maximum_frame_bytes);
      client.#transport.setMaximum(client.#maximum);
      client.#initialized = freeze(structuredClone(result));
      return client;
    } catch (error) { await client.dispose(); throw error; }
  }
  get initialized(): Readonly<InitializeResult> { return this.#initialized; }
  attachment(): LocalAttachment {
    if (!this.#attachment) throw new SdkError('CAPABILITY_UNAVAILABLE', 'attachment unavailable for this transport');
    return this.#attachment;
  }
  observerAttachment(): LocalAttachment {
    if (!this.#observer) throw new SdkError('CAPABILITY_UNAVAILABLE', 'observer attachment unavailable');
    return this.#observer;
  }
  async call<M extends Method>(method: M, params: Params<M>, options: CallOptions = {}): Promise<Reply<M>> {
    if (this.#closed || (this.#stopping && method !== 'events/unsubscribe')) throw new SdkError('DISPOSED', 'client is closed');
    validateWire('Call', { method, params });
    if (!this.#initialized.methods.includes(method) || !this.#initialized.capabilities.includes(method) || (REQUIRED_PROFILES[method] ?? []).some(profile => !this.#initialized.capabilities.includes(profile))) throw new SdkError('CAPABILITY_UNAVAILABLE', 'method or required profile was not negotiated');
    return await this.#enqueue(method, params, options) as Reply<M>;
  }
  reconcile(params: CommandRead, options?: CallOptions): Promise<Reply<'command/read'>> { return this.call('command/read', params, options); }
  snapshot(params: SessionSnapshotRead, options?: CallOptions): Promise<Reply<'session/snapshot'>> { return this.call('session/snapshot', params, options); }
  snapshots(params: SessionSnapshotRead, options?: CallOptions): AsyncIterable<Reply<'session/snapshot'>> { return snapshotPages(this, params, options); }
  events(params: EventsSubscribe, options: StreamOptions = {}): EventStream {
    if (this.#closed || this.#stopping) throw new SdkError('DISPOSED', 'client is closed');
    validateWire('Call', {method: 'events/subscribe', params});
    if (this.#streams.size >= this.#initialized.limits.maximum_subscriptions) throw new SdkError('RESOURCE_LIMIT', 'subscription limit reached');
    const stream = new EventStream(this, params, options, () => this.#streams.delete(stream));
    this.#streams.add(stream); return stream;
  }
  #enqueue(method: Method | 'initialize', params: unknown, options: CallOptions): Promise<unknown> {
    if (this.#closed) return Promise.reject(new SdkError('DISPOSED', 'client is closed'));
    const duration = timeout(options.timeoutMs);
    if (options.signal?.aborted) return Promise.reject(new OperationError('ABORTED', 'local await aborted', command(params)));
    const id = `sdk-${++this.#sequence}`;
    // Encoding freezes caller intent; later caller object edits cannot alter a queued mutation.
    const bytes = encodeFrame({ jsonrpc: '2.0', id, method, params }, Math.min(this.#maximum, 256 * 1024));
    if (this.#queue.length >= MAX_QUEUE || this.#queuedBytes + bytes.length > MAX_QUEUE_BYTES) return Promise.reject(new OperationError('RESOURCE_LIMIT', 'request queue limit reached', command(params)));
    return new Promise((resolve, reject) => {
      const pending: Pending = { id, method, bytes, timeoutMs: duration, commandId: command(params), resolve, reject, signal: options.signal, settled: false };
      if (method === 'session/snapshot') pending.snapshotScope = structuredClone((params as SessionSnapshotRead).scope);
      pending.abort = () => {
        this.#settle(pending, new OperationError('ABORTED', 'local await aborted; execution outcome is unchanged', pending.commandId));
        const index = this.#queue.indexOf(pending);
        if (index >= 0) { this.#queue.splice(index, 1); this.#queuedBytes -= pending.bytes.length; clearTimeout(pending.timer); }
        // An in-flight slot remains occupied until its response or transport deadline.
      };
      pending.signal?.addEventListener('abort', pending.abort, { once: true });
      pending.timer = setTimeout(() => {
        if (this.#active === pending) this.#fail('TIMEOUT');
        else {
          const index = this.#queue.indexOf(pending);
          if (index >= 0) { this.#queue.splice(index, 1); this.#queuedBytes -= pending.bytes.length; }
          this.#settle(pending, new OperationError('TIMEOUT', 'queued request deadline exceeded before send', pending.commandId));
        }
      }, duration);
      this.#queue.push(pending); this.#queuedBytes += bytes.length; this.#pump();
    });
  }
  #pump(): void {
    if (this.#closed || this.#active) return;
    const pending = this.#queue.shift(); if (!pending) return;
    this.#queuedBytes -= pending.bytes.length; this.#active = pending;
    void this.#transport.send(pending.bytes).catch(() => this.#fail('TRANSPORT_INTERRUPTED'));
  }
  #settle(pending: Pending, error?: unknown, value?: unknown): void {
    if (pending.settled) return;
    pending.settled = true;
    if (pending.abort) pending.signal?.removeEventListener('abort', pending.abort);
    if (error !== undefined) pending.reject(error); else pending.resolve(value);
  }
  #receive(value: unknown): void {
    if (this.#closed) return;
    try {
      validateWire('WireEnvelope', value);
      const envelope = value as { id?: unknown; result?: unknown; error?: unknown; method?: unknown };
      const pending = this.#active;
      let lateSnapshot: {scope: Scope; subscription: Id} | undefined;
      if (!pending || envelope.id !== pending.id || 'method' in envelope) throw new Error('uncorrelated response');
      if ('error' in envelope) {
        validateWire('RpcError', envelope.error);
        this.#settle(pending, new RpcFailure(envelope.error as import('@vcp/protocol').RpcError));
      } else {
        if (pending.method === 'initialize') validateWire('InitializeResult', envelope.result);
        else {
          validateWire('ResultValue', envelope.result);
          const result = envelope.result as { kind: string };
          if (!(RESULT_KINDS[pending.method] as readonly string[]).includes(result.kind)) throw new Error('method result mismatch');
          if (pending.settled && pending.snapshotScope && (result.kind === 'snapshot' || result.kind === 'gap')) {
            lateSnapshot = {scope: pending.snapshotScope, subscription: (envelope.result as Reply<'session/snapshot'>).value.subscription};
          }
        }
        this.#settle(pending, undefined, envelope.result);
      }
      clearTimeout(pending.timer); this.#active = undefined; this.#pump();
      if (lateSnapshot) void this.call('events/unsubscribe', lateSnapshot, {timeoutMs: 2000}).catch(() => this.#fail('TRANSPORT_INTERRUPTED'));
    } catch { this.#fail('MALFORMED_PEER'); }
  }
  #fail(code: string): void {
    if (this.#closed) return;
    this.#closed = true;
    const pending = [...(this.#active ? [this.#active] : []), ...this.#queue];
    this.#active = undefined; this.#queue = []; this.#queuedBytes = 0;
    for (const request of pending) { clearTimeout(request.timer); this.#settle(request, new OperationError(code, 'local connection ended; reconcile original command identity', request.commandId)); }
    for (const stream of this.#streams) void stream.close();
    this.#remove(); void this.#transport.close().catch(() => {});
  }
  dispose(): Promise<void> {
    if (this.#disposing) return this.#disposing;
    this.#stopping = true;
    this.#disposing = (async () => {
      for (const pending of this.#queue) { clearTimeout(pending.timer); this.#settle(pending, new OperationError('DISPOSED', 'client disposed before queued send', pending.commandId)); }
      this.#queue = []; this.#queuedBytes = 0;
      await Promise.allSettled([...this.#streams].map(stream => stream.close()));
      this.#fail('DISPOSED');
      await this.#transport.close();
    })();
    return this.#disposing;
  }
}
