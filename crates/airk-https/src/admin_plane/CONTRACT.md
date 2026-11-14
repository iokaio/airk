# Admin plane module (Phase 5)

**Owner:** `airk-https`; the service injects its dispatch and authority hooks.

The service's `/admin` handler (split plan §7.1–7.3; [contracts](../../../../docs/split/contracts.md)
§2.7 (A1, A2) and §4 (I3) as amended by §4.3a, §4.11 and the adopted
[security review](../../../../docs/split/security-review-i3.md)). Every
admission, exchange, cookie, header and envelope decision is
[`admin_contract`](../../../airk-endpoint/src/admin_contract/CONTRACT.md)'s; this module supplies the
observations and carries out the admitted route.

## Interface

Applicability review (§5.1): the reviewed table has 85 rows, of which 79 admit
Admin (49 reads, 26 mutations, 4 destructive calls). Every admitted method
names a native ControlMethod or one of the four approved mappings below.
Destructive rows remain catalog.apply, catalog.split_apply,
recovery.plan_execute and storage.purge with their owning confirmation rules.
The six deliberately non-Admin rows remain indexes.watch_state,
indexes.resolve, explorer.handoff, evidence.native_read_scoped,
requests.prepare and agents.connection_defaults. Search/trust capability hooks
retain honest unavailable behavior when their native owner is absent; the
resident service supplies those hooks. This static applicability review and
injected-handler tests do not qualify every operation through a live listener
or establish read transport parity.

The Usage/Help pilot's [Rust-emitted semantic cases](../../../../fixtures/split/ui_pilot/README.md)
retain full I3 handler envelopes alongside I2 output. Read success retains the
complete native response in `result`; native Usage failure retains it in
`error.native`. Authentication remains native-held. Forbidden request fields and
browser project selection for Help are refused as whole packets before dispatch.
Injected-dispatch handler evidence does not establish live-listener qualification.

- `AdminPlane::new(dispatch, instance)` / `with_sources(…, clock, random)`:
  one per service instance, held by `service_runtime::State`. `Dispatch` is
  injected by `service_runtime` (its own control endpoint through
  `request_for_instance`), so this module never names `service_runtime`
  and adds no module cycle.
- `with_scope_check(ScopeCheck)`: the A2 catalog check, injected by
  `service_runtime` (`service_runtime::scope_grant::check`, the same check
  W4 uses, run on a blocking thread). Without it `search.select_scope`
  answers `not_yet_served`.
- `with_selection_authority(SelectionAuthority)`: the scoped-read
  admission gate's side of an A2 grant (selection authority spec §3.1,
  §3.3), injected by `service_runtime` when the catalog gate exists:
  `register` (`NativeCatalogAdmission::register_selection`) and `revoke`
  (`revoke_selection`, then `RevokeScope` to each affected worker on a
  blocking thread). Without it a grant holds no read authority.
- `AdminPlane::end()`: service end; every session ends and every grant's
  nonce is revoked.
  Native call admission can capture a private in-memory `SessionPermit` after
  validating actual cookie/CSRF facts and renewing idle lifetime once. Its
  pure live-bit/expiry check takes no mutex, so canonical actor/worker gates
  can recheck it without reversing the secrets-to-catalog lock order. Logout,
  expiry observation, eviction and owner drop invalidate every cloned permit;
  no permit extends the session's absolute expiry. Full session purging still
  runs through SecretsGuard so A2 revocation occurs after the secrets lock.
- `with_trust_inspect(TrustInspect)`: `https.trust_status`, injected by
  `service_runtime` (`http_endpoint::trust::saved_identity` then
  `trust::manage(Inspect)`, the callers `airk runtime http-trust --action
  inspect` used; read-only). Without it the row answers `not_yet_served`.
- `AdminPlane::launch_code(port)`: A1, served only on the owner pipe in
  `service_runtime::connection` (`admin_listener_unavailable` without a
  listener).
- `AdminPlane::handle(request, peer, port, draining)`: called by
  `http_gateway::Gateway::handle_on_connection` for every origin-form target
  `admin_contract::route` places in the admin plane, before the bearer
  dispatch. `/mcp` and `/api/v1` keep their admission; they only add
  `cookie_on_bearer_plane` after their Host and `Origin` checks.
- `ConnectionPeer`: the TCP 4-tuple and its cached same-user proof, attached
  to each request by the gateway's accept loop. Observers (H1): Windows
  `GetExtendedTcpTable` owning PID + token SID; Linux `/proc/net/tcp` uid;
  macOS the client socket (local = peer, foreign = listener) found among the
  TCP sockets of the service's effective uid (`proc_listpids(PROC_UID_ONLY)`,
  `proc_pidinfo(PROC_PIDLISTFDS)`, `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)`,
  bounded scan). On macOS a missing match is `Unavailable`, never
  `OtherUser`; either way the API answers `admin_peer_refused`.
- `refusal(code)`: an envelope refusal for the gateway.

## Owned state and invariants

- Launch codes: 256 random bits (`ring::rand::SystemRandom`), unpadded
  base64url; only SHA-256 of the text is stored; TTL 60 s on the injected
  clock; ≤ 8 outstanding, oldest evicted; one atomic compare-and-remove per
  exchange attempt.
- Sessions: in memory, ≤ 16 oldest evicted, idle 30 min (renewed only by an
  admitted `call`) and absolute 12 h, bound to the service instance; cookie
  and anti-forgery token are independent 256-bit values stored as SHA-256
  and compared in constant time.
- No `Debug` on any secret holder; secrets reach only the A1 reply, the
  exchange body and `Set-Cookie`.
- The operation table is `fixtures/split/i3/admin_operations.json`, read with
  `include_str!` and validated once by `validate_table`; a row is served only
  when its surfaces list `admin`, the body names the row's `method` (or the
  method its mapping names, below), and no never-over-HTTP item names it.
  Calls go through the injected `Dispatch` to this service's own control
  endpoint, never a second handler.
- **Mapping of table methods that name no `ControlMethod`**
  (`operations::MAPPINGS`, reviewed by
  `rows_without_a_control_method_follow_the_reviewed_mapping`; contracts
  §4.3a):

  | Row | Table `method` | Served by |
  |---|---|---|
  | `agents.connection` | `HttpConnectionInfo` | `AgentConnectionInfo` (M15) |
  | `agents.ownership` | `HttpConnectionOwnership` | `AgentConnectionOwnership` (M16) |
  | `logs.export` | `LogsExport` | `Logs` |
  | `https.trust_status` | `HttpTrust.Inspect` | the trust inspection above; body exactly `{"http_trust":{"action":"inspect"}}`; a failure is 409 `operation_failed` with `native: {message}` (the CLI's message, ≤ 4,096 characters) |

- **Request identities** (`mutations`; contracts §2.4, §4.3b, U98).
  - **R17 acknowledgment before dispatch:** effectful generate-mode rows
    (except native-journaled Enqueue/ConfigureProject) first return the Rust
    `MutationPreparation` result without sending the mutation. The browser
    retains it before an explicit same-operation `{prepared: handle}` call.
    Session custody stores the immutable native method/original key and claims
    it once under the session mutex. Concurrent/repeated submits never dispatch
    again. `{recover: handle}` observes cached native evidence or calls only
    the exact policy-resume, split-recovery or retry receipt read. Missing,
    expired, mismatched or still-unknown receipts remain nonretryable unknown
    outcomes with all native evidence preserved. Lifecycle, plan creation and
    journal preparation have no canonical receipt here and remain honestly
    unknown after a lost reply; status is never substituted for a receipt.
    Prepared handles are distinct from ordinary review authority, share one
    pinned slot in the existing 32-slot session budget, and end with the cookie
    or service. Known replies cache at most 65,536 encoded bytes per slot
    (32 MiB globally across 16 sessions); larger original replies are delivered
    in full, then `admin_prepared_reply_unavailable` discloses absent cached
    bytes without replay or truncation. Read previews, reviewed calls and
    natural mutations retain their existing protocol.
    Only an explicit same-operation `{acknowledged: handle}` after the client
    retains the full known final outcome releases pinned custody. Known
    refusal removes its slot; known success becomes evictable receipt-only
    custody, with the original mapping valid until actual eviction. Ready,
    Submitted and Unknown cannot be acknowledged. Every eviction/release
    removes the paired preparation/cache, keeping both maps within 32 slots;
    32 content-free acknowledgment tombstones bound safe lost-ack retries.
  - The browser never sends a request identity. Each row's reviewed
    `request_key` rule says where the caller and key live. A body that names
    a rule path is 400 `admin_malformed_request` before anything is sent.
  - **`generate`** rows: keyed mutations and the draft/preview reads. The
    handler reads `RequestEpoch` and draws the caller and nonce from the OS
    random source. Before dispatch, it reserves one `secrets::Review` slot
    (at most 32 including pending/recovery custody), evicting only the oldest
    ordinary preview token when necessary. A live reservation or receipt-only
    recovery identity is never evicted. If no slot can be made available,
    `admin_review_capacity` (409/refused/nonretryable) occurs before any
    mutation is sent. Normal success converts the slot to an ordinary token;
    an ordinary no-effect refusal releases it. The handler removes the exact
    key, and the caller beside it, before serializing any result or failure.
    R18 additionally enumerates native identities through their typed record
    owners (catalog/fence/source/setup/split/lifecycle), including historical
    and other-operation keys. Exact known identities and their `request_key`
    aliases leave opaque result data; unrelated public/unknown data remains
    complete. Only a caller beside the current expected key can become review
    custody. Unexpected private transport or Admin bridge responses refuse
    `admin_internal` before public serialization.
    A selection-key-only draft retains the native reply's caller, rather than
    its unused generated caller draw.
  - **`review`** rows: apply and receipt rows. The body is a `ReviewedCall`
    `{method, review, confirm_text?}`. The handler fills the identity the
    token holds. A missing, unknown or other-session token is 400
    `admin_review_unknown`.
  - Tokens are not consumed: a repeated apply carries the same key and the
    native key idempotency answers it. An R04 receipt-only token is different:
    it restores its identity only into the approved matching receipt read,
    never another receipt or a mutation. Pending tokens are not restorable.
  - **R04 unknown custody** (contracts §4.4.1): the approved policy-resume and
    split-recovery generated mutations retain their reserved original identity
    on a native unknown result or double-lost native reply. The failure is
    503/unavailable/nonretryable with an optional `review` receipt-only handle;
    no successful progress is claimed. A lost reply invents no native body.
    Existing reviewed setup/Make-readable applies retain their original token.
    Other generated rows gain no recovery-handle applicability from this work.
    Request/session/service lifetime and the existing budget remain unchanged.
  - **Interruption:** an RAII reservation guard releases canceled generate/read
    work. Once mutation dispatch starts, cancellation cannot establish no
    effects, so uncertain custody stays pinned until a known response settles
    it or the session ends. No lock is held across native work. Recovery of a
    failure envelope itself lost by the browser is not established by this
    session-only contract.
  - Receipt-only handles stay receipt-only and pinned through the existing
    session lifetime, including after a receipt establishes a known outcome.
    That receipt may resolve the UI's uncertainty; it never grants mutation
    authority. Thirty-two pinned identities refuse further generated issuance
    before effects. Reclaiming settled recovery slots earlier is a follow-up;
    this correction neither increases capacity nor silently discards custody.
  - **Keyed mutation methods** (`Enqueue`, `ConfigureProject`, `Retry`,
    `PlanCreate`, `CatalogPolicyResume`, `Lifecycle`, `CatalogSplitRecover`). `Enqueue` and
  `ConfigureProject` are journaled as `airkctl` journals them: M5
  `ClientRequestPrepare`, M7 `ClientRequestSubmit` by nonce, M8
  `ClientRequestForget` once a job is the answer (a refusal keeps the
  journal entry for `requests.submit`/`requests.forget`). The other keyed
  methods are key-idempotent in their own ledgers and are sent with their
  key. A lost reply is resent once with the same key or nonce, never a new
  one; a second loss answers 503 `service_request_outcome_unknown`
  (`retryable: false`; a journaled request stays in `requests.list`).
  Natural and digest-idempotent mutations are sent once.
- **`request` in the success envelope:** every successful mutation carries
  `request: {nonce, state}`. For keyed methods the nonce is the request
  key's nonce (M5's nonce when journaled); for `requests.submit` and
  `requests.forget` it is the named journal nonce; for the other natural and
  digest-idempotent mutations it is a server-generated nonce that names this
  admin request only (no journal entry, no key, not retryable by nonce).
  `state` is `accepted` for a job without an outcome or a
  `LifecycleAccepted`, otherwise `completed` for existing native successes.
  Refused/unknown outcomes carry no `request`. `mutations::outcome` exhaustively
  matches the typed `ControlResponse` before serialization; JSON status suffixes
  do not classify outcomes. Native catalog/source-resolution unknown variants
  and the reviewed unknown `Failure` codes are unavailable; source-change busy
  and typed refusals retain refusal semantics. Full native evidence is kept
  except the original session-owned caller/key fields, including restored
  apply/receipt identities and nested setup observations.
- **Lifecycle (U85):** `service.stop` and `service.restart` send the keyed
  `Lifecycle` method. The answer is the complete `LifecycleAccepted`
  record (state `accepted`); the service then drains and exits, which ends
  every admin session. The service cannot start itself again: a restart is
  completed by the client that starts the service (`airkctl service start`
  or the login start-up entry), and the page needs a new launch code from
  `airkctl admin open`. No note is added to the result, which stays the
  complete native record.
- **Destructive rows** (`confirm`; U98): `catalog.apply`,
  `catalog.split_apply` and `recovery.plan_execute` are served after the typed
  confirmation; `storage.purge` (U100) after typing the location's path as
  the inventory lists it.
  - The handler reads the target's current display name through `Dispatch`
    and compares `confirm_text` (≤ 256 bytes) exactly. A mismatch, a missing
    text, or a target the read no longer finds is 400
    `admin_confirmation_mismatch`, and nothing is sent.
  - Targets:
    - `catalog.apply`:
      - `delete_project`, `remove_membership`: the index set's name;
      - `retire_index`, `change_index_policy`, `begin_index_source_change`:
        the index name;
      - `revoke_grant`, `delete_context`: the id;
      - every other catalog operation needs no text.
    - `catalog.split_apply`: the original index's name.
    - `recovery.plan_execute`: the first 12 hex characters of the plan id.
  - The request is then sent once, and the native digest decides whether the
    reviewed draft is current.
- **Admin read grant (A2; §4.6, U77):** `search.select_scope` sends
  `{"admin_scope_grant":{"selection":{namespace, expected_catalog_revision,
  index_ids, project_ids}}}`. The selection is checked by the shared W4/A2
  catalog check (existing indexes only, the current catalog revision, every
  index a member of a selected project; refusal `scoped_read_unavailable`),
  then stored in the calling session (`secrets::Grant`) with the service
  instance; a failed check leaves the previous grant. The answer is
  `AdminScopeGranted { scope_identity, catalog_revision, expires_at_ms }`
  (the session's current expiry). Logout, expiry, eviction and restart drop
  it; a grant of another instance is never stored or used; the grant never
  reaches the browser or the pipe.
- **A2 read authority (selection authority §3.1, §3.3, §3.7, N4):** with a
  selection authority, each stored grant draws a fresh nonce (the first 16
  bytes of one OS random draw) and carries a `SelectionScopeRef { owner:
  Admin, grant_nonce, service_instance, scope_identity, catalog_revision }`
  (`secrets::Grant::selection_ref`). The nonce is registered live with the
  gate nested inside the plane's lock (owner, then gate); a refused
  registration answers 409 `scoped_read_unavailable` and stores nothing.
  Every path that drops a grant (replacement, logout, idle or absolute
  expiry found by any session operation, eviction of the oldest session, a
  store whose session ended meanwhile, service end) records its nonce in
  `secrets::Sessions`; the plane's lock guard (`SecretsGuard`) takes those
  nonces and revokes them after the lock is released, so no drop path can
  skip its revocation. The nonce never reaches the browser, the pipe or a
  log (`SelectionScopeRef`'s `Debug` redacts it).
- **Search (`search.call`)** runs only with `selection: "unselected"` (the
  help tool, through `Dispatch`) or `"admin"`. Any other selection is 400.
  Under `"admin"`: no grant → 409 `admin_scope_not_granted`; the grant is
  rechecked against the current catalog (changed → 409
  `scoped_read_unavailable`); a current grant's read runs through the
  injected `SelectionReader` (selection authority S2): the W4 selection read
  path in process, one index or the combined facade, with the session's
  `SelectionScopeRef`, whose nonce never reaches the answer (N3). On the
  pipe, `ToolSelection::Admin` is `admin_scope_not_granted` and
  `AdminScopeGrant` is `admin_session_required`.
- Embedded assets: `crates/airk-cli/admin-assets/` files listed in its
  `manifest.json`, sealed by `build.rs` (missing or stale bytes fail the
  build); lookup by exact name only. `/admin/` and `/admin/launch` serve the
  one shell document `index.html` (M1).
- Lock order: the plane's one `Mutex` is never held across an `.await`.

## Not served yet

- `/admin/api/v1/events` accepts the Rust `EventStreamRequest`, serves bounded
  NDJSON through the existing canonical `Events` read, and preserves complete
  native ServiceEvent data. Its opaque reconnect cursors stay session/service
  scoped. Native pages are 32 events, polled every 1,000 ms, draining `next`
  before advancing to `tail`. Existing limits are 2 streams per session,
  16 globally, 256 queued lines / 1 MiB, and 120 seconds. Stream reservations
  remain globally counted until body end/drop, including expired sessions;
  canceling a body aborts its read observer. Overflow, native gaps, namespace
  replacement, session expiry and logout/end are explicit resync reasons,
  followed by Closing. A retained-prefix gap moves to the native live tail
  after resync, without presenting incomplete retained history as complete.
  A stream does not renew session idle lifetime. Admin requests have a separate
  16-active-call admission budget (`admin_stream_limit` at capacity); session
  exchange permits 8 attempts per 60-second code lifetime before body/secret
  work (`admin_exchange_rate_limited`). These budgets never alter public-read
  plane admission.
- Peer proof qualification: the macOS own-socket observer is implemented
  but not yet qualified on a macOS host; other platforms (no observer) are
  refused with `admin_peer_refused` (fail closed, H1).

## T0

```console
cargo test -p airk-https --lib admin_plane --locked
cargo test -p airk-cli --test admin_plane --locked
```

Each Cargo selection must report nonzero selected tests. Record selected test
identities and outcomes; compatibility re-exports do not include dependency
unit tests. Platform-only cases require their native host.

## Optional Arbiter management (2026-11-14)

The additive management rows are arbiter.status/configure/provision/cancel/remove/recover. Their canonical body is `{ "arbiter": { "request": <Rust ManagementRequest> } }`, matched exhaustively on request.operation. Status is read-only; other rows are naturally revision-fenced mutations. Remove affects only optional private model assets, never indexed sources. Owner pipe or authenticated Admin session plus CSRF applies. Dispatch rechecks the pure admission callback at the configuration CAS or before accepting a background native job; the accepted job subsequently owns its ServiceLease/configuration/cancellation authority. ArbiterApplication and ArbiterNextRead remain native owner-pipe internals and never receive HTTP routes. No browser field creates worker scope or model qualification.
