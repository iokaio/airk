# Native optional evidence stages

Owner: `airk-kernel`. Shared wire types, source binding and validation belong
to `airk-core::arbiter` and `native_focus`; the service owns model eligibility,
native admission, reservations and delivery fences.

Arbiter Focus orders a complete canonical semantic Search result. It does not
grant source access, produce evidence, decide sufficiency or remove results.
Lexical Search and other read operations are excluded. Focus disabled or
excluded retains the ordinary result without added metadata. Context may be
disabled while Focus runs. Search navigation computes its native definition
hints before Focus and preserves those hints.

The original Host-captured read attempt, actual absolute deadline, worker and
authority are required. Ordinary Search uses Core's bounded allocator once;
the optional deadline can only shorten the original deadline. Nested Answer
uses its actual parent allocation and observed remaining work. Request clones
share private accounting; a new captured attempt starts new accounting. No
retry, new attempt, replacement grant or independently reset token allowance
is created. Unpartitioned combined ordinary Search retains its complete
original result with a bounded NotApplied observation.

Both native Answer entry points bind that parent once before their first
canonical read; adapters clone the owning Kernel rather than assigning a
replacement budget. Selection-backed ordinary singleton Search requires the
actual Host-captured Focus-only `PartitionAllowance`, checked with Core's
purpose predicate and bound to the original attempt, selection and index.
Runtime/profile/frame caps only reduce its resources; its captured parent is
retained exactly. The per-index Kernel never infers singleton authority from
its local scope. Focus-only allowances authorize Search, while Context
allowances authorize Answer. Direct ordinary Focus retains its own outer
delivery guard; only genuine combined Context children defer to their parent.

When the initial actual Context application is Ready, Core limits nested
Focus to a cumulative half of the actual local optional token allocation.
Each prefix intersects that remaining stage ceiling, shared remaining work and
qualified whole-pair limits. Known Focus work or unknown held reservations
consume the same ceiling across reads; no round resets it. Context can still
use shared remaining capacity after Focus reaches its ceiling. If Context is
not Ready, Focus has the full shared allowance. Independent ordinary Search
is unchanged. Remaining capacity does not guarantee a Context input will fit.

Eligibility and scope are checked before candidate preparation. The request
binds the complete returned candidate order, full Hits, explicit UTF-8 source
windows and a bounded scoring prefix. A successful complete prefix is applied
atomically with stable ties; the unscored tail and all other fields survive.
Failure or timeout retains original order. Source-authority refusal propagates.
Unknown native work retains its reservation and blocks further optional work;
measured work above reservation also blocks later learned Context decisions.
Post-execution source, policy or identity changes refuse stale delivery.

The transport owner supplies actual reply headroom. Complete receipt framing
is reserved before dispatch, including already retained mandatory Focus facts
and Context history for nested Answer. There is no response-ceiling increase.
Combined children also reserve the actual eligible Focus outer binding before
scoring and in final packing; the parent cannot reclaim that metadata capacity.
An undispatched compact notice is included only when it fits; literal zero
headroom preserves the ordinary result. A native receipt with actual or
unknown work cannot be silently omitted by Gathering or packet packing.
Packing trims presentation and uncharged secondary records first, discloses
omissions and invalidates affected assessments. If mandatory custody cannot
fit the original allocation, the operation fails with no automatic replay.

Canonical typed Search records are the only nested source of Focus costs and
delivery bindings. Focus and Context retain separate stage bindings and share
the original parent work allocation. Costs and phase timings are additive;
candidate counts retain the peak rather than double-counting source reads.

Cheap checks: `arbiter::focus::tests::`,
`arbiter::joint::focus_pack_tests::` and MCP `focus_handler_tests::`.
Injected scorers establish application, scope, custody and framing behavior;
they do not establish optional-model quality or installed transport behavior.

Context-only history/token accounting mutators compile with `local-inference`,
matching their learned Context actuator callers. The shared availability accessor
also compiles for tests so feature-independent Focus accounting regressions keep
their assertions. A no-default-features production build omits these unused
Context helpers; canonical Focus dispatch, authority and budget behavior do not
change, and no local inference capability is added to the lightweight caller.

Canonical Answer reserves its resolved Report/Require saved-source verification
metadata before optional scoring and final packet packing. The reserve is
derived from the native typed receipt, actual limits and its existing metadata
growth allowance; it does not change the public request budget or native frame.
Explicit Off retains the legacy 4096-byte closed-render reserve. Cumulative
mandatory Focus custody and final post-application/deadline packing use the same
additional reserve. Metadata-only refusal remains valid if mandatory custody
cannot fit; no source verification or uncertain native work is replayed.

A genuine combined Context partition's selection digest is the frontend's
whole current member snapshot identity. Open and W4/A2 snapshot domains may
differ from the original installed selection identity; the digest grants no
source authority and must not be compared as if it were that credential.
Kernel still checks Core allowance validity, exact original attempt/index and
actual installed authority fingerprint. Only the ordinary singleton Focus
purpose additionally requires its captured selection digest to equal the
actual installed SelectionScopeRef identity (and an installed selection).
The service Focus scorer applies that same purpose distinction after native
request/worker/source/parent/partition validation, so nested Focus retains the
original whole-selection accounting identity without creating an allowance.


## D08 native host capacity phases

A resident retains its full admitted host peak charge through actual session
teardown. Warm compute may carry a separately authenticated extra-host bound;
it never requires the full resident allocation to be free a second time. The
broker checks that bound against actual fresh OS availability and its unchanged
policy epoch, and the compute lane repeats the same bound before dispatch.
Legacy callers retain their existing conservative full-peak warm bound. Unknown
headroom, foreign charges, policy pressure and stale epochs refuse. Pending
GPU reservations cover future, unallocated work only; real resident device
usage is already reflected in actual driver free memory and is not subtracted
again. A GPU capacity reservation never authorizes a model or runtime.


Release-observer equivalence is exact Arc identity of the authentic reservation
release state, not manifest equality. Service may deduplicate repeated borrowing
of one charge without forgetting distinct unresolved charges. NativeConfiguration
selection equality retains the complete typed document and selected descriptions;
it never narrows configuration4 to a CPU model-only identity.


## G4 scoped device diagnostics (additive read contract)

Canonical airk_diagnostics may carry scoped_device_status, schema
airk.arbiter.scoped-device-status/1. Its exact ScopeBinding and per-stage
IndexApplication distinguish this index's eligibility from complete native
DeviceStageStatus residency/selection observations. No configuration document or
global enrollment list is serialized. Typed unavailable uses revision0 and no
invented stage/model state. Observations are descriptive, never execution or
qualification capabilities. Scalar status and Search/Answer schemas are unchanged.

The native read-only ArbiterDeviceRead method carries actual worker/scope, pinned
publication and original ReadAttemptBudget; response uses the same typed name.
Service repeats actual scope gates and original monotonic budget before/after;
Kernel repeats actual scope/publication fences. Only strict current4 reads and
metadata snapshots are permitted: no migration/CAS/rebind, model/device load or
probe, downloads or native continuation. Default absent controller/original
attempt omits the optional observation. Native absent/retained/corrupt4 yields
truthful unavailable, never fake disabled/current configuration.

Combined diagnostics keeps the complete observation under each corresponding
member.result; it never fuses authorities or global enrollment state. Complete
copy/export retains precision, native fallback reasons and all variant metadata.
The additional observation is bounded256KiB inside unchanged original reply/frame
ceilings. No retries or replacement clock are introduced.

## Private evaluation change set 1 (`software.rs`)

Every layer is off unless the private evaluation fixture binds layers for its
process (`software::bind_evaluation_layers`); product gathering, packing and
Focus are unchanged and `Gathering::new` keeps the product shape. With layers
bound, one Gathering state machine still issues every read through the same
canonical reader under the original revision fence and call, round, candidate,
byte and deadline budgets:

- `followups`: whole chunks of the kept hits, `symbols`/`references` lookups
  for identifiers of the question and kept hits, their `read_file` excerpts
  (with the recipe's `known_hash`), callers and tests, file outlines, and
  contract or README documents that govern a hit's folder. A governing
  document is queued only after a canonical `file_info` read at the same
  revision shows it indexed. A failed optional follow-up (round 2 or later)
  keeps its canonical failure receipt and the remaining queue; a failed
  initial search still refuses.
- `spread`: follow-ups merge one read per kind per cycle; the queue may hold up
  to `fanout.min(candidates)` reads.
- `pool`: the first search returns 16 focused snippets.
- `focus`: Focus scores at most six pairs per Answer, each over a bounded
  window; kept hits follow Focus relevance. Focus still removes nothing from a
  record.
- `present`: every hit is presented once (contained or mostly overlapping
  ranges are dropped), grouped by purpose with exact locations, with a closing
  disclosure; packing omits the least useful kinds first.
- `device`: a Context call is admitted only when its measured cost plus a read
  reserve fits the deadline.

Records, receipts and paragraph grounding keep their existing validation.

### Layer tokens and binding (22 November 2026)

`Layers::parse` parses the value the private fixture reads from
`AIRK_ARBITER_EVALUATION_ANSWER_LAYERS`. It accepts `all`, `none`, the layer
names above, `-name` to remove one, and three change-set-2 tokens:

- `context_b`: Context option B. Joint assessment builds its roster with
  `airk_retrieval::arbiter::context_wide` (wider roster, its own task policy,
  the best Read taken even when a terminal card scores highest). Only a
  profile that declares the option-B policy is scored under it. Product rosters
  are otherwise unchanged.
- `focus_keep=<1..=16>`: hits kept for whole chunks and follow-ups
  (default 4).
- `focus_threshold=<0..=1, at most three decimals>`: the Focus relevance a
  scored hit needs to be kept (default 0.10).

Replay tuning tokens (change set 1 tuning). Each default, and each second
value below, is change set 1's behaviour, so `all` serializes unchanged:

- `pool=<1..=16>`: first-search pool size (default 16); needs `pool`.
- `pool_full=<0..=16>`: top pool hits fetched with full chunks. At or above
  the pool, the first search returns full chunks; 0 keeps snippets; between,
  the snippet search is followed next by one full-chunk `search` of the same
  question for the top hits, its byte bound reserved from the per-read split.
  Default: full chunks only when the pool is at most 8.
- `read_floor=<512..=8192>`: lower clamp of the per-read byte split (default 1536).
- `kept=<1..=16>`: an alias of `focus_keep`.
- `subjects=strict|loose`: strict looks up only snake_case or multi-word
  CamelCase names that a kept hit's text names; loose is the first-three rule.
- `governing=project|any`: project probes governing documents only inside a
  hit's project folder (the folder after the first `crates`, `packages`,
  `apps`, `libs` or `src`, otherwise the top-level folder).

The first search's `pool` and `pool_full` come from the process binding
(`evaluation_layers()`), the same layers `Gathering::with_layers` receives in a
service.

`all` turns on exactly the six change-set-1 layers, never `context_b`. `all`
and `none` reset the values, so values must come after them. An empty,
repeated or unknown token is refused. `bind_evaluation_layers` exists only in
test-support builds. It binds once per process, and a second binding must
repeat the same layers. Without a binding, and always in a product build,
`evaluation_layers()` is `Layers::default()`: every layer off and every value
unset, so the product's gathering, roster and Focus shape are unchanged.
