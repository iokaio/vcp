# CS-3 finite fixture contract and static server

`scripts/evals/cs-browser-contract.cjs` implements a prerequisite for CS-3, not a
browser adapter or executable web-testing qualification. It uses only Node's
standard library; it never launches a browser, subprocess or workspace script.

The version-1 contract fixes one literal `http://127.0.0.1:<port>` origin, an
application identity, an absolute source root, up to 32 explicitly mapped files,
up to 32 typed actions and a lifetime of at most 60 seconds. Actions describe
navigation, click, fill, fixed keys and text assertions; they are validated but
not executed. There is no script, shell, arbitrary CDP or redirect action.

Each mapping binds a portable source path, literal route, supported extension/MIME,
byte length and SHA-256. Files are limited to 64 KiB each and 256 KiB total.
Link checks, bounded file-descriptor reads, source identity/hash checks and snapshot
capture precede listening. Later requests serve captured bytes, not live paths.
This JavaScript capture is not a replacement for Windows held-path isolation or
a claim of protection against every adversarial filesystem race.

The server permits GET/HEAD only, exact Host and any supplied Origin, mapped
routes and `/__vcp_ready` with the declared application identity. Encoded paths,
traversal, queries, absolute request URLs and redirects are unsupported. No source
file is interpreted on the server. Occupied ports fail; the closure holds only
the newly created server and its sockets. Lifetime and request/connection/header
limits bound the owned server. There is no operation to close another server.

Actual local HTTP tests cover identity/readiness, immutable served bytes, route
and authority rejection, source hash/link rejection, occupied-port preservation,
idempotent owned shutdown and timeout with user-server survival. No browser or
model call is involved.

Still required: a broker-owned launcher, full runtime identity, browser profile
isolation, actual interaction assertions, pre-dispatch browser request policy,
redirect/worker/subresource coverage and owner-loss cleanup observed through the
native Job Object boundary. This server does not restrict outbound browser
traffic, claim OS network isolation, execute the declared actions or close CS-3.
