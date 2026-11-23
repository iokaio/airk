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

Before acknowledging activation, the project worker asks the registry whether its own project already has a terminal canceled job. That receipt vetoes semantic setup, so the worker can return from setup reconciliation without inspecting the complete publication. All other projects retain the existing semantic inspection and transactional enrollment checks.

## Answer job table hook (final plan section 12.4.1, L2b)

`Hooks::answer_jobs` is a lookup by control root, supplied by the service
composition root (`Hooks::with_answer_jobs`). The default finds nothing. After
a query connection has built its session tools, and after its native Arbiter
session when it has one, the worker asks the lookup once and, when it returns a
table, gives it to that session (`LocalTools::set_answer_jobs`). The session's
`airk_answer` calls then go through the service's Answer job table; every
other tool, and every session without a table, is unchanged. The worker does
not own the table, its queue or its results: the service does
(`airk-service` `service_runtime/answer_jobs/CONTRACT.md`). A child-process
owner has no table in its process, so its Answers keep the direct path.

A job of that table outlives the connection of the call that made it. While
one is queued or runs under this owner (`LocalTools::answer_jobs_in_flight`),
the owner is not idle, so the 15-second idle exit does not end it. Every other exit (a
drain, a quarantine, the bounded generation lifetime) is unchanged: the job
then stops at its next fence and the table keeps no failure for it.

## Answer job authority (L2c, decided 23 November 2026)

Recorded contract decision (final plan section 12.4.2; review of `24184339`,
H1). The owner gives a session the table only together with an
`AnswerJobAuthority` for that session's redeemed grant
(`LocalTools::set_answer_jobs(jobs, authority)`).

- **`bind(job kernel)`**, at the job's submission: the installed entry of the
  session's grant must be live. The job kernel
  (`Kernel::answer_job_kernel`, its own revocation flag) is registered on
  that entry beside the sessions, at most 64 a lease. The returned guard
  removes and revokes it when the job's work is dropped.
- **What reaches a job's kernel:** what fences the grant. That is a
  `RevokeScope`, a newer grant revision installed for the same grant and
  index, lease expiry (`expire`), lease renewal (the superseded lease's
  readers are fenced) and the owner's close. These revoke registered jobs
  exactly as they revoke sessions.
- **What does not:** `SessionFence::drop` (`detach`) removes and revokes the
  session's own kernel only. The session's 60 s authorization window, its
  30 s read timeout and its stream ending end its own admission and wait,
  not its jobs.
- **`stuck()`**, when a job passes its deadline and grace: an in-process
  owner quarantines itself (`StuckReason::QueryDeadline`), as a stuck direct
  query does. A child-process owner has no table.
- **The maker's session end is observable:** the guard carries a
  cancellation that `SessionFence::drop` cancels. It is a diagnostic seam
  for the delivery record and the tests, never authority.
- An unscoped (legacy) session's authority binds nothing: its kernel has no
  scoped reads to revoke, and its job is bounded by its own deadline and its
  owner.
- A job counts as running for the idle exit only until its deadline and
  grace (`answer_jobs_in_flight`), so a stuck job cannot keep its owner
  alive for ever; the quarantine then lets it leave.

Evidence: `project_worker::scopes::answer_job_tests` (a job kernel outlives
its maker's session teardown and ends with the grant, its lease and the
owner's close), and through the real `query()` path in `airk-cli`
`tests/combined_reads.rs` (`answer_jobs_*`).
