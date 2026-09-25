# Original synthetic adapter contract
No actual SDK is installed by this fixture. Use JSDoc types and the supplied injected transport; no fetch or credentials. summarize(input, transport) accepts a nonempty string at most 1000 UTF-8 bytes, calls transport.send({provider,model,max_output_tokens,input}) once using provider.json, and returns {text,usage}. Response {ok:false,error:{code,message}} becomes an error retaining its code; missing usage remains null. This is project contract practice, not a claim of OpenRouter wire/SDK compatibility.

## Typed response contract, revision 2
Transport responses are JSON values. A success is exactly {ok:true,text:string}
with an optional usage property. Empty text is valid. Absent or null usage returns
null; otherwise usage is exactly {input_tokens,output_tokens}, both nonnegative
safe integers (zero is valid). Return the exact text and validated usage.
A failure is exactly {ok:false,error:{code,message}}, where code is a nonempty
string and message is a string. Throw an Error retaining both code and message.
Reject malformed responses, extra properties, nonstring text, invalid usage and
malformed failures with TypeError. Do not coerce values, retry or call another
transport. Invalid input also throws TypeError before the single transport call.
