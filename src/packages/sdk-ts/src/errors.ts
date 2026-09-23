// SPDX-License-Identifier: Apache-2.0
import type { ApplicationError, Code, RpcError, Retry } from '@vcp/protocol';
import schema from '@vcp/protocol/schema.json' with { type: 'json' };

export type RpcCategory =
  | 'unsupported_method' | 'unsupported_version' | 'unsupported_capability'
  | 'policy_denied' | 'authority_stale' | 'approval_required' | 'approval_stale'
  | 'input_required' | 'version_conflict' | 'command_conflict' | 'outcome_unknown'
  | 'budget_exhausted' | 'resource_limit' | 'provider_retryable' | 'provider_rejected'
  | 'store_unavailable' | 'index_not_ready' | 'cursor_gap' | 'cancelled'
  | 'parse_error' | 'invalid_request' | 'invalid_params' | 'internal_error' | 'unknown';

export interface RpcClassification {
  readonly category: RpcCategory;
  readonly applicationCode?: Code;
  readonly retry?: Retry;
  readonly operation?: string;
}

const applicationCategories = {
  POLICY_DENIED: 'policy_denied', APPROVAL_REQUIRED: 'approval_required',
  APPROVAL_STALE: 'approval_stale', BUDGET_EXHAUSTED: 'budget_exhausted',
  CAPABILITY_UNAVAILABLE: 'unsupported_capability', VERSION_CONFLICT: 'version_conflict',
  PROVIDER_RETRYABLE: 'provider_retryable', PROVIDER_REJECTED: 'provider_rejected',
  STORE_UNAVAILABLE: 'store_unavailable', INDEX_NOT_READY: 'index_not_ready',
  OUTCOME_UNKNOWN: 'outcome_unknown', CANCELLED: 'cancelled', COMMAND_CONFLICT: 'command_conflict',
  INPUT_REQUIRED: 'input_required', AUTHORITY_STALE: 'authority_stale',
  UNSUPPORTED_VERSION: 'unsupported_version', CURSOR_GAP: 'cursor_gap', RESOURCE_LIMIT: 'resource_limit',
} as const satisfies Record<Code, RpcCategory>;
const codes: readonly string[] = schema.definitions.Code.enum;
const retries: readonly string[] = schema.definitions.Retry.oneOf.flatMap(branch => branch.enum);
function constrainedString(value: unknown, constraints: Record<string, unknown>): value is string {
  if (typeof value !== 'string' || value.length > 16 * 1024 * 1024) return false;
  let length = 0;
  for (const character of value) {
    const scalar = character.codePointAt(0)!;
    if (scalar >= 0xd800 && scalar <= 0xdfff) return false;
    length++;
  }
  if (typeof constraints['minLength'] === 'number' && length < constraints['minLength']) return false;
  if (typeof constraints['maxLength'] === 'number' && length > constraints['maxLength']) return false;
  if (typeof constraints['pattern'] === 'string' && !new RegExp(constraints['pattern'], 'u').test(value)) return false;
  return true;
}

// Recognition only, not a replacement for validateWire. Canonical fields, required
// keys and enums come from the bundle; unknown detail shapes stay unclassified.
function applicationDetails(value: unknown): ApplicationError | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== null && prototype !== Object.prototype) return undefined;
  const fields = Object.getOwnPropertyDescriptors(value);
  const known = new Set(['code', 'retry', 'operation', 'explanation', 'reconciliation']);
  if (Object.getOwnPropertySymbols(value).length || Object.keys(fields).some(key => !known.has(key) || !fields[key]?.enumerable || !('value' in fields[key]!))) return undefined;
  if (schema.definitions.ApplicationError.required.some(key => !Object.hasOwn(fields, key))) return undefined;
  const item = value as Record<string, unknown>;
  if (typeof item['code'] !== 'string' || !codes.includes(item['code']) || !Object.hasOwn(applicationCategories, item['code'])) return undefined;
  if (typeof item['retry'] !== 'string' || !retries.includes(item['retry']) || !constrainedString(item['explanation'], schema.definitions.ApplicationError.properties.explanation)) return undefined;
  if (Object.hasOwn(item, 'operation') && item['operation'] !== null && !constrainedString(item['operation'], schema.definitions.Id)) return undefined;
  if (Object.hasOwn(item, 'reconciliation') && item['reconciliation'] !== null && !constrainedString(item['reconciliation'], schema.definitions.ApplicationError.properties.reconciliation)) return undefined;
  return value as ApplicationError;
}

/** Interpret only known numeric/kind/application combinations; never infer from text. */
export function classifyRpcError(error: RpcError): RpcClassification {
  switch (error.code) {
    case -32700: return { category: 'parse_error' };
    case -32600: return { category: 'invalid_request' };
    case -32601: return { category: 'unsupported_method' };
    case -32602: return { category: 'invalid_params' };
    case -32603: return { category: 'internal_error' };
  }
  if (error.code !== -32000) return { category: 'unknown' };
  if (error.data?.kind === 'unsupported_version') return { category: 'unsupported_version' };
  if (error.data?.kind === 'unsupported_capability') return { category: 'unsupported_capability' };
  if (error.data?.kind !== 'application') return { category: 'unknown' };
  const application = applicationDetails(error.data.details);
  if (!application) return { category: 'unknown' };
  return {
    category: applicationCategories[application.code],
    applicationCode: application.code,
    retry: application.retry,
    ...(application.operation == null ? {} : { operation: application.operation }),
  };
}

/** Messages are SDK-authored diagnostics, never peer frame or credential text. */
export class SdkError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = 'SdkError';
  }
}

/** Construct only after validating/bounding the incoming RpcError. */
export class RpcFailure extends SdkError {
  readonly rpcCode: number;
  readonly classification: RpcClassification;
  declare readonly error: RpcError;
  constructor(error: RpcError) {
    super('rpc', 'The server rejected the request');
    this.name = 'RpcFailure';
    this.rpcCode = error.code;
    this.classification = Object.freeze(classifyRpcError(error));
    // Non-enumerable so ordinary diagnostic serialization does not copy peer data.
    Object.defineProperty(this, 'error', { value: error, enumerable: false });
  }
}
