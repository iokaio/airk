# Answer job table

Owner: Service runtime. Plan section 12.4.1, step L2b. An admitted Answer is a
job owned by the service, not by the client's connection. The table admits,
attaches, tickets, schedules and retains Answer jobs. It runs nothing itself:
the runner it is built with performs one Answer, and the kernel's v3 driver
still owns everything inside the Answer.

## Interface

`AnswerJobTable::call(scope, request, model, publication, call_deadline)` is
the one entry point for the service's Answer path. The other entry points are
`status`, `set_runtime_capacity`, `set_costs` and `publication_changed`.

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
  Both are recorded in the recent samples as `cut`.
- The default costs are placeholders until L5b and L7 measure them per
  device (answer-loop-changes entry 19).

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
- **Each started job runs on its own task**, and the runner is called inside
  that task. Dropping or timing out a call never cancels the job. A runner
  that panics, before or after returning its future, finishes the job as
  `Failed` and frees its slot.
- **Waiting.** A caller still receives an outcome that is signalled at its
  deadline, or one that the table has since stopped retaining.

## Security: attach, retention and status

- **Scope.** `AnswerScope::from_binding` digests the installation, the
  catalog, the index, the caller's source-authority fingerprint
  (`ScopeBinding.authorization`) and the configuration revision.
  - A configuration change is therefore a new scope, and results from the
    old configuration are not attached.
  - The service must build the scope only from the caller's own verified
    binding. A client-supplied value is never a scope.
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

## Wiring

The table is not yet on the live call path. Built and tested now, in this
crate:
- the table;
- per-call work;
- the reply builder `reply::pending_reply`;
- the outcome adapter `reply::finished`;
- delivery, `reply::deliver`.

The remaining steps are designed below. They touch files that L3-10 (b) is
still editing, so they land after it, against its handler shape: the
`answer` tool takes `RequestContext`, reports progress through
`ProgressReporter`, and calls `kernel.answer_with(request, AnswerEnv {..})`
inside `execute_verified_response`.

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

1. **Kernel: admission plan and scope.**
   - `AnswerEnv` gains `admission: Option<&AdmissionPlan>`. The plan is a
     kernel type: parts, sub-needs, rounds, whether the Context model is
     used, and the degradations.
   - `answer_with` bounds `Gathering` by the plan's shape, and when the
     plan says so it runs the no-model loop. It appends the plan's
     disclosures to the reply before validating it.
   - A new `Kernel::answer_scope()` returns the binding that `answer_with`
     checks before it runs. The handler builds its `AnswerScope` from that,
     never from client input.
   - **The rung.** Until the kernel can choose a Context rung per Answer,
     which needs L5's lifetime switch, the service passes the configured
     rung as `model`. Its cost table holds only that rung and the no-model
     loop, so the ladder degrades only to no model.
2. **Reply builders:** done (`reply.rs`, above).
3. **Service: ownership and work.**
   - The service state owns one `AnswerJobTable<ContextResponseV3>`, built
     at start with `TokioClock` and `set_runtime_capacity` fed from L5b's
     lane capacity.
   - For each new job, the handler's `JobWork` captures an `AirkServer`
     clone whose read context belongs to the job. That context is built
     with `ReadAttemptContext::new(now + run_ms, fresh token)` and keeps the
     original lifetime guard and delivery-guard factory. It is not a child
     of the calling request's cancellation, so a client that disconnects
     cancels only its own wait.
   - The work runs `answer_with` on a blocking thread (`spawn_blocking`,
     because its reads use `block_on`), clamped by the plan, and returns
     `reply::finished(outcome.response)`.
4. **MCP: the seam.**
   - A new `airk-mcp` module defines an object-safe `AnswerJobs` trait, because
     airk-mcp cannot depend on this crate. Its one method takes the binding,
     the request, the call deadline and the work, and returns a delivery:
     a reply (finished or ticket, and whether it attached), a failure, or
     busy with a retry time.
   - `AirkServer` gains an optional `Arc<dyn AnswerJobs>`. With one, the
     handler goes through the table; without one (the standalone stdio and
     local paths), it calls the kernel directly as today.
   - The service implements the trait for its table and injects it where
     it builds in-process owners.
   - Busy becomes a typed tool error, `answer_queue_busy`, carrying its
     retry time. A failure keeps today's failure mapping.
   - The call deadline is the handler's own deadline: the request's,
     bounded by the read attempt. At that deadline a running job's kernel
     returns its own partial, so a ticket reply results only when the job
     waited or was requeued.
5. **Status.** `QueueStatus` needs a place on the service status surface.
   Adding fields to a status response changes a wire shape, so this needs a
   recorded contract decision before it is built.

**After wiring:** tests through the real handler must show several
concurrent Answers, attach after a client timeout, and a ticket or a
disclosed degradation. Then remove `expect(dead_code)` on `mod answer_jobs`,
and the separate security review follows.

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
