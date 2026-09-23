// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Id } from '@vcp/protocol';

export { Client, OperationError } from './client.js';
export type { CallOptions } from './client.js';
export { launchLocal, attachLocal } from './local.js';
export type { LaunchOptions, AttachOptions, LocalAttachment } from './local.js';
export { EventStream } from './subscriptions.js';
export type { StreamOptions } from './subscriptions.js';
export { SdkError, RpcFailure, classifyRpcError } from './errors.js';
export type { RpcCategory, RpcClassification } from './errors.js';
export type { Method, Params, Reply } from './method-results.js';
export type * from '@vcp/protocol';

/** Allocate an identity explicitly. Calls and reconnects never replace it. */
export function newCommandId(): Id {
  return randomUUID();
}
