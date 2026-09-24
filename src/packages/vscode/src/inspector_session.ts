// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Call, ResultValue, PreviewPage, RoutingOptimizerPreviewView, ArtifactRead } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { ConnectionClient, ConnectionStatus } from './engine_connection.js';
import type { InspectorActionContext, InspectorActionOwner } from './inspector_action_contract.js';
import { boundInspectorState, emptyInspectorState, parseInspectorMessage, type InspectorAction, type InspectorMessage, type InspectorSection, type InspectorState, type InspectorTab } from './inspector_view_model.js';
import { decodeInspectorArtifact, projectInspector } from './inspector_projection.js';

export interface InspectorSelection { task: string | undefined; synchronized: boolean; revision: string | undefined }
export interface InspectorDependencies {
  publish(state: InspectorState): void;
  prompt(input: { key: string; title: string; maxLength: number }): Promise<string | undefined>;
  actions?: InspectorActionOwner;
  refreshMs?: number;
}
type ReadCall = Extract<Call, { method: 'history/query'|'memory/query'|'memory/history'|'context/inspect'|'routing/explain'|'usage/read'|'policy/read'|'routing/status'|'routing/reportRead'|'memory/forgetPreviewRead'|'memory/forgetRead'|'backup/status'|'backup/read'|'artifact/read' }>;
const PROFILES: Partial<Record<ReadCall['method'], string>> = {
  'history/query': 'history/query/1', 'memory/query': 'memory/query-sources/1', 'memory/history': 'memory/history/1',
  'policy/read': 'policy/inspection/1', 'routing/status': 'routing/status/1', 'routing/reportRead': 'routing/optimizer/1',
  'memory/forgetPreviewRead': 'memory/retention/1', 'memory/forgetRead': 'memory/retention/1', 'backup/status': 'backup/publisher/1', 'backup/read': 'backup/publisher/1',
};
const KINDS: Record<ReadCall['method'], ResultValue['kind']> = {
  'history/query':'history', 'memory/query':'memory_query', 'memory/history':'memory_history', 'context/inspect':'evidence', 'routing/explain':'evidence',
  'usage/read':'usage', 'policy/read':'policy', 'routing/status':'routing_status', 'routing/reportRead':'routing_report',
  'memory/forgetPreviewRead':'retention_preview', 'memory/forgetRead':'retention', 'backup/status':'backup_status', 'backup/read':'backup_job', 'artifact/read':'artifact',
};
/** One authorized page. All navigation arguments and opaque handles are transient. */
export class InspectorSession {
  #deps: InspectorDependencies;
  #client: ConnectionClient | undefined;
  #status: ConnectionStatus | undefined;
  #selection: InspectorSelection = { task: undefined, synchronized: false, revision: undefined };
  #tab: InspectorTab = 'history';
  #visible = false;
  #disposed = false;
  #epoch = 0;
  #abort: AbortController | undefined;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #request: ReadCall | undefined;
  #back: ReadCall[] = [];
  #hash: string | undefined;
  #context: InspectorActionContext | undefined;
  #handles = new Map<string, () => Promise<void>>();
  #actionIds = new Set<string>();
  #state: InspectorState = emptyInspectorState();
  #work: Promise<void> | undefined;
  constructor(deps: InspectorDependencies) { this.#deps = deps; }
  state(): InspectorState { return structuredClone(this.#state); }
  current(): InspectorActionContext | undefined { return this.#context; }
  connection(status: ConnectionStatus, client: ConnectionClient | undefined): void {
    const changed = client !== this.#client || status.generation !== this.#status?.generation || status.phase !== this.#status?.phase || status.engineTrust !== this.#status?.engineTrust || status.editorTrusted !== this.#status?.editorTrusted || status.role !== this.#status?.role || status.bindingRevision !== this.#status?.bindingRevision || status.workspaceRevision !== this.#status?.workspaceRevision;
    this.#status = status;
    this.#client = status.phase === 'connected' ? client : undefined;
    if (changed) { this.#request = undefined; this.#back = []; this.#hash = undefined; this.#clear('disconnected', 'Connection changed; retained inspector content cleared.'); if (this.#visible) void this.refresh(); }
  }
  selection(value: InspectorSelection): void {
    if (value.task === this.#selection.task && value.revision === this.#selection.revision && value.synchronized === this.#selection.synchronized) return;
    if (value.task !== this.#selection.task) this.#request = undefined;
    else this.#freshTarget();
    this.#selection = { ...value }; this.#back = []; this.#hash = undefined;
    this.#clear('stale', 'Task selection changed; reading current evidence.'); if (this.#visible) void this.refresh();
  }
  /** Preserve only read targets/query criteria, never an old authorized page or preview. */
  #freshTarget(): void {
    const request = this.#request;
    if (request?.method === 'memory/forgetPreviewRead') this.#request = undefined;
    else if (request?.method === 'artifact/read') this.#request = {method:'artifact/read',params:{...request.params,offset:'0'}};
    else if (request && 'cursor' in request.params) this.#request = {...request,params:{...request.params,cursor:null}} as ReadCall;
  }
  invalidate(reason: string): void {
    this.#freshTarget();
    this.#back = []; this.#hash = undefined;
    if (reason === 'gap') this.#selection = {...this.#selection,synchronized:false};
    // Exact previews cannot be resurrected by ordinary refresh.
    this.#clear('stale', `Inspector invalidated (${reason}); current access must be checked.`);
    if (this.#visible && !['gap', 'hidden', 'reload'].includes(reason)) void this.refresh();
  }
  visible(value: boolean): void {
    if (this.#visible === value || this.#disposed) return;
    this.#visible = value;
    if (!value) { this.#request = undefined; this.#back = []; this.#hash = undefined; }
    this.#clear('stale', 'Inspector visibility changed; retained content cleared.');
    if (value) void this.refresh();
  }
  dispose(): void { this.#disposed = true; this.#visible = false; this.#request = undefined; this.#back = []; this.#clear('disconnected', 'Inspector closed.'); }
  #clear(phase: InspectorState['phase'], message: string): void {
    ++this.#epoch; this.#abort?.abort(); this.#abort = undefined; clearTimeout(this.#timer); this.#timer = undefined;
    this.#handles.clear(); this.#actionIds.clear(); this.#context = undefined; this.#deps.actions?.invalidate();
    this.#state = emptyInspectorState(this.#tab, phase, message); this.#deps.publish(this.#state);
  }
  #valid(epoch: number, client: ConnectionClient): boolean { return !this.#disposed && this.#visible && epoch === this.#epoch && client === this.#client; }
  #action(label: string, work: () => Promise<void>): InspectorAction { const id = randomUUID(); this.#handles.set(id, work); return { id, label }; }
  #default(): ReadCall | undefined {
    const scope = this.#client!.scope, task = this.#selection.task;
    if (this.#tab === 'publisher') return { method:'backup/status', params:{scope} };
    if (!task || !this.#selection.synchronized) return undefined;
    const inspect = { scope, task, target:null, limit:16, cursor:null };
    switch (this.#tab) {
      case 'history': return { method:'history/query', params:{scope,task,selector:null,text:null,artifact:null,expand_compacted:false,limit:16,cursor:null} };
      case 'evidence': return { method:'context/inspect', params:inspect };
      case 'cost': return { method:'usage/read', params:inspect };
      case 'policy': return { method:'policy/read', params:{scope,task,section:'denials',limit:16,cursor:null} };
      case 'routing': return { method:'routing/status', params:{scope,task,section:'policy_entries',limit:16,cursor:null} };
      default: return undefined;
    }
  }
  async refresh(): Promise<void> {
    if (!this.#visible || this.#disposed) return;
    const client = this.#client;
    this.#clear(client ? 'loading' : 'disconnected', client ? 'Checking current access…' : 'Connect to inspect evidence.');
    if (!client) return;
    const epoch = this.#epoch;
    const request = this.#request ?? this.#default();
    if (request && 'task' in request.params && !this.#selection.synchronized) {
      this.#clear('stale', 'Task stream is not synchronized; refresh task state before reading.'); return;
    }
    if (!request) {
      this.#present(undefined, [], this.#selection.synchronized ? 'Select a query or deliberate engine preview.' : 'Select a synchronized task to inspect evidence.');
      return;
    }
    const profile = PROFILES[request.method];
    if (!client.initialized.methods.includes(request.method) || !client.initialized.capabilities.includes(request.method) || (profile && !client.initialized.capabilities.includes(profile))) { this.#clear('unsupported', 'This engine did not negotiate the requested inspector capability.'); return; }
    this.#request = request; this.#abort = new AbortController();
    const work = (async () => {
      try {
        const page = await client.call(request.method, request.params, { signal:this.#abort!.signal, timeoutMs:15_000 });
        if (!this.#valid(epoch, client)) return;
        this.#check(page, request);
        if (page.kind === 'artifact' && request.method === 'artifact/read') {
          const decoded = decodeInspectorArtifact(page.value, request.params, this.#hash); this.#hash = decoded.hash;
          const actions = decoded.next ? [this.#action('Next byte range', () => this.#navigate({ method:'artifact/read', params:{...request.params,offset:decoded.next!} }))] : [];
          this.#present(page, [{title:'Retained artifact bytes',fields:[],text:decoded.text}], 'Authorized bounded artifact range.', actions);
        } else this.#present(page, projectInspector(page), 'Current authorized result; retained history is not current permission.');
      } catch {
        if (this.#valid(epoch, client)) { this.#back = []; this.#hash = undefined;
          if (this.#request && 'cursor' in this.#request.params) this.#request = {...this.#request,params:{...this.#request.params,cursor:null}} as ReadCall;
          else this.#request = undefined;
          this.#clear('unavailable', 'Result unavailable or stale. Refresh restarts the query; prior content and actions were discarded.'); }
      }
    })();
    this.#work = work; await work; if (this.#work === work) this.#work = undefined;
  }
  #check(page: ResultValue, request: ReadCall): void {
    if (page.kind !== KINDS[request.method]) throw new Error('reply kind');
    const value = page.value as unknown as Record<string, unknown>, expected = request.params;
    if ('scope' in value) { const scope = value.scope as {workspace:string;session:string}; if (scope.workspace !== expected.scope.workspace || scope.session !== expected.scope.session) throw new Error('reply scope'); }
    for (const key of ['task','claim','report','operation','section','preview','job','offset'] as const) {
      if (key in expected && key in value && value[key] !== (expected as unknown as Record<string,unknown>)[key]) throw new Error('reply identity');
    }
  }
  #present(page: ResultValue | undefined, sections: InspectorSection[], message: string, additional: InspectorAction[] = []): void {
    if (!this.#client || !this.#status || !this.#visible || this.#disposed) return;
    const context: InspectorActionContext = {tab:this.#tab,client:this.#client,status:this.#status,synchronized:this.#selection.synchronized,...(this.#selection.task ? {task:this.#selection.task} : {}),...(page ? {page} : {})};
    const actions = [...additional, ...this.#navigation(page)];
    this.#context = context;
    const owned = this.#deps.actions?.register(context) ?? []; for (const a of owned) if (!a.disabledReason) this.#actionIds.add(a.id);
    const state: InspectorState = {tab:this.#tab,phase:'current',message,scopeLabel:`Workspace ${this.#client.scope.workspace}; session ${this.#client.scope.session}`,taskLabel:this.#selection.task ?? 'No task selected',sections,actions:[...actions,...owned],commands:this.#deps.actions?.records() ?? []};
    const bounded = boundInspectorState(state);
    if (bounded !== state) { this.#handles.clear(); this.#actionIds.clear(); this.#context = undefined; this.#deps.actions?.invalidate(); }
    this.#state = bounded; this.#deps.publish(bounded);
    this.#timer = setTimeout(() => void this.refresh(), Math.max(1000, this.#deps.refreshMs ?? 15_000)); this.#timer.unref?.();
  }
  async #navigate(request: ReadCall, remember = true): Promise<void> {
    if (remember && this.#request) this.#back = [...this.#back.slice(-7), this.#request];
    this.#request = request; await this.refresh();
  }
  #navigation(page?: ResultValue): InspectorAction[] {
    const actions: InspectorAction[] = [];
    const canRead = (method: ReadCall['method']): boolean => this.#client!.initialized.methods.includes(method)
      && this.#client!.initialized.capabilities.includes(method)
      && (!PROFILES[method] || this.#client!.initialized.capabilities.includes(PROFILES[method]!));
    const open = (label: string, method: ReadCall['method'], show: (id: string) => Promise<void>): void => {
      if (canRead(method)) actions.push(this.#action(label, () => this.#openId(label,show)));
    };
    if (this.#tab === 'optimizer') open('Open report','routing/reportRead',id=>this.showReport(id));
    if (this.#tab === 'publisher') {
      open('Open backup','backup/read',id=>this.showPublisher(id));
      if (page?.kind === 'backup_status' && page.value.active_operation && canRead('backup/read')) {
        const operation=page.value.active_operation;
        actions.push(this.#action('Open active backup',()=>this.showPublisher(operation)));
      }
    }
    if (this.#tab === 'pruning' && this.#selection.task && this.#selection.synchronized) {
      open('Open pruning preview','memory/forgetPreviewRead',id=>this.showPruningPreviewID(id));
      open('Open pruning job','memory/forgetRead',id=>this.showPruningJob(id));
    }
    if (this.#back.length) actions.push(this.#action('Back — recheck access', async () => { const prior = this.#back.pop(); if (prior) await this.#navigate(prior,false); }));
    const request = this.#request;
    if (page && request && 'next_cursor' in page.value && page.value.next_cursor && 'cursor' in request.params) {
      const cursor = page.value.next_cursor;
      actions.push(this.#action('Next page', () => this.#navigate({...request,params:{...request.params,cursor}} as ReadCall)));
    }
    if (page?.kind === 'retention_preview' && page.value.next_offset !== null && page.value.next_offset !== undefined) {
      const p = page.value; actions.push(this.#action('Next preview page',()=>this.#navigate({method:'memory/forgetPreviewRead',params:{scope:p.scope,task:p.task,preview:p.preview,offset:p.next_offset!,limit:16}})));
    }
    const scope = this.#client!.scope, task = this.#selection.task;
    if (task && this.#selection.synchronized) {
      if (this.#tab === 'history' || this.#tab === 'memory') actions.push(this.#action(this.#tab === 'history' ? 'Search history' : 'Search memory', () => this.#query()));
      if (this.#tab === 'policy') for (const section of ['denials','grants'] as const) actions.push(this.#action(`Show ${section}`,()=>this.#navigate({method:'policy/read',params:{scope,task,section,limit:16,cursor:null}},false)));
      if (this.#tab === 'routing') {
        for (const section of ['policy_entries','catalog'] as const) actions.push(this.#action(`Show ${section}`,()=>this.#navigate({method:'routing/status',params:{scope,task,section,limit:16,cursor:null}},false)));
        actions.push(this.#action('Observed routing evidence',()=>this.#navigate({method:'routing/explain',params:{scope,task,target:null,cursor:null,limit:16}},false)));
      }
      if (page?.kind === 'evidence') for (const row of page.value.rows) actions.push(this.#artifact(`Read ${row.id}`, row.content.artifact, task, row.content.offset, row.content.sha256));
      if (page?.kind === 'memory_query') for (const row of page.value.findings) {
        if (row.source.kind === 'claim') { const claim = row.source.claim; actions.push(this.#action(`Claim versions ${claim}`,()=>this.showClaim(claim))); }
        else actions.push(this.#artifact(`Read source ${row.source.artifact}`,row.source.artifact,task,'0',row.source_sha256));
      }
      if (page?.kind === 'memory_history') for (const row of page.value.versions) for (const ref of row.finding.evidence) actions.push(this.#artifact(`Read evidence ${ref.artifact}`,ref.artifact,task,ref.offset,ref.sha256));
      if (page?.kind === 'history') {
        for (const row of page.value.rows) if (row.task) for (const artifact of row.artifacts) actions.push(this.#artifact(`Read ${artifact.id}`,artifact.id,row.task));
        for (const link of page.value.claim_links) actions.push(this.#action(`Claim versions ${link.claim}`,()=>this.showClaim(link.claim)));
        actions.push(this.#action('Session history',()=>this.#navigate({method:'history/query',params:{scope,task:null,selector:null,text:null,artifact:null,expand_compacted:false,limit:16,cursor:null}},false)));
      }
    }
    if (page?.kind === 'routing_report') for (const section of ['summary','cohorts','uncertainty','sources','forecast'] as const) actions.push(this.#action(`Report ${section}`,()=>this.#navigate({method:'routing/reportRead',params:{scope,report:page.value.report,section,limit:16,cursor:null}},false)));
    return actions;
  }
  #artifact(label: string, artifact: string, task: string, offset = '0', hash?: string): InspectorAction {
    return this.#action(label, async () => { this.#hash = hash; await this.#navigate({method:'artifact/read',params:{scope:this.#client!.scope,task,artifact,offset,length:16_384}}); });
  }
  async #query(): Promise<void> {
    const epoch = this.#epoch, client = this.#client!, tab = this.#tab, task = this.#selection.task!;
    const maximum = tab === 'history' ? 512 : 4096;
    const query = await this.#deps.prompt({key:tab,title:`Search ${tab}`,maxLength:maximum});
    if (!this.#valid(epoch,client) || query === undefined || !query.trim() || Buffer.byteLength(query)>maximum || query.includes('\0')) return;
    this.#back = []; this.#hash = undefined;
    if (tab === 'history') await this.#navigate({method:'history/query',params:{scope:client.scope,task,selector:null,text:query,artifact:null,expand_compacted:false,limit:16,cursor:null}},false);
    else await this.#navigate({method:'memory/query',params:{scope:client.scope,task,query,limit:16}},false);
  }
  async #openId(title: string, show: (id: string) => Promise<void>): Promise<void> {
    const epoch=this.#epoch,client=this.#client!;
    const value=await this.#deps.prompt({key:'inspector-reference',title,maxLength:96});
    if (!this.#valid(epoch,client) || value===undefined) return;
    if (!/^[A-Za-z0-9_-]{1,96}$/.test(value)) throw new Error('Invalid inspector reference');
    await show(value);
  }
  async dispatch(input: InspectorMessage): Promise<void> {
    const message = parseInspectorMessage(input); if (!message || this.#disposed || !this.#visible) return;
    if (message.action === 'ready' || message.action === 'refresh') { await this.refresh(); return; }
    if (message.action === 'tab') { this.#tab = message.tab; this.#request = undefined; this.#back = []; this.#hash = undefined; await this.refresh(); return; }
    if (message.action === 'invoke') {
      const epoch = this.#epoch;
      const work = this.#handles.get(message.id); this.#handles.delete(message.id);
      try {
        if (work) { await work(); return; }
        if (this.#actionIds.delete(message.id)) await this.#deps.actions?.dispatch(message.id);
      } catch {
        if (epoch !== this.#epoch || !this.#visible || this.#disposed) return;
        this.#clear('unavailable', 'Action unavailable. Refresh to inspect current state and reconcile submitted commands. No action was replayed.');
        this.#state = boundInspectorState({...this.#state,commands:this.#deps.actions?.records() ?? []});
        this.#deps.publish(this.#state);
      }
    }
  }
  async showClaim(claim: string): Promise<void> { if (!this.#client || !this.#selection.task) return; this.#tab='memory'; this.#back=[]; await this.#navigate({method:'memory/history',params:{scope:this.#client.scope,task:this.#selection.task,claim,limit:16,cursor:null}},false); }
  async showReport(report: string): Promise<void> { if (!this.#client) return; this.#tab='optimizer'; this.#back=[]; await this.#navigate({method:'routing/reportRead',params:{scope:this.#client.scope,report,section:'summary',limit:16,cursor:null}},false); }
  async showPublisher(operation: string): Promise<void> { if (!this.#client) return; this.#tab='publisher'; this.#back=[]; await this.#navigate({method:'backup/read',params:{scope:this.#client.scope,operation}},false); }
  async showPruningJob(job: string): Promise<void> { if (!this.#client || !this.#selection.task) return; this.#tab='pruning'; this.#back=[]; await this.#navigate({method:'memory/forgetRead',params:{scope:this.#client.scope,task:this.#selection.task,job}},false); }
  async showPruningPreviewID(preview: string): Promise<void> { if (!this.#client || !this.#selection.task || !this.#selection.synchronized) return; this.#tab='pruning'; this.#back=[]; await this.#navigate({method:'memory/forgetPreviewRead',params:{scope:this.#client.scope,task:this.#selection.task,preview,offset:0,limit:16}},false); }
  showPruningPreview(page: PreviewPage): void {
    if (!this.#scope(page.scope) || page.task !== this.#selection.task) return;
    this.#tab='pruning'; this.#request={method:'memory/forgetPreviewRead',params:{scope:page.scope,task:page.task,preview:page.preview,offset:page.offset,limit:16}};
    this.#back=[]; this.#clear('loading','Reviewing exact engine pruning preview.'); this.#present({kind:'retention_preview',value:page},projectInspector({kind:'retention_preview',value:page}),'Exact preview; protected targets are not deletion grants.');
  }
  showOptimizerPreview(view: RoutingOptimizerPreviewView): void {
    if (!this.#scope(view.scope)) return;
    this.#tab='optimizer'; this.#request=undefined; this.#back=[]; this.#clear('loading','Reviewing exact engine policy preview.'); this.#present({kind:'routing_preview',value:view},projectInspector({kind:'routing_preview',value:view}),'Exact reviewed policy; applying requires current authorization and all pinned revisions.');
  }
  #scope(scope: {workspace:string;session:string}): boolean { return !!this.#client && this.#visible && scope.workspace===this.#client.scope.workspace && scope.session===this.#client.scope.session; }
}
