# Synthetic MCP-shaped project
Protocol identity 2025-11-25; this fixture does not certify SDK compatibility. server.cjs exports async handle(message, state). Return JSON-RPC result/error objects preserving request id; notifications return null. Initialize before operations. initialize returns {protocolVersion:"2025-11-25",capabilities,serverInfo:{name,version}}: capabilities declares only the features this server implements, as empty objects, and serverInfo name and version are nonempty strings. Unknown methods use -32601 and invalid params -32602. The caller owns transport framing, so handle never writes to stdout or stderr. Never execute source content. No external SDK, registration, filesystem traversal or network.

## Explicit synthetic boundary API, revision 3
These fixture/* methods and state layout are local test APIs, not MCP standard methods.
The caller supplies state = {initialized:false,pending:new Map()}. Request ids are
nonempty strings of at most 64 UTF-8 bytes. A reply is exactly
{jsonrpc:"2.0",id,result} or {jsonrpc:"2.0",id,error:{code,message}}.
Errors preserve the request id. Tests check codes, except the fixed size error below.
Before initialize, ordinary requests error -32000 without changing pending state.
initialize accepts standard initialization params, marks initialized true, installs
state.flush and returns {protocolVersion:"2025-11-25",capabilities:{},serverInfo:{name,version}}
(fixture/* methods are neither tools nor resources). It never clears existing pending work.
notifications/initialized returns null. Unknown request methods error -32601.
Invalid params, extra params, unknown cursors and duplicate delayed ids error -32602.

fixture/list_labels accepts exactly {} or {cursor:"page-2"} or {cursor:"page-4"}.
Return {labels:[the original records in order],nextCursor:"page-2"} for the first two,
then nextCursor:"page-4" for the next two; the final single-record result omits nextCursor.
No numeric, null, empty, inferred or out-of-range cursor is accepted. Do not mutate labels.

fixture/delay accepts exactly {text:string}; text has at most 8192 UTF-8 bytes.
Store state.pending.set(request.id, text), return null, and do not complete it yet.
At most 16 requests may be pending; a seventeenth errors -32002 without changing pending.
state.flush() is synchronous: return an array of delayed replies in insertion order,
each with result {text:storedText}, then empty pending. A second flush returns [].
notifications/cancelled has no id and accepts exactly {requestId:string}.
Delete only the matching pending id and return null. Unknown or repeated cancellation
is a no-op. Malformed cancellation is also a no-op returning null. Cancel after flush
cannot retract any returned reply. No timers, promises representing pending work,
external resources or real wall-clock cancellation are involved.

fixture/echo accepts exactly {text:string} with at most 8192 UTF-8 bytes and returns
{text}. For every immediate or flushed reply, measure Buffer.byteLength(JSON.stringify(reply),"utf8").
4096 bytes is accepted; a larger reply is replaced BEFORE returning it with exactly
{jsonrpc:"2.0",id,error:{code:-32001,message:"Reply exceeds 4096 bytes"}}.
The original oversized result must not be returned or logged. The same rule applies
to non-ASCII and JSON-escaped text. Output size counts the complete envelope, not text alone.
