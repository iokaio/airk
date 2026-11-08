# Local HTTPS API

This describes the **unpublished 1.8.14 implementation**. The [implementation
ledger](research/one-airk-implementation.md) records the remaining qualification
gates. Older AIRK versions do not expose these endpoints.

Local programs can use AIRK without implementing MCP or embedding Workbench.
The HTTPS API invokes the same scoped read handlers and returns their complete
results. It does not expose service shutdown, installation, grant creation,
indexing mutations or private management commands.

## Obtain a connection

Use AIRK's reviewed connection setup to select existing authorized indexes or
index sets. Native setup validates the grant and supplies a versioned connection
description with `endpoint.url`, `connection_id`, exact `selection`,
`ca_certificate_file` and `credential_file`. The native `runtime http-connection`
command accepts an owner-private selection file, an explicit connection ID and
client label. It returns file references, never the bearer secret itself.

For an already enrolled connection, `runtime http-connection-info` accepts the
verified installation directory and connection ID. It checks the protected
credential, expiry, revocation and current native grant, then returns connection
metadata without issuing a new credential or printing its secret. VS Code's
generated local MCP configuration uses the AIRK extension's
`airk.httpCredential` input command to resolve that credential; it does not
depend on adding a token to an already running editor's environment. This
configuration requires the AIRK extension and a local host that supports command
inputs. Reconnect the affected MCP server after changing its configuration.

The preferred URL is `https://127.0.0.1:57421/mcp`. If that port is occupied, AIRK
retains a newly allocated loopback socket and publishes its actual port. Read
the verified connection description instead of hard-coding the preferred port.
After a restart or upgrade, obtain current native connection metadata again;
the service instance, endpoint generation and port can change. A session ID from
the previous service does not authorize a new session.

The endpoint uses the literal IPv4 loopback address. `localhost`, a LAN address,
another hostname or a redirect is not an interchangeable endpoint. The exact
host and port are checked before credentials. Browser `Origin` requests are
rejected; the API is for local applications, not arbitrary webpages.

## Trust and authentication

The server certificate is signed by AIRK's installation-local CA. The CA is
self-signed and must be explicitly trusted by the client. Keep normal certificate
chain, validity and IP-address verification enabled. Add this CA to existing
trusted roots rather than disabling verification or replacing unrelated trust.
The CA is independent of macOS Developer ID signing credentials.

Read the bearer value from the protected `credential_file`. Send
`Authorization: Bearer <value>` on **every** request, including MCP session GET
and DELETE. Never put it in a URL, project configuration, command argument,
diagnostic log or shared file. Native setup binds it to one exact existing
authority. A session ID is not a substitute for the bearer or the grant.

The service checks the current credential ledger and native grant on every
request. Revoked or expired credentials fail. Repeating enrollment with the same
connection ID preserves its original token and expiry; a different authority or
an expired/revoked connection needs a new explicitly reviewed ID. A forward
catalog clock alone can be refreshed without changing the selected authority.

The leaf certificate renews under the existing local CA. Its Authority Key
Identifier names that CA's Subject Key Identifier so strict X.509 clients can
validate the chain. Startup repairs an older leaf missing this extension only
after authenticating the saved issuer key and original certificate; it preserves
the CA certificate, trust settings and scoped credentials. An invalid saved
issuer requires explicit replacement rather than silently changing trust.
Issuer expiry does not
silently authorize a new issuer. The explicit native rotation transaction below
stages and binds its replacement; stopped/expired-owner recovery must retain
verified native exclusion rather than fabricate a successful TLS health read.
VS Code host trust and actual platform rotation have separate qualification gates.

### Explicit issuer rotation

`runtime http-issuer-rotation --install-dir <verified-installation> --action status`
returns the complete native transaction, its receipt digest, the current endpoint
and the native-computed descriptor digest. A null transaction means no rotation
has been prepared. These are read-only observations, not permission to activate.
Never recompute a receipt digest from another language's JSON formatting.

| Action | Required review and effect |
| --- | --- |
| `prepare` | Supply `--operation-id <32hex>`, `--expected-ca-sha256 <old-CA>` and `--expected-descriptor-sha256 <native-digest>`. Stage a new issuer without activating it. |
| `trust-new` | Supply the same operation and `--expected-receipt-sha256 <current-native-digest>`. Install only the reviewed staged issuer through the owned OS-trust adapter. Unsupported/unknown outcomes remain explicit. |
| `commit` | Pin the current operation/receipt. Native coordination drains/stops and restarts the exact service with the replacement. Grants, credentials and selections do not widen. Requests can disconnect. |
| `finalize` | Pin the current operation/receipt after the native overlap expires. Remove only exact owned old trust/key material; an expired deadline alone never claims cleanup happened. |
| `abort` | Pin a prepared operation/receipt. Preserve the old issuer and remove only owned staged replacement effects. A transaction whose activation began cannot be aborted through this path. |

Ordinary overlap is 24 hours. Native `prepare --overlap-seconds` accepts a reviewed
60–86400-second value for explicit qualification; the extension uses the ordinary
default and offers no short preset. Keep the operation ID and observe its receipt
after an unknown result. Do not replay a mutation with a fresh operation ID.
Organization-managed payload policy currently refuses rotation before Prepare;
the operation does not override that ownership or change lifecycle mode. Explicit
User Stop remains authoritative. A user must explicitly clear that stop through
Start before preparing; an expired issuer can then use verified stopped-native
exclusion when a real TLS health read is unavailable. The receipt retains its
`preparation_basis`, exact old ownership and native stop proof instead of claiming
a successful old HTTPS connection.

Explorer Management exposes **Open HTTPS issuer rotation receipt** and **Review
HTTPS issuer rotation**. Every effect has its own fingerprint/operation review.
First stage old/new trust in this window's exact owned Codex/Claude bundle
references and, where needed, the OS user store; then activate and reconnect each
client. The immutable bundle updates compare preimages, retain unrelated roots
and comments, preserve original files and config scope, and never persist bearer
environment values. Changed/unowned references require review. Finalized or
aborted transactions allow explicit retry of this window's corresponding bundle
cleanup; partial publication retains its full observations instead of rolling
back foreign state or declaring success.

CLI and the existing Explorer management-command path expose the same complete
native status/receipt, including unknown fields; the Workbench opens that JSON
for copies/exports. These administrative actions are not added to the public
retrieval HTTPS tool catalog. Guarded recognized-config migration can update an
owned saved URL after native port relocation, but it does not reload a running
external host. Other windows, unmanaged trust and unobserved clients remain
pending actual reconnect/tool-discovery/scoped-read verification. Never turn
internal HTTPS or renderer/mock tests into a claim of completed client rotation.

The active trusted extension watches the control directory and endpoint
publication as untrusted hints, including while Explorer is closed. It coalesces
events and reads authenticated native Health before refreshing recognized owned
URLs with exact preimage checks. Healthy idle watches do not poll native state;
missing/error watches reattempt attachment at 30-second intervals, and transient
Health failures receive a finite safe-read retry budget. This does not start a stopped service,
re-enroll credentials or widen a saved selector. Closing the extension disposes
its watches and clients. Actual external clients still need their own reconnect.

## Routes

All routes use the same origin as `endpoint.url`.

| Method and path | Input | Response |
| --- | --- | --- |
| `GET /api/v1/tools` | Authorization header | `airk.local-api/1` catalog with complete input schemas, canonical names and supported aliases |
| `POST /api/v1/tools/{name}` | Authorization and `Content-Type: application/json`; body is the tool's argument object | Complete MCP tool-result JSON, including `content`, `structuredContent` and `isError` when present |
| `POST /mcp` | Standard Streamable HTTP MCP initialization/messages | MCP JSON or SSE according to the protocol |
| `GET /mcp` | Standard MCP session headers | Session SSE stream |
| `DELETE /mcp` | Standard MCP session headers | Closes that MCP session |

The raw JSON API requires no MCP initialization and accepts no `Mcp-Session-Id`.
Discover tool names from the catalog for the selected authority; combined reads
can have a different catalog. Unknown tools and mutation tools are refused. Query
parameters are unsupported. The request body is the argument object itself,
not a JSON-RPC envelope or an extra `arguments` wrapper.

The executable developer examples use standard runtimes without third-party
HTTP clients. Save fresh complete connection JSON from verified native setup in
an owner-private file outside the repository. The native setup/read-only getter
must have verified the referenced files' private ownership; arbitrary JSON is
not proof of authority. List the supported tools before selecting a read:

| Runtime | Catalog command |
| --- | --- |
| Python 3.11+ | `python examples/local_https_client.py --connection <absolute-private-file>` |
| Node.js 24+ | `node examples/local_https_client.cjs --connection <absolute-private-file>` |
| PowerShell 7.4+ on Windows | `pwsh -NoProfile -File examples/local_https_client.ps1 -Connection <absolute-private-file>` |

The [Python](../examples/local_https_client.py) and
[Node](../examples/local_https_client.cjs) examples support Unix owner/mode and
file-identity checks. The [PowerShell example](../examples/local_https_client.ps1)
currently supports Windows protected-file reading: exact current-user ownership,
no foreign allow entries, no redirects/hard links and bounded handle observations.
Unix PowerShell ownership is not implemented; use Node/Python there.

Add `--tool <advertised-name> --arguments '<JSON-object>'` for Python/Node, or
`-Tool <advertised-name> -Arguments '<JSON-object>'` for PowerShell. Node and
PowerShell also accept `--arguments-file <absolute-private-file>` and
`-ArgumentsFile <absolute-private-file>` respectively, avoiding OS command-line
length limits. That JSON object is bounded to 64 KiB and uses the same protected
file checks; it contains arguments, never a bearer. Do not supply both inline
and file arguments.

All examples add the local CA to normal trust, retain IP/name and certificate
validity checks, follow no redirects and print complete JSON, including non-200
refusals and tool errors. Node preserves its effective default roots, including
existing extra roots. PowerShell first accepts the normal platform chain; its
additional local-CA chain path preserves hostname checks and server-auth policy
without changing any trust store. Node/PowerShell enforce an absolute 125-second
request deadline; Python's socket timeout is 125 seconds per blocking operation.
The request and response budgets are 64 KiB and 1 MiB respectively.
Node/PowerShell disable proxies,
do not automatically retry requests and exit unsuccessfully on non-200 replies,
tool errors or incomplete/oversized data. Their generic transport errors exclude
the credential. These small read-only examples are not native setup, a service
manager, or proof that an external MCP application's trust is configured.

For example, after reading the catalog and finding `search`, a request has this
shape (use the actual advertised name and schema):

```http
POST /api/v1/tools/search HTTP/1.1
Host: 127.0.0.1:<published-port>
Authorization: Bearer <read-from-protected-file>
Content-Type: application/json

{"query":"service ownership","limit":5,"response_mode":"full"}
```

The JSON API retains Full results. `X-AIRK-Response-Mode: terse` is rejected on
these routes. MCP defaults to Terse; MCP clients may request `full` or `terse`
through that header. A supported explicit tool argument takes precedence over
the header. Preferences are request-local, so concurrent clients do not change
each other's response mode.

An HTTP 200 tool result can contain `isError: true`; inspect the complete result.
Preserve source locations, revision/publication identity, verification,
readiness, coverage, continuations and retry details. A rendered text excerpt
does not replace the structured result. Follow only the returned continuations
under the same scope and budgets; never silently widen an index subset.

## Concurrency, limits and recovery

Connections and requests are asynchronous. Multiple VS Code windows, CLI
processes and local programs share native admission and the configured evidence
capacity. Changing concurrency applies to new admissions; it does not cancel
already accepted work. Status has its existing reserved admission path.

The current HTTP bounds include 128 TCP connections, 32 concurrent TLS
handshakes, 128 MCP sessions overall and eight sessions per credential. Session
initialization expires after 15 seconds and idle sessions after 300 seconds.
Request bodies are bounded to 64 KiB and complete JSON replies to 1 MiB. Native
tool budgets can impose smaller limits. Output accounting remains charged while
network buffers retain bytes; oversized output fails instead of becoming a
successful truncated result. SSE replay through `Last-Event-ID` is unsupported.

Authentication, admission and transport failures use non-200 HTTP statuses and
an `airk.local-api-error/1` body with `error.code`. Examples include
`credential_required`, `credential_refused`, `native_grant_refused`,
`query_admission_busy`, `upgrade_in_progress`, `request_body_limit` and
`request_deadline_exhausted`. Other canonical tool failures remain complete tool
results. Treat a disconnected stream as incomplete, not as an empty result.

A typed native pre-admission `selection_busy` can consume one internal retry
after a 50 ms wait within the original budget. This does not replay work whose
execution is uncertain. Client cancellation stops delivery while already
accepted native work retains its capacity until actual completion or proven
worker retirement. Do not automatically replay arbitrary transport errors.

Native catalog changes invalidate outstanding delivery tickets. Open a fresh
connection and MCP session when instructed; the service revalidates the exact
authority. During an upgrade, new evidence admissions are closed while accepted
work drains. Bootstrap and private management continue through owner-checked
Windows pipes or Unix sockets; no HTTPS shutdown endpoint exists.
