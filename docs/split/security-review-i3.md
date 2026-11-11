# I3 admin HTTP contract: security review

Status: Independent U38/U55 security review of contracts.md §4, 11 November 2026; specification review, no code.

Scope: [contracts](contracts.md) §4.1–§4.10 and [split plan](split-plan.md)
§7.1–§7.3 and §8.3, with decisions U5, U6, U7, U25, U38, U55 and U77. The
existing HTTPS listener was read to ground each finding:
`crates/airk-cli/src/http_gateway.rs`, `crates/airk-cli/src/http_endpoint*`,
`crates/airk-cli/src/service_runtime/http.rs`,
`crates/airk-mcp/src/remote/http.rs` and [local HTTPS API](../local-https-api.md).
No build, test or browser was run. Browser behaviour cited below is the
reviewer's understanding of current Chromium, Firefox and Safari releases and
must be confirmed by the real-browser check that §4.10 already requires.

Severity rule: high or critical means a remote web page or another local user
can perform a management mutation, read evidence or obtain a credential, or
the specification cannot be implemented safely as written.

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 2 |
| Medium | 5 |
| Low | 9 |
| Info | 6 |

Both high findings come from one assumption in §4.5 and §4.7: that the launch
code reaches only the user's browser, and that Host, `Origin` and
`Sec-Fetch-Site` show that a request came from that browser. Those headers are
set by browsers, but any local process can forge them. The listener admits
every loopback peer (`http_gateway.rs:217`), and the URL carrying the code
passes through process arguments and, in remote workspaces, through port
forwarding. A same-user peer proof (H1) and a ban on forwarding the admin plane
(H2) fix both without changing a U decision.

The question about cookies that are not port-scoped (§4.6) is answered in M2.
The anti-forgery token is enough to make a leaked cookie useless, provided the
five rules in M2 are added.

## Findings

| Id | Severity | Attack | Affected rule | Fix (spec text below) |
|---|---|---|---|---|
| H1 | High | Another local user reads the launch URL from a process command line while it is being opened: on Linux `/proc/<pid>/cmdline` is world-readable by default, and `ps` on macOS shows the arguments of other users' processes. The URL appears there for `xdg-open`, `gio`, `open` or a newly started browser. That user then sends the exchange POST from a non-browser client with forged `Host`, `Origin`, `Sec-Fetch-Site` and `Content-Type`, before or instead of the victim's browser. They receive the cookie and the anti-forgery token, which give management mutations and full evidence reads (U7). The same peer can use a leaked cookie together with a guessed or leaked token, because nothing ties a session to the OS user. | §4.5.1–3, §4.7 (exchange row: "code is the proof"), §4.6; listener admits every loopback peer (`http_gateway.rs:217`) | Add same-user peer admission to the session exchange and to every `/admin/api` request. Deliver the launch URL without placing it in any process argument where peer proof is not qualified. Fail closed. |
| H2 | High | In a remote workspace (SSH, WSL, container, tunnel), §4.5.7 sends the URL through the editor's `openExternal`. For a loopback URL, VS Code forwards the port to the client machine's loopback, usually on the same port number, so the exact `Host` check passes. Other users on the client machine can then reach the remote service's whole listener, and the forwarded connection reaches the service from the forwarder process, which runs as the victim. That defeats H1's peer proof. On a Linux client the URL is also passed to the browser in its arguments, so another client user can take the code and exchange it through the forward. Port auto-forwarding (`remote.autoForwardPorts`) can expose the listener even without the admin flow. | §4.5.7, §4.10 ("remote-workspace port forwarding"), plan §8.3 | The extension never opens, forwards or converts (`asExternalUri`) an admin URL from a remote extension host. `airkctl admin open` refuses to run without a local graphical session. Forwarding the admin plane is unsupported and documented as such. |
| M1 | Medium | The anti-forgery token is returned to the `/admin/launch` page, which then navigates to `/admin/` (§4.5.4), so the token is lost with the page's memory. As written the shell cannot make any API call. An implementer has to improvise: a token in the `/admin/#…` fragment, `localStorage`, a cookie, or a "get token" endpoint authenticated by the cookie. The last option makes the token derivable from the cookie, and M2 shows the cookie leaks to other servers on 127.0.0.1. Reloads and second tabs are also undefined. | §4.5.2, §4.5.4, §4.6 bullet 2 | One document handles both launch and shell. Specify where the token may be kept, and state that no endpoint ever returns it. |
| M2 | Medium | Cookies are scoped to the host and ignore the port. `__Host-airk_admin` is sent to every HTTPS server on 127.0.0.1 that the browser visits with a same-site or top-level request, and any such server, including one run by another local user or a development server running untrusted packages, can set a cookie with the same name (cookie tossing, fixation, logging the user out, cookie bombing). The token is enough only if the cookie is never sufficient on its own, the server never adopts a cookie value it did not mint, and responses do not reveal whether a leaked cookie is still live (§4.4 returns 401 or 403 depending on which check failed). | §4.6 review item, §4.4 status mapping | Answer the review item with five binding rules: an independent token, a fresh cookie minted on every exchange, exactly one cookie of that name, one uniform 401, and stored hashes that are never logged. |
| M3 | Medium | Indexed documents and code are attacker-controlled. "Rendered as text data, never as HTML" is one sentence and cannot be tested. The CSP already stops script execution, but injected markup in a Markdown view, link or export can still forge parts of the trusted interface (for example a fake instruction to run `airkctl agents connect …`), leak data through links, or produce an executable export. Bidirectional control characters in paths can spoof file names. | §4.8 ("Indexed evidence…"), plan §7.2 Content | Add Trusted Types to the CSP, ban HTML sinks, require a sanitizing Markdown pipeline with a scheme allowlist, export only as attachments, and show bidi and control characters visibly. Add fixtures. |
| M4 | Medium | §7.2 makes browser trust in the installation issuer a normal setup step on Windows (CurrentUser Root) and macOS (login keychain). The issuer is a 10-year CA with no name constraints (`tls.rs:517-521`: "NameConstraints are deferred"). Its private key sits in the service state bundle, whose location the user may choose (U39) and which may be copied to backups. Anyone who obtains that key can issue certificates for any domain that the user's browser trusts, which allows interception of all of the user's TLS traffic, not just AIRK's. | plan §7.2 "Certificate trust"; §4.9 (keys never over HTTP is correct but insufficient) | Split issuers carry critical name constraints limited to 127.0.0.1 and are qualified in each supported browser. Store the issuer key apart from the leaf key, and do not ask for trust until the constraints are qualified. |
| M5 | Medium | On Linux the browser does not trust the issuer, so users learn to click through certificate warnings for `127.0.0.1`. Bookmarks point at the preferred port 57421 (`service_http.rs:12`), which another local user can take before AIRK binds it; AIRK then moves to an ephemeral port (`http_endpoint.rs:67-79`). The squatter's page looks like AIRK, has a certificate the user clicks through, and can ask the user to paste the output of `airkctl admin url`. With H1 applied, a pasted code cannot be exchanged by the other user's process, and a POST from the squatter's page carries the wrong `Origin`. The remaining risk is that this protection depends on H1 alone (or on the redirect fallback where H1 is not qualified), and that users get used to treating any 127.0.0.1 certificate as AIRK. | §4.5, plan §7.1 ("bookmarks are not reliable"), plan §7.2 Linux trust | AIRK pages never ask for a code or URL. `admin url` warns on stderr. Before opening, `airkctl` checks the preferred port and prints the issuer fingerprint. Document how to set up NSS/Firefox trust per user on Linux. |
| L1 | Low | Path rules are a denylist (`..`, `%2e`, …). Mixed-case or double encoding, `;` parameters, NUL bytes and absolute-form request targets are not covered. Methods and `Upgrade` (WebSocket) are not specified, and a query string is refused only on API routes. Assets are looked up by exact name in memory, so traversal into the filesystem is not possible; the risk is router confusion between `/admin` and the existing pipeline. | §4.2 path rules; `http_gateway.rs:359-374` ordering | Replace the denylist with an allowlist grammar matched on raw bytes, branch on the `/admin` segment before the existing `Origin` refusal, return 405 for other methods, and refuse `Upgrade`. |
| L2 | Low | Answering 401 for an unknown cookie and 403 for a wrong anti-forgery token tells someone holding a leaked cookie that the session is still live. | §4.4 | Covered by the M2 rule for a uniform 401. |
| L3 | Low | The exchange endpoint needs no session, and the admin plane shares the 128 connections and 32 handshakes of `/mcp`. Another local user can flood it. Guessing a 256-bit code is infeasible, so the risk is availability only. | §4.5.3, §4.1 Limits | Rate-limit exchange attempts and admit admin requests through a separate bounded semaphore. |
| L4 | Low | Logout and invalidation are underspecified. The specification does not say how the cookie is cleared under `__Host-` rules, does not let the user end every session after a suspected leak, and does not say whether the 120-second event reconnects reset the 30-minute idle timer. If they do, an idle tab never expires. | §4.5.5, §4.6 | Specify the clearing `Set-Cookie`, add an owner-pipe method that ends all sessions, and state that event streams do not reset the idle timer. |
| L5 | Low | Other 127.0.0.1 servers can push the `Cookie` header past the 16 KiB header buffer (`http_gateway.rs:236`), which makes AIRK unusable. Hyper answers with a bare status, not the §4.4 envelope. | §4.4 "always the envelope" | Document cookie bombing as a denial of service only. Allow hyper's bare 431 or 400 as the documented exception to the envelope rule. |
| L6 | Low | "Admin cookie on `/mcp` or `/api/v1` → `cookie_on_bearer_plane`" is a new refusal on a bearer plane whose rules are meant to "stay byte-for-byte". The cookie name, the order of checks and any effect on existing clients that send unrelated cookies are not specified. | §4.6 bullet 3 | Refuse only the exact `__Host-airk_admin` name, after the existing Host and `Origin` checks, and record it as an I3 addition under U24. |
| L7 | Low | The header set lacks `X-Frame-Options` for older engines, and `form-action 'self'` allows injected forms to post to the same origin. They would fail the anti-forgery check, but there is no reason to allow them. | §4.8 | Add `X-Frame-Options: DENY` and `form-action 'none'`. Trusted Types are covered in M3. |
| L8 | Low | §7.3 lists "HTTPS issuer trust and rotation" on the Service page, but §4.9 forbids certificate trust over HTTP. Rotation stages new trust (`trust-new`). | plan §7.3 vs §4.9 | The page shows trust and rotation status and the exact `airkctl` command. Any step that changes trust has no HTTP route. |
| L9 | Low | The new secrets (launch code, cookie, anti-forgery token) are not listed in the redaction rules for logs, diagnostic reports, I2 output envelopes, extension output channels or receipts. | §2.7 (code only), plan §7.3 Logs | Extend redaction to all three secrets and to the `Cookie` and `X-AIRK-CSRF` request headers. |
| I1 | Info | A browser extension that can access `https://127.0.0.1/*` or all URLs can read the URL, page memory and responses. Such an extension has the same power as the user. | §4.10 | Document this as out of scope. |
| I2 | Info | Any process of the same user can call A1 and so open an admin session. Such a process can already read the owner-private state. | §2.7 | Document this. Sandboxed applications of the same user that can reach the owner pipe are outside the I1 claim. |
| I3 | Info | Browser history, session restore and history sync keep the original `#c=` URL after `replaceState`. The code is single-use, lasts 60 s and dies on restart, so the copy is harmless after consumption or expiry. | §4.5.1 | Keep the fragment. Reject the `?code=` alternative, because the code would then reach the request line, the server and proxy surfaces, and history. |
| I4 | Info | Browsers without Fetch Metadata, such as Safari before 16.4, fail closed on every API call. | §4.7 | Publish a supported-browser list. |
| I5 | Info | The existing pipeline refuses POST requests during an upgrade checkpoint (`http_gateway.rs:474-489`). The admin envelope lists only draining and `not_yet_served`. | §4.4 | Admin mutations return 503 `upgrade_in_progress` under the same checkpoint. |
| I6 | Info | Chromium's Local Network Access and Private Network Access prompts may block some cross-site probes, but they are not on every browser. | §4.7 | Nothing in the design relies on them. Keep it that way. |

## Recommended specification text (medium and above)

### H1 — Same-user peer admission and launch delivery

Add to §4.5 and §4.7, and to the §4.7 table as a column:

> **Peer proof.** Host, `Origin` and Fetch Metadata are browser-set and are
> forgeable by any local process; they defend only against web pages. Against
> other local users the admin plane requires a same-user peer proof. The
> session exchange and every `/admin/api/v1/*` request are admitted only when
> the service proves that the TCP peer socket for that connection belongs to
> the service's own OS user. Windows: the owning PID of the connection's
> 4-tuple from `GetExtendedTcpTable`, then that process's token user SID.
> Linux: the `uid` column of the 4-tuple's row in `/proc/net/tcp`. macOS:
> the 4-tuple is found among the sockets of the service user's own processes
> (`proc_listpids`/`proc_pidfdinfo`). The result is cached per TLS connection.
> If the proof is not obtained, the request is refused with 403
> `admin_peer_refused` and the same envelope. The decision is a pure function
> over an injected observer, so every platform's rules are tested on every
> host. Static GETs need no proof. `/mcp` and `/api/v1` are unchanged.
>
> **Launch delivery.** The launch URL never appears in a process argument,
> environment variable or file that another user can read. `airkctl admin
> open` writes an owner-only HTML redirect file (mode 0600 or an owner-only
> DACL, in the I4 owner-private runtime folder) that navigates to the launch
> URL, opens that file by path, and deletes it after 60 s or on its next run.
> A browser that cannot read the file (for example a confined Snap browser)
> is opened with the URL in its arguments only on a platform whose peer proof
> is qualified. On a platform where neither the redirect file nor the peer
> proof is qualified, `airkctl admin open` refuses with
> `admin_launch_unprotected`, and a user decision is recorded before that
> platform supports admin sign-in.
>
> **Navigation to the launch page.** `/admin/launch` also accepts
> `Sec-Fetch-Site: cross-site`, because opening the redirect file produces a
> navigation from an opaque origin. The page makes no data call and needs no
> cookie, so this is safe. All other static routes keep `none` or
> `same-origin`.

Fixtures: `launch_exchange.json` adds a forged-header exchange from a foreign
peer (refused), a same-user peer (accepted), an unavailable observer (refused)
and a foreign peer holding a valid cookie and token (refused).

### H2 — No forwarding of the admin plane

Replace §4.5.7 and add to plan §8.3:

> **Open AIRK Admin** runs `airkctl admin open` (as §7.2 already says), not
> `admin url`. The code never passes through extension memory, the editor's
> opener or its logs. In a remote extension host (`vscode.env.remoteName` is
> set) the command is not run. Explorer shows the exact `airkctl admin open`
> command to run on that host's own desktop. The extension never calls
> `openExternal` or `asExternalUri` with an admin URL or the service port.
>
> `airkctl admin open` refuses with `admin_no_local_desktop` when it is not
> in a local graphical session (Linux: no `DISPLAY` or `WAYLAND_DISPLAY`, or
> `SSH_CONNECTION` is set; Windows: a non-interactive or SSH logon session;
> macOS: no Aqua session). `airkctl admin url` is for manual use only. It
> prints the URL to stdout only when stdout is a terminal, and writes a
> warning to stderr that the URL is a single-use credential that must not be
> pasted into any page.
>
> Forwarding the AIRK port (SSH `-L`, editor port forwarding, WSL localhost
> forwarding to the host) is unsupported. The service is started outside the
> editor's process tree, and the documentation tells remote users to set the
> editor's `onAutoForward: ignore` for the AIRK port. The peer proof (H1)
> cannot detect forwarding, so this rule is the control.

### M1 — Where the anti-forgery token lives

Replace §4.5.2 and §4.5.4 and the second bullet of §4.6:

> `/admin/launch` serves the same shell document as `/admin/`. The shell reads
> and clears `#c=` with `history.replaceState(null, "", "/admin/")` before any
> other action, performs the exchange, and keeps the returned token in memory
> without navigating. The token is never placed in a URL, cookie,
> `localStorage`, IndexedDB or the DOM. It may be mirrored in `sessionStorage`
> under a key that includes `service_instance`, so that a reload of the same
> tab keeps working; it is removed at logout and on any 401. A new tab or
> window has no token and shows "Open with `airkctl admin open`". No endpoint
> returns the token or anything derived from it, other than the exchange
> response. The token is an independent 256-bit random value, not derived from
> the cookie.

### M2 — Cookies that ignore the port (closes the §4.6 review item)

Replace the §4.6 review item with:

> The cookie reaches every HTTPS server on 127.0.0.1 and can be overwritten by
> any of them. It is therefore treated as half of a credential:
> (1) every `/admin/api` request needs both the cookie and the independent
> anti-forgery token (M1), plus the H1 peer proof; (2) the exchange always
> mints a new cookie value, and a cookie presented with the exchange request
> is ignored, never adopted; (3) a request with zero or more than one
> `__Host-airk_admin` cookie, an unknown cookie, an expired session or a wrong
> token gets the same 401 `admin_session_invalid` and the same body (§4.4
> keeps 403 only for Host, `Origin`, Fetch Metadata and peer failures, which
> are checked first); (4) other cookies are ignored and never logged; (5) the
> service stores only SHA-256 of the cookie and token values, looks sessions up
> by that hash, compares tokens in constant time, and never logs or reports
> either value. With these rules a leaked or overwritten cookie gives no
> access; the worst result is a forced logout.

### M3 — Rendering untrusted evidence

Add to §4.8:

> CSP adds `require-trusted-types-for 'script'; trusted-types 'none'`. Shell
> and shared UI code never use `innerHTML`, `outerHTML`, `insertAdjacentHTML`,
> `document.write`, `dangerouslySetInnerHTML` or `srcdoc`. A lint rule enforces
> this in `packages/ui` and `packages/admin`. Evidence text, paths, symbols and
> snippets are rendered as text nodes. If Markdown is rendered at all, it goes
> through a parser with raw HTML disabled into a tree builder that permits
> only listed elements. Link targets must be `https:`, `http:` or an in-app
> route; every other scheme is shown as text. External links get
> `rel="noopener noreferrer"`. No remote image is loaded. Exports and copies
> are produced as `application/json`, `text/markdown` or `text/plain` blobs
> with a download filename, never as `text/html`. Bidirectional and C0/C1
> control characters in paths and identifiers are shown as visible escapes.
> Fixture `fixtures/split/i3/evidence_rendering.json` holds hostile documents
> (script tags, event-handler attributes, `javascript:` links, SVG, bidi
> overrides, dangling markup) together with their expected text rendering.

### M4 — Name constraints on the issuer

Add to plan §7.2 "Certificate trust" and as a qualification gate:

> New split issuers carry a critical NameConstraints extension that permits
> only iPAddress `127.0.0.1/32` and dNSName `invalid` and excludes iPAddress
> `::/0`. They keep `pathLenConstraint 0`. The issuer is qualified in every
> browser and client that 1.9.0 claims: a leaf for 127.0.0.1 is accepted and
> a leaf for any DNS name or other IP is rejected. Setup does not offer
> browser trust on a platform until that qualification passes there. The
> issuer private key is stored in its own owner-private file, separate from
> the leaf key and the served bundle, and is read only to renew or rotate.
> Uninstall removes the owned trust through its receipt (existing behaviour).
> An issuer from before the split is never given browser trust.

### M5 — Linux trust and port squatting

Add to §4.5 and plan §7.2:

> AIRK pages never ask the user to paste a code, URL or token; the
> documentation says so. Before opening the browser, `airkctl admin open`
> checks whether the preferred port 57421 is held by a listener whose
> certificate does not chain to this installation's issuer. If so, it warns
> that bookmarks to that port are not AIRK. On Linux it prints the issuer's
> SHA-256 fingerprint and the documented per-user NSS/Firefox import steps
> (manual, user-run). Linux qualification records whether `__Host-` cookies,
> `fetch` and streams work after a certificate click-through in each browser
> it claims.

## Spec items checked and found sound

- DNS rebinding: an exact single `Host` of `127.0.0.1:<port>` plus a leaf
  whose only SAN is 127.0.0.1 (`tls.rs:283`). A rebound name fails TLS
  validation, and if it gets past TLS it fails the `Host` check.
- Cross-origin `fetch`, `no-cors` and form POSTs: browsers send `Origin` on
  every POST, so an exact `Origin` match plus `Sec-Fetch-Site: same-origin`
  refuses them. The custom `X-AIRK-CSRF` header and the JSON content type
  force a CORS preflight, which receives 403 with no CORS headers.
  `SameSite=Strict` is a third layer, and pages on other 127.0.0.1 ports
  (`same-site`) are still refused by the `Origin` check.
- EventSource and WebSocket cannot set the anti-forgery header, and every API
  route is POST-only. L1 makes the `Upgrade` refusal explicit.
- Iframes and clickjacking: `frame-ancestors 'none'`. COOP `same-origin`
  separates opener handles. CORP `same-origin` and `nosniff` stop cross-site
  subresource reads and stop JSON responses from running as script.
- `script-src 'self'`: every other same-origin route (`/mcp`, `/api/v1`, API
  errors) returns JSON with `nosniff`, so none of them can serve as a script
  gadget.
- The code is carried in the fragment, which never appears in a request line,
  in `Referer` or in server logs. `Referrer-Policy: no-referrer` and
  `no-store` cover the launch page.
- The code is 256-bit, stored as SHA-256, lasts 60 s on an injected clock and
  is consumed by one atomic compare-and-remove. Unknown, expired, used and
  malformed codes get the same 401. Restart invalidates codes and sessions,
  because both live only in memory and are bound to `service_instance`.
- Unauthenticated first navigation loads only static files, and every API call
  returns 401. Section 4.10 already requires a separate test, and H1 adds the
  peer and foreign-header fixtures.
- Planes stay separate: `Authorization` on `/admin` is refused, the bearer
  plane keeps refusing every `Origin` (`http_gateway.rs:372-374`) before any
  credential is read, and the bearer is never accepted from a browser.
- The never-over-HTTP list (§4.9) covers the bearer, private keys,
  `ConnectionGrant`, `HttpEnroll`, A1, window leases, upgrade checkpoints,
  uninstall, trust changes and the raw relay. The operation table test (no
  route for a `never_over_http` row) enforces it.
- Assets are looked up by exact name in a sealed in-memory inventory, so
  traversal into the filesystem is impossible. L1 tightens how routes are
  matched.
- The admin read grant (U7, U77) is bound to the session and service
  instance, never sent to the browser, and revoked at logout, expiry and
  restart. It cannot change a window's or an agent's grant.
- Mutations use only POST plus the anti-forgery token, and destructive
  operations use the native draft, preview, digest-apply and typed
  confirmation flow. One handler per operation, so no logic is duplicated for
  the browser.
- After a port change on restart, the old origin's sessions are dead and the
  new origin starts with no session and needs a new code. The old cookie
  reaches the new port but is refused (M2 rule 3).
- Request and response bounds, body limits (exchange ≤ 1 KiB) and event
  stream caps are bounded and match the existing listener constants.

## Verdict

No high or critical finding remains if the recommended changes are applied.

H1 and H2 are fixed by adding the specification text above; neither changes a
U decision. H1 fails closed: a platform where neither the peer proof nor the
redirect-file delivery is qualified refuses admin sign-in, and supporting it
then requires a recorded user decision. Under U55, M1–M5 and the low findings
are applied to contracts.md §4 and plan §7 as specification fixes before C5
lands.
