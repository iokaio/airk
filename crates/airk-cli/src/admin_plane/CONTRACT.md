# Admin plane module (Phase 5)

The service's `/admin` handler (split plan §7.1–7.3; [contracts](../../../../docs/split/contracts.md)
§2.7 (A1) and §4 (I3) as amended by §4.11 and the adopted
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
- `AdminPlane::launch_code(port)`: A1, served only on the owner pipe in
  `service_runtime::connection` (`admin_listener_unavailable` without a
  listener).
- `AdminPlane::handle(request, peer, port, draining)`: called by
  `http_gateway::Gateway::handle_on_connection` for every origin-form target
  `admin_contract::route` places in the admin plane, before the bearer
  dispatch. `/mcp` and `/api/v1` keep their admission; they only add
  `cookie_on_bearer_plane` after their Host and `Origin` checks.
- `ConnectionPeer`: the TCP 4-tuple and its cached same-user proof, attached
  to each request by the gateway's accept loop.
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
  when its surfaces list `admin`, the decoded `ControlMethod` is the row's
  `method`, and no never-over-HTTP item names it. Calls go through the
  injected `Dispatch` to this service's own control endpoint, never a second
  handler.
- Embedded assets: `crates/airk-cli/admin-assets/` files listed in its
  `manifest.json`, sealed by `build.rs` (missing or stale bytes fail the
  build); lookup by exact name only. `/admin/` and `/admin/launch` serve the
  one shell document `index.html` (M1).
- Lock order: the plane's one `Mutex` is never held across an `.await`.

## Not served yet

- Mutations and destructive rows answer `not_yet_served` until the journaled
  admin request path (§2.4, server-generated keys) lands.
- `/admin/api/v1/events` answers `not_yet_served`.
- Peer proof: Windows (`GetExtendedTcpTable` + token SID) and Linux
  (`/proc/net/tcp`). macOS has no observer yet, so its API is refused with
  `admin_peer_refused` (fail closed, H1).

## T0

```console
cargo test -p airk-cli --lib admin_plane
cargo test -p airk-cli --test admin_plane
```
