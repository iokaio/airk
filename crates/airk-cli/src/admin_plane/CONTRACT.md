# Admin plane module (Phase 5)

The service's `/admin` handler (split plan §7.1–7.3; [contracts](../../../../docs/split/contracts.md)
§2.7 (A1, A2) and §4 (I3) as amended by §4.3a, §4.11 and the adopted
[security review](../../../../docs/split/security-review-i3.md)). Every
admission, exchange, cookie, header and envelope decision is
[`admin_contract`](../admin_contract/CONTRACT.md)'s; this module supplies the
observations and carries out the admitted route.

## Interface

- `AdminPlane::new(dispatch, instance)` / `with_sources(…, clock, random)`:
  one per service instance, held by `service_runtime::State`. `Dispatch` is
  injected by `service_runtime` (its own control endpoint through
  `request_for_instance`), so this module never names `service_runtime`
  and adds no module cycle.
- `with_scope_check(ScopeCheck)`: the A2 catalog check, injected by
  `service_runtime` (`service_runtime::scope_grant::check`, the same check
  W4 uses, run on a blocking thread). Without it `search.select_scope`
  answers `not_yet_served`.
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
  | `storage.inventory`, `storage.purge` | `StorageInventory`, `StoragePurge` | `not_yet_served`: no service handler (Phase 7) |
  | `service.login_startup_status`, `…_install`, `…_uninstall` | `LoginStartup.*` | `not_yet_served`: login startup is managed offline by `airkctl` until Phase 7 (U75) |

- **Request identities** (`mutations`; contracts §2.4, §4.3b, U98).
  - The browser never sends a request identity. Each row's reviewed
    `request_key` rule says where the caller and key live. A body that names
    a rule path is 400 `admin_malformed_request` before anything is sent.
  - **`generate`** rows: keyed mutations and the draft/preview reads. The
    handler reads `RequestEpoch` and draws the caller and nonce from the OS
    random source. It removes the key, and the caller beside it, from the
    native result and keeps both in the session (`secrets::Review`, at most
    32, oldest dropped). The success envelope names the review token; a
    refusal issues none.
  - **`review`** rows: apply and receipt rows. The body is a `ReviewedCall`
    `{method, review, confirm_text?}`. The handler fills the identity the
    token holds. A missing, unknown or other-session token is 400
    `admin_review_unknown`.
  - Tokens are not consumed: a repeated apply carries the same key and the
    native key idempotency answers it.
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
  `LifecycleAccepted`, otherwise `completed`. Refusals carry no `request`.
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
  confirmation. `storage.purge` stays `not_yet_served` (no service handler).
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
- **Search (`search.call`)** runs only with `selection: "unselected"` (the
  help tool, through `Dispatch`) or `"admin"`. Any other selection is 400.
  Under `"admin"`: no grant → 409 `admin_scope_not_granted`; the grant is
  rechecked against the current catalog (changed → 409
  `scoped_read_unavailable`); a current grant answers 503 `not_yet_served`
  (below). On the pipe, `ToolSelection::Admin` is `admin_scope_not_granted`
  and `AdminScopeGrant` is `admin_session_required`.
- Embedded assets: `crates/airk-cli/admin-assets/` files listed in its
  `manifest.json`, sealed by `build.rs` (missing or stale bytes fail the
  build); lookup by exact name only. `/admin/` and `/admin/launch` serve the
  one shell document `index.html` (M1).
- Lock order: the plane's one `Mutex` is never held across an `.await`.

## Not served yet

- **Reads under an A2 grant (and W4).** Under the catalog gate every
  evidence read is admitted against a persisted, user-reviewed catalog grant
  (`GrantId`, `expected_grant_revision`, `UserReviewedProjectGrant`
  provenance; `combined_frontend::read_snapshot`,
  `catalog_coordinator::native_scoped_selection`,
  `catalog_authority::build_catalog_read_authority`); a project-binding read
  is refused there. An in-memory grant has no `GrantId`, so its read needs a
  new scoped-read authority kind in the kernel's admission — a shared
  contract change (kernel hooks, read descriptors) that is its own reviewed
  step under the C5 security review, not part of this module. Until then a
  current grant's `search.call` answers `not_yet_served`.
- Destructive rows (typed confirmation wire form undefined) and the Phase 7
  mapping rows above.
- `/admin/api/v1/events` answers `not_yet_served`.
- Peer proof qualification: the macOS own-socket observer is implemented
  but not yet qualified on a macOS host; other platforms (no observer) are
  refused with `admin_peer_refused` (fail closed, H1).

## T0

```console
cargo test -p airk-cli --lib admin_plane
cargo test -p airk-cli --test admin_plane
```
