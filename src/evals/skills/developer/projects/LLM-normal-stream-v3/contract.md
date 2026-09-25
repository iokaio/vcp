# Synthetic streaming contract
stream.cjs exports collect(chunks, signal). Chunks are Uint8Array fragments of UTF-8 newline-delimited JSON, not a provider wire-format claim. Events: {type:"delta",text}, {type:"done",usage}, {type:"error",code}. At most 4096 input bytes and 32 events. Return {text,status,usage,error}; status is completed, incomplete, cancelled or error. Text accumulates deltas once. Only done completes; absent usage is null. Abort signal must stop consumption. Preserve error code. Decode split UTF-8 bytes incrementally.

## Consumption and limit boundaries, revision 2
Check cancellation before the first read and between chunks. If abort occurs while
awaiting the next chunk, discard that chunk, do not request another, and return
cancelled with text from previously processed chunks. The tests trigger this
deterministically from the iterator; no timer or wall-clock responsiveness claim.
Exactly 4096 bytes and 32 events are accepted. Exceeding either bound must throw
or return status:error, even when all input is otherwise valid NDJSON. Do not
silently truncate or report completed. Terminal events are last in these fixtures;
malformed JSON and multiple terminal events are outside the declared test inputs.
