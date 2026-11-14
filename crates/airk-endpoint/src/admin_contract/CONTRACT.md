# Admin contract module (I3)

**Owner:** `airk-endpoint`; `airk-cli` retains a public compatibility re-export.

Owner of the I3 admin HTTP contract between the service's `/admin` plane and
the browser adapter (split plan §7; [contracts](../../../../docs/split/contracts.md)
§4, amended by §4.11 and the adopted
[security review](../../../../docs/split/security-review-i3.md)). Contract step
C5: types, pure decisions and fixtures only. Its one consumer is the Phase 5
handler [`admin_plane`](../../../airk-https/src/admin_plane/CONTRACT.md).

## Interface

- **Constants**: envelope schema `airk-admin/1`, cookie `__Host-airk_admin`,
  header `x-airk-csrf`, 256-bit secrets as 43-character canonical base64url,
  code TTL 60 s, session idle 30 min / absolute 12 h / ≤ 16, bodies ≤ 1 KiB
  (exchange) and ≤ 64 KiB, responses ≤ 32 MiB, streams ≤ 2 per session and
  ≤ 16 total, 256 events / 1 MiB buffer, 120 s per stream.
- **Routes** (`route`): an allowlist on raw target bytes. Every target starting
  with `/admin` is the admin plane; only `[A-Za-z0-9/._-]` can match; exact
  routes plus `/admin/assets/<name>` and `/admin/api/v1/call/<operation>`.
  Anything else under the plane is 404; a target that is not origin-form is
  never admin-routed.
- **Admission** (`admit(&RequestFacts, Option<&SessionFacts>) -> Admission`):
  plane, Host (403), route (404), `Upgrade` (400), `OPTIONS` (403) and method
  (405), then `Origin`, Fetch Metadata and same-user peer proof (403), bearer
  (401 `admin_bearer_refused`), session and token (one uniform 401
  `admin_session_invalid`), content type (400) and declared size (413).
  Static GETs need no session or peer proof; `/admin/launch` alone also accepts
  `Sec-Fetch-Site: cross-site` (H1 redirect file). The exchange ignores
  presented cookies (M2). Only `call` renews the idle timer (L4).
- **Peer proof** (`peer_proof`): pure decision over the Windows SID, Linux
  `/proc/net/tcp` uid or macOS own-socket observation; missing → unavailable.
- **Launch exchange** (`decode_exchange`, `exchange`, `launch_url`): fragment
  delivery; every code failure is the same 401 `admin_launch_code_invalid`.
- **Cookies** (`admin_cookies`, `session_cookie`, `CLEAR_SESSION_COOKIE`,
  `bearer_plane_cookie`): exact-name counting; the bearer plane refuses only
  that name (403 `cookie_on_bearer_plane`, after its existing checks).
- **Headers** (`response_headers`): CSP with Trusted Types and
  `form-action 'none'`, `nosniff`, `no-referrer`, `X-Frame-Options: DENY`,
  COOP/CORP, Permissions-Policy, `no-store` except digest-named assets; no
  CORS header. Bare 400/431 from hyper are the only non-envelope bodies (L5).
- **Envelopes** (`Envelope`, `SuccessEnvelope`, `FailureEnvelope`,
  `StreamLine`, `AdminCode`, `ErrorClass`): status by class; native refusals
  keep their code with class `refused`. The reviewed unknown-outcome codes
  validate only as nonretryable `unavailable` failures (503).
- **Operation table** (`OperationRow`, `RequestKeyRule`, `KeyMode`,
  `Confirmation`, `NeverOverHttp`, `validate_table`): row schema and table
  rules; the reviewed table is the fixture. Since U98 (contracts §4.3b) a row
  may name where its request identity lives (`generate` or `review`), and
  every destructive row names its confirmation target.
  - Table rules: paths are 1–4 lowercase segments; `generate` is not allowed
    on an `operation_digest` row; `review` is allowed only on `read` and
    `operation_digest` rows; `confirmation` is present exactly on destructive
    rows.
- **Reviewed calls** (`ReviewedCall`, U98): the body of a review-mode or
  destructive call, `{method, review?, confirm_text?}`. The success envelope's
  optional `review` names the session-held identity; codes
  `admin_review_unknown` and `admin_confirmation_mismatch` (400). Limits:
  `MAX_CONFIRM_TEXT_BYTES` 256, `MAX_REVIEWS_PER_SESSION` 32.
- **Receipt-only recovery** (R04, contracts §4.4.1; narrow user-approved D8
  exception, 13 November 2026): `FailureEnvelope.review` is optional, canonical
  32-hex and allowed only on an unknown, nonretryable `unavailable` failure.
  It is not successful completion and introduces no failure `request` field.
  `recovery_receipt` names only the policy-resume and split-recovery receipt
  pairs pinned by the independent fixture. The handler must bind a retained
  identity to that matching read, session and service lifetime; never mutation,
  another receipt or replay. Reserve custody before effects within the existing
  32 reviews; `admin_review_capacity` is 409/`refused`, nonretryable when custody
  cannot be reserved. Handler custody and UI consumers land separately.

## Invariants

- **Events/rate completion (§5.1):** events POST accepts only
  `EventStreamRequest {cursor?}`. Its opaque cursor is retained in the current
  session and service, with at most the existing 256 event positions retained
  per session. Unknown/evicted cursor positions resynchronize; they never
  invent history. Poll the existing canonical `Events` with no project filter,
  page limit 32 (the owning native maximum), following `next` before `tail`.
  Poll every 1,000 ms, matching the existing I2 observer interval. Preserve full
  content-free native ServiceEvent values. Overflow, native gap, changed native
  namespace, session expiry and session end have distinct StreamResync values;
  emit Closing at termination. Existing 2/session, 16/service, 256-event/1-MiB
  queue and 120-second stream limits remain unchanged. Delivery does not extend
  session idle lifetime. Stream cancellation releases its reservation.
  Exchange admits at most the existing 8-code capacity per existing 60-second
  launch-code lifetime, counting attempts before body/secret work. Other Admin
  work uses a separate 16-active-request budget derived from session capacity.
  The public evidence plane has no new rate or admission policy.

- **R17 preparation, before effects:** a non-journaled keyed mutation's
  ordinary body prepares and validates its immutable native method and returns
  `MutationPreparation` in the success result; it sends no mutation. The client
  retains that acknowledgment before submitting `PreparedMutationCall` to the
  same operation. The opaque handle is bound to the exact operation, native
  method and original key in the current cookie/service lifetime. Submit
  atomically claims it once; concurrent/repeated submits never redispatch.
  `RecoverMutationCall` at that same operation observes the cached known answer
  or calls only the matching native receipt read with the original identity.
  A missing receipt is not permission to replay. Lifecycle has no canonical
  receipt and remains honestly unknown after a lost response. Preparation and
  uncertain custody share the existing 32-review budget, cannot be evicted,
  and end on logout, expiry, eviction or service restart. Foreign, expired,
  changed-operation and body-rebinding handles refuse before native dispatch.
  This applies only to effectful generate-mode rows. Journaled
  Enqueue/ConfigureProject keep their native journal flow; reviewed rows,
  natural mutations and preview/read rows keep their existing admission.
  Known native reply caching is limited to 65,536 actual encoded bytes per
  slot (at most 32 MiB across 32 slots × 16 sessions). Larger ordinary replies
  are delivered in full; custody retains the original immutable method/key
  without the reply body. Subsequent submission without a cached reply returns
  nonretryable `admin_prepared_reply_unavailable`; recovery still uses only the
  actual matching receipt, or reports that same honest unavailability.
  Explicit same-operation `AcknowledgedMutationCall {acknowledged}` is sent
  only after retaining the full known final outcome. Ready/Submitted/Unknown
  custody cannot be acknowledged. Known refusal releases its slot; known
  success becomes evictable receipt-only custody with its original mapping
  valid until actual eviction. Review release/eviction removes the paired
  preparation/cache. At most 32 content-free acknowledgment tombstones retain
  operation/token binding for safe lost-ack reply retries. Server knowledge
  alone never releases custody; an acknowledgment never replays a mutation.

- Leaf module: only serde, serde_json and schemars. It must not import any
  other module of this crate.
- Secrets (code, cookie, token) never appear in `Debug`; `RequestFacts` has no
  `Debug`. Redaction lists cover `authorization`, `cookie`, `x-airk-csrf` and
  the exchange fields `code` and `csrf` (L9).
- Decisions take injected observations and an injected clock; no I/O.
- Contract values change only with a reviewed fixture change. Fixture
  expectations are written by hand in the generator, never computed by it.
- R04's independently authored `fixtures/split/i3/native_outcomes.json` pins
  unknown native outcomes as nonretryable `unavailable` failures, busy as a
  refusal, and accepted/completed controls. Native bodies retain all evidence
  except the existing session-owned caller/key redaction. The HTTPS adapter
  repair consumes these cases separately. The user-approved failure-handle
  exception in contracts §4.4.1 changes no persisted state or other wire plane.
- Absent recovery handles keep existing failure bytes. Recovery reservations
  share the existing review budget and lifetime; this contract does not
  authorize evicting an outstanding recovery identity to send another mutation.

## Fixtures

`fixtures/split/i3/{admission_cases,launch_exchange,headers,envelopes,admin_operations}.json`,
generated by `python fixtures/split/i3/admission_cases.py [--check]`.
`packages/contracts-gen/i3.schema.json` is pinned by
`admin_contract_schema_matches_the_checked_in_copy` (rewritten only with
`AIRK_WRITE_CONTRACT_SCHEMA=1`); `python scripts/dev/contract_types.py i3
--check|--write` renders `i3.ts` beside it.
`fixtures/split/i3/recovery_reviews.json` independently pins initial receipt
applicability, capacity and unknown-code semantics; it is not derived from the
implementation under test.
`fixtures/split/i3/mutation_preparation.json` independently pins the R17
preparation/submit/recovery wire and refusal semantics.

## T0

```console
python fixtures/split/i3/admission_cases.py --check
cargo test -p airk-endpoint --lib admin_contract --locked
python scripts/dev/contract_types.py i3 --check
```

Each Cargo selection must report nonzero selected tests. Record selected test
identities and outcomes; compatibility re-exports do not include dependency
unit tests. Platform-only cases require their native host.

## Optional Arbiter management (2026-11-14)

The additive management rows are arbiter.status/configure/provision/cancel/remove/recover. Their canonical body is `{ "arbiter": { "request": <Rust ManagementRequest> } }`, matched exhaustively on request.operation. Status is read-only; other rows are naturally revision-fenced mutations. Remove affects only optional private model assets, never indexed sources. Owner pipe or authenticated Admin session plus CSRF applies. Dispatch rechecks the pure admission callback at the configuration CAS or before accepting a background native job; the accepted job subsequently owns its ServiceLease/configuration/cancellation authority. ArbiterApplication and ArbiterNextRead remain native owner-pipe internals and never receive HTTP routes. No browser field creates worker scope or model qualification.
