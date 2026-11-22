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

- **Inputs.** `scope` is an `AnswerScope` built from the caller's own
  verified `ScopeBinding`. `request` is a validated `ContextRequestV3`.
  `model` is the Context rung the configuration would plan with (`None` means
  the no-model loop). `publication` is the caller's current publication
  digest when it is known. `call_deadline` is when this call must return.
- **Replies:**
  - `Finished`, with the outcome and whether the call attached;
  - `Pending`, with an `AnswerTicket`: `running` says whether the job has
    started (the reply is then a partial with this continuation) or is still
    queued (the reply is a ticket);
  - `Busy`, with `retry_after_ms`, when nothing could be queued.
- **The runner** receives a `StartedJob`:
  - the request;
  - the applied `Shape` (parts, sub-needs, rounds and Context rung);
  - the degrade-to-fit `Degradation`s, which the reply must disclose;
  - `run_ms`, the time from dispatch to the job's own deadline;
  - `requeued`, set when the wait outran the caller's deadline.

  It returns the value, the publication the job ran on and the bytes to count
  against retention.

## Admission

Admission estimates the queue wait and the work, and then does the first of
these that applies:
1. **Admit.** The request runs at its requested shape when the estimate fits
   its deadline.
2. **Degrade to fit.** Reductions are applied one step at a time, cheapest
   loss first:
   - rounds down to two;
   - Large to Compact;
   - rounds down to one;
   - sub-needs and parts down to two, then down to one;
   - last, the no-model loop.

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
   partial at the deadline.

The shape is fitted again at dispatch against the time actually left. If the
wait has outrun the caller's deadline, the job runs on a fresh window of its
own deadline, flagged `requeued`, so that the caller can attach. The work
outlives the call.

**Estimator.** The work is `fixed + parts x (per_part + sub_needs x rounds x
per_sub_need_round)` for the job's rung. That figure is scaled by a learned
correction: an EWMA of elapsed over estimated time, kept between 0.25 and 4.
The default costs are placeholders until L5b and L7 measure them per device
(answer-loop-changes entry 19).

**Busy.** A scope may hold `max_active_per_scope` queued and running jobs
(default 8), and the table holds at most 4,096. Past either limit, the call
gets `Busy` and no job is made. So one client cannot fill the lane.

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
- **Each started job runs on its own task.** Dropping or timing out a call
  never cancels the job. A runner that panics finishes the job as `Failed`
  and frees its slot.

## Security: attach, retention and status

- **Scope.** `AnswerScope::from_binding` digests the installation, catalog,
  index and the caller's source-authority fingerprint (`ScopeBinding.
  authorization`). The service must build it only from the caller's own
  verified binding. A client-supplied value is never a scope.
- **Attach by digest.** A request attaches to an existing job only when its
  scope is equal and its request digest is equal.
  - The request digest covers the schema, the question and every budget
    field except `deadline_ms`. The continuation is not part of it.
  - The same question asked in another scope is a separate job.
- **Continuations.** An explicit continuation attaches only within its own
  scope, and only to the same request.
  - An unknown, expired or foreign continuation id gets one identical
    refusal, so an id never reveals whether another scope's job exists.
  - A mismatched request is refused separately, but only within the scope
    that owns the job.
- **Ids.** A job id is a framed SHA-256 over a random 32-byte salt generated
  per table, the scope, the request digest and a sequence number. Ids are
  therefore not derivable from a request, and they do not repeat across
  restarts.
- **No replay.** A finished result attaches only when it ran on the caller's
  current publication, when that publication is supplied. A failed job is
  attached and not replayed for the whole retention window.
- **Retention.**
  - A finished result is kept for 10 minutes after it finishes. It is
    dropped earlier when `publication_changed` reports another publication
    of its index.
  - Retained bytes are capped at 64 MiB by default, and the oldest result is
    evicted first. Waiting calls hold their own reference, so eviction never
    loses a result that a waiter was owed.
  - `publication_changed` is called only by the service's own publication
    observer, never on a caller's word, because it drops other callers'
    results on that index.
  - Nothing is persisted. A service restart drops the table.
- **Queue status** is aggregate. It holds the gathering limit and the counts
  of running, queued and retained jobs and bytes. Each entry gives its state,
  position, wait, estimated start and finish and shape, and status also
  carries the cost correction and the recent cost samples. It never carries a
  job id, digest, scope, question or result.

## Wiring

The table is not yet on the live call path. The MCP Answer handler is being
switched to v3 by L3, and the v1 and v2 paths are being deleted. The wiring
is a follow-up, listed in `docs/arbiter/answer-loop-changes.md` entry 19.

## Focused checks

Run `cargo nextest run -p airk-service --lib -E "test(/answer_jobs::/)"`. The
tests use explicit clock values for the schedule and paused Tokio time for
the table. They wait on events under deadlines and never sleep. They cover:
- attach after a client timeout and after a disconnect;
- several concurrent Answers in earliest-deadline order within the limit;
- the ageing bound;
- a ticket, and degrade-to-fit disclosures, when the deadline cannot be met;
- requeue after an outrun deadline;
- retention, publication eviction and the byte cap;
- scope isolation of attach and continuations;
- `Busy`;
- status without identities;
- a panicking runner.
