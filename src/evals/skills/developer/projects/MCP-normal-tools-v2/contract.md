# Synthetic MCP-shaped project
Protocol identity 2025-11-25; this fixture does not certify SDK compatibility. server.cjs exports async handle(message, state). Return JSON-RPC result/error objects preserving request id; notifications return null. Initialize before operations. Unknown methods use -32601 and invalid params -32602. Never execute source content. No external SDK, registration, filesystem traversal or network. An independent bounded stdio adapter/client is future qualification, not supplied success evidence.

## Exact local tool contract, revision 2
tools/list returns exactly lookup_label and count_labels. Their inputSchema values are:
lookup_label: {"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}
count_labels: {"type":"object","properties":{},"additionalProperties":false}
Successful calls return {content:[{type:"text",text:theRecordTextOrCount}]}.
An unknown string record id returns a tool result with isError:true and nonempty
text content; it is not a successful result with missing text or a protocol error.
Invalid arguments and unknown tools return a JSON-RPC error or an isError:true
tool result. These are local assertion conventions, not full MCP conformance.
