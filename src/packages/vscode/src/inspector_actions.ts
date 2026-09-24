// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Mutation, ResultValue, RoutingOptimizerEdit } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { InspectorActionContext, InspectorActionOwner } from './inspector_action_contract.js';
import type { InspectorAction } from './inspector_view_model.js';
import type { InspectorSession } from './inspector_session.js';
import { InspectorJournal, type InspectorCommandMethod } from './inspector_journal.js';

interface Dependencies {
  current(): InspectorActionContext | undefined;
  session(): InspectorSession;
  prompt(title: string, value?: string): Promise<string | undefined>;
  choose(title: string, choices: readonly string[]): Promise<string | undefined>;
  confirm(message: string): Promise<boolean>;
}
type Entry = { context: InspectorActionContext; disabled: boolean; run(): Promise<void> };
const sameScope = (a: InspectorActionContext, b: InspectorActionContext) => a.client.scope.workspace === b.client.scope.workspace && a.client.scope.session === b.client.scope.session;
const counter = (value: string | undefined): value is string => typeof value === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;

/** Deliberate controls own transient arguments; durable storage owns only receipts. */
export class InspectorActions implements InspectorActionOwner {
  #entries = new Map<string, Entry>();
  #busy = false;
  #expiry = new WeakMap<InspectorActionContext, number>();
  constructor(private readonly deps: Dependencies, readonly journal: InspectorJournal) {}
  invalidate(): void { this.#entries.clear(); }
  records() { return this.journal.records().slice(-128).map(({commandId,method,phase}) => ({commandId,method,phase})); }
  #valid(context: InspectorActionContext): boolean { const current = this.deps.current(); return current === context && current.client === context.client && sameScope(current, context); }
  #check(context: InspectorActionContext): void { if (!this.#valid(context)) throw new Error('Inspector changed; refresh and review again.'); }
  #reason(context: InspectorActionContext): string | undefined {
    return context.status.role !== 'controller' ? 'Connect explicitly as controller.'
      : !context.status.editorTrusted || context.status.engineTrust !== 'trusted' ? 'Current editor and engine trust are required.'
      : !context.status.bindingRevision || !context.status.workspaceRevision ? 'Current binding and workspace revisions are required.'
      : (!context.client.initialized.methods.includes('command/read') || !context.client.initialized.capabilities.includes('command/read')) ? 'Durable command reconciliation is unavailable.'
      : this.journal.records().some(r => r.scope.workspace === context.client.scope.workspace && r.scope.session === context.client.scope.session && ['submitting','unknown'].includes(r.phase)) ? 'Reconcile the pending command before another submission.' : undefined;
  }
  register(context: InspectorActionContext): readonly InspectorAction[] {
    this.#entries.clear(); const actions: InspectorAction[] = [];
    const add = (label: string, run: () => Promise<void>, method?: string, extra?: string) => {
      if (method && !context.client.initialized.methods.includes(method)) return;
      const profile = method?.startsWith('routing/') ? 'routing/optimizer/1' : method?.startsWith('memory/') ? 'memory/retention/1' : method?.startsWith('backup/') ? 'backup/publisher/1' : undefined;
      const taskBound = method === 'routing/preview' || method?.startsWith('memory/');
      // Retention previews are access-checked reads, not mutation admission.
      const authorityReason = context.status.phase !== 'connected' ? 'Connect to inspect current engine state.'
        : method === 'memory/forgetPreview' ? undefined : this.#reason(context);
      const disabledReason = method ? authorityReason ?? (taskBound && (!context.task || !context.synchronized) ? 'Select a synchronized task before reviewing task-bound changes.' : undefined) ?? (!context.client.initialized.capabilities.includes(method) || (profile && !context.client.initialized.capabilities.includes(profile)) ? 'Required inspector capability was not negotiated.' : undefined) ?? extra : undefined;
      const id = randomUUID(); this.#entries.set(id, {context,disabled:!!disabledReason,run});
      actions.push({id,label,...(disabledReason ? {disabledReason} : {})});
    };
    if (context.client.initialized.methods.includes('command/read') && context.client.initialized.capabilities.includes('command/read') && this.journal.records().length) add('Reconcile submitted commands', async () => { await this.journal.reconcile(context.client.scope, context.client.call.bind(context.client)); if (this.#valid(context)) await this.deps.session().refresh(); });
    if (context.tab === 'optimizer') {
      add('Capture optimization report', () => this.#capture(context), 'routing/reportCapture');
      if (context.page?.kind === 'routing_report') add('Preview policy edit', () => this.#previewEdit(context), 'routing/preview');
      add('Preview policy rollback', () => this.#previewRollback(context), 'routing/preview');
      if (context.page?.kind === 'routing_preview') {
        const preview = context.page.value, expires = this.#expiry.get(context) ?? Date.now() + preview.expires_in_ms;
        this.#expiry.set(context, expires);
        const method = preview.operation === 'apply' ? 'routing/apply' : 'routing/rollback';
        add(preview.operation === 'apply' ? 'Apply reviewed policy' : 'Apply reviewed rollback', async () => {
          if (Date.now() >= expires || !await this.deps.confirm('Apply the exact policy shown in this review? Current engine revisions and authority will be checked again.')) return;
          this.#check(context); if (Date.now() >= expires) throw new Error('Preview expired; create another preview.');
          await this.#submit(context,method, mutation => context.client.call(method,{scope:context.client.scope,mutation,expected_binding_revision:preview.binding_revision,preview_id:preview.preview_id,preview_sha256:preview.preview_sha256}),undefined,undefined,expires);
        }, method);
      }
    }
    if (context.tab === 'pruning' && context.task) {
      add('Preview pruning for selected task', () => this.#pruning(context), 'memory/forgetPreview');
      if (context.page?.kind === 'retention_preview') {
        const preview = context.page.value, expires = this.#expiry.get(context) ?? Date.now() + preview.expires_in_ms;
        this.#expiry.set(context, expires);
        add('Apply reviewed pruning', async () => {
          if (Date.now() >= expires || !await this.deps.confirm(`Apply ${preview.action} to the reviewed selection? Protected targets remain protected. Backup copies may require separate cleanup.`)) return;
          this.#check(context); if (Date.now() >= expires) throw new Error('Preview expired.');
          const task = await context.client.call('task/read',{scope:context.client.scope,task:preview.task}); this.#check(context);
          await this.#submit(context,'memory/forget',mutation => context.client.call('memory/forget',{scope:context.client.scope,task:preview.task,mutation:{...mutation,expected_revision:task.value.revision,steering_revision:task.value.steering_revision},preview:preview.preview,preview_digest:preview.digest}),preview.task,undefined,expires);
        }, 'memory/forget');
      }
    }
    if (context.tab === 'publisher') {
      const page = context.page;
      if (page?.kind === 'backup_status') {
        const capability = page.value.capability;
        add('Publish encrypted workspace backup', async () => {
          if (capability.state !== 'loaded' || !await this.deps.confirm('Publish an encrypted backup of this entire workspace to the configured local vault? This does not confirm cloud transfer or a verified restore.')) return;
          this.#check(context);
          await this.#submit(context,'backup/create',mutation => context.client.call('backup/create',{scope:context.client.scope,mutation,expected_binding_revision:context.status.bindingRevision!,capability:capability.reference,expected_capability_generation:capability.generation}));
        }, 'backup/create', capability.state !== 'loaded' ? 'Connect a controller with an explicitly selected publisher profile.' : page.value.busy ? 'The publisher has an active operation.' : undefined);
      }
      if (page?.kind === 'backup_job') {
        const job = page.value;
        add('Request backup cancellation', async () => {
          if (!await this.deps.confirm('Request cancellation? Already published data and outstanding cleanup obligations remain visible.')) return;
          this.#check(context);
          await this.#submit(context,'backup/cancel',mutation => context.client.call('backup/cancel',{scope:context.client.scope,mutation,expected_binding_revision:context.status.bindingRevision!,operation:job.operation,expected_operation_revision:job.revision,expected_job_revision:job.job_revision ?? null}),undefined,job.operation);
        },'backup/cancel');
        add('Retry backup operation',async () => {
          if (!job.job_revision || !await this.deps.confirm('Explicitly retry this backup operation using its current native job and publisher capability?')) return;
          this.#check(context); const status = await context.client.call('backup/status',{scope:context.client.scope}); this.#check(context);
          if (status.value.capability.state !== 'loaded') throw new Error('Publisher capability is not loaded.');
          const capability = status.value.capability;
          await this.#submit(context,'backup/retry',mutation => context.client.call('backup/retry',{scope:context.client.scope,mutation,expected_binding_revision:context.status.bindingRevision!,operation:job.operation,expected_operation_revision:job.revision,expected_job_revision:job.job_revision!,capability:capability.reference,expected_capability_generation:capability.generation}),undefined,job.operation);
        },'backup/retry',!job.job_revision ? 'Native job revision is not available.' : undefined);
      }
    }
    for (const record of this.journal.records().filter(r => r.scope.workspace === context.client.scope.workspace && r.scope.session === context.client.scope.session && ['accepted','reconciled'].includes(r.phase)).slice(-8)) {
      if (context.tab === 'optimizer' && record.method === 'routing/reportCapture') add(`Open report ${record.commandId}`,()=>this.deps.session().showReport(record.commandId));
      if (context.tab === 'publisher' && record.method === 'backup/create') add(`Open backup ${record.commandId}`,()=>this.deps.session().showPublisher(record.commandId));
      if (context.tab === 'pruning' && record.method === 'memory/forget' && record.task === context.task && record.target) add(`Open pruning job ${record.target}`,()=>this.deps.session().showPruningJob(record.target!));
    }
    return actions;
  }
  async dispatch(id: string): Promise<void> {
    const entry = this.#entries.get(id); if (!entry || entry.disabled || this.#busy || !this.#valid(entry.context)) return;
    this.#entries.delete(id); this.#busy = true;
    try { await entry.run(); } finally { this.#busy = false; }
  }
  async #submit(context: InspectorActionContext, method: InspectorCommandMethod, send: (mutation: Mutation) => Promise<ResultValue>, task?: string, target?: string, expires = Infinity): Promise<void> {
    this.#check(context);
    const id = await this.journal.begin(context.client.scope,method,{...(task ? {task} : {}),...(target ? {target} : {})});
    if (!this.#valid(context) || Date.now() >= expires) { await this.journal.settle(id,'rejected'); throw new Error('Inspector changed or preview expired before submission.'); }
    try {
      const result = await send({command_id:id,expected_revision:context.status.workspaceRevision!,steering_revision:'0'});
      const receipt = result.kind === 'forgotten' ? result.value.acceptance : result.kind === 'acceptance' ? result.value : undefined;
      if (!receipt) throw new Error('Unexpected receipt.');
      await this.journal.settle(id,'accepted',receipt,result.kind === 'forgotten' ? result.value.job.job : undefined);
      if (this.#valid(context)) {
        if (method === 'routing/reportCapture') await this.deps.session().showReport(id);
        else if (method.startsWith('backup/')) await this.deps.session().showPublisher(target ?? id);
        else if (result.kind === 'forgotten') await this.deps.session().showPruningJob(result.value.job.job);
        else await this.deps.session().refresh();
      }
    } catch (error) {
      const failure = error as {code?:string;classification?:{retry?:string;applicationCode?:string}};
      const rejected = failure.code === 'rpc' && failure.classification?.retry !== 'reconcile_original' && ['POLICY_DENIED','APPROVAL_STALE','VERSION_CONFLICT','CAPABILITY_UNAVAILABLE','AUTHORITY_STALE','COMMAND_CONFLICT'].includes(failure.classification?.applicationCode ?? '');
      await this.journal.settle(id,rejected ? 'rejected' : 'unknown');
      if (this.#valid(context)) this.deps.session().invalidate('command outcome requires reconciliation');
      throw new Error(rejected ? 'Engine rejected the action; refresh and review again.' : `Reconcile submitted command ${id}; it will not be replayed.`);
    }
  }
  async #capture(context: InspectorActionContext): Promise<void> {
    const coverage = await this.deps.choose('Report coverage',['session','workspace']); this.#check(context); if (coverage !== 'session' && coverage !== 'workspace') return;
    const from = await this.deps.prompt('Window start in Unix milliseconds; blank includes all retained history',''); this.#check(context); if (from === undefined || (from !== '' && !counter(from))) return;
    const until = await this.deps.prompt('Window end in Unix milliseconds',Date.now().toString()); this.#check(context); if (!counter(until) || (from !== '' && BigInt(from) >= BigInt(until))) return;
    await this.#submit(context,'routing/reportCapture',mutation => context.client.call('routing/reportCapture',{scope:context.client.scope,mutation,expected_binding_revision:context.status.bindingRevision!,coverage,window:{from:from || null,until}}));
  }
  async #policyRevision(context: InspectorActionContext): Promise<string> {
    if (!context.task) throw new Error('Select a synchronized task.');
    const result = await context.client.call('routing/status',{scope:context.client.scope,task:context.task,section:'policy_entries',limit:1,cursor:null}); this.#check(context);
    return result.value.persisted?.revision ?? '0';
  }
  async #previewEdit(context: InspectorActionContext): Promise<void> {
    if (context.page?.kind !== 'routing_report') return;
    const field = await this.deps.choose('Policy field',['input_tokens','output_tokens','quality_floor_bps']); this.#check(context); if (!field) return;
    const value = await this.deps.prompt('New limit (nonnegative integer)'); this.#check(context); if (!counter(value)) return;
    let edit: RoutingOptimizerEdit;
    if (field === 'quality_floor_bps') { if (BigInt(value)>10000n) return; edit={field,value:Number(value)}; }
    else if (field === 'input_tokens' || field === 'output_tokens') edit={field,value}; else return;
    const revision = await this.#policyRevision(context);
    const result = await context.client.call('routing/preview',{scope:context.client.scope,expected_policy_revision:revision,proposal:{kind:'apply',report:context.page.value.report,edits:[edit]}});
    this.#check(context); this.deps.session().showOptimizerPreview(result.value);
  }
  async #previewRollback(context: InspectorActionContext): Promise<void> {
    const target = await this.deps.prompt('Historical policy revision to restore'); this.#check(context); if (!counter(target)) return;
    const revision = await this.#policyRevision(context);
    const result = await context.client.call('routing/preview',{scope:context.client.scope,expected_policy_revision:revision,proposal:{kind:'rollback',target_revision:target}});
    this.#check(context); this.deps.session().showOptimizerPreview(result.value);
  }
  async #pruning(context: InspectorActionContext): Promise<void> {
    if (!context.task) return;
    const action = await this.deps.choose('Pruning action for selected task',['exclude','restore_recall','compact','purge']); this.#check(context);
    if (action !== 'exclude' && action !== 'restore_recall' && action !== 'compact' && action !== 'purge') return;
    const result = await context.client.call('memory/forgetPreview',{scope:context.client.scope,task:context.task,action,selector:{schema_version:1,tree:{operator:'match',value:{kind:'task',value:context.task}}},limit:16});
    this.#check(context); this.deps.session().showPruningPreview(result.value);
  }
}
