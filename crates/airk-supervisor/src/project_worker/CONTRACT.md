# Project worker diagnostic context

The worker preserves Core job outcomes and Kernel's fixed indexing failure
stages; raw causes remain in Host's private failure-detail ring. Public job and
log summaries contain the same content-free code, without source paths or raw
exception text.

Index and Reconcile call the same native Kernel indexing entry point. Its early
semantic-upgrade preflight must carry the same fixed diagnostic stages as the
later indexing path. A corrupt current owner yields
`index_vector_settings_metadata_incomplete` before model/inference admission:
`vectors_enabled` calls `vector_policy`, which validates `current_scope` before
the indexing branch's next explicit current-scope observation. The first failed
operation owns the stage; it is not relabeled to a later stage. Snapshot selection alone does not validate
the current owner's JSON; the first selection-helper diagnosis was disproved
by the retained cheap regression and removed. The underlying error chain and
typed recovery markers remain. SourceUnavailable retains existing recovery;
Repair's approved-source/PlanError mapping is unchanged.

The focused regression corrupts only a disposable native-generated owner,
observes the exact code/detail on native preflight and both execution paths, then
restores its original bytes before asserting. Source bytes and publication stay
unchanged. No wire shape, persisted format, retry allowance or job identity is
changed.

## Synchronous optional-control callbacks and async IO

Focus application/ranking and Context application/action/next-read callbacks
are synchronous Kernel interfaces over the existing native control client.
They can be invoked from the project owner's async driver as well as a blocking
worker. Direct ambient Handle::block_on inside an entered runtime is forbidden.
Every sibling callback uses one shared driver bridge; callback-specific request,
instance/worker/authority checks, single-attempt transport, original absolute
deadlines and typed source refusals remain unchanged.

The production project owner has a two-worker multithread Tokio runtime. The
bridge yields its async worker through block_in_place and drives the callback on
that same runtime handle. A caller outside Tokio uses a local current-thread IO
driver. An entered current-thread caller uses one joined scoped IO-driver thread;
block_in_place is never attempted there. Factories construct timers/IO futures
inside the selected driver, and no work detaches or outlives the synchronous
callback. No permanent per-session runtime/thread, process, global queue, retry,
new request budget or authority is introduced. Existing admission bounds callers.

The direct embedding-pool connector's Score/load/status/settlement operations
already use their own bounded airk-pool-client communication thread/runtime and
remain unchanged. Runtime bundle/work observations remain async. Native model
execution, lease/reservations and completion/drain stay under their existing
owning service; this transport bridge creates no inference owner.

Native I1 QueryResult embeds the existing closed MCP-shaped tool result
(content, optional structuredContent and isError). The negotiated public SDK
resultType discriminator belongs to the outer MCP transport, not this native
frame. Normal LocalTools dispatch already omits it; worker-level early scoped
and recognized unscoped error serialization must omit it too. Keep every error
body, scope observation, retry flag and native-body distinction unchanged.
Kernel closed-envelope sizing continues to refuse arbitrary extensions; no
frame/allocation increase or unknown-field trimming is authorized.

Cheap proof: response_tests::early_worker_errors_use_closed_native_envelopes
constructs real SDK refusals, demonstrates the SDK discriminator, then checks
actual worker helpers against Kernel context_member_payload_bytes without a
model/worker process. A framing correction does not establish that an underlying
member operation succeeded or authorize replay of a failed finite Answer.

## D06 connection admission does not shorten an accepted read parent

The60-second broker connection authorization and matching worker admission check
are unchanged and continue to refuse every new query after expiry. The already
accepted query retains its original decoded ReadAttemptBudget and partition;
that admission expiry is not a second execution deadline. A connection admitted
near its end may finish its one accepted bounded operation while remaining
unusable for another query. Broker already retains active expired connections
until actual finish; cancellation/disconnect never fabricates completion.

The existing125-second worker hard-reap boundary and shorter original caller
or Answer deadline remain; there is no new allowance or wire field. Every
native application, source/selection/configuration callback and delivery poll
keeps its actual authority/liveness checks. Revoked or expired installed scope
cannot be delivered. No Window TTL, installed-scope lifetime, grant TTL or
55-second frontend reuse policy changes. The administrative native read sibling
already preserves its accepted120-second read beyond a pre-submit55s check.

Cheap controls preserve the captured wire parent/identity on decode and use
injected broker time to prove an accepted query can settle after admission
expiry while a new one is refused. These establish clock ownership and no-replay
behavior; they do not qualify a model, live service or real R154 outcome.
