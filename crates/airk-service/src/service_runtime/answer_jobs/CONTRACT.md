# Answer job table

Owner: Service runtime. Plan section 12.4.1, step L2b, revised by section
12.4.2, step L2c. An admitted Answer is a job owned by the service, not by the
client's connection. The table admits, attaches, tickets, schedules and
retains Answer jobs. It runs nothing itself: the runner it is built with
performs one Answer, and the kernel's v3 driver still owns everything inside
the Answer.

**Where this file and "Service-owned jobs" below differ, that section
governs.** It is the recorded L2c contract decision (23 November 2026) and
replaces the L2b rules that a job runs only inside its maker's window, that a
failure is attached by digest, and that a running job is joined whatever its
window.

## Service-owned jobs (L2c, decided 23 November 2026)

Recorded contract decision for the kernel, the supervisor and the service,
from final plan section 12.4.2 as the user approved it on 23 November 2026
and the review of `24184339` (H1, H2, M1 to M7). The kernel half is in the
kernel arbiter contract ("Answer job kernel"), the owner half in the
supervisor's `project_worker/CONTRACT.md` ("Answer job authority"), the public
shapes in the Core arbiter contract ("Answer job options and delivery
record").

**What a ticket means.** An accepted job holds enough authority and
information to finish its Answer if every client closes now, until the job's
own deadline and on the publication it was accepted for.

**Bound at submission**, when the handler hands the call to the table:
- **the request as asked.** Its `deadline_ms` is the job's work budget. The
  caller's wait never shortens it.
- **the authorized scope:** a job read kernel (`Kernel::answer_job_kernel`).
  It is a copy of the caller's verified, frozen grant authority with the same
  fingerprint, grant memberships and revisions, index and source roots, and
  a revocation flag of its own. The owner registers it with the installed
  lease the caller's session redeemed (`AnswerJobAuthority::bind`), so
  revoking that grant revokes the job. A table is never given to a session
  without such an authority (`LocalTools::set_answer_jobs` takes both).
- **the publication:** the read snapshot the caller's call selected.
- **the configuration:** its revision is part of the scope digest.
- **the budget.** The plan is fitted to it at dispatch, as before.

**At dispatch** the job's work checks that its kernel is still live, then
mints a native attempt of its own: a fresh request id and an expiry at the
end of the job's window (at most 600 s and the Answer read ceiling). It opens
a read context of its own, with its own cancellation and none of the maker's
transport guards or custody, and runs the loop on its owner's blocking pool.
The card and Focus calls carry the job's attempt, so the lane and the loop
ledger key them by the job.

**What bounds a job's authority.**
- **In scope:** exactly its maker's frozen authority, never more. The job
  kernel is derived from the maker's verified session and changes nothing in
  it. Root, collection and index isolation, the original document locations
  and source read-only behaviour are the session's own.
- **In time,** the earliest of:
  - its deadline (dispatch plus `deadline_ms`) plus a 5 s supervised grace;
  - the installed lease of its grant: lease expiry and lease renewal fence
    the job's readers exactly as they fence a session's (at most 300 s a
    lease, the bound the direct path already has);
  - revocation of that grant, or its replacement by a newer grant revision;
  - its owner's close, quarantine or drain, and service shutdown;
  - an authorized cancel.
- **Not** its maker's connection, the maker's session teardown
  (`SessionFence`), the connection's 60 s authorization window, its 30 s read
  timeout or the maker's transport read attempt.
- A finished result is kept for at most 10 minutes after it finishes, under
  the byte and count caps, and is dropped by a new publication.

**Residual: the lease bounds a job (recorded, not closed).** The catalog
control renews an installed lease only when a new connection on that grant
needs it after expiry, and a renewal fences the superseded lease's readers,
the job's included. Nothing renews a lease for a job whose maker has gone.
So a job, running or queued, lives at most until the lease that was in
force when it was submitted ends: at most 300 s from that lease's
installation, whatever its own `deadline_ms`. A queued job whose lease ends
before dispatch is refused at dispatch with the typed, retryable
`answer_authority_ended`, for its own invocation only; nothing is kept for
other callers. Carrying jobs across a renewal of the same grant revision
(the supervisor moving their kernels to the renewed entry, and the card and
Focus calls naming the renewed lease) is the follow-up that would lift this
bound. Owner: L2c follow-up, before Answers longer than the lease are relied
on.

**Every call is authorized again.** Status, result and cancel reach a job
only through a call that has redeemed a live grant in its own session and
whose own scope digest equals the job's. A handle alone grants nothing. A
revoked grant cannot redeem a session, so it cannot be delivered anything.

**The caller's wait is not the work.** A call waits at most the least of its
request's deadline, its `wait_ms` when given, and its transport read attempt,
less the ticket margin. The job's window is its own `deadline_ms` from
dispatch. A queued job expires at its own due time (submission plus
`deadline_ms`) unless it is interactive.

**Reuse.** Reuse by request digest stays on by default, within one scope
(authority, configuration, reply reserve and `include_status` rendering) and
on the caller's publication, and only for a job asked with **the same
`deadline_ms`** as the caller's. The digest already covers every other
budget field, so reused work was made under exactly the caller's own limits:
it is valid under the caller's own budget (M4) and never cut for someone
else's shorter deadline (M1). Within that:
- a queued job (it has not run; a retry waits for it rather than queueing
  a duplicate);
- a running job whose shape covers the caller's;
- a finished result whose shape covers the caller's and that is complete, or
  cut no earlier than the caller's deadline (a requeued run's shorter
  window).

A retry of the same arguments has the same deadline, so it reuses; a call
that asks for another deadline runs on its own.

`fresh: true` never reuses: it always starts an independent run. An
invocation key gives exact retry recovery. The same key in the same scope
with the same request recovers its job, finished, failed or running. The same
key with a changed request, or another `include_status` or reserve, is the
typed refusal `answer_invocation_conflict`. A key lives as long as its job.
A key recovers its job only on a publication that job is valid for, as a
digest and a continuation do: after a new publication the same key and
request run again, and the key then names the new job (independent review
of the rebased L2c, `an_invocation_key_never_recovers_a_result_of_an_older_publication`).

**Failures belong to their invocation** (M5, H1). A failed job's failure goes
to the calls waiting on it when it fails, to a continuation that names the
job, and to its invocation key. A call by digest never receives it and runs a
new job. A job that did nothing a retry could repeat is dropped as before.

**Attended and unattended.** A job is attended while at least one call waits
on it. The table dispatches attended queued jobs first. An unattended queued
job starts only when no attended job is queued, and is dropped unrun, keeping
its place in line, once its due time has passed. A running job is never
pre-empted. `interactive` (Workbench) does one thing only: an unattended
interactive job is not dropped at its due time, so it may run to completion.
It never ranks a job ahead of an attended one, and its effect is bounded by
the job's own deadline and its lease. It is caller input and grants nothing
else.

**Caller input is bounded.** `wait_ms` is 1 to the request's
`deadline_ms` (at most 600 s), and the wait is further bounded by the
transport's own read attempt. The invocation key is 1 to 128 bytes of
`A-Z a-z 0-9 . _ : -`. The key is looked up under the caller's principal
(installation and authority fingerprint) and index, as the digest is under
the scope, so a key can neither widen authority nor collide across
authorities.

**Supervised deadline** (M2). The job's blocking task is supervised. At its
deadline plus the 5 s grace:
- its owner is quarantined (`AnswerJobAuthority::stuck`; an in-process
  owner's `StuckReason::QueryDeadline`), so it admits nothing more;
- the calls waiting on the job get the typed unretained failure
  `answer_job_stuck`;
- the owner stops counting the job as running, so it cannot hold the idle
  exit open for ever;
- the job's gathering slot stays held, as settling, until the blocking task
  has really returned. Only then can another job use the capacity.

**Each delivery is rendered for its caller.** The calling handler renders
under the caller's own response mode, verification controls, usage span and
delivery guard, as before. The reply's `elapsed_ms` is the caller's own wait,
and the reply is validated against the caller's own budget before delivery.
Beside the reply, the delivery record (`delivery`) gives the job's handle,
queue time, run time, the caller's wait, and whether the work was reused.

**Test support: the job gate.** `wiring::test_gate` is compiled only with
the `test-support` feature, which only dev-dependencies enable (`airk-cli`'s);
the product and release builds do not, so there the environment variable is
never read. With `AIRK_TEST_ANSWER_JOB_GATE` naming a loopback address the
test listens on, each job connects out to it before its work starts, sends
`started <job>`, and obeys `m` (wait for the maker's session to end, answer
`maker-closed`) and `r` (release). The service opens no listener for it,
every wait has a 120 s deadline, and it gates only; it never changes what the
job does. No existing event could stand in for it: a job with a disabled
model finishes in milliseconds, before a test can close its maker's session.

**Superseded L2b rules:** "A job never runs outside its maker's window"
(ticketed and requeued jobs now run in their own window), "Failed: a failed
job on the same publication is always attached", and "A running job is still
joined whatever its window". The claims and the resubmission of a call whose
job never ran stay, for owners that stop before dispatch.

## Interface

`AnswerJobTable::call_with_work(scope, request, model, publication,
call_deadline, work)` is the one entry point for the service's Answer path
(`ServiceAnswerJobs`, see "On the live path"). `call` is the same without
work, for the table's own tests. The other entry points are `status`,
`set_runtime_capacity`, `set_costs` and `publication_changed`; none of them
is wired yet.

- **Construction.** The table is built inside the service's Tokio runtime and
  keeps its handle. It spawns every job on that handle, so it may be
  dispatched from any thread, including a capacity observer outside the
  runtime.
- **Inputs to `call`:**
  - `scope`, an `AnswerScope` built from the caller's own verified
    `ScopeBinding`;
  - `request`, a validated `ContextRequestV3`;
  - `model`, the Context rung the configuration would plan with (`None`
    means the no-model loop);
  - `publication`, the caller's current `publication_digest`, when known;
  - `call_deadline`, when this call must return.
- **Replies:**
  - `Finished`, with the outcome, whether the call attached, and the
    degradations;
  - `Pending`, with an `AnswerTicket` and the degradations: `running` says
    whether the job has started (the reply is then a partial with this
    continuation) or is still queued (the reply is a ticket);
  - `Busy`, with `retry_after_ms`, when nothing could be queued.

  The degradations are the ones fitted at dispatch, and they are the same
  for the caller that made the job and for every caller that attached.
- **Per-call work.** `call_with_work` lets a caller supply its own work
  (`JobWork`) for a job it makes. The work is taken when the job starts. A
  call that attaches, or is told `Busy`, drops its work unused, so a job's
  work runs once. A job made without work runs the table's runner.
- **The runner** (or the work) is called inside the job's own task, with a
  `StartedJob`:
  - the request;
  - the applied `Shape` (parts, sub-needs, rounds and Context rung);
  - the degradations the reply must disclose;
  - `run_ms`, the time from dispatch to the job's own deadline;
  - `requeued`, set only when the wait consumed the caller's window;
  - the publication the table expects the job to run on.

  It returns the value, the publication the job actually ran on and the
  bytes to count against retention.
- **A job that did nothing a retry could repeat.** A runner or a work that
  returns `Unretained` ends its job as `JobOutcome::Dropped`: the outcome
  goes to the calls waiting on the job, and the table keeps nothing and
  learns nothing from it. With `not_run`, the job never started, and its
  place in line is kept for a retry (see "Scheduling").

## Admission

Admission estimates the queue wait and the work, and then does the first of
these that applies:
1. **Admit.** The request runs at its requested shape when the estimate fits
   its deadline.
2. **Degrade to fit.** Reductions are applied one step at a time, cheapest
   loss first:
   - rounds down to one;
   - sub-needs and parts down to two, then down to one;
   - last, the no-model loop.

   The Context stage has one model, the Arbiter, so there is no step from
   one model to another (step LH retired Large and its model step;
   change-log entry 62).

   Each reduction is disclosed with reason `deadline_estimate` when the
   requested work alone exceeds the deadline, and `queue_wait` otherwise. A
   rung with no costs is unavailable and is disclosed as `model_unavailable`.
3. **Ticket.** When even the smallest answer cannot fit after the wait, the
   call gets a ticket, and the job is queued. The ticket carries the id, the
   queue position, the estimated start and finish, and `retry_after_ms`, all
   within the Core ticket bounds. The ticketed job's deadline window opens
   when it is dispatched.
4. **Smallest, disclosed.** When the smallest answer cannot fit the deadline
   even with no wait, a ticket would never help. The job is admitted at the
   smallest shape with its degradations, and the kernel returns its disclosed
   partial at the deadline. Such a job is not marked `requeued`.

The shape is fitted again at dispatch against the time actually left. If the
wait has consumed the caller's window, the job runs on a fresh window of its
own deadline and is flagged `requeued`, so that the caller can attach. The
work outlives the call.

**Attaching without loss.** A request whose scope and request digest equal
those of a live job attaches to it only when attaching gives the call no less
than its own admission would:
- **Running or finished:** the job's dispatched shape must be at least as
  large in every dimension, and on at least as good a rung, as the shape
  this call would be admitted with. A call that could only get a ticket
  attaches, because the existing job is sooner and discloses its own
  reductions.
- **Finished, but cut by its deadline:** the result is what the job had by
  then, so it also needs a call with no more time than the job had (its
  window at dispatch, with the 1 s tolerance). A call with a longer deadline
  runs on its own. The runner reports the cut: for an Answer, the reply's
  partial marker or a part's stop names the deadline. The clock alone does
  not show it, because the loop keeps a reserve and returns before its
  window ends. A running job is still joined whatever its window: the work
  is under way and is not replayed.
- **Queued:** the job's window must end no earlier than this call's. For a
  ticketed job, its deadline must be at least as long.
- **Otherwise** the call runs as a separate job, and the digest then names
  the newer job. An explicit continuation always reaches its own job.
- **Failed:** a failed job on the same publication is always attached, so
  uncertain work is not replayed.

**Estimator.** The work is `fixed + parts x (per_part + sub_needs x rounds x
per_sub_need_round)` for the job's rung. That figure is scaled by a learned
correction: an EWMA of elapsed over estimated time, kept between 0.25 and 4.
- A run that reached its deadline teaches the correction nothing, and
  neither does a smallest-disclosed run whose estimate exceeded its window.
  Both are recorded in the recent samples as `cut`. A run counts as having
  reached its deadline when its runner reports the cut, or when it ended
  within the tolerance of its window.
- The default costs are placeholders until L5b and L7 measure them per
  device (answer-loop-changes entry 19).

**Unmeasured costs.** While the costs are placeholders (`costs_measured` is
false, the service's setting until `set_costs` feeds measured figures), they
order and bound the queue, but they never reduce a job that still has its
whole window. A request that did not wait runs as asked, as it did before
the table existed. A job that waited is fitted to what is left of its window,
as above. A run under unmeasured costs teaches the correction whenever it
finished inside its window.

Known limit: a job that waited is fitted with the placeholders and the
correction learned so far. Until a few runs have taught the correction, a
waiting job can lose more than it needed to. (Before step LH, Large's
placeholder put the full default request at 408 s; the Arbiter's puts it at
204 s.) Every such reduction is disclosed. Measured costs (`set_costs`)
remove the limit.

**One rung per job.** With `configured_rung_only` (the service's setting),
each job is fitted with its own Context rung and the no-model loop alone,
because the kernel cannot change the rung per Answer. The ladder ends at the
no-model loop. Since step LH retired Large, the Arbiter is the only rung and
the ladder never climbs, so this setting changes no plan today; it is kept so
that a second rung could never be chosen per job by accident.

**Busy.** The limits on queued and running jobs are:
- 8 per scope;
- 32 per principal, meaning the installation and source-authority
  fingerprint, across all of its scopes;
- 4,096 in the whole table.

Past any of them, the call gets `Busy` and no job is made.

## Scheduling

- **The gathering limit** is `max(2, min(runtime_capacity, user_cap))`.
  `runtime_capacity` is injected; it is L5b's figure, and 2 until that
  figure exists. The user cap never takes the limit below the floor of 2.
- **Queued jobs hold nothing:** no model, memory or ledger reservation.
- **Dispatch order** is the smallest key `due + ageing x enqueued` (ageing
  defaults to 1), with ties broken by arrival.
  - This is earliest-deadline-first with an ageing guard. A later arrival
    can overtake an earlier job only while `t x (1 + ageing) < due0 + ageing
    x t0`, so every job's wait is bounded and no request starves.
  - Turn-taking per round inside the model lane is the lane's job (L4 and
    L5b), not the table's.
- **With the model lane** (L4, `airk-inference`
  `pool/arbiter/CONTRACT.md`, "Lane fairness for Answer /3 parts"). The two
  bound different things and do not overlap:
  - **The table** decides which Answers gather at all, service-wide and
    across indexes: at most the gathering limit, earliest deadline first with
    ageing. A queued Answer holds no lane job.
  - **The lane** shares the one model compute thread among the Answers that
    are gathering. It keys every card call and Focus pair by the Answer's
    native attempt and its parts, lets one attempt hold at most three of an
    index's four jobs, and runs next a job of another index or attempt than
    the last. This is the turn-taking of plan section 12.4.1, idea 4; it is
    not by deadline.
  - **One table job is one lane attempt.** A job runs under its maker's
    native attempt, so every call attached to it shares that one lane
    identity, and no call adds lane jobs by attaching.
  - **At the floor of two**, two Answers on one index both get model turns:
    one holds at most three lane jobs, so the other always has one.
  - **Above two**, the lane does not queue what it cannot admit: a card call
    or Focus pair refused at the lane (four jobs on the index, or eight
    queued in all) is a known non-dispatch, and the loop takes the
    deterministic order for it and discloses that. So a gathering limit
    above what the lanes serve costs model share, not time. That is why the
    limit is to come from L5b's runtime capacity and not from a guess.
  - **The estimator prices a job as if it had the lane to itself.** Two
    Answers gathering on one model thread each take longer than one alone.
    The learned correction (up to four times) absorbs that from observed
    runs; the cost table does not model it.
- **Each started job runs on its own task**, and the runner is called inside
  that task. Dropping or timing out a call never cancels the job. A runner
  that panics, before or after returning its future, finishes the job as
  `Failed` and frees its slot.
- **Waiting.** A caller still receives an outcome that is signalled at its
  deadline, or one that the table has since stopped retaining.
- **A place in line.** A job that reports it never ran is removed, and its
  order key (the deadline and arrival it was ordered by) is kept as a claim
  for its scope and request, for the retention time and at most one per
  scope and request.
  - The next call for that scope and request, by digest or with the dropped
    job's id as its continuation, is admitted afresh with that order key and
    a window of its own. It goes where the dropped job stood, ahead of every
    request that arrived after it.
  - A claim is used once, and it is never visible to another scope.

## Security: attach, retention and status

- **Scope.** `AnswerScope::from_binding` digests the installation, the
  catalog, the index, the caller's source-authority fingerprint
  (`ScopeBinding.authorization`) and the configuration revision.
  - A configuration change is therefore a new scope, and results from the
    old configuration are not attached.
  - The service must build the scope only from the caller's own verified
    binding. A client-supplied value is never a scope.
  - On the live path the scope also folds in the caller's reply reserve
    (`with_reply_reserve`), so a reply packed for one reserve is never
    delivered to a call that needs another. The principal and the index, and
    so every limit, are unchanged by it.
- **Attach by digest.** A request attaches to an existing job only when its
  scope and request digest are both equal, and only under the no-loss rule
  above.
  - The request digest covers the schema, the question and every budget
    field except `deadline_ms`. The continuation is not part of it.
  - The same question asked in another scope is a separate job.
- **Continuations.** An explicit continuation attaches only within its own
  scope, and only to the same request on a current publication.
  - An unknown, expired or foreign continuation id gets one identical
    refusal, so an id never reveals whether another scope's job exists.
  - The id of a job that was dropped without running is honoured once,
    within its own scope and for the same request, while its claim lasts:
    the call is admitted afresh in that job's place. In any other scope, and
    after the claim is used or expired, it is an unknown id like any other.
  - A mismatched request and a superseded publication are refused
    separately, but only within the scope that owns the job.
- **Ids.** A job id is a framed SHA-256 over a random 32-byte salt generated
  per table, the scope, the request digest and a sequence number. Ids are
  therefore not derivable from a request, and they do not repeat across
  restarts.
- **Publications.**
  - The table records the publication a job is expected to run on at
    dispatch. That is the newest one observed for its index, or the
    caller's.
  - The runner's report replaces it.
  - A started or finished job serves a caller only on the caller's
    publication or, when the caller does not know it, on the newest one
    observed.
- **`publication_changed`.** It takes the index and the publication from the
  service's own publication observer, never from a caller.
  - An older generation of the same incarnation is ignored.
  - Otherwise, every finished result and failure of another publication on
    that index is dropped at once. Each job running on another publication
    is superseded: it is never attached, its continuation is refused, and
    its outcome goes to its waiters but is not kept.
  - A result that finishes on a publication other than the newest observed
    one is not kept either.
- **Retention.**
  - A result is kept for 10 minutes after it finishes, unless it is dropped
    earlier as above.
  - Each retained entry, a failure included, is charged for its result
    bytes, its question and a 1 KiB overhead. The cap is 64 MiB and 1,024
    retained entries, and the oldest entry is evicted first. No entry is
    free.
  - Waiting calls hold their own reference, so eviction never loses a
    result that a waiter was owed.
  - Nothing is persisted. A service restart drops the table.
- **Queue status** is aggregate. It holds the gathering limit and the counts
  of running, queued and retained jobs and bytes. Each entry gives its state,
  position, wait, estimated start and finish and shape, and status also
  carries the cost correction and the recent cost samples. It never carries a
  job id, digest, scope, question or result.

## On the live path

An `airk_answer` call that reaches a project owner running in the service goes
through the table. Every other composition (stdio, a child-process owner, a
local session, a server without a table) calls the kernel directly, exactly as
before. So does a combined Answer member, which runs under its parent's
allowance and is never a job of its own.

**The pieces.**
- **Kernel** (`airk-kernel`, `arbiter/answer/admission.rs`): `AdmissionPlan`,
  `Kernel::answer_admitted`, `Kernel::answer_scope` and
  `Kernel::answer_context_model`. Their contract is in the kernel arbiter
  contract, "Admission plan".
- **MCP seam** (`airk-mcp`, `answer_jobs.rs`): the `AnswerJobs` trait with
  `AnswerCall`, `AnswerAdmission`, `AnswerFailure` and `AnswerDelivery`, the
  handler's caller side (`run`) and the job's work. `airk-mcp` cannot depend
  on this crate, so the service implements the trait.
- **Service** (`wiring.rs`): `ServiceAnswerJobs` implements `AnswerJobs` for
  the table, and the service state owns it. `install` and `for_control` make
  it findable by control root, because a project owner reaches its service
  only through the supervisor's plain-function `Hooks`
  (`Hooks::with_answer_jobs`).
- **Owner** (`airk-supervisor`, `project_worker.rs`): each query connection's
  session is given the table once (`LocalTools::set_answer_jobs`).

**One call.**
1. The handler admits the call and selects its read snapshot as for any read.
   It validates the request and asks the kernel for the verified binding
   (`answer_scope`) and the Context model. Nothing a client sent is part of
   the scope.
2. It hands the table the binding, the request with its deadline clipped to
   the time the call really has, the Context model, its reply reserve, the
   deadline of its wait and its work.
3. The table attaches, admits, tickets or refuses, as above. The scope is the
   binding's digest with the reply reserve folded in
   (`AnswerScope::with_reply_reserve`): a reply is packed to its maker's
   reserve, so calls with different reserves never share a job. The
   publication is the binding's.
4. The call waits for the job until shortly before its own deadline, or until
   its own cancellation, or until its owner stops. A tenth of its time, at
   most 2 s, is kept back so that a ticket is delivered inside the deadline.
   A caller that is cancelled or times out ends only its own wait.
5. The calling handler renders what comes back under the caller's own
   response mode, verification controls, usage span and delivery guard:
   - a finished reply, also for a call that attached;
   - a ticket-shaped reply for a job still queued or running (below);
   - `answer_queue_busy` when nothing was queued: a typed, retryable tool
     error with `retry_after_ms` and `query_submitted: false`;
   - the job's failure, with the typed receipt the direct path would have
     rendered when the job kept one.

**Replies for a job without a result.** The Core reply contract allows a
reply without parts only in the ticket shape: partial at `admission` with
reason `queued`, no continuation in the marker, and the ticket's id as the
handle.
- A queued job's reply is its ticket.
- A running job's reply is also a ticket, with position 1, start 0 and
  the degradations fitted at its dispatch.
- `PartialMarker::continuation` stays for a reply that carries gathered
  work.

`pending_reply` builds both and validates them against the request budget
before returning.

**One job.** The work belongs to the call that made the job.
- It runs `Kernel::answer_admitted` on the owner's blocking pool, under the
  plan the table fitted at dispatch. The owner's threads carry its execution
  scope, and the job's reads block on the owner's runtime.
- Its read context is the job's own (`ReadAttemptContext::job_owned`): the
  maker's guards, custody and native attempt, a deadline no later than the
  maker's, and a cancellation of its own.
- It is live until its own deadline or until its owner's admission closes. An
  owner that stops ends the job at its next fence.
- While it runs, its owner counts it (`LocalTools::answer_jobs_in_flight`),
  so the owner does not leave as idle although the maker's connection may be
  gone. A queued job holds nothing and keeps no owner: if its owner has left
  when its turn comes, it is dropped as not run and its place is kept.
- A job whose maker has left runs outside the owner's query broker. The
  table's gathering limit is what bounds such jobs.
- Progress goes to the maker's progress token. A call that attached gets
  only its own reporter's heartbeat.

**A job never runs outside its maker's window.** Every card and Focus call
carries the maker's native attempt, which the service checks and which ends
with the maker's original deadline; the kernel's original deadline can only
shorten. So:
- **Inside the window.** A job that did not wait, or that waited and still
  fits, runs to its maker's deadline. Until then it survives its maker's
  disconnect or client timeout, and a retry attaches to it.
- **Beyond the window.** A job whose window the table opened past its
  maker's (a ticketed job at its dispatch, or a requeued one) is not run. Its
  work reports `not_run`. The table drops it, keeps its place in line as a
  claim, and keeps no failure.
- **A call still waiting on such a job** is submitted again once with its own
  work, if it has at least a second left. Otherwise it is told
  `answer_queue_busy` with a 1 ms retry.
- **So a ticket holds a place in line, not a running job.** The retry, by
  digest or by the ticket's id, is admitted in that place with a window of
  its own.

Superseded by L2c: a job now mints a native attempt of its own at dispatch
("Service-owned jobs"); the maker's attempt is not replaced, only not used.

**Failures.**
- A failure is kept for the retention time on its publication, as above,
  unless a retry could repeat nothing. Not kept: a job that never ran, a job
  whose owner was stopping (`Draining`, a closing admission or a closed job
  context), and a refusal typed safe to repeat (`read_retry_hint`). These go
  to the calls waiting on the job and are then forgotten.
- A failure carries the handler's own encoding: the message and, where the
  direct path would have rendered a typed receipt (a stale refusal, a scope
  refusal, an argument error), that receipt. Only `airk-mcp` writes and reads
  it. It is held only in service memory and delivered only within the job's
  scope.

**Not wired yet**, each with its owner:
- **Queue status on the status surface:** deferred until after LG (primary's
  decision, 22 November 2026). It is within the plan's authorized many-agent
  work (sections 12.8 and 12.9). The status response gains fields, so the
  wire-shape change is recorded when it is built. Owner: L2b.
- **`publication_changed`** from the service's publication observer. Until
  then a result of an older publication is never served, because every
  caller states its own publication, but it is dropped only by retention and
  the caps. Owner: L2b.
- **`set_runtime_capacity`** from L5b's lane capacity: the gathering limit is
  the floor of 2. Owner: L5b with L2b.
- **`set_costs`** with measured stage costs: the placeholders stay
  unmeasured. Owner: L5b and L7.
- **Custody** (change log entry 21): the job's reply is kept; its custody is
  still dropped. Owner: L2b.
- **Progress for attached callers:** a call that attached hears only its own
  reporter's heartbeat, not the job's stages. Owner: L2b.
- **A job past its maker's deadline:** built in L2c ("Service-owned jobs"),
  bounded by its lease (the residual recorded there).

`expect(dead_code)` stays only on those unwired entry points and on fields
that only the tests read. The separate security review of the wired path
follows.

## Focused checks

Run `cargo nextest run -p airk-service --lib -E "test(/answer_jobs::/)"`. The
tests use explicit clock values for the schedule and paused Tokio time for
the table. They wait on events under deadlines, including a test hook that
counts waiting calls, and never sleep. They cover:
- attach after a client timeout and after a disconnect, both observed
  waiting before the release;
- concurrent Answers whose later arrivals have earlier deadlines, run in
  earliest-deadline order within the limit;
- the ageing bound;
- a ticket, and degrade-to-fit disclosures, including a slower Compact;
- requeue only after a real wait;
- no-loss attachment;
- publication supersession, older notifications and stale results;
- retention charges and count caps;
- scope, configuration and principal isolation;
- `Busy`;
- status without identities, and learning only from uncut runs;
- runner panics, both before and inside the future;
- a dispatch from a non-Tokio thread.

The wired path is tested in `wiring/tests.rs`
(`test(/answer_jobs::wiring::/)`), through the real `airk_answer` handler
(`LocalTools::call_with_read_context`), the real kernel loop over a small
real index and the real table. The native controller is a disabled one, so
these tests claim nothing about models. A gate at the `AnswerJobs` seam holds
each job's work until the test opens it; every wait is on an event under a
deadline. They cover:
- two and four concurrent Answers: all complete, two at a time, none starved
  and none run twice;
- a retry after the client's transport cancelled the call: it attaches to
  the job that kept running, and a later call gets the kept result without
  another run;
- a request whose work cannot fit its deadline: it runs reduced, says so,
  and the loop stays inside the plan;
- a request that cannot start in time: a ticket at once, and a complete
  answer on its retry;
- a full scope: `answer_queue_busy` with a retry time, and nothing queued.

At the seam, without a handler or a kernel:
- a job that did not run is dropped, and the call waiting on it gets a job
  of its own;
- a job is fitted on its own rung and only drops to the no-model loop;
- an owner finds only the table of its own service.

Every reply of those tests, the ticket-shaped one included, also passes the
check the delivery planes make on a singleton Answer
(`context_delivery_bindings`: Core validation under the widest budget).

In a real service (`airk-cli`, `tests/combined_reads.rs`,
`a_repeated_answer_in_one_service_is_the_reply_its_job_table_kept`): the
service gives its table to the project owner through the supervisor hook,
and the same Answer sent three times over the control pipe is run once and
then answered by the kept reply. This is the one test of the injection
itself.

On the pure schedule:
- a result its deadline cut never answers a call with more time, and teaches
  the cost correction nothing;
- unmeasured costs never reduce a request that did not wait, still reduce
  one that waited, and still learn;
- a job dropped without running keeps its place in line for the retry, by
  digest and by its ticket, once and only in its own scope;
- a place in line expires with the retention time.

## L2c evidence

- **Supervisor** (`project_worker::scopes::answer_job_tests`): a job kernel
  outlives its maker's session teardown (H1; failed on the old code), and
  ends with a revocation, a newer grant revision, lease expiry (also for a
  queued job), lease renewal and the owner's close; at most 64 a lease.
- **MCP** (`answer_jobs::tests`): the job runs on its own kernel and its own
  attempt past its maker's window after the maker's kernel is revoked; a
  revoked job kernel is refused with `answer_authority_ended`, never kept; a
  job past its deadline and grace quarantines its owner, stops counting as
  running and holds its capacity until its work settles.
- **Service** (`answer_jobs::wiring::tests`, through the real handler where
  a handler is involved): two and four `fresh` concurrent calls are two and
  four executions; isolation across grants and `include_status`; key
  recovery, key conflict and keys that do not cross authorities; bounded
  caller input; a panicking job fails its own callers and a retry runs
  again; eviction under the byte cap while callers wait; a new publication
  is never answered from the old one; a stuck job's capacity is not reused
  before it settles; `interactive` never delays an attended job; reuse only
  of work asked with the same deadline.
- **End to end** (`airk-cli` `tests/combined_reads.rs`,
  `answer_jobs_outlive_their_makers_session_and_are_recovered_by_their_handle`):
  through the real service, supervisor `query()`, owner and handler, with
  the test gate. The maker's session is torn down before its job may run;
  the job's scoped reads succeed, and the result is recovered by handle with
  one execution. Repeated for a queued job with no polling client. A
  mutation that restores the shared revocation flag makes it fail.
- **Not covered end to end:** revoking the grant while the maker is gone.
  The live service offers no test-side grant revocation short of the catalog
  control plane, so this stays at the supervisor level (above): there, the
  revoked job kernel refuses its next read, and the revoked entry refuses a
  new session, so nothing can be delivered under it.
