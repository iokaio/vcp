// SPDX-License-Identifier: Apache-2.0
import type { EventsSubscribe, Id, SessionSnapshotRead } from '@vcp/protocol';
import type { Client, CallOptions } from './client.js';
import type { Reply } from './method-results.js';
import { SdkError } from './errors.js';

export type StreamOptions = CallOptions & { pollIntervalMs?: number };
type Page = Reply<'events/subscribe'>;
function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(new SdkError('ABORTED', 'event stream closed')); return; }
    const abort = () => { clearTimeout(timer); reject(new SdkError('ABORTED', 'event stream closed')); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve(); }, ms);
    signal.addEventListener('abort', abort, { once: true });
  });
}
export class EventStream implements AsyncIterableIterator<Page> {
  #client: Client;
  #params: EventsSubscribe;
  #options: StreamOptions;
  #onClose: () => void;
  #abort = new AbortController();
  #externalAbort: () => void;
  #subscription?: Id;
  #cursor?: string;
  #pending?: Promise<Page>;
  #closed = false;
  #busy = false;
  #empty = false;
  #gap = false;
  #sequence: bigint;
  #close?: Promise<void>;
  constructor(client: Client, params: EventsSubscribe, options: StreamOptions, onClose: () => void) {
    this.#client = client; this.#params = structuredClone(params); this.#options = {...options}; this.#onClose = onClose;
    this.#sequence = BigInt(params.after_sequence);
    const poll = options.pollIntervalMs ?? 100;
    if (!Number.isSafeInteger(poll) || poll < 10 || poll > 10_000) throw new SdkError('INVALID_ARGUMENT', 'poll interval must be 10..10000 milliseconds');
    this.#externalAbort = () => { void this.close(); };
    options.signal?.addEventListener('abort', this.#externalAbort, { once: true });
    if (options.signal?.aborted) queueMicrotask(this.#externalAbort);
  }
  [Symbol.asyncIterator](): AsyncIterableIterator<Page> { return this; }
  async next(): Promise<IteratorResult<Page>> {
    if (this.#busy) throw new SdkError('RESOURCE_LIMIT', 'only one event read may be pending');
    if (this.#closed || this.#gap) { await this.close(); return { done: true, value: undefined }; }
    this.#busy = true;
    try {
      if (this.#empty) await delay(this.#options.pollIntervalMs ?? 100, this.#abort.signal);
      const options = this.#options.timeoutMs === undefined ? {} : { timeoutMs: this.#options.timeoutMs };
      this.#pending = (this.#subscription && this.#cursor
        ? this.#client.call('events/next', { scope: this.#params.scope, subscription: this.#subscription, cursor: this.#cursor }, options)
        : this.#client.call('events/subscribe', this.#params, options)).then(page => {
          // Remember even a late reply so abandoned subscribe can be unsubscribed.
          if (this.#subscription !== undefined && this.#subscription !== page.value.subscription) throw new SdkError('MALFORMED_PEER', 'event subscription changed');
          this.#subscription = page.value.subscription;
          if (page.kind === 'events') this.#cursor = page.value.cursor;
          return page;
        });
      let abort!: () => void;
      const cancelled = new Promise<never>((_, reject) => {
        abort = () => reject(new SdkError('ABORTED', 'event await aborted'));
        this.#abort.signal.addEventListener('abort', abort, { once: true });
        if (this.#abort.signal.aborted) abort();
      });
      let page: Page;
      try { page = await Promise.race([this.#pending, cancelled]); }
      finally { this.#abort.signal.removeEventListener('abort', abort); }
      if (page.kind === 'gap') this.#gap = true;
      else {
        for (const event of page.value.events) {
          if (event.scope.workspace !== this.#params.scope.workspace || event.scope.session !== this.#params.scope.session || BigInt(event.sequence) <= this.#sequence) throw new SdkError('MALFORMED_PEER', 'event scope or ordering mismatch');
          this.#sequence = BigInt(event.sequence);
        }
        this.#empty = page.value.at_end && page.value.events.length === 0;
      }
      return { done: false, value: page };
    } catch (error) {
      if (error instanceof SdkError && error.code === 'MALFORMED_PEER') void this.#client.dispose();
      void this.close(); throw error;
    }
    finally { this.#busy = false; }
  }
  async return(): Promise<IteratorResult<Page>> { await this.close(); return { done: true, value: undefined }; }
  close(): Promise<void> {
    if (this.#close) return this.#close;
    this.#closed = true; this.#abort.abort();
    this.#options.signal?.removeEventListener('abort', this.#externalAbort);
    const cleanup = async () => {
      try { await this.#pending; } catch { /* transport failure already reported */ }
      if (this.#subscription) {
        try { await this.#client.call('events/unsubscribe', { scope: this.#params.scope, subscription: this.#subscription }, { timeoutMs: 2000 }); }
        catch { /* connection disposal also owns native subscription cleanup */ }
      }
    };
    this.#close = new Promise<void>(resolve => {
      const timer = setTimeout(resolve, 2500);
      void cleanup().finally(() => { clearTimeout(timer); resolve(); });
    }).finally(this.#onClose);
    return this.#close;
  }
}

/** Yield every page, including explicit gaps. No implicit resnapshot or cursor reuse. */
export async function* snapshotPages(client: Client, params: SessionSnapshotRead, options?: CallOptions): AsyncGenerator<Reply<'session/snapshot'>> {
  const initial = structuredClone(params);
  let request = initial;
  let subscription: Id | undefined;
  let watermark: string | undefined;
  let sequence: string | undefined;
  let eventCursor: string | undefined;
  let complete = false;
  const cursors = new Set<string>();
  try {
    for (let pages = 0; pages < 8192; pages++) {
      const page = await client.snapshot(request, options);
      if (subscription !== undefined && subscription !== page.value.subscription) throw new SdkError('MALFORMED_PEER', 'snapshot subscription changed');
      subscription = page.value.subscription;
      if (page.kind === 'gap') { yield page; return; }
      const own = (scope: typeof initial.scope) => scope.workspace === initial.scope.workspace && scope.session === initial.scope.session;
      if (!own(page.value.session.scope) || page.value.tasks.some(task => !own(task.scope)) || (watermark !== undefined && (watermark !== page.value.watermark || sequence !== page.value.sequence || eventCursor !== page.value.event_cursor))) throw new SdkError('MALFORMED_PEER', 'snapshot cut changed across pages');
      watermark = page.value.watermark;
      sequence = page.value.sequence; eventCursor = page.value.event_cursor;
      complete = page.value.complete;
      yield page;
      if (complete) return;
      const cursor = page.value.next_cursor;
      if (!cursor || cursors.has(cursor)) throw new SdkError('MALFORMED_PEER', 'snapshot continuation did not advance');
      cursors.add(cursor); request = { ...initial, cursor };
    }
    throw new SdkError('RESOURCE_LIMIT', 'snapshot page bound reached');
  } catch (error) {
    if (error instanceof SdkError && error.code === 'MALFORMED_PEER') await client.dispose();
    throw error;
  } finally {
    // Completed snapshots retain their event cursor for explicit consumer use.
    if (!complete && subscription) {
      let timer: NodeJS.Timeout | undefined;
      try { await Promise.race([client.call('events/unsubscribe', {scope: initial.scope, subscription}, {timeoutMs: 2000}), new Promise<void>(resolve => {timer = setTimeout(resolve, 2500);})]); }
      catch { /* connection disposal also owns native subscription cleanup */ }
      finally { clearTimeout(timer); }
    }
  }
}
