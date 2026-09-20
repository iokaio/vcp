# History and retention controls

`history list` browses raw canonical events. `history search TEXT` searches
retained event facts, not governed knowledge or implicit artifact contents.
Rows label purged content, compacted presentation, and recall exclusion
independently. `--expand-compacted` expands presentation without reversing
retention. Memory values and evidence require the governed memory inspector.

`--since` is inclusive; `--before` is exclusive. Timestamps require `Z` or an
explicit numeric offset. Date-only inputs require `--utc-offset-minutes` and
mean midnight at that fixed offset. The parser never assumes the machine's
timezone or resolves ambiguous daylight-saving times. The same typed selector
is used for listing, search, preview, and saved automatic policies.

All filters are ANDed with the selected workspace. Additional filters include
task, root, path, actor, agent, provider, model, event kind, and claim kind.
`--selector` accepts the bounded versioned selector tree for combinations and
status/supersession predicates. Unknown source metadata does not match a
predicate, including its negation; a root or claim filter cannot invent
metadata on a raw event.

Pages contain at most 128 rows, bounded content summaries, and an optional JSON
cursor. Pass that cursor back with identical filters and limits. Its upper
event boundary remains fixed across appends; newer events are counted
separately. Current authority or deletion changes require a fresh cursor.
Artifact IDs link to `inspect ID --view outputs --offset 0 --length 65536`;
successive ranges expose retained output beyond terminal tails. Use
`history list --artifact ID` for source-event backlinks. Truncating a view never
removes retained source content.

`history prune --preview [FILTERS] --action purge` persists the exact normalized
selection, protected references, byte estimate, and backup limitations.
`exclude`, `restore-recall`, and `compact` are distinct actions. Review the
returned preview, then use `prune apply PREVIEW_ID`. Large previews show counts
and bounded examples; `prune show
PREVIEW_ID --offset N --limit 64` pages through every selected, dependent, and
protected reference. Apply loads the persisted selection and checks current
source/authority revisions; it never reruns a
broad filter and silently adds newly matching data. `prune cleanup RECEIPT_ID`
retries pending physical work. Logical unavailability, local cleanup, pending
generations, and retained backups remain separate receipt facts.

`retention show` displays notification cadence and explicit automatic policy.
`retention set --notification-only` disables automatic actions. An automatic
policy requires `--automatic` with the typed selector, action, and cadence;
updating a saved policy also requires `--expected-revision`.
Automatic policy cadence is evaluated when the canonical owner next starts,
after recovery and before normal execution. Setting a policy does not execute
it immediately. Read-only inspection does not evaluate automatic work; there
is no hidden daemon or promised wall-clock execution while the owner is closed.
Weekly notices are nonblocking and do not delete content. History controls include a notice
when history is strictly older than 30 days; acknowledgement is persisted only
after output is presented.

The same structured facts are emitted in JSONL mode. Commands use typed local
controller operations. A live owner serializes prune with dispatch and permits
inspection while paused; no history operation resumes children or invokes a
model. Offline operations retain the exclusive canonical store lock and use
the same service, never editing index files directly.

The open terminal accepts `/history list`, `/history search TEXT`,
`/history prune --preview`, `/prune show|apply ID`, and `/retention show|set`.
`/next` first advances through the displayed page and then its canonical
cursor. These controls remain available while paused. Arguments are separated
by whitespace; use the finite CLI for quoted multiword values and selector JSON
containing spaces. Terminal controls always use the active workspace.
