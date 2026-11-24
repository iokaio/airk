# Local HTTP API

Local programs can use AIRK without implementing MCP or embedding Workbench.
The API invokes the same scoped read handlers and returns their complete
results. It does not expose service shutdown, installation, indexing mutations
or private management commands.

Since local trust plan C (23 November 2026) the service serves loopback over
**plain HTTP** on `127.0.0.1` only. There is no installation CA, leaf
certificate, issuer rotation or OS trust step. Earlier builds served
`https://127.0.0.1:<port>`; see [Upgrading from an HTTPS build](#upgrading-from-an-https-build).

> **No token (U110, 15 November 2026).** `/mcp` and the `/api/v1` tool routes
> take no credential: the gateway strips and ignores any `Authorization`
> header and serves every request as the built-in local agent session. Any
> local process of any OS user that can reach 127.0.0.1 can read every index.
> A read names its scope per call (`index_ids`, `index_set_ids` or
> `workspace`; see [MCP tools](mcp.md)). Local trust plan B removed enrollment,
> credentials, connection IDs, revocation and agent choice.

## Obtain the endpoint

`airkctl agents endpoint --output json` prints the endpoint record, and the
service publishes the same record in its owner-private control directory as
`http-1/endpoint.json`. Its `url` is `http://127.0.0.1:<port>/mcp`.
`airkctl agents connect` writes that URL (and the `X-AIRK-Response-Mode`
header) into an agent's configuration; nothing else is needed.

The preferred URL is `http://127.0.0.1:57421/mcp`. If that port is occupied,
AIRK keeps a newly allocated loopback socket and publishes its actual port.
Read the endpoint record instead of hard-coding the preferred port. After a
restart or upgrade, read it again: the service instance, endpoint generation
and port can change, and a session ID from the previous service does not
authorize a new session.

The endpoint uses the literal IPv4 loopback address. `localhost`, a LAN
address, another hostname or a redirect is not an interchangeable endpoint.

## What still guards the API

Plain HTTP does not widen who can read: the listener binds `127.0.0.1` only,
and the threats it guards against are web pages driving the API through a
browser.

- **Host.** The exact `Host` header `127.0.0.1:<port>` is required (DNS
  rebinding).
- **Origin.** A browser `Origin` other than the Admin page's own
  `http://127.0.0.1:<port>` is refused on every route; the bearer plane refuses
  browser origins outright (cross-site requests).
- **Admin.** The Admin pages keep their whole browser guard: the one-time
  launch code issued over the owner pipe (`airkctl admin open`), the session
  cookie `airk_admin` (`HttpOnly; SameSite=Strict; Path=/`), the anti-forgery
  token, the same-user peer check and the `Origin` check. The cookie has no
  `__Host-` prefix and no `Secure` attribute, because Safari keeps no `Secure`
  cookie on `http://127.0.0.1`. Cookies are not port-scoped, so another local
  HTTP service on 127.0.0.1 could read or overwrite it; `HttpOnly` and the
  server-side session binding limit that. A request presenting the Admin
  cookie on the bearer plane is refused (`cookie_on_bearer_plane`).

## Upgrading from an HTTPS build

An installation from a build that served loopback over HTTPS migrates
automatically:

- **Endpoint record.** The service reads the old `https://` endpoint record as
  a legacy predecessor, keeps its port and publishes the next generation over
  `http://`.
- **OS trust.** Setup's upgrade step (in the user's session, never the headless
  service, because macOS may ask) removes the AIRK CA the old setup installed,
  and only that one: the certificate named by the old ownership receipt
  `http-1/user-trust-1.json` in phase `installed`, matched by DER SHA-256, from
  CurrentUser\Root on Windows or the login keychain on macOS. Linux never had
  OS trust. A receipt in an interrupted phase (`prepared`, `remove_prepared`)
  is reported as residue and not acted on. Afterwards the old TLS files are
  deleted by exact name: `ca.pem`, `tls-identity.json`,
  `retired-private-key.json`, `user-trust-1.json`, `user-trust-1.lock`,
  `issuer-rotation.json`, `issuer-rotation.lock` and
  `rotation-user-trust-preimage.json`.
- **Agent configs.** An entry that still names `https://127.0.0.1:<port>/mcp`
  is recognised as AIRK's and listed as `https_endpoint_mismatch`; `airkctl
  agents repair` (or Reconnect in VS Code) rewrites it to the current
  `http://` URL and strips old `headersHelper` and bearer fields. Reconnect
  the external client afterwards.

## Routes

All routes use the same origin as the endpoint URL.

| Method and path | Input | Response |
| --- | --- | --- |
| `GET /api/v1/tools` | None | `airk.local-api/1` catalog with complete input schemas, canonical names and supported aliases |
| `POST /api/v1/tools/{name}` | `Content-Type: application/json`; body is the tool's argument object | Complete MCP tool-result JSON, including `content`, `structuredContent` and `isError` when present |
| `POST /mcp` | Standard Streamable HTTP MCP initialization/messages | MCP JSON or SSE according to the protocol |
| `GET /mcp` | Standard MCP session headers | Session SSE stream |
| `DELETE /mcp` | Standard MCP session headers | Closes that MCP session |

The raw JSON API requires no MCP initialization and accepts no `Mcp-Session-Id`.
Discover tool names from the catalog; unknown tools and mutation tools are
refused. Query parameters are unsupported. The request body is the argument
object itself, not a JSON-RPC envelope or an extra `arguments` wrapper.

The executable developer examples use standard runtimes without third-party
HTTP clients. Each reads the owner-private `http-1/endpoint.json` that the
service publishes in its control directory, refuses redirected, linked,
changed or non-private files, and connects over plain HTTP. Pass the absolute
service control directory. List the supported tools before selecting a read:

| Runtime | Catalog command | Read with a scope |
| --- | --- | --- |
| Python 3.11+ | `python examples/local_http_client.py --control-dir <absolute-control-dir>` | `... --tool search --arguments '<JSON-object>' --index <index-id-or-name>` |
| Node.js 24+ | `node examples/local_http_client.cjs --control-dir <absolute-control-dir>` | `... --tool search --arguments '<JSON-object>' --index <index-id-or-name>` |
| PowerShell 7.4+ on Windows | `pwsh -NoProfile -File examples/local_http_client.ps1 -ControlDir <absolute-control-dir>` | `... -Tool search -Arguments '<JSON-object>' -Index <index-id-or-name>` |

Read tools need a scope on every call: `--index`/`-Index` (sent as
`index_ids`) and `--set`/`-Set` (sent as `index_set_ids`) take ids or names and
repeat; `--workspace`/`-Workspace <absolute-folder>` reads the default scope
applied for that folder in the AIRK Explorer. A read with none is a
`scope_required` tool error. The catalog call takes no scope. `airkctl tools
call <tool> --index <id>` (or `--set`, `--workspace`) takes the same open read
path, so its result equals the example's.

The [Python](../examples/local_http_client.py) and
[Node](../examples/local_http_client.cjs) examples support Unix owner/mode and
file-identity checks. The [PowerShell example](../examples/local_http_client.ps1)
supports Windows protected-file reading. Unix PowerShell ownership is not
implemented; use Node/Python there.

Node and PowerShell also accept `--arguments-file <absolute-private-file>` and
`-ArgumentsFile <absolute-private-file>`, avoiding OS command-line length
limits; that JSON object is bounded to 64 KiB. The examples follow no
redirects, use no proxy and print complete JSON, including non-200 refusals and
tool errors. Node/PowerShell enforce an absolute 125-second request deadline;
Python's socket timeout is 125 seconds per blocking operation. The request and
response budgets are 64 KiB and 1 MiB.

For example, after reading the catalog and finding `search`:

```http
POST /api/v1/tools/search HTTP/1.1
Host: 127.0.0.1:<published-port>
Content-Type: application/json

{"query":"service ownership","index_set_ids":["<set-name>"],"limit":5,"response_mode":"full"}
```

The JSON API retains Full results. `X-AIRK-Response-Mode: terse` is rejected on
these routes. MCP defaults to Terse; MCP clients may request `full` or `terse`
through that header. A supported explicit tool argument takes precedence over
the header. Preferences are request-local.

An HTTP 200 tool result can contain `isError: true`; inspect the complete result.
Preserve source locations, revision/publication identity, verification,
readiness, coverage, continuations and retry details. Follow only the returned
continuations under the same scope and budgets; never silently widen an index
subset.

## Concurrency, limits and recovery

Connections and requests are asynchronous. Multiple VS Code windows, CLI
processes and local programs share native admission and the configured evidence
capacity. Changing concurrency applies to new admissions; it does not cancel
already accepted work.

The HTTP bounds include 128 TCP connections, 128 MCP sessions overall and
eight sessions per client context (every agent shares the one local agent
session context, so the eight-session bound applies across all agents). The
request header read times out after 15 seconds. Session initialization expires
after 15 seconds and idle sessions after 300 seconds. Request bodies are
bounded to 64 KiB and complete JSON replies to 1 MiB; native tool budgets can
impose smaller limits. Output accounting remains charged while network buffers
retain bytes, and every byte written to the socket passes the connection's
delivery fence: after a catalog fence, a request deadline, a cancellation or a
policy change, no further byte of that response reaches the socket. Oversized
output fails instead of becoming a truncated result. SSE replay through
`Last-Event-ID` is unsupported.

Admission and transport failures use non-200 HTTP statuses and an
`airk.local-api-error/1` body with `error.code`, for example `host_refused`,
`origin_refused`, `cookie_on_bearer_plane`, `query_admission_busy`,
`upgrade_in_progress`, `request_body_limit` and `request_deadline_exhausted`.
Other canonical tool failures remain complete tool results. Treat a
disconnected stream as incomplete, not as an empty result.

| HTTP status | `error.code` | Phase | Effect |
| --- | --- | --- | --- |
| 429 | `query_admission_busy` | Evidence admission | No work submitted |
| 503 | `native_validation_busy` | Bounded native scope validation | No work submitted; no session state changed |

These are complete refusals. Unknown codes, delivery fences and HTTP 200
JSON-RPC/tool errors are failures, even if a payload resembles a retry hint. An
incomplete exchange or an oversized response is uncertain and is never
replayed. A typed native pre-admission `selection_busy` can consume one
internal retry after a 50 ms wait within the original budget.

Native catalog changes invalidate outstanding delivery tickets. Open a fresh
connection and MCP session when instructed. During an upgrade, new evidence
admissions are closed while accepted work drains. Bootstrap and private
management continue through owner-checked Windows pipes or Unix sockets; no
HTTP shutdown endpoint exists.
