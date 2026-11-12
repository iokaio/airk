# Selection read authority: specification for the U38/U55 review

Status: draft written 12 November 2026 for the independent security review
(U38 as amended by U55). It is a written proposal, traced from source at
`160ae8de` (`feat/split`). No code, build or test was run for it, and it
authorizes no code. It makes the reads under the W4 window scope grant
(contracts §2.5) and the A2 admin scope grant (contracts §2.7 amendment)
servable. Today both answer `not_yet_served`.

Revision 2 (12 November 2026) adopts the smallest fixes for review findings
F1–F11. Revision 3 folds in the re-review's N1–N6; the re-review left no high
or critical finding. §8 maps each finding to the sections it changed. Q1 is decided
(§3.5); Q2–Q5 remain open for the user.

Marks: **[V]** verified in source at the cited line; **[U]** unverified or
inferred; **[P]** proposed by this document.

## 1. Problem and current flow

U77: selecting an existing index as a window's (or the admin session's) search
scope issues an in-memory read grant. It never creates or changes indexes,
index sets or agent grants. W4 and A2 already check and store such a grant
[V `service_runtime/scope_grant.rs` `decide`/`check`;
`service_runtime/window_plane.rs:257-282` `grant`;
`admin_plane/mod.rs:431-466` `grant`]. Reads under it are not served
[V `service_runtime.rs:588-593` W4 → `not_yet_served`;
`service_runtime/tools_call.rs:155` `Window` bails;
`admin_plane/mod.rs:401-428` `granted_read` → `NotYetServed`]. The reason is
that every scoped read is admitted against a persisted, user-reviewed catalog
grant identified by `GrantId`:

1. **M3 selection.** `tools_call::run` (`service_runtime/tools_call.rs:133`)
   resolves `ToolSelection::Catalog { request: ScopedConnectRequest }` with
   `service_frontend::resolve_scoped` (`service_frontend.rs:287`). That does a
   `CatalogDetail` of the index (`:312`), refreshes only the metadata clock and
   requires a non-retired, import-complete index [V].
2. **Client connect.** `project_client::scoped_tools` (`project_client.rs:1199`)
   builds a backend whose `Connection::connect_once_selected` (`:234`) asks
   `Health`, re-fences the catalog clock, then sends
   `ControlMethod::ConnectScoped { request }` (`:299`). The reply's
   `ConnectionGrant.scope` must name the same index, `grant_id` and grant
   revision (`:355-359`). The client also requires the service version to
   equal its own (`:268-271`) [V].
3. **Service admission.** `service_management.rs::connect` (`:323`) checks the
   gate's catalog clock, then calls
   `catalog_coordinator::native_scoped_selection` (`catalog_coordinator.rs:234`).
   That function loads the persisted `CatalogEntity::Grant { grant_id }` and
   requires it to be non-revoked, on the same host and at
   `expected_grant_revision`. It then returns `NativeCatalogAuthoritySelection`
   (`catalog_authority.rs:26`) carrying `grant_id`/`grant_revision` [V].
4. **Authority build.** `CatalogControlHandle::issue_scoped`
   (`project_supervisor/catalog.rs:498`) sends `Operation::Connect` to the
   supervisor actor, through `catalog_authority::build_catalog_read_authority`
   (`catalog_authority.rs:50`) and `validate_snapshot` (`:234`). These read the
   persisted `ReviewedGrant`, the index, the selected projects, memberships and
   sources at the exact catalog revision. They require each frozen membership's
   scope to be a subset of the current one, and source revision and policy to
   match. The result is `airk_kernel::scoped_reads::ScopedReadAuthority
   { grant: ReviewedGrant, provenance: GrantProvenance, sources, … }` (`:301`;
   kernel `scoped_reads.rs:58`) [V].
5. **Worker admission.** `catalog_admission.rs` reserves and installs the
   scope. `reserve_lease` (`:623`) builds `ScopedReadRef { grant_id:
   authority.grant.id, grant_revision, fingerprint }` (`:632-639`).
   `installed` is keyed by `(worker, GrantId, IndexId, fingerprint)` and
   `highwater` by `(worker, GrantId, IndexId)` (`:288-289`). Fences target
   `ScopeTarget::{Index, Grant, Membership, Project, LegacyPromotion}`
   (`:159-169`). The authority travels in `project_protocol::Command::
   InstallScope { authority: Box<ScopedReadAuthority> }` (`project_protocol.rs:
   34-60`) [V]. Whether that channel is in process or a child pipe today is
   [U] (the module says "private child control"; the supervisor speaks of
   "in-process owner" drains).
6. **Kernel restriction.** The worker applies `Kernel::with_scoped_reads`
   (kernel `scoped_reads.rs:509`). `SessionScope::attach` compiles a
   `CompiledReadScope` from the selected memberships' `ScopeAtom`s
   (`compile`, `:190`) and fences every reader. Each result is annotated with
   `scope_observation: ScopeObservation { provenance: GrantProvenanceKind,
   grant: ScopeGrantObservation { id, revision }, members, … }`
   (core `scoped_reads.rs:105-125`; `annotate_scoped_read`). The fingerprint is
   `framed_digest("airk/scoped-read-authority/1", [json(authority)])`
   (`:183-188`) and also binds cursors (`"airk/scoped-cursor/1"`, `:710`) [V].

Combined (several indexes) fans the same path out per index:
`combined_frontend::read_snapshot` (`combined_frontend.rs:104`) builds one
child `ScopedConnectRequest` per member (`:185`) under the one persisted grant
[V].

A W4/A2 grant has no `GrantId`, no `ReviewedGrant` and no provenance, so it
cannot enter steps 3–6 without a second authority source.

## 2. D8-style wire proposal: catalog-grant bytes unchanged [P]

Rule: every value produced for a catalog grant serializes byte-identically to
today. The new source appears only in new optional fields that are absent for
catalog grants, and its digests use new domain strings.

### 2.1 Core types (`airk_core`)

New in `airk_core::scoped_reads`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOwnerKind { Window, Admin }

/// In-memory selection grant identity. Never a bearer: it carries no lease,
/// cookie or session id, and possession of it admits nothing (§4). `Debug`
/// is hand-written and prints `grant_nonce` as `[REDACTED]` (F4).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionScopeRef {
    pub owner: SelectionOwnerKind,
    /// 128 random bits drawn by the service when the grant is stored; a new
    /// grant (re-select) gets a new nonce.
    pub grant_nonce: Incarnation,
    pub service_instance: Incarnation,
    pub scope_identity: Sha256Digest,      // scope_grant::decide's identity
    pub catalog_revision: CatalogRevision,
}
```

`ScopedReadRef` (core `service.rs:1726`) changes as follows:

```rust
pub struct ScopedReadRef {
    pub schema: u32,
    pub lease_generation: u64,
    pub index_id: IndexId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<GrantId>,                 // was GrantId
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_revision: Option<CatalogRevision>,   // was CatalogRevision
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SelectionScopeRef>,      // new
    pub fingerprint: Sha256Digest,
}
```

`validate` requires exactly one source: (`grant_id` and `grant_revision`
both `Some`, `selection` `None`) or (both `None`, `selection` `Some`). A
`Some` field serializes exactly as the bare value did, so catalog-grant bytes
do not change. Deserializing an old frame still works. A missing grant pair
now parses and is then refused by `validate` rather than by serde [P]. This
also affects `ConnectionGrant.scope` (`service.rs:1748-1760`) and the
`project_protocol` scope messages.

`ScopedReadRef` is also persisted (F1). `FenceTarget { worker, scope:
ScopedReadRef }` (core `catalog_fences.rs:87-90`) sits in `FencePlan.targets`
(`:152-158`). The plan is part of `FenceIntent` (`:342-353`) and
`FencedReceipt` (`:500-506`), and therefore of
`NativeIndexRetirementEvidence.retirement` (`:23-27`). Those are written as
JSON payloads to SQLite `index_operation_receipts` (airk-store
`registry/catalog/operations/fences.rs:138`, `:406-411`, `:608`) [V].

Rule [P]: a `ScopedReadRef` with `selection: Some` never enters a
`FenceTarget`. §3.3 revokes selection scopes in memory before a fence plan is
built and excludes them from its targets. Every persisted fence payload
therefore still holds only catalog-grant refs, whose bytes are unchanged, so
no persisted format changes. `FencePlan`/`FenceTarget` validation
(`catalog_fences.rs:181-188` validates each `target.scope`) additionally
refuses a target whose scope has `selection: Some`, so a violation fails
closed rather than being written.

No `ControlMethod` and no client-visible frame carries `SelectionScopeRef`
(F4). It exists in the owning plane and on the in-process admission call.
After that it is carried by `ConnectionGrant.scope`, which reaches the project
worker over the private host↔worker IPC: the connection writes the grant to
the worker endpoint with `local_ipc::write_message` (`project_client.rs:
362-369`) (N3). The worker also holds it in its `Scopes` ledger. Rule [P]: no
worker reply, `Accepted` frame, tool result body, failure body, journal entry
or log serializes a `ScopedReadRef` with `selection: Some`, or a
`SelectionScopeRef`. The only serialized carriers are `ConnectionGrant` and
the `project_protocol` scope commands on the private host↔worker channel.

`ScopeObservation` (core `scoped_reads.rs:105`) changes as follows:

```rust
pub struct ScopeObservation {
    pub schema: u32,
    pub qualifier: ScopeQualifier,
    pub fingerprint: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<GrantProvenanceKind>,      // was required
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<ScopeGrantObservation>,         // was required
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<ScopeSelectionObservation>, // new
    pub members: Vec<ScopeMemberObservation>,
}
pub struct ScopeSelectionObservation {
    pub owner: SelectionOwnerKind,
    pub scope_identity: Sha256Digest,
    pub catalog_revision: CatalogRevision,
}
```

`validate` requires (`provenance` and `grant`) xor `selection`.
`ScopeSelectionObservation` deliberately omits `grant_nonce` and
`service_instance`. `GrantProvenanceKind` is not extended, so no persisted
`GrantProvenance` value or its `kind()` mapping changes [P]. Reads require at
least one project (§3.5), so every member keeps non-empty `memberships`. The
existing `validate` rule (`:146-151`) applies unchanged, with no relaxation.

`query_scope::installed_body` (core `combined_reads/query_scope.rs:225-240`)
matches on `observation.provenance` and echoes `"grant"`. It gains a
selection branch: `mode` stays `frozen_single_index`, `"grant": null`, a new
`"selection"` key, and a reason text for window/admin selections. The
catalog-grant body is unchanged [P].

### 2.2 Kernel authority (`airk_kernel::scoped_reads`)

`ScopedReadAuthority` and its fingerprint domain stay unchanged. A sibling
type is added:

```rust
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionReadAuthority {
    pub schema: u32,                         // 1
    pub backing: ProjectBinding,
    pub index_id: IndexId,
    /// The selected projects that have a membership of `index_id` (§3.2
    /// step 3); never empty.
    pub project_ids: Vec<QueryProjectId>,
    pub selection: SelectionScopeRef,
    /// Current `Membership.scope` atoms of those projects for `index_id`.
    pub atoms: Vec<(QueryProjectId, ScopeAtom)>,
    pub sources: Vec<SourceDescriptor>,
    pub membership_revisions: Vec<(QueryProjectId, CatalogRevision)>,
    pub source_revision: CatalogRevision,
    pub policy_epoch: u64,
    pub policy_digest: Option<Sha256Digest>,
}
```

`SelectionReadAuthority::validate` mirrors `ScopedReadAuthority::validate`
(kernel `scoped_reads.rs:118-182`) (F6):
- `schema == 1`;
- `project_ids` non-empty, unique and at most `MAX_SELECTION`;
- every atom's project is in `project_ids`, and every project in
  `project_ids` has at least one atom and one `membership_revisions` entry
  (exactly one per project, no extras);
- every atom validates (`ScopeAtom::validate`);
- `sources` has unique ids and unique absolute `original_root`s (≤ 4096
  bytes), at most `MAX_SOURCE_ROOTS`, and its id set equals exactly the union
  of the atoms' `source_root_id`s;
- `policy_epoch == 0` exactly when `policy_digest` is `None`;
- `selection.catalog_revision` equals the revision the authority was built at;
- the encoded size is at most `MAX_SCOPED_AUTHORITY_BYTES` (48 KiB, `:25`).

Its fingerprint is `framed_digest(b"airk/scoped-read-authority/selection/1",
[json(self)])`. The domain differs from `airk/scoped-read-authority/1`, so a
selection fingerprint can never equal a catalog one for the same bytes.
Cursor identity keeps `airk/scoped-cursor/1` over whichever fingerprint is
installed, so cursors cannot cross sources [P].

Internally `SessionScope.authority` becomes a private enum
`Authority { Catalog(ScopedReadAuthority), Selection(SelectionReadAuthority) }`.
Every current reader of `session.authority.grant.memberships` goes through
the same per-enum accessors:
- `require_scoped_policy_binding` (`:333`);
- `policy_observation_has_full_scope`;
- `scoped_source_projection`;
- `scoped_source_attribution` (`:581`);
- `observation` (`:68`);
- `compile` (`:190`).

`compile` keeps producing one `CompiledReadScope`, so path, source and alias
checks, `covers_sources`, fences and `annotate_scoped_read` are the same
code [P]. A new public constructor `Kernel::with_selection_reads
(SelectionReadAuthority)` sits beside `with_scoped_reads`, which keeps its
signature. `project_protocol::Command::InstallScope`/`RenewScopeLease`
carry a `Box<InstallAuthority>` enum. Whether that is a wire change depends
on [U] in §1 step 5. Either way it is private host↔worker control, not a
client wire.

### 2.3 Fixtures and goldens

Byte-identical (must not be edited; a diff fails the step):
- `fixtures/split/parity/*.json`: 76 `scope_observation` occurrences on 36
  lines [V by grep]. All are expected to be catalog-grant reads [U], which the
  step's golden reproduction checks.
- `fixtures/split/i1/control_methods.json` groups: `tools_call` (all 7
  cases, including `catalog_selection_terse_empty_arguments` and
  `admin_selection`), `admin_scope_grant` (`grant`, `several_indexes`), and
  every other group.
- `fixtures/split/i1/window_plane.json`: all 18 cases. The case
  `scope_select_in_memory_grant` already shows the W4 grant and the M3
  `selection: {"window": …}` frame with an elided result. Serving W4 makes
  that case's replay reach the grant rather than `not_yet_served`.
- `fixtures/split/i2/*`, `fixtures/split/i1/request_recovery.json`,
  `admin_launch_code.json`.

New cases [P]. All are additive, and each is a checked-in, hand-reviewed
value:
- Core serde cases: a new `fixtures/split/i1/selection_authority.json` with a
  `ScopedReadRef` for each source, a `ScopeObservation` for each source,
  `SelectionScopeRef`, rejection cases (both sources present, neither,
  half a grant pair), and the selection fingerprint of one fixed
  `SelectionReadAuthority` (pins the new domain).
- A frozen catalog-grant `ScopedReadRef`/`ScopeObservation` byte case in the
  same file, copied from current output before the change (golden capture),
  proving the optional fields are absent.
- `window_plane.json`: no new case is needed for W4 itself. Read refusals
  under a grant (`scoped_read_unavailable` after a catalog revision moves) are
  [P] new cases `scope_read_stale_after_catalog_change` and
  `scope_read_after_release`.

## 3. Semantics [P]

### 3.1 Binding

- **W4:** the grant lives in `Window.grant` (`window_plane.rs:90`) and is
  reached only with `(window_id, lease)` through `locate` (`:431-446`: alive,
  then a constant-time lease compare). The plane belongs to one
  `service_instance`.
- **A2:** the grant lives in the admin `secrets::Session.grant`
  (`admin_plane/secrets.rs:126-133`). It is reached only with the session
  cookie, and only when `session.instance == grant.instance ==
  plane.instance` (`:256-262`).
- **Both:** at store time the service draws `grant_nonce` and builds
  `SelectionScopeRef { owner, grant_nonce, service_instance, scope_identity,
  catalog_revision }`. `WindowGrant` and `secrets::Grant` gain the
  `grant_nonce` field. In the same owner-plane critical section, the owner
  registers the nonce as live with the admission gate (§3.4); lock order is
  §3.7. The lease and cookie never leave their planes. M3 under
  `Window`/`Admin` resolves the stored grant in the owning plane, then passes
  only the `SelectionScopeRef` and the `WindowScopeSelection` to admission.

### 3.2 Read-time re-check and in-process connect

For each M3 call:
1. Resolve the grant from its owner:
   - W4: `window_unknown`, `window_lease_invalid` or
     `window_scope_not_granted`.
   - A2: `admin_scope_not_granted`. On the pipe, `ToolSelection::Admin` stays
     `admin_scope_not_granted` (`tools_call.rs:99`).

   Expiry is swept first (§3.3).
2. Run `scope_grant::check(selection)` outside every lock. The result must
   equal the stored `(scope_identity, catalog_revision)` exactly. `decide`
   requires the selection's exact revision, so any of these refuses with
   `scoped_read_unavailable`:
   - a catalog revision change;
   - a retired or removed index;
   - a removed project or membership.

   The grant is not refreshed automatically. The user re-selects, which W4/A2
   already define ("never re-granted automatically"). A selection with no
   `project_ids` refuses here with `scoped_read_unavailable` (§3.5).
3. For each `index_id` in the selection, the read's projects are the selected
   projects that have a current membership of that index with a non-empty
   `scope` (F7, N6). A membership with an empty scope grants no authority
   (`index_catalog.rs:281`), so its project is dropped from that index's
   subset. `decide` requires only that each index is a member of at least one
   selected project (`scope_grant.rs:86-106`), so the subset is computed at
   build time. An index whose subset is empty is refused with
   `scoped_read_unavailable`. A multi-index read refuses as a whole, as
   Combined does for an unavailable member.
4. Connect in process (F4). The M3 handler calls a new
   `Management::connect_selection` (beside `connect`,
   `service_management.rs:323`). That function calls
   `CatalogControlHandle::issue_selection` (beside `issue_scoped`,
   `project_supervisor/catalog.rs:498`) with a `SelectionConnectRequest
   { selection_ref, index_id, project_ids }`. It does not use
   `project_client`'s pipe connect (`Connection::connect_once_selected`,
   `project_client.rs:234-302`, reached through `scoped_backend`, `:1207`),
   and no `ControlMethod` is added.
   - Admission requires the gate's catalog clock to equal
     `selection_ref.catalog_revision` exactly. It never rolls the revision
     forward, unlike the client's catalog path (`project_client.rs:283-285`
     sets `expected_catalog_revision = clock.revision`). A newer clock refuses
     with `scoped_read_unavailable`.
   - The service builds `SelectionReadAuthority` at exactly that revision with
     the checks of `validate_snapshot` (`catalog_authority.rs:234`) minus the
     grant, plus these: every read project has a current membership of the
     index; `require_policy_ready` (`catalog_authority.rs:192`) passes;
     `source_import == Complete`; and every source is enabled and owned by the
     index.
   - `native_scoped_selection` is not used.
   - Delivery (N2): `ScopedConnectionDelivery`
     (`project_supervisor/catalog.rs:274-280`) today exposes its bytes only
     through the guarded pipe writer `deliver` (`:312`) and the handoffs
     (`:285-310`), each inside `gate.with_scope_issue`. A new
     `ScopedConnectionDelivery::take_in_process(self) -> Result<
     ConnectionGrant>` moves the grant out of `response`
     (`ControlResponse::Connection { grant, .. }`) inside
     `gate.with_scope_issue(issue, worker, scope, ..)`. The issue check and
     the live-nonce check (§3.4) therefore guard the hand-over exactly as
     they guard a pipe write. It refuses a delivery whose scope has no
     `selection`, so catalog deliveries keep the pipe-only path.
   - A `project_client::Backend` selection variant holds the
     `SelectionScopeRef`, the `WindowScopeSelection` and the read projects
     instead of `catalog_session`. Its connect and every reconnect call
     `connect_selection` → `take_in_process`, re-running the §3.2 checks.
     They never call `Connection::connect_scoped`/`connect_once_selected`.
     The grant then goes to the worker as in N3 (§2.1).
5. Failure bodies echo only the `WindowScopeSelection` and the
   `scope_identity` (F4). A selection-path analogue of
   `airk_mcp::remote::scoped_setup_failure` (`remote.rs:101-108`, which
   echoes the whole `ScopedConnectRequest` as `requested_scope.selection`)
   puts `{"window_scope_selection": …, "scope_identity": …}` in
   `requested_scope`. It never includes `grant_nonce`, the lease, the cookie
   or `service_instance`.

Refusal codes: `window_scope_not_granted`, `admin_scope_not_granted`,
`window_unknown`, `window_lease_invalid` (owner absent);
`scoped_read_unavailable` (stale, unknown, retired, not ready, no project,
outside the selection, nonce not live); `service_restarted` for a session
whose instance changed (contracts §3.7). All of these codes exist already;
none is new.

### 3.3 Revocation: a non-fence path (F1, F3, F9)

**Owner revocation.** Selection revocation never uses the fence machinery and
persists nothing. Each owner event calls `NativeCatalogAdmission::
revoke_selection(nonces)`, which does the following:
- Under the gate lock: remove each nonce from the live set (§3.4) and mark
  every installed entry with that nonce `Phase::Revoked`. Do not bump
  `generation`, and write nothing to the registry.
- Outside the gate lock: send `RevokeScope { identity, scope }` to each
  affected worker (the existing worker message, `project_protocol.rs:47-50`).
  On its acknowledgement, prune the entry in both the gate and the worker
  (§3.4).

The exchange (N1): the existing `Expected::Revoke { ticket: FenceTicket,
target }` (`project_supervisor/catalog.rs:638-659`) and `record_revoked`
(`catalog_admission.rs:1644-1683`) cannot be reused. `record_revoked` requires
`state.fence(ticket)` and `pending.targets.contains(&target)` (`:1652-1655`),
and owner revocation has no fence. A new `Expected::RevokeSelection { worker,
scope }` carries no `FenceTicket`, and its acknowledgement calls a new
`NativeCatalogAdmission::record_selection_revoked(worker, scope)`. That
function requires a `Selection` key and a `Revoked` entry for that worker,
records the acknowledgement, prunes the entry and, if a pending fence lists
the pair (below), marks it acknowledged there.

Sweeps and purges return their dropped nonces (N4). `WindowPlane::sweep`
(`window_plane.rs:210`), `Sessions::purge` (`secrets.rs:274-276`) and the
oldest-session eviction in `Sessions::create` (`secrets.rs:170-176`, `while
self.sessions.len() >= MAX_SESSIONS`) return the nonces of the grants they
drop as a `#[must_use]` value. The caller passes them to `revoke_selection`
after releasing the owner lock (§3.7). Close, release, replace and logout
return their nonce the same way.

| Owner event | Nonces revoked |
|---|---|
| W3 close; W4b release; a new W4 (replaces) | That window's nonce |
| Window lease expiry | Swept lazily: `WindowPlane::handle` sweeps on every window-plane method (`window_plane.rs:134-152`). This spec adds a sweep at M3 `Window` dispatch, and each sweep revokes the nonces of the windows it drops |
| Admin logout (`Sessions::remove`), a new A2 | That session's nonce |
| Admin idle/absolute expiry and eviction | Lazy: `Sessions::purge` (`secrets.rs:274-276`) runs on session operations, including `grant` and `set_grant`. Each purge revokes the nonces of the sessions it drops |
| Service restart | All W/A state, gate state and in-process installations are lost; a ref naming the old instance is refused |

Because expiry is lazy, an expired owner's scope can stay installed until the
next sweep or purge. It is still unusable: every read re-resolves the owner
first (§3.2 step 1), and admission requires the nonce to be live (§3.4).

The F2/F3 live check closes the race between close and install.
`reserve_lease`, `acknowledge_install` and `with_scope_issue` each require the
nonce to be live under the gate lock. Revocation removes it under the same
lock. Any interleaving therefore either installs before the revocation, and is
then revoked by it, or is refused after it.

**Catalog fences (F1, N1, N5).** Existing `ScopeTarget::{Index, Membership,
Project}` fences apply to selection scopes, but those scopes never become
durable targets. `PendingFence` (`catalog_admission.rs:233-246`) gains an
in-memory field `local_selection_revocations: Vec<(WorkerIdentity,
ScopedReadRef)>`, which is never part of `durable: StoreFencePlan` and never
persisted. In `close_admission_with_census` (`:879`):
1. The target list is built as today (`:915-927` collects installed
   non-revoked scopes matching the target). Selection entries are filtered
   out of `targets` and collected separately.
2. Local revocation happens only after the last refusal check (N5). Those
   checks are:
   - the namespace and revision check (`:908-911`);
   - `retention_budget` on the candidate and on the durable plan
     (`:948`, `:964`);
   - the `MAX_PLAN_BYTES` bound (`:959-960`);
   - the `FenceKey` check (`:961-963`).

   Immediately before `state.pending = Some(PendingFence { .. })` (`:971`),
   each collected selection entry has its nonce removed from the live set and
   is marked `Revoked`. It is recorded in `local_selection_revocations`.
   A refused close therefore revokes nothing.
3. The plan (and so `FenceIntent`/`FencedReceipt` in
   `index_operation_receipts`) contains only catalog-grant targets.
4. Outside the lock, `RevokeScope` is sent for each listed pair through
   `Expected::RevokeSelection`. `record_selection_revoked` marks the pair
   acknowledged. `observe_worker_reaped` (`:1710`) also marks every listed
   pair of the reaped worker as satisfied.
5. `require_commit_ready` (`:1685-1698`) additionally requires every pair in
   `local_selection_revocations` to be acknowledged or reaped. Until then the
   fence cannot commit, exactly as an unconfirmed catalog target blocks it
   today (`pending.targets.iter().all(|target| pending.confirmed(target))`).

The owning plane learns of a fence revocation lazily. Its next read finds the
nonce not live and answers `scoped_read_unavailable`; the user re-selects.

Why excluding selection scopes from durable targets is safe:
- The `ConnectionGrant` holder for a selection scope is the service's own M3
  handler, in process (§3.2 step 4).
- A selection scope is meaningful only for its `service_instance`.
- A service crash or restart drops the nonce set, the gate and every
  in-process owner. A child-host worker [U: whether its job object or owner
  watch ends it with the service on every platform] cannot be reached with a
  selection scope after restart, because a new gate has no live nonce and
  refuses its ref.

What `restore_pending_fence` sees (`catalog_admission.rs:1860-1990`): it runs
only on an empty gate (`installed`, `workers` and `retired_workers` empty,
`:1864-1868`) and rebuilds `highwater`/`installed` from persisted plan
targets keyed by `grant_id` (`:1918-1950`). Those targets contain catalog
grants only, so it sees no selection scope. The `PendingFence` it builds
(`:1993`) starts with `local_selection_revocations` empty, because a
restarted gate has no selection scopes. Its only change is that
`target.scope.grant_id` becomes `Option` (§2.1). For a persisted target, a
`None` there is refused (`ScopeReadRefused`).

### 3.4 Admission keying, live nonces and bounds (F2)

`catalog_admission.rs` replaces `GrantId` in its in-memory keys with a
private `AuthorityKey` [P]:

```rust
enum AuthorityKey { Grant(GrantId), Selection(Incarnation /* grant_nonce */) }
installed: BTreeMap<(Incarnation, AuthorityKey, IndexId, Sha256Digest), Installed>
highwater: BTreeMap<(Incarnation, AuthorityKey, IndexId), CatalogRevision>
live_selections: BTreeMap<Incarnation /* nonce */, SelectionOwnerKind>
```

These maps are in memory [V `:288-290`, built in `new` `:417-419`].

- **Live set.** The owning plane registers a nonce on store and removes it on
  revoke (§3.3). `reserve_lease` (`:623`), `acknowledge_install` (`:770`) and
  `with_scope_issue` (`:817`) refuse a `Selection` key whose nonce is not
  live.
- **Pruning.** Unlike catalog entries, a selection entry that is `Revoked`
  and acknowledged is removed from `installed` and `highwater` in the gate.
  In the worker's `Scopes` (`project_worker/scopes.rs:48-53`, tombstones kept
  until exit) it is removed on the same acknowledgement. A nonce is never
  reused and is no longer live, so the entry cannot be resurrected; a
  resurrection attempt fails the live check. Catalog-grant entries keep the
  never-evict rule unchanged. The existing test
  `bounded_install_ledger_never_evicts_expired_or_revoked_authority`
  (`catalog_admission/tests.rs:649`) stays as it is and gains a sibling test
  for selection pruning.
- **Separate caps (N6).** Selection entries do not count toward
  `MAX_SCOPES` (128, gate `:34` checked at `:724`; worker `scopes.rs:16`).
  The admin session limit is `MAX_SESSIONS = 16`
  (`admin_contract/mod.rs:36`, enforced by eviction in `Sessions::create`,
  `secrets.rs:176`). Live owners are therefore at most 64 windows + 16
  sessions = 80.
  - Gate cap: `MAX_SELECTION_SCOPES = 80 × MAX_SELECTION`. Each live nonce
    has at most one entry per selected index.
  - Worker cap: one worker serves one backing, so it holds at most one live
    selection entry per live nonce. The worker's selection cap is 80 live
    entries plus at most 80 revoked-but-unacknowledged entries. When the
    revoked bound is reached, a new selection install on that worker is
    refused (`scoped_read_unavailable`) until acknowledgements prune entries.

  Catalog-grant capacity therefore cannot be exhausted by selection reads,
  and the reverse holds too.
- **High-water.** For `Grant` it is the grant revision, as today. For
  `Selection` it is the nonce's `catalog_revision` (one value per nonce).
- **Fence matching.** `ScopeTarget::applies` matches Index, Membership and
  Project targets against selection entries for local revocation (§3.3).
  `ScopeTarget::Grant` never matches them. No `ScopeTarget::Selection`
  variant is added, because owner revocation is not a fence.
- **Leases.** Lease generations, expiry, `MAX_LIFETIME` and the 300 s cap
  (`service_management.rs:338`) apply. A lease is renewed only after a fresh
  §3.2 re-check.

### 3.5 Read restriction (Q1 decided: at least one project, F5)

- Reads require at least one selected project. This fails closed, consistent
  with `Membership.scope`'s "Empty means no authority, never all sources"
  (core `index_catalog.rs:281`) and with the catalog path, which requires a
  non-empty `project_ids` (`ScopedConnectRequest::validate`,
  `service.rs:48-52`).
- Grant time is unchanged. `decide` accepts `project_ids: []`
  (`scope_grant.rs`, test `a_member_index_of_the_current_catalog_is_granted`
  grants `selection(&[1], &[])`), and W4/A2 keep granting such a selection,
  so the existing W4/A2 tests and fixtures stay valid. A read under it
  answers `scoped_read_unavailable` at §3.2 step 2, before any connect. This
  was chosen over refusing at grant time because it leaves `decide`, its
  tests and the `admin_scope_grant`/`window_plane.json` grant cases untouched.
- `atoms` are the current `Membership.scope` atoms of each read project (§3.2
  step 3) for that index, read at the exact catalog revision. This is the
  same source the catalog path freezes into `ReviewedGrant.memberships`.
- `SessionScope::compile` turns atoms into `CompiledReadScope` exactly as for
  catalog grants. Results, coverage, continuation and error details therefore
  come from the same handlers and differ only in `scope_observation` (§2.1)
  and the selection-specific bodies listed in §5.

### 3.6 Multi-index fan-out

W4/A2 selections carry up to `MAX_SELECTION` indexes. One index uses a
single in-process selection connection (§3.2 step 4). Several indexes use the
combined facade (`combined_frontend` + `airk_mcp::unified::Unified::
combined`), with a `read_snapshot` variant that builds members from the
selection (per-index project subsets, §3.2 step 3) instead of
`selection.resolve(&persisted.grant)` (`combined_frontend.rs:104-200`). Its
children are the `project_client::Backend` selection variant (§3.2 step 4),
never `project_client::scoped_backend`, which the catalog `child` uses today
(`combined_frontend.rs:488-497`) (N2). The
combined merge, budgets and continuation stay shared. The combined snapshot
identity gets a new domain string for selection snapshots [P; the current
identity input is [U], not traced].

### 3.7 Lock order and the windows lock (F8)

Lock order is owner plane, then gate, and never the reverse. An owner may
take the gate lock briefly, nested inside its own critical section, for the
live-set register and removal only. The gate never calls into a plane. Worker
messages are sent with no lock held.

The W4 grant check must not run under `state.windows`. Today
`WindowPlane::grant` calls the validator after `locate` while the caller
holds the lock (`window_plane.rs:269`; the dispatch lock at
`service_runtime.rs:609`; the note at `service_runtime.rs:118-124`).
Proposed:
1. Lock, `locate`, copy `(window_id, lease)` and the window's
   `grant_sequence` (a per-window counter bumped on every grant store,
   release and replace), then unlock.
2. Run `scope_grant::check` on a blocking thread.
3. Lock again and `locate` again. The window must be alive with the same lease
   and an unchanged `grant_sequence`; otherwise answer `window_unknown`
   (window gone) or `scoped_read_unavailable` (a concurrent grant or release
   won, so the last writer is the one that held the lock). Then draw the
   nonce, register it live (gate, nested), collect the replaced grant's
   nonce, bump `grant_sequence`, store, renew and unlock.
4. With no lock held, `revoke_selection` the replaced nonce.

Revocations from close, release and sweep follow the same pattern: collect
nonces under the owner lock, release it, then revoke. On read: resolve and
copy under the lock, re-check outside it. The admin plane already checks
before storing (`admin_plane/mod.rs:437-457`: check, then `set_grant`
re-finds the session). It gains the same collect-then-revoke step for the
replaced grant and for logout and purge.

### 3.8 U77 invariant

No step writes the catalog or the registry:
- `scope_grant::check` and the authority build open the registry read-only
  (`open_saved_before`) and use `catalog_detail`/`catalog_state` only [V for
  `check`; P for the build].
- Selection revocation persists nothing (§3.3).

No grant, index, project, membership or index set is created or changed. No
`GrantId` is created, synthesized or reused. Tests assert that the catalog
revision and the `index_operation_receipts` rows are unchanged across grant,
read, revoke and an Index fence that revokes a selection scope.

## 4. Threat model

- **Trust boundary (F11).**
  - W4: a grant is obtainable by any caller with access to the owner pipe. The
    pipe is restricted to the installing OS user [V contracts §2.5 and the
    owner-pipe rules]. Any process running as that user can open a window.
    The lease exists only in that `airkctl session`'s memory, and the session
    `control` relay refuses W1–W4b/W7/W8 by method [V contracts §2.5].
  - A2: a browser holding an admin session cookie. That session was created
    through the single-use launch code (A1), which only the owner pipe issues,
    on a connection whose peer proof shows the same user
    [V `admin_plane/mod.rs:192-210`, contracts §4.5].
  - In both cases, scope authority is at most what owner-pipe access already
    reaches. The grant restricts reads; it does not elevate them.
- **A stolen or replayed scope identity.** `scope_identity`,
  `catalog_revision` and `selection` appear in replies and failure bodies and
  are not secret. Possessing them admits nothing: a read needs the lease or
  the cookie, and `grant_nonce` exists only inside the service (`Debug`
  redacted, never in a frame or failure body, §2.1, §3.2 step 5). No
  `ControlMethod` accepts a `SelectionScopeRef` (F4). The selection
  `ConnectionGrant` bearer stays in the service process because the connect
  is an in-process call (§3.2 step 4). This satisfies W4's "no bearer leaves
  the service".
- **Cross-window isolation.** `locate` requires id and lease. A window's grant
  cannot be used with another window's lease (`window_plane.json`
  `cross_window_refused`). Nonces differ per grant, so admission keys and
  revocations never alias between windows.
- **Cross-session isolation.** A cookie maps to one session. The grant is
  bound to the session and instance. Logout, purge and eviction revoke its
  nonce. The pipe has no admin session.
- **Cross-instance.** Every `SelectionScopeRef` names its `service_instance`.
  Admission refuses foreign instances and non-live nonces, and a restart
  drops all state.
- **Widening.**
  - Admission refuses an index outside `selection.index_ids`.
  - Projects are limited to the selected projects that are members of the
    index.
  - A selection with no projects reads nothing (§3.5).
  - The compiled scope never exceeds current membership atoms.
- **TOCTOU.**
  - A catalog change after the re-check is refused at admission by the exact
    revision rule (§3.2 step 4).
  - A catalog change after install revokes the scope locally through the
    matching Index/Membership/Project fence (§3.3).
  - An owner revocation racing an install is closed by the live-nonce check
    (§3.3, §3.4).
- **Denial of service (corrected, F2).**
  - Selection installations are bounded separately from `MAX_SCOPES`: by
    80 × `MAX_SELECTION` in the gate, and by 80 live plus 80 unacknowledged
    revoked entries per worker (N6).
  - Revoked and acknowledged selection entries are pruned, so the ledger
    cannot be filled permanently by repeated re-selection. One owner holds
    one live nonce, so its live entries are at most `MAX_SELECTION`.
  - At most 64 windows and 16 admin sessions (`MAX_SESSIONS`) apply.
  - Each read re-runs a bounded registry read (2 s wait,
    `scope_grant.rs:27`).
  - Worker capacity and `catalog_connection_busy` bound connections.

## 5. Implementation split [P]

Line counts are estimates, not exit criteria.

**Step C (contract), about 16 files, about 1,000 lines plus fixtures.**

Core:
- `crates/airk-core/src/scoped_reads.rs`: `SelectionOwnerKind`,
  `SelectionScopeRef` (redacting `Debug`), `ScopeSelectionObservation`,
  optional `ScopeObservation` fields and their validation.
- `crates/airk-core/src/service.rs`: `ScopedReadRef` optional fields and
  their validation.
- `crates/airk-core/src/catalog_fences.rs`: refuse `selection: Some` in a
  `FenceTarget`.
- `crates/airk-core/src/combined_reads/query_scope.rs`: the selection body.
- `crates/airk-core/src/combined_reads/read_retry.rs:83-84`: hand-written
  retry eligibility compares `observation.grant.id`/`.revision` with
  `SelectedScopeObservation.grant_id`/`grant_revision`. It gains a selection
  branch, which stays hand-written and is tested against Rust-emitted cases
  (F10).
- `crates/airk-core/src/combined_reads.rs:195`: `SelectedScopeObservation`
  (required `grant_id`/`grant_revision`) gains the same optional or
  selection treatment (F10).
- `crates/airk-core/src/combined_reads/header_templates.rs` and
  `crates/airk-core/src/usage_page.rs`: audit for grant fields; add selection
  rendering where grant fields appear (F10; contents [U], not traced).

Kernel:
- `crates/airk-kernel/src/scoped_reads.rs`: `SelectionReadAuthority` and its
  `validate`, the `Authority` enum and accessors, `with_selection_reads`.

Callers of the changed fields, each becoming `Some(..)` with no behaviour
change:
- `catalog_admission.rs`;
- `project_client.rs:355-359`;
- `airk-mcp/src/remote.rs`;
- `usage_client.rs:363`;
- the TS mirrors (`packages/vscode/src/scopeSelection.ts:49` reads
  `observation.grant`).

Fixtures:
- `fixtures/split/i1/selection_authority.json` plus its wire test.

Tests:
- serde round-trip and catalog byte identity;
- the xor validation;
- `Debug` redaction;
- `FenceTarget` refusal of selection refs;
- `SelectionReadAuthority::validate` cases (source union, per-project atoms,
  bounds, 48 KiB);
- selection fingerprint domain separation;
- kernel compile from selection atoms equal to the catalog compile for the
  same atoms;
- the observation shape;
- read-retry selection cases.

Golden reproduction: the parity fixtures and the `control_methods.json` groups
stay unchanged.

**Step S (consumers), about 18 files, about 1,500 lines.** Split into S1
(W4/window reads plus admission) and S2 (A2 reads).

Admission and workers:
- `catalog_authority.rs`: `build_selection_read_authority`.
- `catalog_admission.rs`: `AuthorityKey`, `live_selections`,
  `revoke_selection`, `record_selection_revoked`, pruning, the caps,
  `PendingFence.local_selection_revocations`, local revocation in
  `close_admission_with_census` after the last refusal check,
  `require_commit_ready`, and `observe_worker_reaped`.
- `project_supervisor/catalog.rs`: `ConnectionSelection::Selection`,
  `issue_selection`, `Expected::RevokeSelection`,
  `ScopedConnectionDelivery::take_in_process`.
- `project_client.rs`: the `Backend` selection variant (connect and reconnect
  through `connect_selection`).
- `project_protocol.rs`, `project_worker/scopes.rs`: the install enum,
  selection pruning and the cap.
- `service_management.rs`: `connect_selection`.

Read paths:
- `combined_frontend.rs`: a selection snapshot whose children are the
  selection `Backend` variant. Its query-scope body at `:661`
  emits `"mode":"reviewed_grant"` with `"grant":{…}`; it gains a
  selection mode (F10).
- `service_runtime/tools_call.rs`: the `Window` arm (sweep, then
  re-check, then in-process connect).

Owner planes:
- `service_runtime/window_plane.rs`: the nonce, `grant_sequence`, the §3.7
  split, sweep-revoke.
- `service_runtime.rs`: serve W4.
- `admin_plane/mod.rs`, `admin_plane/secrets.rs`: serve `granted_read`; the
  nonce; revoke on replace, logout and purge.

airkctl:
- `airkctl/src/cli.rs`, `airkctl/src/cli/session.rs`: the `Io` seam from
  TODO(3f.3) at `session.rs:~1065`.

Docs:
- contracts §2.5/§2.7;
- `admin_plane/CONTRACT.md`;
- `docs/admin-pages.md`;
- the ledger row.

Tests:
- Pure decision tests: granted; stale revision; retired index; index outside
  the selection; no project; missing owner; other instance; nonce not live;
  released; re-selected nonce.
- Admission tests:
  - a refused close (namespace, retention, plan bytes) revokes no selection
    scope;
  - a fence cannot commit until each local selection revocation is
    acknowledged or its worker reaped;
  - `restore_pending_fence` starts with no local revocations;
  - `take_in_process` refuses a catalog delivery and a non-live nonce;
  - the worker cap and the revoked bound;
  - an empty-scope membership is dropped and an empty subset is refused;
  - selection and grant scopes never alias;
  - revoke-before-install and install-before-revoke interleavings;
  - pruning after acknowledgement;
  - the separate cap;
  - an Index fence revokes a selection scope locally while the persisted
    plan holds only catalog targets;
  - `restore_pending_fence` is unchanged.
- Window plane:
  - the check runs with the lock released;
  - a window closed during the check gives `window_unknown`;
  - a concurrent re-grant gives `scoped_read_unavailable`;
  - expiry swept at M3 revokes;
  - each dropped-nonce return (sweep, purge, create eviction, close, release,
    replace, logout) triggers `revoke_selection` (N4).
- Serialization rule (N3): no worker reply, tool body or failure body
  contains a selection ref.
- Admin plane `tests.rs`/`tests/admin_plane.rs`: the current grant's search
  is now served; stale and no-project are refused; logout revokes.
- `airkctl` `session/tests.rs`: `scope_select_in_memory_grant` reaches the
  read.
- A CLI/MCP/admin parity test comparing the full selection read with the
  catalog read over the same atoms, normalizing only the documented
  selection fields.

## 6. Open questions for the user

1. **Decided (review F5):** reads require at least one project. Grants without
   projects are still accepted and their reads answer
   `scoped_read_unavailable` (§3.5).
2. **D8 approval.** Do you approve the optional-field change to
   `ScopedReadRef`, `ScopeObservation` and `SelectedScopeObservation`, the new
   domain `airk/scoped-read-authority/selection/1` and the new fixture file?
   These are additive for catalog grants, but a reader that requires `grant`
   (`packages/vscode/src/scopeSelection.ts:49` reads `observation.grant`;
   `read_retry.rs:83-84`) must handle its absence.
3. **Re-check cost.** A full `scope_grant::check` on every M3 call, versus a
   cheaper clock-only check (namespace and revision equal) plus the existing
   fences and the live-nonce set. This spec proposes the full check.
4. **Revision drift.** Any catalog revision change invalidates every
   selection grant (exact-revision rule), so the extension or admin page must
   re-select. Should an automatic re-select on `scoped_read_unavailable` be
   specified for the session (I2), or stay a user action?
5. **Lease renewal.** Should a read under a W4 grant renew the window, as a
   successful W4 does (ledger 3f.2 reading), or should only W2 renew?

## 7. Unverified items carried forward

- Whether every parity `scope_observation` is a catalog-grant read (§2.3).
- Whether a child-host worker always ends with the service on every platform
  (§3.3).
- The contents of `header_templates.rs` and `usage_page.rs` (§5).
- The combined snapshot identity input (§3.6).

## 8. Review resolution

| Finding | Severity | Resolution | Sections |
|---|---|---|---|
| F1 `ScopedReadRef` persisted via fence targets | HIGH | Selection scopes never become durable `FenceTarget`s. They are revoked locally (marked `Revoked`, worker `RevokeScope` acknowledged) before the plan is built, and are filtered out. `FenceTarget` validation refuses selection refs. No persisted format changes. `restore_pending_fence` sees catalog targets only | §2.1, §3.3, §3.8, §5 |
| F2 install ledgers never evict | HIGH | Gate live-nonce set; the live check in `reserve_lease`/`acknowledge_install`/`with_scope_issue`; pruning of revoked and acknowledged selection entries in gate and worker; separate `MAX_SELECTION_SCOPES`; DoS bound corrected | §3.4, §4, §5 |
| F3 revocation path | MED | Non-fence `revoke_selection`: under the gate lock, remove the nonce and mark entries `Revoked` (no generation bump, nothing persisted); outside the lock, `RevokeScope`. The close-versus-install race is closed by the live check | §3.3 |
| F4 no client carrier for selection refs | MED | No `ControlMethod` carries `SelectionScopeRef`; connect is in process (`connect_selection`/`issue_selection`), not `project_client`'s pipe; exact revision, never rolled forward; `Debug` redacts `grant_nonce`; failure bodies echo only the selection and `scope_identity` | §2.1, §3.2, §4 |
| F5 no-project reads | MED | Q1 decided: reads need at least one project. Grant-time behaviour (`decide` accepts empty) is unchanged; reads answer `scoped_read_unavailable` | §2.1, §2.2, §3.2, §3.5, §6 |
| F6 authority validation | MED | `SelectionReadAuthority::validate` mirrors the catalog `validate`: source set equals the atom union; per-project atoms and memberships; unique, bounded, at most 48 KiB | §2.2 |
| F7 per-index projects | LOW | Per index, only the selected projects with a membership of that index | §2.2, §3.2, §3.6 |
| F8 lock order and last-writer race | LOW | Owner then gate only; collect under the owner lock, revoke after release; per-window `grant_sequence` checked before store | §3.1, §3.7 |
| F9 lazy expiry | LOW | Expiry revocation is lazy (window-plane sweep, admin purge); sweep added at M3 dispatch; reads re-resolve the owner and need a live nonce | §3.2, §3.3 |
| F10 missing consumers | LOW | Added `read_retry.rs:83-84`, `SelectedScopeObservation`, `header_templates.rs`, `combined_frontend.rs:661` (`mode: reviewed_grant`), `usage_page.rs` | §5, §6 |
| F11 trust boundary | INFO | Restated as owner-pipe access (the same OS user); A2 sessions come from pipe-issued launch codes | §4 |
| Count correction | — | Parity: 76 `scope_observation` occurrences on 36 lines | §2.3 |
| N1 fence commit with local revocations | MED | `PendingFence.local_selection_revocations` (in memory); `Expected::RevokeSelection` without `FenceTicket`; `record_selection_revoked`; worker reap satisfies a pair; `require_commit_ready` requires all; restore starts empty; [U] removed | §3.3, §5, §7 |
| N2 in-process delivery | MED | `ScopedConnectionDelivery::take_in_process` inside `with_scope_issue`; `Backend` selection variant reconnects through `connect_selection`; combined children use it, never `scoped_backend` | §3.2, §3.6, §5 |
| N3 grant reaches the worker | LOW | Corrected carrier statement (private host↔worker IPC); no worker reply or body serializes selection refs | §2.1, §5 |
| N4 dropped nonces | LOW | `sweep`, `purge` and `create` eviction return dropped nonces as `#[must_use]`; each path tested to call `revoke_selection` | §3.3, §5 |
| N5 revocation ordering | LOW | Local revocation only after the last refusal check, just before `state.pending` is set | §3.3, §5 |
| N6 caps and empty scopes | LOW | `MAX_SESSIONS = 16` resolved; gate cap 80 × `MAX_SELECTION`; worker cap 80 live + 80 unacknowledged revokes; empty-scope memberships dropped, empty subset refused | §3.2, §3.4, §4, §5, §7 |
