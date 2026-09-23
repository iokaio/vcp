# ADR-044: Controlled local process bootstrap

Date: 2026-09-23
Status: selected for P9-02; controlled stdio implementation in construction.

The TypeScript client cannot establish the Windows inherited-handle contract with
an ordinary child-process launch. A native `vcp local-bridge` helper therefore
owns launch and authentication. The client selects an explicitly trusted absolute
executable path. Workspace content cannot select the executable. Bootstrap
configuration travels in a bounded stdin frame; normal CLI argument parsing and
JSONL diagnostics never run on these protocol streams.

The bridge opens and holds its executable against write/delete replacement, then
uses `CreateProcessW` with an explicit application path, fixed `local-server`
argument, a small environment allowlist and `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`.
Only child stdin/stdout/stderr and a query/synchronize parent-process proof are
inherited. Both helpers clear inheritance on standard handles before proceeding;
the server also clears inheritance on the validated parent proof. No credential
or challenge appears in arguments, environment variables, discovery files or logs.

The bridge validates the actual created process handle against SID, token session,
creation time, image path and file identity. The server validates the inherited
parent proof and matching executable/user/session. A fresh random challenge passes
only through the private anonymous pipes. Decimal strings preserve 64-bit identity
fields across TypeScript. A separate child guard owns termination rights only for
the process it created; reconnect process handles have query/synchronize rights.

Authentication permits scoped observation and, for a controller-capable connection,
explicit controller commands. It does not acquire a lease or resume work. The
server uses an existing validated workspace selection and the real canonical writer
lock. Attachment does not prepare a provider, acquire credentials or start retained
execution. Wire parameters cannot supply actor, connection identity or ownership
tokens. Read authority refresh never refreshes an existing controller token.

Blocking anonymous-pipe I/O runs in dedicated helper threads with bounded queues.
Queue exhaustion, invalid UTF-8, truncated/oversized frames and output deadlines
close the connection. EOF is observed independently of an outstanding RPC so that
dropping its waiter invokes owned connection-loss handling. Normal bridge shutdown
closes server stdin and allows bounded canonical cleanup before child-only process
termination. A forced stop remains crash recovery, not proof of graceful completion.

The initial stdio server lives for its bridge connection. Named-pipe attachment and
reconnect will reuse held process identity and synchronous token impersonation,
with a server-instance credential delivered by this trusted bootstrap. A pipe name
or discovered PID alone will never authorize attachment. Persistent credential
discovery and remote endpoints are outside this decision. P9-02 acceptance still
requires named-pipe processes, live event recovery and the remaining execution
adapters; this construction decision does not mark those complete.
