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
