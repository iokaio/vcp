// SPDX-License-Identifier: Apache-2.0
import type { ResultValue } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { ConnectionClient, ConnectionStatus } from './engine_connection.js';
import type { InspectorAction, InspectorState, InspectorTab } from './inspector_view_model.js';
export interface InspectorActionContext {
  readonly tab: InspectorTab;
  readonly client: ConnectionClient;
  readonly status: ConnectionStatus;
  readonly task?: string;
  readonly synchronized: boolean;
  readonly page?: ResultValue;
}
export interface InspectorActionOwner {
  register(context: InspectorActionContext): readonly InspectorAction[];
  dispatch(id: string): Promise<void>;
  records(): InspectorState['commands'];
  invalidate(): void;
}
