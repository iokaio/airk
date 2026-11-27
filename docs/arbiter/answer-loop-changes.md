# Answer loop: changes made under the user's standing authorization

On 22 November 2026 the user wrote: "you are authorized to make all the
changes necessary that you would recommend to make section 12 work properly.
Document each of these changes so that I can review them after you complete
L7."

This log records each change that implementation made under that authorization:
changes that section 12 of the [plan](final-plan-fp16.md) does not already
spell out, or that differ from it. The user reviews them after L7. Each
entry gives the date, the step, what changed, why, the evidence, and where it
is recorded in code and contracts.

The user added the same day: "The goals have not changed but the constraints
should only be a guide. When you need to change them to achieve the goal, do
and document it for review." So section 12's limits, defaults, design choices,
sequencing and gate mechanics are guides. Any change to them is logged here.
The identity rules, honest reporting, the ban on weakening checks, and the
limits on publication still apply in full.

## Entries

### 1. Answer reply limit 128 KiB (user decision, recorded for completeness)

- **Step:** L1.
- **What:** Answer v3 replies may be up to 131,072 bytes. Every other tool and
  v2 keep 65,536.
- **Why:** the user set it on 22 November.
- **Where:** plan section 12.9, and the airk-core arbiter `CONTRACT.md` entry
  for wire contract version 3.

### 2. Compact decision log in the reply

- **Step:** L1.
- **What:** each decision in the reply is a compact record. The full card
  lists, scores and roster detail move to a decision log in service custody.
  The reply carries the log's digest and counts. Decisions may be omitted to
  fit, and the omission is disclosed.
- **Why:** an independent review measured about 85 decisions, roughly 76 KB,
  for a full 4-part loop at the defaults. That alone exceeds the old 64 KiB
  frame, and none of it could be dropped, so a compound Answer would have
  failed instead of returning a disclosed partial reply.
- **Where:** airk-core `answer_loop.rs` and its `CONTRACT.md` entry.

### 3. Card slots separate from decision ordinals

- **Step:** L1.
- **What:** decisions are numbered in sequence. Only model-scored calls take a
  card slot, which is limited by `context_calls`.
- **Why:** deterministic decisions were consuming the model-call slots, so a
  4-part Answer ran out after about 40 decisions even when no model had been
  called.
- **Where:** airk-core `answer_loop.rs` and `native_loop.rs`.

### 4. Budget limits raised to the 4 × 4 × 3 shape

- **Step:** L1.
- **What:** read calls max 160 (default 132). Card calls (slots) max 64
  (default 48). Derived bounds are documented with their arithmetic in the
  contract.
- **Why:** round 1 at the maximum shape needs 100 reads and the full loop
  132. The first limits guaranteed a Budget stop.
- **Where:** airk-core `CONTRACT.md`.

### 5. Split chooses a whole partition

- **Step:** L1.
- **What:** one Split decision picks an ordered partition of up to 4 ranges.
- **Why:** one break per decision could not express 3 or 4 parts.

### 6. Restored v2 information in v3

- **Step:** L1.
- **What:** v3 regains:
  - the clarification-required and no-justified-read stops;
  - the v2 gap kinds;
  - each decision's basis (as counts and a digest, with the detail in the
    log);
  - typed reasons for unavailable models;
  - work-unknown flags;
  - the latest-decision applicability rule.
- **Why:** the first draft dropped them, against the rule that inputs and
  fields are preserved across views.

### 7. Partition allowance version 3

- **Step:** L1.
- **What:** a new allowance whose parent is the v3 budget, used by the native
  v3 requests.
- **Why:** with the v2 allowance, every partitioned v3 call would have been
  limited to 8,192 tokens and 120 s.

### 8. Evaluation-only bounded-hybrid admission for the gates

- **Step:** L5 (decided).
- **What:** the gates run the FP16 Compact and Large graphs through a new
  evaluation-only admission that reuses the release verifier. It runs under
  the scientific runtime manifest, so no approval pin moves. The Compact
  product route is built in parallel.
- **Why:** both FP16 graphs are reachable today only through the release
  path, which requires a qualified product profile.
- **Where:** the private design `L5-cuda-prep.md`. The CUDA `CONTRACT.md`
  records it when it lands.

### 9. One calibration-set document per profile

- **Step:** L1.
- **What:** a profile's single calibration asset is now one document that
  holds the calibration of each card task.
- **Why:** a profile can carry only one calibration asset, and the loop has
  four card tasks.

### 10. Focus on CUDA uses the 1,024-token W8 graph

- **Step:** L5 phase A.
- **What:** the primary Focus CUDA route is the 1,024-token graph `a16fcaa1`.
  - It has W8 weights with FP32 activations, the same graph and weights as
    the CPU product profile.
  - It opens under strict GPU placement, with all 1,250 nodes on CUDA, at
    about 114 ms per warm call and 2.2 GiB of GPU memory.
- **Why:**
  - It gives the same evidence as CPU Focus, and it works today.
  - An FP16 export at 1,024 tokens is blocked, because the exporter refuses
    the W8 source.
  - W8 is smaller than FP16, so rule A1 holds.
- **Precision check (12.7.3):** the Focus arms are FP32, FP16 and W8, all
  at 1,024 tokens. FP32 and FP16 come from a new static-1,024 export of the
  FP32 source `19b20ad4`. The 512-token FP16 route (about 40 ms per call)
  is used only if that export fails.
- **Evidence:** the L5A receipts (`g1`, `g2`, `g4`, and the census).

### 11. First CUDA measurements of the protected Compact and Large graphs

- **Step:** L5 phase A. These are measurements; no change was made.
- **Runs:** raw scientific sessions, not yet the scorer path.

| Model and graph | Time at 1,024 tokens | Time at 8,192 tokens | GPU memory |
| --- | --- | --- | --- |
| Compact `147be075` | 186 ms | 1.69 s | about 11.3 GiB |
| Large `c1b57761` | 426 ms | 4.3 s | about 21 GiB, already at 32 tokens |

- **Earlier figure superseded:** inventory C's Compact figure (0.66 s,
  6.3 GiB) came from a different graph.
- **Follow-up:** Large's memory at small inputs is being investigated in
  phase B.

### 12. Evaluation-only bounded-hybrid admission and protected emitter

- **Step:** L5 phase B, in progress.
- **What:** implemented as entry 8 describes.
- **Identity:** digest-input movements are listed in the L5 identity-moves
  record, and each is recorded in the owning contract.

### 13. Receipts and the full decision detail live in the decision log

- **Step:** L1.
- **What:** the Answer v3 reply carries the compact decisions and a summary
  of the decision log. The Focus batch receipts and card receipts live only in
  the log, which is service custody. The reply's Focus accounting reconciles
  against the log summary and against the Focus receipts embedded in the
  searches.
- **Why:** they cannot fit in the reply frame together with the evidence.
- **Measured:** a full default loop (85 decisions, 48 records) is 105,079
  bytes, leaving 25,993 bytes of room under 128 KiB. It would not fit 64 KiB.
- **Budget limits finally set:**

  | Limit | Maximum | Default |
  | --- | --- | --- |
  | Read calls | 160 | 132 |
  | Candidates | 160 | 132 |
  | Card calls | 64 | 48 |
  | Focus pairs | 384 | 192 |
  | Rounds | 3 | 3 |
  | Sub-needs | 4 | 4 |
  | Parts | 4 | 4 |

- **Where:** airk-core `answer_loop.rs`, `answer_loop/log.rs` and the
  contract's budget table.

### 14. Shared transport caps raised for Answer in L2

- **What:** these caps refuse or cut an Answer reply over 64 KiB, and are
  raised for Answer only in L2:
  - the MCP reply budget swap and response checks;
  - the remote stdio line cap;
  - the frontend combined Answer limits;
  - the console client;
  - the kernel Focus reply budget.
- **Already enough:** the service and Arbiter frames are 4 MiB, and the
  VS Code stdio session is 16 MiB.

### 15. The reply is checked against its decision log

- **Step:** L1.
- **What:** `ContextResponseV3::validate_with_log` joins the reply to its
  decision log. It checks:
  - the summary and the digest;
  - each included decision against its log entry;
  - the question and scope;
  - every part, need and round;
  - the order in which decisions were omitted;
  - the Split partition.

  The kernel must call it before delivery.
- **Why:** once receipts moved into the log, the reply's own checks were
  self-attested.
- **Related rules:**
  - Loop searches carry no embedded Focus receipt, so Focus is batched only.
  - Records in the reply refer to the reply's scope instead of repeating it.
  - The deadline is checked against a measured `elapsed_ms`, with 1 s of
    tolerance, because summed work overlaps.

### 16. Realistic size: evidence must come first in the reply

- **Step:** L1, measured. This is a rule for L3.
- **Measured:** with realistic search records (the medians of 115 real
  records, about 6.8 KB each), a full 4 × 4 × 3 loop is 393,844 bytes, about
  three times the 128 KiB frame. Packed under L1's rules, it keeps only 9 of
  48 records, beside all 85 decisions.
- **The rule for Union, L3:**
  - Evidence comes first. Decisions are omitted before records, and are
    reported through the log summary; the full log stays in custody.
  - Loop evidence is sent as compact spans per sub-need (location, content
    hash and the excerpt that was read), not as whole search records with
    their metadata.
  - L3 measures the realistic full loop again, and reports how many records
    and spans survive.
- **Why:** an agent gains from evidence, not from decision bookkeeping. This
  follows the user's guidance that constraints are guides.

### 17. FP16 CUDA profiles for Compact and Large, measured on the A6000

- **Step:** L5 phase B.
- **What:** the evaluation-only admission is built, so the protected graphs
  open through Answer's scorer path with an unchanged release verifier.
  Profiles are emitted with memory measured in that path.

| Profile | GPU memory declared | Measured peak | Warm call at 1,024 / 8,192 tokens |
| --- | --- | --- | --- |
| Compact FP16 | 8 + 2 GiB | 6.24 GiB | 201 / 1,742 ms |
| Large FP16 | 18 + 2 GiB (17 before review) | 15.24 GiB | 443 / 4,388 ms |

- **Review correction:** an independent review found that the memory summary
  had read only the smallest width. Re-deriving from every width raised
  Large's figures to those in the table, and Large was re-emitted and
  confirmed. Compact stayed at 8 GiB. The review also found five digest-input
  movements that had not been recorded; they are now recorded in the CUDA
  `CONTRACT.md` section "L5 digest-input movements".

- **Large fits 24 GB.** Large's earlier 21 GiB came from ORT's memory
  pattern in the raw session. The scorer path disables the pattern and uses
  14.2 GiB.
- **Numeric replay on the 100-row CAL panel:** FP16 against FP32 stays
  within 4e-4 raw on both models, and every winner agrees. Compact and Large
  agree on only 54 of 100 winners, so model size matters far more than
  precision, as 12.7.3 expected.
- **Focus at 1,024 tokens on CUDA:**

  | Focus arm | Warm call | GPU memory |
  | --- | --- | --- |
  | W8 | about 114 ms | 2.4 GiB |
  | FP16 | 51–56 ms | 2.25 GiB |
  | FP32 | about 107 ms | 3.24 GiB |

- **Identity movement:**
  - The G5 and protected-MHA math digests moved, so FP16 CUDA profiles
    emitted before this change no longer open. All of them were private and
    unqualified.
  - The `cpu_source_transition` pins are still stale: the same 9 tests fail,
    to be resolved in L4.
- **Recorded in:** the CUDA `CONTRACT.md` section "L5 evaluation diagnostics
  and protected admission".

### 18. Answer's own time ceilings, reply limit, progress and client timeouts

- **Step:** L2, commit `756e0448`.
- **Ceilings for Answer:**
  - a 600 s deadline (300 s default) and a 605 s read-attempt ceiling, set in
    Core, the host, MCP, IPC, the supervisor, the service, the frontend, the
    HTTPS gateway, `airkctl`, `airk-cli` and the VS Code extension;
  - a 131,072-byte reply limit.
  - Every other tool keeps 125 s and 65,536 bytes. The persisted resource
    policy is unchanged.
  - Nested reads may run to 605 s, so that an Answer's own reads keep
    working without moving the Focus digest closure. The entry points still
    refuse other tools above 125 s.
- **A fix found on the way:** the HTTPS gateway was capping every HTTP
  Answer at 125 s. It no longer does.
- **Progress:** MCP progress notifications for Answer, at most 1 per second,
  with a heartbeat every 15 s. Across processes, only the heartbeat travels
  for now. Carrying the driver's part, round and stage events across the
  worker, service and frontend needs a streaming IPC channel, which is a
  follow-up.
- **Codex:** `tool_timeout_sec` is set to 605 where `airkctl` writes agent
  configuration. The documentation section "Long Answer calls and client
  timeouts" covers Claude Code and Codex.
- **Measured:** headless Claude Code, version 2.1.330, against a test
  server.

  | Transport | Call length | Progress sent | Result |
  | --- | --- | --- | --- |
  | stdio | up to 610 s | yes or no | waited |
  | HTTP | 330 s | every 10 s | waited |
  | HTTP | 330 s | none | aborted at 300 s, "sent no response or progress for 300s" |

  So over HTTP, which is AIRK's agent transport, progress is required. The
  loop's driver reports at every stage change, and the heartbeat is the
  floor.
- **Kept:** the 300 s default.
- **Review fixes** (L2's second commit): the HTTPS output stream of an
  Answer call now runs to Answer's ceiling instead of the ordinary 120 s, and
  an HTTP session with a call in flight no longer idles out (rmcp's
  keep-alive closed it after the idle timeout even while an Answer ran). No
  progress notification follows a call's result. `agents repair` also
  raises a missing or short Codex `tool_timeout_sec` on an otherwise healthy
  entry. Service stop and upgrade still cancel an in-flight Answer within
  their existing 10 to 35 s owner stop periods rather than waiting up to
  605 s for it.
- **Closed gap (22 November 2026, branch `loop/mcp-idle`):** the review-fix
  test `an_answer_in_flight_keeps_its_http_session_past_the_idle_timeout`
  aborted after about 280 s on a 128 GiB allocation. The cause was in the
  test only. Its 400 s paused-time sleep made rmcp's real-clock SSE
  keep-alive send pings in a tight loop, and the test collected the body
  without a bound. The keep-alive is now a private constructor parameter,
  and production keeps 15 s. The test turns it off and reads the body under a
  64 KiB bound. With the keep-alive turned back on, it now fails in 0.024 s
  with "length limit exceeded". Production cannot buffer without bound: on
  the real clock, a ping is sent at most every 15 s, and the stream is
  pulled only as the client reads, under the gateway's `OutputBody` budget.
  Record and evidence: `crates/airk-mcp/src/remote/CONTRACT.md`.
- **Closed gap (22 November 2026, branch `loop/l2b-2`): a live HTTPS Answer
  past 120 s.**
  - **Fault:** no test held a live TLS connection past 120 s. The
    connection fence also read the system clock while the stream bound read
    Tokio's, so the IO deadline could not be driven under paused time.
  - **Change:** the fence's IO deadline now reads Tokio's clock (`io_now`),
    which is the system clock in production. The per-connection serve is one
    function (`Gateway::serve_accepted`). Test builds can turn rmcp's
    real-clock keep-alive off (`HttpReadService::without_sse_keep_alive`,
    `test-support` only).
  - **Evidence:**
    `a_live_tls_connection_keeps_an_answer_open_past_120_seconds_to_its_result`
    passes. Its head arrives under 1 s and its result after at least 130 s,
    with every body read bounded. With the fence widening disabled, it fails.
  - **Record:** `crates/airk-mcp/src/remote/CONTRACT.md`.
- **Closed gap (22 November 2026, branch `loop/l2b-2`): the stop and upgrade
  wait (L2 M3).**
  - **Fault:** the worst service stop, modelled on an injected clock in
    source order, is about 60.25 s:
    - the 30 s arbiter cutoff, with the 10 s connection drain and the 5 s
      revoke inside it;
    - a 0.25 s supervisor turn;
    - the in-process owners' 10 s grace and 20 s drain, in parallel.

    That is above the 60 s that `airkctl service stop|restart` and the setup
    drain waited. So a stop that stayed within all of its own bounds could
    be reported as failed.
  - **Change:** both waits are now 90 s, and their messages name the bound.
    The service's stage bounds are named constants that the stop path uses
    (`service_runtime/stop_budget.rs`).
  - **Why the stages were not overlapped:** the owner stop begins only when
    the supervisor is dropped, after the arbiter's settlement, and the
    supervisor's figures are private. Both are airk-supervisor changes, and
    that crate is being edited for L3-10 (b), so they are left as a
    follow-up.
  - **Evidence:** the test
    `stop_budget::tests::the_worst_stop_exceeded_sixty_seconds_and_fits_the_stop_wait`.
    It models the stage bounds; it does not time a real stop.
  - **Record:** `crates/airk-service/src/service_runtime/CONTRACT.md`, "Stop
    and upgrade wait".

### 19. The Answer job table: choices made where section 12.4.1 left room

- **Step:** L2b, on branch `loop/l2b`. Entry 18 is L2's entry.
- **What was built:** the service-owned job table in
  `crates/airk-service/src/service_runtime/answer_jobs/`. It covers
  attach-by-digest, tickets and continuations, the admission estimator,
  degrade-to-fit, earliest-deadline dispatch with ageing, the gathering
  limit, 10-minute retention and aggregate queue status. Its contract is
  `answer_jobs/CONTRACT.md`.
- **Choices, each a guide the plan left open:**
  1. **The request digest leaves out `deadline_ms`.** A retry usually sends
     less time, and it must still attach. The continuation is left out too.
     A retry that would get more on its own never takes a shorter run's
     more degraded answer (review note on `ab31451e`). It attaches only
     when the existing job gives it no less than its own admission would;
     otherwise it runs as a separate job. The full rule is in the
     contract, "Attaching without loss".
  2. **The degrade order:** rounds to two, the model step, rounds to one,
     sub-needs and parts to two, then to one, and last the no-model loop.
     The aim is to keep more of the question before cutting model quality
     to nothing. The model step moves to Compact only where Compact is
     cheaper; after it, each shape takes the cheaper of Large and Compact
     (review C7).
  3. **No ticket when a ticket cannot help.** If even the smallest answer
     cannot fit the request's own deadline with no wait, the request is
     admitted at the smallest shape and disclosed. It is not ticketed. The
     kernel's partial at the deadline is then the answer.
  4. **A ticketed job gets its full deadline** from its actual dispatch. An
     admitted job whose wait consumed its deadline also runs on a fresh
     window (`requeued`), because the work outlives the call. A job that
     never waited is never `requeued` (review C4). On the live path a job
     cannot run past its maker's deadline, so such a job is dropped and its
     place in line kept: see "Wiring" below.
  5. **The ageing guard** is the dispatch key `due + 1 x enqueued`. This
     bounds every job's wait. A test shows a 600 s Answer under a stream of
     20 s Answers starting by 295 s; under pure EDF it would start at 585 s.
  6. **The gathering limit** is `max(2, min(capacity, user cap))`. The user
     cap does not go below the floor of 2. The capacity is 2 until L5b
     injects it through `set_runtime_capacity`.
  7. **Fairness:** at most 8 queued and running jobs per scope, 32 per
     principal (installation and source-authority fingerprint) across its
     scopes (review S3), and 4,096 in the table. A call past these limits
     gets `Busy` with a retry time, and no job is made.
  8. **Failures are retained** for the same 10 minutes on their own
     publication, so a retry attaches to the failure instead of replaying
     uncertain work. A new publication drops them (review C1).
  9. **Retained entries** are capped at 64 MiB and 1,024 entries, and the
     oldest is evicted first. Every entry is charged its result, its
     question and 1 KiB, so no entry is free (review S1).
  10. **The estimator costs are placeholders until L5b and L7 measure
      them:**
      - no model: 2 s fixed, 0.5 s per part, 2 s per sub-need-round;
      - Compact: 4, 2 and 4 s;
      - Large: 8, 4 and 8 s.

      A learned correction (an EWMA of elapsed over estimated, kept
      between 0.25 and 4) adjusts them in the meantime. It skips runs that
      reached their deadline (review C5). While unmeasured, they never
      reduce a request that did not wait: see "Wiring" below.
  11. **Publications (review C1 and S4).** The table records the
      publication a job is expected to run on at dispatch, and the runner's
      report replaces it.
      - The service's observer reports each new publication of an index;
        an older generation of the same incarnation is ignored.
      - On a new publication, every result and failure of another
        publication is dropped, and running jobs on another publication are
        superseded: never attached, their continuations refused, and their
        outcomes delivered to waiters but not kept.
  12. **Degradations in replies** are the ones fitted at dispatch, the same
      for every caller, whether it made the job or attached (review C3).
- **Security:** a job is visible only within its `AnswerScope`, which is the
  installation, catalog, index, the caller's source-authority fingerprint
  and the configuration revision (review S2). A foreign, unknown and expired
  continuation all get one identical refusal. Job ids are salted per table.
  Status carries no ids, scopes or questions. Details are in the contract's
  security section, for the separate security review.
- **Review of `ab31451e`:** findings C1 to C7, T1 to T3 and S1 to S4 were
  all confirmed against the source and fixed in the follow-up commit, with
  regression tests. The resolutions are in
  `answer-loop/L2b/review-ab31451e.md`, under the private run root.
- **Wiring (built 22 November 2026, branch `loop/l2b-wire`):** the table is
  on the live Answer path of every project owner that runs in the service.
  The contract's "On the live path" section describes it. What follows is
  each choice made and each gap found while wiring, with the fix that closed
  it.
  - **The call site is the MCP Answer handler, through an `airk-mcp` trait
    the service implements** (`AnswerJobs`), as designed. Every transport
    ends in that handler inside the owner, and only it holds the kernel's
    verified binding (`Kernel::answer_scope`) and the kernel call the job's
    work runs. The service state owns the table. Owners find it by control
    root through the supervisor hook `Hooks::with_answer_jobs`, because an
    owner reaches its service only through plain functions. A server without
    a table, and a combined Answer member, call the kernel directly as
    before.
  - **Kernel: `Kernel::answer_admitted` instead of a field in `AnswerEnv`.**
    The design named `AnswerEnv.admission`. A separate entry leaves
    `AnswerEnv`, `answer_with` and every existing caller and test untouched,
    which keeps this change apart from the loop work in progress. The plan
    is `AdmissionPlan` (`answer/admission.rs`); the reply is validated
    against the budget as asked, with the plan's reductions disclosed.
  - **The running-job reply is a ticket.** The Core contract allows a reply
    without parts only in the ticket shape. So a running job's reply is a
    ticket with position 1, start 0 and the degradations fitted at its
    dispatch, rather than a partial with a continuation.
  - **The configured rung only, now enforced by the table**
    (`configured_rung_only`). The kernel cannot choose a Context rung per
    Answer until L5's lifetime switch, so each job is fitted with its own
    rung and the no-model loop alone. Before this, a Large job under
    deadline pressure could be planned on Compact, which the kernel cannot
    honour.
  - **Gap found and closed: a job cannot outlive its maker's native
    attempt.**
    - **Plan constraint:** section 12.4.1, idea 2, the work outlives the
      call.
    - **What holds:** a job survives its maker's disconnect or client
      timeout until the maker's original deadline, and a retry attaches to
      it.
    - **What does not:** a job cannot run past that deadline. The native
      attempt every card and Focus call carries ends there, and the
      kernel's original deadline can only shorten.
    - **What was wrong:** a job the table re-windows past it (a ticketed
      job at its dispatch, or a requeued one) would have run with an
      expired attempt, failed at its first owner observation, and left a
      failure that every retry attached to for ten minutes.
    - **Fix:** such a job is not run. It is dropped without a kept failure
      (`JobOutcome::Dropped`), its place in line is kept as a claim, and
      the retry, by digest or by the ticket's id, is admitted in that place
      with a window of its own. A call still waiting on the dropped job is
      submitted again once. So a ticket holds a place in line, not a
      running job.
    - **Not built:** running a job past its maker's deadline needs a native
      attempt of the job's own. That is a kernel and service attempt
      contract change and needs a security review. Owner: L2b.
  - **Gap found and closed: an idle owner would have stopped a running
    job.** A project owner leaves after 15 s without a connection. A job
    whose maker had gone would then have been ended by its own owner, and the
    retry would have found nothing to attach to. A running job is now
    counted as its owner's (`LocalTools::answer_jobs_in_flight`), and an
    owner with one is not idle. A queued job keeps no owner; if its owner has
    left when its turn comes, it is dropped as not run and its place is kept.
  - **Gap found and closed: failures a retry may safely repeat were kept.**
    Choice 8 keeps every failure for ten minutes. A job ended by a stopping
    owner, by its own closed context, or by a refusal typed safe to repeat
    (`read_retry_hint`) is no longer kept; the calls waiting on it get the
    outcome and the next call is admitted afresh. Every other failure is
    kept as before, now with the typed receipt the direct path renders.
  - **Gap found and closed: placeholder costs reduced a request that did
    not wait.** With the placeholders of choice 10, the full default request
    on Large is estimated at 408 s against the 300 s default deadline. The
    first Answers of every service lifetime would have lost a round on a
    figure nobody measured. Unmeasured costs (`costs_measured` false, the
    service's setting) now only order and bound the queue: a request that
    has its whole window runs as asked, as it did before the table. A
    request that waited is still fitted to what is left of its window.
    `set_costs` makes the costs measured.
  - **Gap found and closed: a result cut by a short deadline answered calls
    with more time.** The no-loss rule of choice 1 compares shapes, and a
    shape is fitted to its window. With unmeasured costs a request that did
    not wait keeps its full shape whatever its deadline, so a 60 s run that
    its deadline cut would have answered a 300 s retry of the same request
    for ten minutes. A finished result now records the window of a run its
    deadline cut, and it answers only a call with no more time than that. The
    reply itself reports the cut (its partial marker or a part's stop names
    the deadline), because the loop returns before its window ends; the same
    report now also keeps such a run out of the cost correction, which the
    clock alone did not.
  - **Choice: the reply reserve is part of the scope.** A reply is packed to
    its maker's reserve for verification metadata. Calls with different
    reserves therefore never share a job.
  - **Choice: a call waits a little less than its deadline** (a tenth of
    its time, at most 2 s), so that a ticket for a job still queued or
    running is delivered inside the deadline.
  - **New typed tool error `answer_queue_busy`** (`retryable`,
    `retry_after_ms`, `query_submitted: false`) for a call that could not be
    queued, as designed.
  - **Still not wired, each with its owner** (contract, "Not wired yet"):
    queue status on the status surface (deferred until after LG, primary's
    decision of 22 November 2026; owner L2b), `publication_changed` (L2b),
    `set_runtime_capacity` (L5b with L2b), `set_costs` (L5b and L7), custody
    (entry 21, L2b) and progress for attached callers (L2b).
  - **With L4's model lane.** The table decides which Answers gather, by
    earliest deadline with ageing and within the gathering limit. The lane
    then shares the one model thread among those Answers, by attempt and
    part and not by deadline: one attempt holds at most three of an index's
    four lane jobs. One table job is one lane attempt, because a job runs
    under its maker's native attempt. At the floor of two, two Answers on
    one index both get model turns. Above it, what the lane cannot admit is
    not queued but decided deterministically and disclosed, so the limit
    must come from L5b's capacity. The estimator prices a job as if it had
    the lane to itself; the learned correction absorbs the rest. The
    contract's "Scheduling" section has the detail.
- **Evidence:**
  - `airk-service`, `answer_jobs::` tests: the table (29) and the wired
    path (`answer_jobs::wiring::`, 11), which runs the real `airk_answer`
    handler, the real kernel loop over a real index and the real table. They
    show two and four concurrent Answers completing two at a time with none
    starved, attach after a client's transport cancelled the call, a ticket
    at once and a complete answer on the retry, a disclosed reduction, and
    `answer_queue_busy`.
  - `airk-mcp`, `answer_jobs::` tests (4): how a failure travels, when a
    caller's wait ends, and that a job's work runs the admitted plan and
    only inside its maker's window.
  - `airk-kernel`, `arbiter::answer::admission::` tests (4): the plan
    bounds the budget, discloses each reduction against the budget asked,
    never widens, and refuses what it cannot honour.
  - The handler tests use a disabled native controller. They claim nothing
    about models, devices or an installed service.

Further entries are added as implementation proceeds.

### 20. Canonical reads in flight are not bounded by the Answer reserve

- **Step:** L3 (review fix M5 in `7b65167a`; wider since L3-9's four reads
  in flight).
- **Plan constraint:** section 12.4, a call past its deadline is never an
  empty failure.
- **What holds:** card and Focus calls end at `deadline - compose_union_ms`;
  a canonical read is started only before the 5 s dispatch reserve.
- **What does not:** `CanonicalReadFn` has no deadline and local reads are
  bounded only by the read attempt, so up to four reads already in flight
  at the reserve point can outlast it, and then the reply fails
  `validate_work` instead of returning a Deadline partial.
- **Follow-up owner:** L4/L2b, bound reads by the Answer deadline in the
  service read attempt. Kernel contract: `airk-kernel/src/arbiter/CONTRACT.md`,
  "Answer /3 loop".

### 21. Answer custody is dropped after delivery until the job table keeps it

- **Step:** L3-10 (primary ruling, 22 November 2026).
- **Plan constraint:** section 12.4.1, result retention.
- **What holds:** the kernel returns `AnswerCustody` (the complete decision
  log and every intact canonical result) beside the reply, and the reply
  carries the log digest.
- **What does not:** the MCP handler keeps only the reply; custody is not
  retained for continuation or audit.
- **Follow-up owner:** L2b, keep custody in the Answer job table keyed by the
  log digest.

### 22. The Context model switches within one lifetime, never resident together

- **Step:** L5 phase C.
- **What:**
  - The evaluation fixture's single CUDA slot is now a map keyed by manifest.
    One lifetime may attach two CUDA Context profiles, one Compact and one
    Large, on the same lease, gate and adapter.
  - Switching between them is an ordinary Configure. Before a warm reserves,
    it releases a different private CUDA resident of the same stage, observes the release and the final charge drain,
    and only then loads the new model. The existing rollback path reloads
    the old model if the new one fails.
  - A protected FP16 profile carries no graph result. Its review pin is the
    bounded policy from its pass-1 graph diagnostic receipt.
  - GPU revocation releases every private entry the owner holds.
- **Why:** plan 12.5.2 item 4. Both gate arms then run on one installation
  and one set of indexes without a restart, and each arm's peak memory is its
  own model's.
- **Measured** on the RTX A6000 at the scorer level: in one process and one
  lease, the ignored `l5c_switch` selector opened and dropped the private
  Answer scorer directly, with 20 CAL panel rows per phase. It did not go
  through Configure, the service warm or its release path, so these figures
  are not a measurement of the service switch itself:

  | Phase | Reload (cold open) | First call | Warm median | Peak GPU memory | Release |
  | --- | --- | --- | --- | --- | --- |
  | Compact FP16 | 7.9 s | 420 ms | 265 ms | 6.03 GiB | 0.10 s |
  | Large FP16 | 16.9 s | 732 ms | 589 ms | 14.02 GiB | 0.16 s |
  | Compact FP16 again | 7.8 s | 325 ms | 270 ms | 6.03 GiB | 0.10 s |

  - The second Compact phase gave bit-identical scores to the first.
  - The process's sampled peak was 14.24 GiB, below the 20.05 GiB sum of the
    two models, so the scorer-level switch never held both.
  - The service's release order (release, observed release, charge drain,
    then reserve, including drain timeout and cancellation) is covered by
    unit tests with recorded fake steps.
  - The rows are about 1,400 tokens each.
- **Where:** service `arbiter/lifecycle.rs` (`device_switch_release`,
  `drain_prior`), `arbiter/evaluation.rs` (`with_private_gpu`), the fixture's
  `cuda_answer/same_install.rs`; the service arbiter `CONTRACT.md` section
  "Context switch within one lifetime". Receipt: `L5C/runs/s2-switch.json`.
- **Run record:** the first launch, `s1-switch`, exited in 0.4 s before any
  GPU work, because the private launch wrapper passed no SHA pins. The rerun,
  `s2-switch`, used unchanged inputs. Both are in `L5C/logs/gpu-commands.log`.
- **Still open:** no live service lifetime has switched the two entries on the
  GPU. Owner: the LG preparation run.

### 23. The short trial ran on the scorer path, not on the evaluation indexes

- **Step:** L5 phase C. This changes plan 12.5.2 item 6 for now.
- **What:** the Compact, Large, Compact trial of entry 22 ran as an ignored
  retrieval test (`l5c_switch`) on frozen CAL panel rows, not as loop calls
  in a service lifetime on the evaluation indexes.
- **Why:** three things the indexed trial needs are not ready:
  - the loop over MCP v3, which L3 is switching now;
  - profiles that admit the loop's card tasks, which wait on L4's card
    policies;
  - a rebuilt fixture image and a two-entry FP16 asset directory.
- **Still to do before LG** (owner: the LG preparation run): one indexed
  lifetime with both FP16 entries, once L3 and L4 land. It needs:
  - FP16 profiles re-emitted with the four card tasks, because L4's
    `runtime.rs` change moves the FP16 math-policy digest;
  - a two-entry roster and asset directory;
  - a two-entry mode in `gpu_fixture.py`;
  - a fixture image rebuild.

  The service code for it is built and unit-tested.

### 24. Idle unload: built, with the setting reachable only from the evaluation

- **Step:** L5 phase C.
- **What:**
  - Each stage's model unloads after 30 minutes with no model call (plan
    12.5). The unload runs only on a settled Ready stage with no call in
    flight, claims the lifecycle like Configure, and uses the ordinary
    release and drain.
  - The first call after an unload starts the reload and returns at once
    without advice. The Answer runs on reads or without the model, so a
    reload never blocks it or times it out.
  - Status shows Draining during the unload, then Requested with the
    reason.
- **Changed from the plan:**
  - The per-stage time can be set only through the private evaluation for
    now.
  - A product setting and an explicit pre-warm request change persisted
    configuration and the wire, so they wait for a contract decision (owner L1).
- **Reload times** for the admission estimator (L2b) are in entry 22.
- **The reply:** an Answer that meets a reload discloses `context_absent` or
  `focus_absent`, which the loop already reports when a model did not run. A
  reason that names the reload needs a wire addition (a typed unavailable
  reason from the service's scoring reply and a matching degradation); that is
  left for L1/L3.
- **Where:** service `arbiter/idle.rs` and the service arbiter `CONTRACT.md`
  section "Idle unload".

### 25. The Context ladder is a Core rule; product selection does not call it yet

- **Step:** L5 phase C.
- **What:** `device::context_ladder` walks Large on the GPU, then Compact on
  the GPU, then Compact on the CPU.
  - Large never runs on the CPU.
  - A step down carries the typed reasons collected so far, plus the
    existing reserved `smaller_model_selected` reason, so the wire list does
    not change.
- **Changed from the plan:** product device selection does not call it yet.
  There is no product CUDA Context route to step down from. Wiring it into
  selection waits for the Compact FP16 product route (owner: L5, design item 1.6).
- **Where:** airk-core `arbiter/device/ladder.rs` and the Core arbiter
  `CONTRACT.md` section "Context ladder".

### 26. L5b runtime capacity: the rule is built, concurrent runs are not

- **Step:** L5b.
- **What:** the capacity rule of plan 12.5.1 is in
  `airk_core::arbiter::device::capacity`:
  - Auto allows at most 2 runs per stage;
  - the reserve floor is the larger of 1.5 GB and 10% of the device;
  - a run is added only for sustained need, and kept only with a gain of at
    least 1.3 times and every run within budget;
  - under pressure it sheds to one run at once, and adds runs back one at a
    time after a quiet period.

  Tests cover the floor, the gain proof, the cap and shedding under
  injected memory pressure.
- **Not built:** concurrent runs on one session. `Scorer` serializes every
  run behind one session mutex in retrieval `runtime.rs`, which L4 is
  editing now. Per-run CUDA streams need that change first. Owner: L5b, after L4 integrates.
- **Not measured:** the gain. Measuring it without concurrent runs would need
  a second session, which is a second copy and forbidden by 12.5.1.
- **One copy only** already holds structurally: one resident per stage, and
  the switch of entry 22 releases before it loads.

### 27. Review of the L5 phase C commit: findings closed

- **Step:** L5 phase C, review fixes. The fixes are in the commit that adds
  this entry; the reviewed commit is the phase C commit before it.
- **Review:** an independent read-only review found four medium and four low
  findings (private record `L5C/review-8028001.md`).
- **Closures:**

  | Finding | What was wrong | What changed | Evidence |
  | --- | --- | --- | --- |
  | M1 | A call could read Ready before it was counted, so an idle unload could release under it. A failed drain left the stage Draining with no way back. | Calls are counted before availability is read. The unload checks for calls and sets Draining in one critical section. A failed drain leaves the reload marker. | `idle::tests`: `a_call_in_flight_skips_the_unload_and_keeps_the_resident_ready`, `a_failed_drain_is_never_stranded` |
  | M2 | The reload marker could be taken while the unload still held the lifecycle, or lost when a restore was refused, and a refused restore overwrote the phase. | The marker is taken only when no operation owns the lifecycle, and put back when the reload is refused. A refused restore marks Failed only when nothing claimed the lifecycle since. The product reload now uses the device-configured warm that a service start uses. | `the_reload_marker_survives_a_busy_lifecycle_and_a_refused_reload`, `a_refused_restore_never_overwrites_a_newer_operation` |
  | M3 | A pass-1 review receipt was accepted even when its CPU payload check had failed, and a receipt of another profile of the same model was accepted. | The receipt must be complete and error-free, with a passing CPU payload check. It must name the operator-pinned pass-1 profile, with the same id, model and device execution except placement and memory. The same rule applies in `l5c_switch`. | `review_pin_tests` (2 tests) |
  | M4 | No test covered the release-before-reserve order, and the scorer-level GPU figures were described as the switch measurement. | The order is one function with a test seam, used by the switch, the budget drain, the idle unload and every release. The text now says the figures are scorer-level. | `release::tests` (4 tests); entry 22 and the service contract |
  | L1 | A reload was not named in the Answer reply. | Not closed in code. The reply still discloses `context_absent` or `focus_absent`. A reload-specific reason needs a wire addition. | Owner L1/L3; recorded in the service contract |
  | L2 | Ladder truncation could drop the Large-on-CPU refusal row. | The step-down rows are kept as one group. | `the_large_cpu_refusal_survives_the_reason_limit` |
  | L3 | A lowered user cap was ignored during a capacity trial. | The cap applies first. | `a_lowered_user_cap_ends_a_pending_trial` |
  | L4 | The ignored GPU selector polls with short sleeps. | Kept: bounded and reported, in an ignored hardware selector. | — |

- **Validation:**
  - Service arbiter lib tests (all features): 154 run, 151 passed. The 3 that
    fail also fail on the base without these changes.
  - Core device tests: 69 run, 69 passed.
  - Clippy is clean for the service (all features, `gpu-cuda`,
    `gpu-cuda,test-support`, `local-inference`, none), Core and the
    retrieval tests.
  - No GPU run was needed for these fixes.

### 28. Combined Answer moves to /3: the member allowance, hand-back by count and the Focus profile in the reply

- **Step:** L3-10 (a), the Core and kernel combined paths beside /2. The
  primary agent recorded the contract decision under section 12.9; it is in
  the Core arbiter contract, "Combined Answer /3 allowance".
- **What was wrong:** the combined (multi-index) Answer handed each member
  a /2 `PartitionAllowance` (a /2 `Budget` parent and one global assessment
  ordinal per assessment), and the /3 loop ignored any allowance
  (`partition: None` in card and Focus requests). A member could therefore
  spend the whole request budget, and /3 had no combined summary.
- **What changed:**
  - L1's `PartitionAllowanceV3` is reused, not duplicated. Its card and
    Focus ordinals must be the prefixes `1..=k` (counts local to each
    member's own Answer), and its resources gain `focus_pairs`
    (`LoopResources`). `PartitionAllowanceV3::clamp` bounds a member budget.
  - Kernel: `with_answer_partition_for` installs the allowance;
    `answer_with` clamps the request budget to it; card and Focus requests
    carry it.
  - Core `combined_context`: `initial_allocations_v3`, `LoopUse` (hand-back
    by count from the member reply's work and decision log),
    `grow_allowance`, `returned_allowance`, `without_models`, the
    `airk.combined-context/2` summary (`CombinedContextSummaryV2`), and
    `reply_delivery_bindings`. The wrapper's `context` field is the
    `CombinedContext` enum (/2 or /1) until /2 is deleted.
  - Kernel `combined_reads`: `compose_answer`, `answer_payload_admission`
    (the widest /2 summary inside the 131,072-byte Answer frame) and
    `answer_reply_focus_binding_bytes`. A member envelope may take the
    Answer frame.
  - **Reply addition, decided here under 12.9:** `model_work.focus_profile`
    (required, nullable). The batched Focus receipts stay in custody, so a
    delivery owner holding only the reply (the MCP delivery guard, or the
    frontend for a combined member) could not bind the Focus stage, and the
    policy-delivery check compares the Focus profile. Core validates it
    against the logged receipts. The generated TypeScript and the semantic
    cases were regenerated by their Rust owners.
- **Known limit:** in /3, `max_bytes` bounds both retention and the reply,
  so a combined member gathers within its share of the combined frame.
- **Commits:** the source, contracts and generated views landed in `594e9fbf`
  under a mislabelled message (`Add the peak-memory measurement script used
  for the HTTP idle-test fix`: the primary agent's commit swept up the staged
  L3 index). Two new test files were not staged and land in the following
  `Complete L3-10 (a)` commit; at `594e9fbf` alone the airk-core tests do
  not compile, because `combined_context/answer.rs` declares a missing
  `tests` module.
- **Validation:** Clippy `-p airk-core -p airk-kernel --all-targets
  --all-features -D warnings` exit 0, and the same for every dependent crate
  (mcp, https, frontend, service, supervisor, host, airkctl, cli). Full nextest
  for airk-core and airk-kernel: 1,287 run, 1,285 passed, 2 failed, both in the
  new tests, whose expectations were wrong (a 24-slot share needs 25 to exceed
  it; the test's baseline makes one Focus call, not two). After the fix, the
  arbiter, combined, answer-loop and native-loop selectors: 432 run, 432 passed.
- **Not changed:** no airk-state/1 persisted format. The MCP handler, the
  read-attempt and host carriers and the frontend switch in L3-10 (b).
- **Core files changed (for L4's pinned identities):**
  `crates/airk-core/src/native_loop.rs` and
  `crates/airk-core/src/arbiter/answer_loop/log.rs`; also
  `answer_loop.rs`, `combined_context.rs` and the new
  `combined_context/answer.rs`. `cards.rs`, `calibration.rs` and
  `focus_batch.rs` are unchanged.
- **Evidence:** Core tests `v3_allowance_has_a_budget_v3_parent_and_slot_ordinals`,
  `v3_allowance_clamps_the_member_budget`, `reply_validates_against_its_log`
  and the `combined_context::answer::tests` module; kernel tests
  `a_combined_member_answer_stays_within_its_partition` and
  `combined_answer_v3_admission_takes_the_answer_frame`.
- **Open, owner L3:** an Answer meeting an idle model reload discloses only
  `context_absent` or `focus_absent`; naming the reload needs a typed
  unavailable reason and a matching degradation (entries 24 and 27). It was
  not folded into this change, which does not touch degradations.

### 29. LG preparation: two-entry inputs, a pair launcher mode, CUDA Focus in the seam, and the re-emission procedure

- **Step:** L5, LG preparation. It does not depend on L4's code.
- **Asset directories (corrected):**
  - The first version of `cuda_pair.py assets` hard-linked the assets (0
    bytes copied). That was wrong. The fixture opens every input through a
    rooted directory that refuses a file with more than one link ("rooted
    file has aliases"). The warm also refuses a hard-linked CUDA payload.
    A link also aliases its original, so other lifetimes could not open the
    original while the link existed.
  - The links (`lg-inputs`, and a payload link set beside the CUDA DLLs) were
    removed, and every original was checked back to a single link.
  - `assets` now copies, authenticated before and after (test
    `test_an_aliased_source_is_refused`). Budget: Compact FP16 3.45 GB, Large
    FP16 8.05 GB, Focus 1.14 GB.
- **Pair launcher mode:**
  - `cuda_pair.py pair` writes the roster, the review pins and the profile
    rows. It applies the review rule of entry 27 again.
  - `gpu_fixture.py --model pair` starts a lifetime with the entries.
  - `stage` resolves the lifetime's own rows by name, so `run_plan.py`
    `switch_context` switches the entries in one lifetime.
- **CUDA Focus in the seam (closed):**
  - The earlier gap: the seam attached Context entries only, so a CUDA Focus
    row was refused and Focus ran on the CPU.
  - Now the seam takes one strict-GPU Focus entry on the same lease and
    adapter, in the Focus stage's own map. It is probed under the evaluation
    binding and warmed through the standard charged route, where the scorer
    verifies strict placement. Revocation, idle unload and the release order
    cover it, and there is still one loaded copy per stage.
  - Measured on the A6000 in a fresh synthetic lifetime with the W8
    1,024-token profile `a3157dd1`:
    - warm 8.6 s, release 0.69 s;
    - 106-112 ms per warm call (the first call of an Answer took
      278-480 ms);
    - two Answers with 8 Focus calls each, 2.13 s and 1.75 s, same evidence
      digest;
    - GPU memory used: 557 MiB baseline, a 2,357 MiB sampled peak.
  - Receipt: `L5C/runs/f3-focus-seam-summary.json`.
- **Two earlier attempts:**
  - The first failed on the hard links.
  - The second failed because the worktree image checks its inputs against
    the worktree's own `artifacts` root, so the MiniLM model had to be copied
    there too.
  - The worktree copies (31.0 GB) were reclaimed after the check.
- **Tests:**
  - `test_cuda_pair.py` (10), `test_gpu_fixture.py` (7), `test_harness.py`
    (75) and `test_fixture.py` (34);
  - service arbiter lib 151 of 154 (the 3 base failures);
  - Core device 69.
- **Re-emission:** the exact procedure for the FP16 profiles with the four
  card tasks (`admit_loop_tasks`) once L4 integrates is in `GPU_FIXTURE.md`,
  section "Re-emitting the FP16 profiles for the loop". It adds a test-only
  `L5A_LOOP_TASKS=1` switch to the emitter, so no digest-input file moves.
  It then needs a new pass-1 graph run and a confirmation on the GPU.
- **Review of the Focus seam (closed; private record `L5C/review-focus-seam.md`):**

  | Finding | What was wrong | What changed | Evidence |
  | --- | --- | --- | --- |
  | M1 | A Focus release error skipped the Context release, and a repeated revocation never retried. | Both stages are released and the errors joined. The private release runs on every revocation, changed or not. | `a_failed_focus_release_still_releases_context_and_reports_both` |
  | M2 | Nothing bound the CUDA Focus profile to its CPU twin or to a numeric check. | It must share the twin's compiler, tokenizer, fixture, task, policies, ABI, limits, graph and weights (FP16 Focus is refused), and it pins a passing parity receipt. | `a_cuda_focus_profile_must_match_its_cpu_twin`, `a_cuda_focus_entry_needs_a_passing_parity_receipt_for_its_twin`; A6000 parity: 15 of 15 winners, largest difference 1.7e-5 within 1e-3 (`L5C/runs/p1-focus-parity.json`) |
  | L1 | The negative Python test passed for the wrong reason. | It now includes a Context entry and asserts the strict-GPU message. | `test_a_strict_gpu_focus_entry_joins_without_a_review` |
  | L2 | The Focus paths had no Rust unit tests. | Tests cover the roster shape, the twin rule, the parity rule, both-stage release and route stages. | the four tests above, `private_routes_name_their_stage` |
  | L3 | The post-revocation reason said no CPU variant exists. | It now points to the stage's CPU profile. | text |
  | L4 | `assets` trusted asset names and left partial copies on failure. | Plain file names only; a failure removes the partial copy and writes `<dir>.failed.json`. | `test_asset_names_must_be_plain_file_names`, `test_a_failed_copy_removes_the_partial_output_and_keeps_a_receipt` |
  | L5 | Some comments and docs were stale. | Updated. | — |

- **Validation:**
  - service arbiter lib tests 156 of 159 (the 3 base failures);
  - Python `test_cuda_pair.py` 12, `test_gpu_fixture.py` 7,
    `test_harness.py` 75 and `test_fixture.py` 34;
  - Clippy clean for the service (all features, `gpu-cuda`,
    `local-inference`, none) and for the retrieval tests.

### 30. Loop task policies check roster shape instead of rebuilding rosters

- **Step:** L4.
- **What:** the Split, Plan, Coverage and Follow-up task policies in
  `airk-retrieval` check each segment's roster shape before compile (symbol
  order, stage classes, keep-whole first for Split, the reference card last
  for Plan, covered first for Coverage, at least two reads for Follow-up). The
  design's "rebuild the roster from the proposals and refuse on mismatch" was
  not built.
- **Why:** L3 put the proposers in the kernel and the decision log already
  binds each roster by `roster_digest`; rebuilding in retrieval would have
  duplicated the kernel's proposers in a second owner.
- **Identity:** each policy hashes its own source, the shared `loop_cards.rs`,
  its question and a new Core loop semantics digest (card, calibration,
  batched-Focus, log and native loop sources). Values are pinned by a control
  and recorded in `crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md`.

### 31. Loop card tasks run under the Context enrollment

- **Step:** L4.
- **What:** a Context profile declares either the one-turn task or exactly the
  four card tasks. A card call is admitted when the index is enrolled for the
  Context stage's existing task; the persisted configuration and
  `Stage::permits` are unchanged.
- **Why:** adding the card tasks to persisted enrollments would change an
  `airk-state/1` format without need.
- **Consequence:** the release catalog's Context descriptors still declare
  only the historical one-turn task, so product Answer uses the deterministic
  order until a Context profile is re-emitted with the card tasks
  (`runtime::admit_loop_tasks`, zero-rule calibration set). That is L5's
  profile work and the product rollout after L7.

### 32. Evaluation routing removed from `runtime.rs`; the CPU row moved

- **Step:** L4.
- **What:** `runtime.rs` no longer routes option-B Context or
  SoftwareEvidenceV1 Focus profiles. The closed CPU Focus row moves to the new
  source by a recorded decision with an inverse-replay proof
  (`cpu_source_transition/CONTRACT.md`, "Answer loop admission supplement").
  The 8 transition controls and the embedded-catalog control pass again.
- **Consequence:** `runtime.rs` is an input of the FP16 math policy, so the
  FP16 CUDA profiles emitted in L5 no longer open and must be re-emitted (with
  the card tasks, for the gates). Recorded in the CUDA `CONTRACT.md`.
- **Evidence:** `answer-loop/L4/proof/` (patches, intermediate source, hook
  diff) and the nextest receipts in `answer-loop/L4/`.

### 33. A separate Answer /3 ledger beside the /2 joint ledger

- **Step:** L4.
- **What:** `loop_ledger.rs` holds card slots up to `context_calls`, Focus
  ordinals up to `focus_calls`, the attempt pool and the per-part, per-round
  allowance. `joint_ledger.rs` is not edited.
- **Why:** Answer /2 still runs until L3-10 deletes it; one ledger serving
  both budget shapes would have changed /2's slot rules underneath it.

### 34. Focus: one call per batch with per-candidate questions

- **Step:** L4. The plan left this to L4 "by measured cost".
- **What:** a batched Focus call carries up to 16 questions and each pair job
  carries its candidate's own question. One batch is one Focus ordinal and
  one reservation.
- **Why:** every pair is already its own lane job, so the per-pair model work
  is identical either way; one call per sub-need group would add per-call
  overhead (IPC, two application observations, a reservation) and spend the
  `focus_calls` slots. The wall-clock overhead was not measured in L4, which
  ran no GPU work; L5/LG measure it on the A6000.

### 35. Review of L4-1 to L4-3: findings and their closure

- **Step:** L4. An independent review of `a2774d46`, `08566656` and
  `4eeb2cd2` found three medium and five low findings. All are closed in
  `9fea69ae` (code and tests), `a2275554` (the replay script and row pin) and
  this documentation commit. Test counts are from the receipts in
  `answer-loop/L4/` (`nextest-l4-r*.log`).

| Finding | What was wrong | What changed | Evidence |
| --- | --- | --- | --- |
| M1 | Answer /3 jobs had no per-index bound across attempts, so one index's Answers could fill all 8 queue slots | Fewer than 4 jobs of any kind per index, across every attempt; one attempt holds at most 3 of them, so a different Answer or an ordinary job always has a slot; no two jobs of one attempt share a part; ordinary jobs keep fewer than 2 ordinary jobs per index. The first fix (`9fea69ae`) still let one 4-part Answer take all 4 slots; corrected in `f24460d2` | `lane::tests::one_attempt_holds_at_most_three_jobs_so_another_always_has_a_slot`, `a_second_answer_is_admitted_while_the_first_holds_three`, `several_attempts_share_one_index_ceiling_and_a_second_index_keeps_room`; the test that asserted the old behaviour was replaced |
| M2 | Plan selection ignored the request's `max_sub_needs`, so a log could record more chosen sub-needs than the reply had | `loop_cards::select` takes `max_sub_needs` (clamped to 1..=4) and Plan chooses at most that many; the service passes `parent_budget.max_sub_needs`; the kernel's acceptance check limits Plan to `budget.max_sub_needs.clamp(1, 4)` (kernel edit, `answer/model.rs`) | `loop_cards::tests::plan_never_chooses_more_than_the_request_s_max_sub_needs`, `settlement_tests::plan_settlement_never_chooses_more_than_max_sub_needs` |
| M3 | A lane refusal before queueing was reported as unknown work, so it kept its reservation and burned a slot | Every error of the lane's `score` is now a typed `AdmissionRefused` (it precedes queueing); the card and Focus paths release such a call through `loop_ledger::refused` and report a known non-dispatch (`Unavailable` with zero observed tokens). The slot or ordinal stays used | `pool::arbiter::tests::per_index_capacity_includes_the_actual_running_request` (typed refusal), `settlement_tests::a_refusal_before_queueing_releases_the_reservation`, `focus_batch::tests::a_batch_with_no_queued_pair_is_released_as_known_zero_work` |
| L1 | Batched Focus re-attributed observed tokens by the reservation, not by the pairs that ran | Recorded, not changed: the native Focus request has no candidate-to-part map, so the service cannot attribute exactly; a batch cut short charges every reserved part-round in proportion. The ledger doc comment now says so. Exact attribution needs a per-candidate part in `NativeFocusRequestV3` (Core and kernel, L3's owner) | `focus_batch::tests::a_batch_cut_short_keeps_canonical_order_and_charges_known_work` pins the recorded behaviour |
| L2 | The transition control did not encode the inverse-replay proof and did not pin the new row | The control now pins the new row `e0575e94…`; `scripts/arbiter/evaluation/runtime_row_replay/replay.py` (README beside it) reproduces the chain from Git history and the retained hook patch | `replay.py` exit 0, 8 checks (receipt `answer-loop/L4/proof/replay-receipt.txt`); control `loop_admission_supplement_moves_the_runtime_row_without_relabelling_legacy_evidence` |
| L3 | The lane key used a batch's lowest part, so "one job per (index, attempt, part)" was not exact | `LaneKey.parts` is the set of every part a job serves; two jobs of one attempt conflict when their sets intersect | `lane::tests::a_job_may_not_share_a_part_with_another_job_of_its_attempt`, `part_masks_name_every_served_part` |
| L4 | The service's choice-to-decision mapping was not a digest input; an observation-only type moved every card identity; the batch receipt's identity did not cover batched Focus | The mapping moved to `loop_cards::outcomes` (hashed with the policies); `NativeCardModel` moved to `native_loop/card_model.rs`, outside the hashed `native_loop.rs`; the batch ranking moved to the new `loop_focus.rs`, and a batch receipt's identity is `loop_focus::policy_digest(pair policy)`. The identities moved once more, recorded in the retrieval compiler `CONTRACT.md` | `loop_cards::tests::outcomes_map_choices_to_card_digests_rules_and_abstentions`, `loop_focus::tests::*`, `recorded_loop_policy_identities` |
| L5 | Only helpers of the service path were tested | The settlement of a card call and of a Focus batch was extracted (`settle_card_call`, `settle_focus_batch`) and is driven by a fake lane through the ledger, selection and outcome mapping; the supervisor's narrowing is one helper with its own test. Not covered: the stage owner's own admission checks against a live configuration and supervisor, which need a native resident | `card_scoring::settlement_tests` (4), `focus_batch::tests` (4), `runtime_metadata::arbiter::tests::answer_loop_calls_keep_the_original_attempt_and_clamp_to_the_answer_ceiling` |

- **Validation of the fixes:** `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` exit 0. nextest over Core, IPC and the
  supervisor (all tests) and the arbiter tests of kernel, inference,
  retrieval and service: 1,369 run, 1,367 passed; the two failures were the
  pending Batched Focus identity pin, filled in the same commit and then
  passing in the retrieval rerun (308 of 308), and the base failure of
  entry 36. Per area: lane 8, card settlement 4, Focus settlement 4,
  `loop_focus` 2, supervisor narrowing 1, loop ledger 9, CPU transition 13.

### 36. Checks that failed outside L4 code, and their state

- **Core fixture pins and line endings.** Two Core controls hashed
  `fixtures/arbiter/profile-downloads.json` as checked out. Their pin is the
  hash of a CRLF working copy, while the committed blob is LF
  (`.gitattributes` `eol=lf`), so they failed on any clean checkout. They now
  hash the fixture in its CRLF form, whatever the checkout; the pin is
  unchanged. Closed in `9ae44d61`; evidence `downloads::tests::empty_backups_preserve_exact_retained_part_and_profile_identity`
  and `stage_tests::retained_profile_bytes_have_no_stage_insertion_or_digest_reidentification` pass in a clean (LF) worktree.
- **Scoped device status and the Answer ceiling.** The control expected a
  126 s parent to be refused; since L2, an Answer read attempt may run to
  605 s. It now asserts 605 s accepted, one nanosecond more refused, and
  every other tool still refused above 125 s. Closed in `9ae44d61`; evidence
  `read_device_status::tests::diagnostic_clock_refuses_expiry_wrong_clock_and_oversized_parent`.
- **`failure_tests::ambiguous_or_corrupt_configuration_status_…`** fails at
  the base `ed2e0926` as well (run in the L4 worktree, receipt
  `nextest-base-ed2e0926-failure-tests.log`: 2 run, 0 passed). It fails in
  airk-store `DeviceConfigurationStore::inspect_device_binding` with a JSON
  parse error. Not caused by L4; open, owner primary agent.

### 37. L4 deviations from the design, recorded

These were listed in the private `answer-loop/L4/deviations.md`; this is
their committed record. Entries 30 to 34 cover the roster check, the Context
enrollment, the routing removal, the separate ledger and the Focus decision.

- **No `LoopRoster` type.** L3 sends `CardSegment`s; retrieval renders and
  selects over them.
- **A Core loop semantics digest was added** (`answer_loop/semantics.rs`),
  because Core had none and L1 required the card and batched-Focus semantics
  in the policy identities.
- **Loop admission lives in `runtime/loop_tasks.rs`**, so `runtime.rs`, a
  pinned source and an FP16 math-policy input, moved once, by hooks only.
- **Attribution uses the Core `TokenAttribution`**, remainder to the lowest
  (part, round), matching the kernel's reservation.
- **Ordinary lane jobs** keep fewer than two jobs per index, counting Answer
  jobs on it.
- **`ArbiterCardModel` and Core `NativeCardModel`** were added for the card
  model observation.
- **Open, not done in L4:**
  - `force_single_segment` (design 5.3) belongs to the kernel's batching (L3);
  - the deletion of `scoring.rs`, `joint_scoring.rs`, `context_wide.rs`,
    next-read in `tasks.rs` and the supervisor v2 methods waited for L3-10;
    on 22 November the primary agent deferred it to after the independent
    review of the integrated loop, under the user's 12-hour deadline, so it
    is not part of L4's integration (owner L4);
  - the Focus per-call overhead measurement needs the A6000 (L5/LG);
  - Coverage rosters carry no evidence excerpts (`CardSegment` has only the
    subject and card texts), which limits what a model can judge; this needs
    a Core and kernel change;
  - the release Context profiles must be re-emitted with the card tasks
    before product Answer scores cards; L5's FP16 gate profiles must be
    re-emitted because `runtime.rs` moved;
  - `crates/airk-ipc/CONTRACT.md` still needs the three loop methods'
    deadlines, after L2 commits it.

Further entries are added as implementation proceeds.

### 38. L4 rebased onto L3-10 (a): identities, kernel edits and follow-ups

- **Step:** L4, rebased onto `feat/arbiter-final` at `89ae826e` (L3-10 (a))
  and then onto `feb72c94` (L5C's LG preparation, which took entry 29). The
  L4 entries follow upstream's 29: L4's original entries 19 to 26 are entries
  30 to 37, this is entry 38, and entry 39 closes the points this entry first
  left open. The commit hashes in them and in the owning `CONTRACT.md` files
  name the final replayed commits.
- **Identities moved with L3's Core edits.** L3-10 (a) changed Core
  `native_loop.rs` and `answer_loop/log.rs`, both inputs of the loop semantics
  digest, so every loop identity moved. No L4 source changed for this. The
  recorded values in `crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md`
  and the control `recorded_loop_policy_identities` were updated:

  | Identity | Before | After |
  | --- | --- | --- |
  | Core loop semantics | `0da27971…` | `488496bc…` |
  | Split | `c63c95f9…` | `ccea442c…` |
  | Plan | `47263415…` | `5d219860…` |
  | Coverage | `32c7f08c…` | `c6711506…` |
  | Follow-up | `ce541ca1…` | `89809e2d…` |
  | Batched Focus (release pair policy) | `f67a532b…` | `50ba5c85…` |

  The CPU Focus row and `runtime.rs` (`c9008eef…`) did not move.
- **Kernel edits merged.** `answer/decide.rs` (the policy from retrieval)
  and `answer/plan.rs` (`REFERENCE` from retrieval) applied unchanged. In
  `answer/model.rs`, L3 had already limited Plan acceptance to
  `budget.max_sub_needs.min(MAX_SUB_NEEDS)`; that equals the L4 rule
  (`clamp(1, MAX_SUB_NEEDS)`), because the budget refuses `max_sub_needs`
  below 1, so L3's line was kept and the L4 edit there reduced to
  `decide::policy(stage)?`.
- **Partition allowance /3.** L3 changed `PartitionAllowanceV3.resources` to
  `LoopResources` (with `focus_pairs`) and made the ordinals prefixes. The
  loop ledger's test fixture follows the new type (folded into L4-3 on the second rebase). Open when written, owner L4, closed in entry 39: the
  ledger checks a partition's card and Focus ordinals and tokens but not its
  `focus_pairs`; the kernel's member accounting bounds them today.
- **Open when written (found while merging), closed in entry 39:** the kernel's acceptance check now
  requires a top-score decision to leave out no card scoring above a chosen
  one. In the evaluation-only top-score mode, a Plan segment whose reference
  card scores highest chooses the best proposal (at least one sub-need) while
  the reference scores above it, so the kernel refuses that advice and the
  deterministic order decides. Calibrated mode is not affected (it carries a
  rule). One of the two rules must yield before LG uses top-score Plan.
- **Validation after the rebase:** workspace Clippy with `-D warnings` exit 0.
  nextest over Core, IPC and the supervisor (all) and the arbiter tests of
  kernel, inference, retrieval and service: 1,421 run, 1,419 passed; the two
  failures were the identity pin, re-pinned in `e2dc6f6f` (then 48 of 48 in the
  affected loop, transition, ledger, card and Focus settlement tests), and
  the base failure recorded in entry 36.

### 39. The two points entry 38 left open, closed

- **Step:** L4, on `feb72c94`; closed in `38e141a0`.
- **Partition Focus pairs (owner L4), closed.**
  - **What was wrong:** the loop ledger checked a partition's ordinals and
    tokens but not its `focus_pairs`, nor the parent budget's `focus_pairs`.
  - **What changed:** a Focus reservation names its pairs
    (`FocusCall.pairs`, the request's `score_count`, at least one). The ledger
    refuses a call whose pairs would exceed the partition's
    `resources.focus_pairs`, or the parent's `focus_pairs` across the
    attempt. Pairs stay charged like the ordinal; they are never refunded.
  - **Evidence:** `loop_ledger::tests::focus_pairs_are_bounded_by_the_partition_and_the_parent`.
- **Top-score Plan and the reference card (found in entry 38), closed.**
  - **What was wrong:** the kernel refused top-score Plan advice whenever
    the reference card ("no further sub-need") scored highest, because it
    required that no card left out score above a chosen one. Retrieval's
    Plan selection then takes the best proposal, so that advice was always
    refused.
  - **Rule:** the reference card is a threshold and never choosable. The
    kernel refuses advice that chooses it, and its top-score check ignores
    it: no choosable card left out may score above a chosen one. Recorded in
    the kernel arbiter `CONTRACT.md` (review item M7).
  - **What changed:** `answer/model.rs` `acceptable` (kernel edit in this
    branch; L3 is not editing `model.rs`). The synthetic scorer of the kernel
    model tests now chooses Plan's best proposal, as retrieval does.
  - **Evidence:** `answer::model::tests::top_score_plan_is_accepted_when_the_reference_card_scores_highest`,
    and the argmax assertion of `batched_segments_argmax_per_segment` now
    over choosable cards.
- **Replayed commits.** The one test-target break the first rebase showed,
  the loop ledger fixture's move to `LoopResources`, is folded into L4-3 on
  this rebase. Clippy was run on the final commit only, not on each replayed
  commit. The identity pins of the earlier commits still
  hold the values of their own time, so `recorded_loop_policy_identities`
  passes only from the identity re-pin commit on.
- **Validation:** workspace Clippy with `-D warnings` exit 0, after the
  last source edit. The full nextest set (Core, IPC and the supervisor in
  full, plus the arbiter tests of kernel, inference, retrieval and service):
  1,423 run, 1,422 passed. The one failure is the base failure of entry 36.
  Per area: kernel `answer::model` 10, loop ledger 10, lane 9, card
  settlement 4, Focus settlement 4, CPU transition 13, identity pins 1.

### 40. Answer /3 through the MCP handler and every transport; /3 parity; the review of L3-10 (a) closed

- **Step:** L3-10 (b). Numbered after L4's entries 30 to 39.
- **What was wrong:** after L3-10 (a), the MCP `airk_answer` handler, the
  read-attempt and host carriers and the frontend still ran Answer /2; the
  /3 loop was reachable only from kernel tests. The independent review of
  (a) found seven defects in the combined /3 path (below).
- **What changed:**
  - MCP: `AnswerArgs.request` is `ContextRequestV3`; the handler reports
    progress (`ProgressReporter`), calls `answer_with` and delivers the reply
    (custody dropped, entry 21); each Answer gets fresh loop accounting
    (`Kernel::with_fresh_answer_accounting`), since one MCP session's kernel
    serves several Answers and `bind_answer_loop` binds once.
  - Carriers: `ReadAttemptContext::with_answer_partition`, host
    `Query.answer_partition` (validated on deserialize), frontend
    `project_client` sends and checks it, supervisor admission refuses an
    Answer /2 partition and checks a /3 one against the parent budget.
  - Frontend combined Answer on /3 (`initial_allocations_v3`,
    `answer_payload_admission`, hand-back by count, `compose_answer`). Its
    `SharedBudgets` keep the bounds of every other combined read (65,536
    bytes, 120 s): the live path failed with "max_bytes must be 1..65536"
    before this, and the wrapper takes its frame from the summary.
  - A canonical read inside an Answer wave whose owner answers "repeat the
    same request" (`publication_busy`, retryable, at most 1 s) is repeated up
    to three times within the dispatch reserve (`answer.rs` `busy_retry`).
    Before, a transient busy publication became that read's failure
    evidence, so two transports gave different replies (seen in the HTTPS
    parity test).
  - Saved-source verification reads an Answer /3 reply: its span
    projections are the locators at the reply publication
    (`verification/inventory.rs`); before, a /3 reply verified nothing
    (`not_applicable`).
  - Help content, CLI help and `docs/cli-reference.md` regenerated by their
    owners (the CLI files also pick up arbiter commands added earlier without
    regeneration, which failed at the base), and
    `fixtures/split/arbiter/public_answer_cases.json`.
  - Parity: `crates/airk-cli/tests/common/parity/context.rs` compares full
    /3 replies after Core validation, normalizing only transport identities
    (record ids by position, rebound wherever cited), digests that bind
    record ids, read bodies or measured work (renamed by first appearance,
    so equality patterns stay compared) and timing (`elapsed_ms`, measured
    microseconds, part deadline shares); lists the kernel keeps in record-id
    order (`record_uses`, evidence lists) are compared in logical-id order. The combined summary's byte
    counters are normalized after Core validation because they include reply
    lengths. The /2 replay through the v1 `Gathering` has no /3 equivalent
    (span projections, deviation 8) and its tests were replaced.
  - Parity, the member deadline: the combined-wrapper check in
    `combined_reads.rs` (`current_body_copy`) still held every member's
    `allocation.deadline_ms` to the 120,000 ms request clock of the other
    combined reads, so it refused a correct Answer /3 wrapper, whose members
    declare the summary's budget deadline (300,000 ms by default; "invalid
    original member allowance" in the many-index and open parity test). The
    product was right (Core contract, "Combined Answer /3 allowance", frame
    and time). The check now admits, for an `airk.combined-context/2`
    wrapper only, the deadline its summary names within the 600,000 ms
    Answer ceiling, and keeps 120,000 ms for every other wrapper. Test
    `current_member_deadline_ceiling_exceeds_120s_only_for_an_answer_wrapper`.
  - Tests moved to /3: airk-mcp `arbiter_handler_tests` (the two v2
    failures kept their purpose: an unknown card outcome keeps the evidence,
    blocks later model work and is never replayed, while a typed scope
    refusal stops delivery; a policy withdrawn after an accepted card call,
    at the first and at the last fence, refuses the stale Answer without
    replay or loss of the ordinary search), `arbiter_public_tests`,
    `verification_tests`, `read_attempt/admission_tests`,
    `remote/parent_context_tests`; frontend `read_attempt_tests` and
    `answer/verification/tests`; service `tools_call/tests`; airk-cli
    `combined_reads.rs` and `admin_plane.rs`.
- **Review of L3-10 (a), all seven closed here** (details in the Core and
  kernel arbiter contracts): 1 the compose frame, 2 member bytes as frame
  shares, 3 a question-sized minimum per member, 4 not reproduced (the
  stale refusal covers it), 5 widest sizing for every member, 6 model
  shares over model members, 7 no model work after unknown model work.
- **Left on /2, with owner:** the VS Code Workbench, the Admin Search screen
  (and its built `admin-assets`) and `scripts/check-service-clients.py`
  still send /2, which the handler now refuses (owner: the client owner
  the primary assigns). Retained /2 receipts in the service
  `final_distribution` validation and the one-turn evaluation history
  (`software_forks.rs`) keep local /2 shims. Kernel /2 code (joint path,
  v1 `Gathering`) stays until (c), which is deferred to after the LG gate.
- **Validation** (Windows, the main checkout's target, dev profile at
  opt-level 2, all features, `--locked --offline`; every command with its
  exit code is in `commands.tsv`, with the logs beside it, in the private
  `answer-loop/L3` folder):
  - Clippy `-D warnings`, all targets, exit 0 each time: `clippy-l3-10b-h`
    over core, kernel, mcp, https, frontend, service, supervisor, host,
    airkctl, cli and ipc; after the later edits, `clippy-l3-10b-j` and
    `clippy-l3-10b-k` over kernel, mcp, frontend, service, cli, https,
    supervisor, airkctl and host (for `-k` only airk-cli had changed).
  - `nextest-l3-10b-h`, the broad run (all of core, kernel, mcp, https,
    frontend, supervisor, host, airkctl and ipc; the service tests matching
    `tools_call|arbiter|final_distribution|singleton`; the cli binaries
    `combined_reads` and `admin_plane`): 2,458 run, 2,431 passed, 27
    failed. The 27 are the 25 base failures below and the two cli Answer
    parity tests, which the next three runs close.
  - `nextest-l3-10b-i`, after the frontend `SharedBudgets` and parity edits
    (all of frontend; the cli `combined_reads` tests matching
    `answer|context|verification`): 157 run, 155 passed; the same two
    failed.
  - `nextest-l3-10b-j`, after the busy-read repeat (kernel
    `arbiter::answer::`; the same cli tests): 88 run, 87 passed; the
    many-index and open parity test failed on the member deadline check.
  - `nextest-l3-10b-k`, after the member deadline fix (all of the cli
    `combined_reads` binary): 64 run, 64 passed (3 ignored tests skipped).
  - Not run again after the busy-read repeat, the last kernel edit: the
    core, mcp, https, frontend, supervisor, host, airkctl, ipc and service
    tests of the broad run. The repeat acts only on a retryable "repeat the
    same request" failure of a canonical read inside an Answer wave, and
    none of those tests sets one up (checked by reading them, not by a run).
  - Known failures at the base, 25, none in a file this commit changes:
    - 22 in airkctl: 18 `agents::config` tests, 3 `agent_entries` tests and
      `cli::session::tests::real_command_invoker_carries_exact_agent_entries_through_the_session`
      ("Automatic editing cannot safely retain a recovery copy here"). The
      same 22 fail with the same diagnostics in L2's
      `nextest-4-baseline-head.log`.
    - 3 in the service arbiter lib, the "3 base failures" of entry 29:
      `read_device_status::tests::diagnostic_clock_refuses_expiry_wrong_clock_and_oversized_parent`
      (it expects a 126 s parent refused, which the 605 s nested-read
      ceiling of entry 18 admits),
      `failure_tests::ambiguous_or_corrupt_configuration_status_never_invents_a_legacy_digest_or_writable_config`
      ("expected value at line 1 column 1") and
      `focus_scoring::tests::cpu_focus_receipt_uses_native_admitted_declared_policy_and_refuses_unbound_or_changed_description`
      ("selected model compiler/task source policy is stale or
      inconsistent"). They fail with the same messages in
      `L5C/logs/q7-nextest-service.log`, the run behind entry 29's count of
      156 of 159; no crate changed between that work's commit (`3e5dc40b`)
      and this commit's parent. Owner: the primary assigns.
  - Format is not clean and was left alone: `cargo fmt --all -- --check`
    (`fmt-l3-10b-k`) differs in 50 files, 15 touched by this commit and 35
    not. Reformatting them would change digest inputs and files whose
    checks had passed. Owner: the primary assigns.
- **Core files changed (for L4's pinned identities):** none of
  `answer_loop/{cards,calibration,focus_batch,log}.rs` or `native_loop.rs`;
  changed: `combined_context/answer.rs` (and its tests),
  `verification/inventory.rs`, `arbiter/CONTRACT.md`.

### 41. The LG arms run in separate lifetimes, side by side on the A6000

Numbered 30 until 22 November 2026; renumbered because L4's entries take
30 to 39.

- **Step:** LG preparation. This changes a constraint: plan 12.5.2 item 4 and
  12.7.1 had both Context arms in one lifetime, switched by Configure.
- **What:** arms C (Focus and Compact FP16), L (Focus and Large FP16) and F
  (Focus only) each run in their own fixture lifetime, with their own
  installation and indexes. The lifetimes run at the same time on the one
  A6000.
- **Why:** the user set a deadline for the gate. Side-by-side lifetimes run
  the arms in parallel instead of in series.
- **Memory:** measured peaks, per lifetime:

  | Arm | GPU memory | Total |
  | --- | --- | --- |
  | C | Focus about 1.8 GiB + Compact 6.0 GiB | about 8 GiB |
  | L | Focus about 1.8 GiB + Large 14.0 GiB | about 16 GiB |
  | F | Focus about 1.8 GiB | about 2 GiB |

  Together that is about 26 GiB of the A6000's 48 GiB, leaving room for the
  reserve floor (4.8 GiB) and the display.
- **Kept:**
  - one loaded copy of each model per lifetime;
  - the same build, indexes (copied, settled), scenarios and Focus route in
    every arm.
- **Changed:** the arms no longer share a process. Device contention between
  them is a measured condition of the gate, recorded with each arm's timing.
- **Roots:**
  - Each arm restarts from a closed CPU base lifetime in its own root.
  - A GPU lifetime cannot be a restart origin, because its Ready lists the
    CUDA profiles too. So the precision arms need roots of their own.
  - The bases were indexed ahead, on the current image. The restart
    evidence and disk budget (about 30 GB per root) are in `GPU_FIXTURE.md`.

### 42. munarium replaces airk as the second evaluation repository; LG restarts as series 06

- **Step:** LG preparation. Recorded for completeness: the user decided it on
  22 November 2026 at about 21:00 UTC (plan section 2, "munarium replaces
  airk").
- **What:**
  - The evaluation's two indexes are munarium (commit `9948345d`, 1,038
    tracked files) and bookbuilder (`bookbuilder-v2`, commit `238b22a7`). No
    evaluation lifetime indexes airk.
  - munarium gets ten new development scenarios with reference notes, of the
    same ten kinds. bookbuilder's ten are unchanged. airk's ten are history.
  - The LG roots start again as series `06-gate-lg`, each with a fresh closed
    CPU base holding only those two indexes. The `05-gate-lg` roots are
    withdrawn: all three base lifetimes had failed after indexing airk (C and
    L at the bookbuilder-v2 index step, F at a busy source change).
  - The leak probe becomes a check per repository, plus a check that a
    lifetime holds no airk index. Neither repository holds a file of this
    experiment, so no benchmark path needs excluding.
- **Why:** the user wrote that with munarium "we don't have to worry about the
  index providing answers about the experiment and the results should be more
  valid in spite of having to wait for the index to build."
- **Readings I took, for the user to correct:**
  - munarium's sealed set is written after LG; LG does not use it.
  - Training pairs would come from munarium and bookbuilder; nothing trains
    before the LG decisions.
  - Reports treat munarium like bookbuilder: locations and note identifiers,
    no quoted source.
  - "Stop indexing the airk code" applies to the evaluation. The user's
    installed AIRK service and its own airk index were not touched.
- **Sequencing changed with it:**
  - L4 is integrated in the main tree after L3-10 (b), not in its worktree,
    because the finished agents' Cargo targets were removed on the user's
    word.
  - The job-table wiring enters the gate image only if it is built and
    reviewed before that image; otherwise the gate's concurrency test runs on
    the unqueued path and the report says so.
- **Not repeated (user: "Do not repeat tests"):** checks that passed on
  unchanged inputs, and every one-turn measurement.
- **Where:** plan sections 2, 3.1, 3.2, 5.4, 11, 12.6 and 12.7;
  `docs/agent-current-work.md`; the ledger entries of 22 November at 21:20 and
  21:31 UTC; the harness changes on `loop/l5c`.

### 43. Coverage rosters show bounded evidence excerpts

- **Step:** L3, item (d), done in the main tree after L4 was integrated.
  It closes the open item entry 37 lists ("Coverage rosters carry no
  evidence excerpts").
- **What was wrong:**
  - The Coverage question asks which verdict fits each sub-need "given only
    its shown evidence". No evidence was shown.
  - A Coverage roster held the sub-need's query as its subject and its
    cards: `s1 covered: <two locations>`, then read-more and search cards.
    The model saw paths and line numbers, and no text of what was read.
  - The subject was the bare query. A definition sub-need and a callers
    sub-need of one identifier have the same query, so the model could not
    tell what kind of evidence the sub-need asked for.
  - The decision log recorded every span as presented in full, for model
    decisions too.
- **What changed:**
  - **Core (`native_loop.rs`):** `CardSegment` gains `evidence`, a list of
    `CardEvidence` rows: path, lines, the Focus relevance when Focus scored
    the span, an excerpt and the bytes cut after it. Only Coverage segments
    carry it, at most eight rows, in Focus rank order.
  - **The bound:** one call's excerpts total at most
    `evidence_allowance(reserved tokens)` bytes, which is the reservation
    less 2,048 tokens for the rest of the view, at one byte a token. The
    8,192-token reservation allows 6,144 bytes, 768 for each of eight
    spans. Core refuses a request above its allowance.
  - **Kernel (`answer/coverage.rs`, `model.rs`):** a Coverage call shows
    each sub-need the spans gathered for it this round, ranked by Focus.
    Every span of the call first gets an equal share of the allowance; what
    the shorter texts leave goes to the cut ones in call order; each excerpt
    ends on a character boundary. The cut depends only on the texts, their
    order and the allowance. Excerpts are prefixes of hits already read and
    retained under the Answer's revision fence; nothing new is read.
  - **Subject:** a Coverage roster's subject is now the sub-need in the
    words of its Plan card, for example `need callers: restart_guard`.
  - **Retrieval (`loop_cards.rs`):** the Coverage view shows each segment's
    `evidence` rows as `{at, focus, text, cut}`, with `at` in the
    `path:start-end` form the Covered card cites, and two more sentences of
    rules. A sub-need with nothing to show has `evidence: []`.
  - **Decision log:** for a decision taken in a dispatched Coverage call,
    `basis` is the byte range of each excerpt and `basis_omitted` is every
    cut tail and every span not shown. The reply's compact basis counts
    both, so the agent can see that something was cut.
- **What did not change:**
  - The no-model path. A decision taken without a card call builds no
    excerpt and logs the whole spans, as before. Card texts, rosters,
    roster digests, selection and the reply shape are unchanged.
  - The budget. No budget field, ledger rule, share or token pool changed;
    the bound comes from the reservation each call already holds (plan
    12.3). The call still compiles under its reservation as the ceiling.
  - Split, Plan and Follow-up views. They are byte-identical to before.
- **Wire change:** the private native request
  `airk.arbiter.advice.request/3` gains the optional segment field
  `evidence`. It is absent when empty, so a request without evidence
  serializes and hashes as before. Kernel and service ship together, and
  only the same-version supervisor sends it. No public schema, generated
  TypeScript view or persisted format changed.
- **Identities moved** (recorded decision in
  `crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md`, "Fourth move").
  `native_loop.rs` is an input of the Core loop semantics digest and
  `loop_cards.rs` of every card policy:

  | Identity | Before | After |
  | --- | --- | --- |
  | Core loop semantics | `488496bc…` | `307cb4a3…` |
  | Split | `ccea442c…` | `fecca3cb…` |
  | Plan | `5d219860…` | `487f22fd…` |
  | Coverage | `c6711506…` | `846f1db0…` |
  | Follow-up | `89809e2d…` | `104b24bb…` |
  | Batched Focus (release pair policy) | `50ba5c85…` | `19e85647…` |

  `runtime.rs` (`c9008eef…`), the CPU Focus row, the release Focus pair
  policy and the one-turn Context policy did not move. The proof that the
  old values no longer describe the source is the control
  `recorded_loop_policy_identities` failing with the old pins on the new
  source (private receipt `answer-loop/L4/int-nextest-2c-ids-old-pins.log`).
  The FP16 profiles for the gate are emitted with the new card task
  policies.
- **Why one byte a token, and 2,048 tokens for the frame:** the kernel has
  no tokenizer, so it bounds excerpts in bytes. Measured with the gate
  profiles' tokenizer on this repository's text
  (`scripts/arbiter/evaluation/card_excerpt_tokens`), no chunk needed more
  than 0.7 tokens a byte, the view around the excerpts of a full
  two-sub-need call needed about 1,060 tokens, and a full call used between
  about 2,000 and 4,600 of its 8,192 tokens. A span made only of digits,
  quotes, line breaks or control characters can need more than one token a
  byte. Such a call is refused by the lane's token ceiling before any model
  work, its reservation is released and the decision takes the
  deterministic order with the refusal recorded, as for any oversized
  input.
- **Limits, for the review:**
  - An excerpt is the start of its span. Where the relevant lines come
    after the cut, the model sees the beginning and the cut is disclosed,
    not corrected.
  - Follow-up rosters still show no evidence and their subject is still the
    bare query. Showing evidence there needs another wire decision and
    moves the identities again; naming the kind in the subject is a
    kernel-only change. Recorded in the kernel arbiter `CONTRACT.md`, "What
    does not hold yet".
  - A Coverage call may now use up to its whole reservation, where it used
    a few hundred tokens before. The pool and the per-round allowance
    already assume 8,192-token card calls (plan 12.3), so no limit changed,
    but a full loop spends more of its pool on Coverage than it did.
  - No model has scored these rosters yet. Whether the excerpts improve the
    Coverage decisions is what the gate measures.
- **Evidence (tests with hand-written expectations):**
  - Core `native_loop::tests`:
    `the_excerpt_allowance_is_the_reservation_above_the_frame`,
    `coverage_segments_carry_ranked_evidence_within_the_reservation`,
    `only_coverage_segments_carry_evidence`,
    `a_call_s_excerpts_stay_within_its_reservation_s_allowance`,
    `a_segment_without_evidence_keeps_its_wire_form`.
  - Kernel `answer::coverage::tests`:
    `excerpt_cuts_share_the_allowance_equally_then_in_call_order`,
    `excerpt_cuts_end_on_character_boundaries`,
    `a_covered_a_partial_and_a_missing_sub_need_show_their_own_spans`,
    `excerpts_follow_the_focus_rank_and_disclose_every_cut`.
  - Kernel `answer::model::tests`, through the whole loop with the
    synthetic scorer:
    `coverage_calls_show_each_sub_need_its_spans_and_log_what_was_shown`,
    `a_long_span_is_cut_to_the_excerpt_allowance_and_the_log_names_the_cut`,
    `evidence_carries_each_span_s_focus_score_in_rank_order`,
    `without_a_model_the_log_names_whole_spans_and_nothing_is_cut`.
  - Retrieval `loop_cards::tests`:
    `coverage_prompt_shows_each_sub_need_s_evidence_in_request_order`, and
    the moved pins in `recorded_loop_policy_identities`.
- **Validation (main tree, all features):**
  - workspace Clippy with `-D warnings` over all targets: exit 0;
  - nextest over Core and IPC in full, and the arbiter tests of kernel,
    retrieval, supervisor and MCP with the service's card, loop ledger and
    Focus batch tests: 1,058 run, 1,058 passed (Core 497, IPC 34, kernel
    179, retrieval 309, service 23, supervisor 9, MCP 7);
  - not run again: the CLI transport parity tests. They run without a
    model, and the no-model reply did not change.
- **Where:** Core, kernel and retrieval compiler `CONTRACT.md` files (Core:
  "Coverage evidence excerpts"; kernel: "Coverage rosters show bounded
  evidence excerpts"; retrieval: "Coverage evidence in the view" and
  "Fourth move").

### 44. Follow-up rosters show what was read; the review of entry 43 is closed

- **Step:** L3, item (d), second round, in the main tree. It closes the limit
  entry 43 recorded ("Follow-up rosters still show no evidence"), applies
  the independent review of `54b1e7a2` (one high, five medium and six low
  findings; private review `answer-loop/review-d/review-54b1e7a2.md`), sets
  the excerpt allowance from text measured in the gate's two repositories,
  and answers the primary's two questions on the token pool and on shedding.
- **What was wrong:**
  - **Follow-up:** a Follow-up roster held the sub-need's bare query and the
    candidate reads. The model chose a read without knowing what kind of
    sub-need it was for, how far it was covered, or what had been read.
  - **H1, the log said more than happened:** the basis of a decision was
    fixed before its card call was sent. A Coverage decision whose call the
    lane refused, lost or the model abstained on was logged as resting on
    the excerpts, although the deterministic order took it from the whole
    spans.
  - **M1:** an excerpt was always the start of its span, while Focus may
    have scored a window further in. The relevance beside an excerpt could
    describe text that was not shown. A read that returned only part of its
    text (`Hit.truncated`) was not marked.
  - **M2:** the compiler refuses any input holding its decision marker
    `<decision>`. One span quoting it cost its whole call, in every round
    that gathered it.
  - **M3:** a call whose reservation left no room for excerpts was still
    made, with `evidence: []` and a rule saying there was no text to judge
    by, and its choice replaced the deterministic verdict.
  - **L2:** two sub-needs of one call that gathered the same span showed it
    twice, cut differently.
  - **L4:** two rule sentences overstated ("most relevant first",
    "everything shown of what was read").
  - **The reply could not tell a shed call from the no-model path:** a
    roster a present model did not score for lack of slots, tokens or time
    was logged `deterministic`, like a roster with no model at all.
  - **The view's key order was an accident:** it was built as a JSON map,
    which this build serialises in alphabetical order, so the rules came
    after the evidence.
- **What changed:**
  - **Follow-up rosters (Core, kernel, retrieval):** a Follow-up segment now
    carries the sub-need in the words of its Plan card (for example
    `need callers: restart_guard`), its coverage `verdict` (`partial` or
    `missing`, and whether a model's Coverage choice or the deterministic
    order set it), and the same bounded evidence rows as Coverage: this
    round's ranked spans. Core requires the verdict on a Follow-up segment
    and refuses it anywhere else; Split and Plan still refuse evidence. The
    candidate cards are unchanged.
  - **Evidence rows:** a `CardEvidence` row gains `skipped_bytes` (text
    before the excerpt), `truncated` (the read returned a snippet) and
    `same_as` (the place of an earlier row of the call that shows the same
    text). A segment gains `unshown`, the gathered spans the call shows
    nothing of.
  - **M1:** an excerpt starts where the text Focus scored starts
    (`focus_window`, carried from the Focus ranking), so the relevance
    describes what is shown. The log names the shown range, and the head
    and the tail as omitted.
  - **L2:** a call shows a text once. A later span with the same text (the
    same span gathered for a second sub-need, or a copy at another place)
    is a row that names the row showing it and takes none of the allowance.
  - **The cut rule:** every shown text of a call gets the same number of
    bytes, or all of itself where it is shorter; that number is the largest
    that keeps the sum within the allowance. The result no longer depends
    on call order.
  - **M3:** a cut excerpt shows at least 256 bytes (`MIN_EXCERPT_BYTES`:
    about six lines of code or a short paragraph). A call that cannot give
    every span that much, or all the span has, is not made. The
    deterministic order decides and the part's `ModelShare` is disclosed.
  - **Reservation:** a card call's reservation is also capped by what its
    part and round have left, so a round near its allowance gets a smaller
    call with shorter excerpts where a whole 8,192-token reservation no
    longer fits.
  - **M2:** the view writes the decision marker as the compiler writes
    every other control token (`<¦decision¦>`), in subjects, card texts,
    locations and evidence text. Only the model's view changes: the
    request, its identity and the log keep the original text and byte
    ranges. `compiler.rs` is unchanged.
  - **H1:** the log's `basis` and `basis_omitted` describe what the
    decision that stands rests on. Excerpt ranges are logged only when a
    model's choice stands. A decision the deterministic order took (no
    model, a call not dispatched, refused, lost, or an abstention) names the
    whole spans and omits nothing. The Core type docs and the kernel
    contract say the same.
  - **A new decision source, `not_dispatched`:** a present model's card
    call that was not sent is logged with the model, the fallback rule and
    the reason (`token_budget`, `deadline` or `unavailable`). `deterministic`
    now means only the no-model path or a one-card roster. `context_absent`
    still means no card call was dispatched at all.
  - **The view (retrieval):** built from typed rows, so its key order is
    fixed: `task`, `rules`, then per segment `s`, `part`, `need`, `round`,
    `subject`, `verdict`, `verdict_by`, `evidence`, `not_shown`, `cards`. An
    evidence row is `{at, focus, text, before, after, snippet, same_as}`.
    The rules text is corrected (L4) and explains the new rows; Follow-up
    also explains the verdict and what each kind of read does.
- **The allowance, from measurement:**
  - **Rule:** five excerpt bytes for every four reserved tokens above a
    2,048-token frame (`EXCERPT_BYTES_NUMERATOR` 5,
    `EXCERPT_BYTES_DENOMINATOR` 4): 7,680 bytes at the 8,192-token
    reservation, 960 for each of eight spans. It was 6,144 bytes.
  - **Measured** with the gate profiles' tokenizer (`aeb13307…`), every
    text file cut into 960-byte windows and quoted as the view quotes it
    (`scripts/arbiter/evaluation/card_excerpt_tokens`, private receipt
    `answer-loop/L4/int2-excerpt-tokens-final.log`, SHA-256 `aa167989…`):

    | Source | Windows | Median | p95 | p99 | p99.9 | Maximum |
    | --- | --- | --- | --- | --- | --- | --- |
    | munarium at `9948345d` (1,020 files, 30.2 MB) | 31,884 | 0.374 | 0.604 | 0.669 | 0.753 | 0.863 |
    | bookbuilder-v2 export (8,091 files, 88.3 MB) | 94,879 | 0.308 | 0.522 | 0.585 | 0.746 | 2.000 |

    The figures are tokens per byte.
  - **The tail:** in munarium, JSON (up to 0.863) and generated Python (up
    to 0.807). In bookbuilder-v2, 12 windows of two CSV number tables
    (0.875 to 1.192) and 33 windows of four JSON files (0.866 to 2.000),
    most of them runs of escaped characters.
  - **The frame:** a two-sub-need call with eight cards and eight located
    spans, without excerpt text, is modelled at 1,269 tokens (Coverage) and
    1,321 tokens (Follow-up) of the 2,048 kept for it.
  - **Why 5/4:** a full call fits while its excerpts average at most 0.895
    tokens a byte. A call made only of munarium's densest window uses 7,948
    of 8,192 tokens (3.0% to spare); its densest eight consecutive windows
    of one file (0.739) use 6,997 (14.6% to spare). At three bytes for two
    tokens that call no longer fits (minus 13.2%) and 41 munarium windows
    pass the limit. At one byte a token the bookbuilder tail is still
    refused (34 windows), so a smaller allowance buys almost nothing.
  - **Windows a full call cannot hold:** 43 of 94,879 bookbuilder windows
    (0.045%) and no munarium window; 27 of 58,118 runs of eight consecutive
    bookbuilder windows of one file (0.046%). This is a rate of windows,
    not of calls: a call is refused when the average of its excerpts passes
    the limit, so the share of refused calls can be up to eight times the
    share of windows, or lower. The windows are fixed-offset slices of
    every file, not the spans retrieval returns. The rate in use is read
    from the gate's receipts. (Corrected after the delta review, L-e; the
    first wording claimed the rate in use was at most the window rate.)
  - **On a refusal:** the lane refuses the call before any model work, the
    reservation is released, the receipt records zero observed tokens, the
    decision takes the deterministic order on the whole spans, the slot
    stays used, and the model goes on to the next call (tests below).
- **The token pool (the primary's first question).** At the gate's settings
  (1,024-token Focus pairs, four sub-needs a part that stay gaps, a typical
  0.355 tokens a byte):
  - a full evidence call observes about 3,800 tokens (frame about 1,100,
    excerpts about 2,700);
  - one part's round costs about 30,200 tokens: Focus 16,384 (sixteen
    pairs, half the round allowance), two Coverage calls and two Follow-up
    calls. The fourth call fits only because its reservation is now capped
    by what the round has left (about 4,900 tokens). The last round has no
    Follow-up: about 24,000. A part over three rounds: about 84,900;
  - **with the old default pool of 131,072 tokens:** a one-part Answer is
    funded through round 3. A two-part Answer (65,536 a part) loses the
    model in round 3: Focus scores four pairs a part and no card call has
    room. A four-part Answer (32,768 a part) loses it in round 2;
  - **decision (budget, recorded here and in the Core contract):** the
    default `input_tokens` is now 196,608, which is two parts times three
    rounds times the 32,768-token round allowance. A two-part Answer is
    funded through every round. `round_input_tokens` was already at its
    maximum. The accepted maximum of `input_tokens` (262,144) is unchanged;
  - **what the default still does not fund:** a four-part Answer gets
    49,152 tokens a part: round 1, Focus in round 2, and then no card call.
    Four parts through three rounds would need about 340,000 tokens, above
    the accepted maximum. A caller that expects four parts can raise
    `input_tokens` to 262,144 and still loses the model in round 3;
  - **time cost:** the measured card-call times (265 ms on Compact and 589
    ms on Large, medians of 20 warm calls in `answer-loop/L5C/runs/
    s2-switch.json`) are of calls without evidence: 7 cards and 1,302 to
    1,480 input tokens. An 8,192-token call was measured earlier at about
    1.8 s (Compact) and 4.8 s (Large). Read as a straight line between
    those points, a 3,800-token evidence call takes about 0.85 s on Compact
    and 2.2 s on Large. That is an estimate, not a measurement of an
    evidence call. For a two-part Answer the larger pool adds round 3's
    four Coverage calls (about 3.4 s on Compact, 8.8 s on Large) and twelve
    more Focus pairs a part, whose time is not measured here. If every
    added token were spent on card calls, the bound is about 16 s on
    Compact and 41 s on Large, inside the 300 s default deadline. The
    model-time shedding below still applies.
- **Shedding (the primary's second question):**
  - **The estimate** is kept per Answer and per stage (`shares::CostModel`).
    Before a stage's first call it is a fixed 12,000 ms. After each call it
    is three quarters of the old value plus one quarter of the measured
    wall time of that call. It has no per-profile constant. One Answer runs
    on one Context profile, so from the second call of a stage on it
    follows that profile on that device.
  - **The rule:** a card call is not sent when the estimate exceeds the
    part's model time for this barrier: the time left before the Compose
    and Union reserve, divided by the active parts and by the rounds left.
  - **What a reader can count:** the reply's `decision_log.counts` has one
    row per stage, source and model. `model` and `abstained` are decisions
    of a call that ran. `unavailable` is a call that was sent and refused or
    lost. `not_dispatched` is a call a present model was not sent, and each
    such decision carries its reason. `deterministic` is the no-model path.
- **Low findings not fixed, with owners** (kernel arbiter `CONTRACT.md`,
  "What does not hold yet"): L1 (the allowance ignores a profile's compiler
  ceiling below the reservation; matters when a 2,048-token CPU profile
  declares the card tasks), L3 (`basis.omitted` counts cut ranges and
  unshown spans as one number), L5 (the script's frame model copies the view
  texts by hand), L6 (the 2,048-token frame is assumed, not checked: long
  subjects and paths can pass it, and the lane then refuses the call).
- **Wire changes:**
  - **Private native request `airk.arbiter.advice.request/3`:** the segment
    fields `unshown` and `verdict` and the evidence fields `skipped_bytes`,
    `truncated` and `same_as`. `unshown`, `verdict` and `same_as` are absent
    when empty. A request with evidence serialises differently from entry
    43's; kernel and service ship together and only the same-version
    supervisor sends it.
  - **Public reply and decision log:** the decision `source` gains the kind
    `not_dispatched` (`{kind, fallback, reason}`), and `decision_log.counts`
    may carry rows of that source, each naming the model that was present.
    The arbiter schema, the generated TypeScript views and the Rust-emitted
    semantic cases are regenerated through their owners. A reader that
    lists source kinds must know the new one.
  - **Default budget:** `input_tokens` 196,608. A combined Answer shares the
    larger pool among its members.
- **Identities moved** (recorded decision in the retrieval compiler
  contract, "Fifth move"):

  | Identity | Before (`54b1e7a2`) | After |
  | --- | --- | --- |
  | Core loop semantics | `307cb4a3…` | `2f89a3ae…` |
  | Split | `fecca3cb…` | `f1ecc7d9…` |
  | Plan | `487f22fd…` | `064ec90a…` |
  | Coverage | `846f1db0…` | `c2eab6c9…` |
  | Follow-up | `104b24bb…` | `b1edccc6…` |
  | Batched Focus (release pair policy) | `19e85647…` | `f90f9c77…` |

  `runtime.rs`, `compiler.rs` and `compiler/qwen.rs` did not change, so the
  Qwen compiler policy, the release Focus pair policy, the one-turn Context
  policy and the three FP16 math-policy digests did not move. The proof
  that the old values no longer describe the source is
  `recorded_loop_policy_identities` failing with the old pins (private
  receipts `answer-loop/L4/int2-nextest-ids-old-pins.log` and
  `int2-nextest-ids-live.log`). The FP16 profiles for the gate are emitted
  with the new card task policies.
- **What did not change:** the no-model path (no excerpt is built, no card
  call is made, the log names the whole spans and the source is
  `deterministic`); card texts, rosters, roster digests and selection;
  Split and Plan views apart from their key order; the Focus ranking; the
  ledger rules and every other budget field.
- **A service defect found on the way** (its own commit): `airk-service`
  did not compile with default features after L4 was integrated. Service
  arbiter `CONTRACT.md`, "A service built without `local-inference`".
- **Evidence (tests with hand-written expectations):**
  - Core `native_loop::tests`:
    `the_excerpt_allowance_is_five_bytes_for_four_tokens_above_the_frame`,
    `coverage_segments_carry_ranked_evidence_within_the_reservation`,
    `a_repeated_text_is_a_row_that_names_the_row_showing_it`,
    `follow_up_segments_state_their_verdict_and_may_show_evidence`,
    `split_and_plan_segments_carry_no_evidence_or_verdict`,
    `a_call_s_excerpts_stay_within_its_reservation_s_allowance`,
    `a_segment_without_evidence_keeps_its_wire_form`.
  - Core `answer_loop::tests`:
    `a_call_not_dispatched_names_its_model_and_reason_and_holds_no_slot`,
    `a_reply_counts_calls_not_dispatched_apart_from_the_no_model_path`,
    `default_budget_is_the_literal_recorded_decision`.
  - Kernel `answer::coverage::tests`:
    `excerpt_cuts_give_every_span_the_same_level_or_all_it_has`,
    `excerpts_follow_the_focus_rank_and_disclose_every_cut`,
    `a_call_that_cannot_show_every_span_the_minimum_is_not_made`,
    `an_excerpt_starts_where_focus_scored_and_marks_a_snippet`,
    `a_multi_byte_span_is_cut_on_a_character_and_logged_in_bytes`,
    `a_text_is_shown_once_in_a_call_and_its_repeats_name_that_row`.
  - Kernel `answer::model::tests`, through the whole loop with the
    synthetic scorer:
    `coverage_and_follow_up_calls_show_each_sub_need_what_was_read`,
    `a_model_s_choice_rests_on_the_excerpts_it_was_shown`,
    `a_decision_the_model_does_not_take_rests_on_the_whole_spans` (an
    abstained, a refused and a lost Coverage call, each with a cut span),
    `a_reservation_too_small_to_show_the_spans_makes_no_call` (at the
    frame, one token below the least excerpt, and at it),
    `evidence_follows_the_focus_rank_and_starts_at_the_scored_text` (it
    fails if ranking does nothing),
    `without_a_model_the_log_names_whole_spans_and_nothing_is_cut`.
  - Retrieval `loop_cards::tests`: `coverage_view_is_pinned_byte_for_byte`,
    `follow_up_view_is_pinned_byte_for_byte`,
    `split_and_plan_views_are_pinned_byte_for_byte`,
    `hostile_text_stays_inside_its_own_string_and_loses_the_decision_marker`,
    and the moved pins in `recorded_loop_policy_identities`.
  - Retrieval `compiler::qwen::tests`:
    `hostile_evidence_stays_data_through_the_card_view_and_the_prompt`
    (quotes, backslashes, chat-template tokens, the decision marker and
    text imitating rules, options and evidence rows, through the view, the
    compiler's escape and the prompt).
  - Service `card_scoring::settlement_tests`:
    `an_input_the_lane_refuses_releases_the_reservation_and_decides_nothing`;
    `loop_ledger::tests`:
    `a_ticket_s_window_is_the_parent_deadline_and_never_renews`.
- **Limits:**
  - No model has scored these rosters yet. Whether the excerpts, the verdict
    and the subject improve the decisions is what the gate measures.
  - The view marks a snippet; it does not fetch the rest of it.
  - The repeat rule compares whole span texts within one call. Overlapping
    spans of one file and repeats across calls, rounds and parts are the
    subject of the duplicate rule (entry 45).
  - The time figures for evidence calls are interpolated, as said above.
- **Validation (main tree):**
  - Clippy with `-D warnings` over all targets: the workspace with all
    features, exit 0; Core, kernel, retrieval, inference and service with
    default features, exit 0; `airk-cli` without default features, exit 0;
  - nextest, all features, over Core and IPC in full and the arbiter tests
    of kernel, retrieval, supervisor and MCP with the service's card, loop
    ledger and Focus batch tests: 1,074 run, 1,074 passed (Core 501, IPC
    34, kernel 185, retrieval 313, service 25, supervisor 9, MCP 7);
  - the generated views are in sync with the schema
    (`scripts/dev/contract_types.py arbiter --check`, exit 0);
  - not run in this round: the CLI transport parity binary
    (`combined_reads`) and a TypeScript typecheck. The parity binary runs
    once after the duplicate rule (entry 45), which changes the reply. No
    hand-written TypeScript lists the decision source kinds.
- **Where:** Core arbiter `CONTRACT.md` ("Evidence, verdict and calls not
  dispatched"), kernel arbiter `CONTRACT.md` ("Card rosters show bounded
  evidence excerpts" and "What does not hold yet"), retrieval compiler
  `CONTRACT.md` ("Fifth move" and "Evidence and verdict in the view"),
  service arbiter `CONTRACT.md`, and
  `scripts/arbiter/evaluation/card_excerpt_tokens/README.md`.
- **After the delta review** (private review
  `answer-loop/review-d/review-936c9b94.md`: every finding on `54b1e7a2`
  closed, one new medium and eight lows), in a kernel-only commit that
  moves no identity:
  - **Medium, fixed:** an excerpt always started at the text Focus scored,
    so a span that would have fit whole lost its head. The reviewer's case
    is one 3,072-byte read whose identifier first appears at byte 1,900:
    the model saw bytes 1,900 to 3,072 while about 6,500 bytes of allowance
    went unused. Now a text that fits its share is shown whole, and a
    longer one is shown as a window of the share's size that holds the
    scored text and starts earlier where fewer bytes follow
    (`coverage::window`). The log names what lies before and after the
    window as omitted. Tests:
    `a_window_holds_the_scored_text_and_uses_its_whole_size`,
    `a_span_that_fits_is_shown_whole_wherever_focus_scored_it`, the
    tail-anchored case in
    `an_excerpt_starts_where_focus_scored_and_marks_a_snippet`, and the
    loop test, renamed
    `evidence_follows_the_focus_rank_and_holds_the_scored_text`.
  - **L-d, fixed in part:** a window that character boundaries leave a few
    bytes short of 256 grows by whole characters while the call's allowance
    has spare bytes (test
    `a_window_short_by_a_character_grows_when_the_allowance_has_the_bytes`).
    It is not paid for by shortening another window.
  - **L-e, corrected:** the wording on expected refusals, above and in the
    measurement README.
  - **Recorded with owners, not changed** (kernel arbiter `CONTRACT.md`,
    "What does not hold yet"): L-a (a repeat row's relevance), L-b
    (`same_as` names a place), L-c (a rule sentence in `loop_cards.rs`) and
    a stale doc comment in `native_loop.rs`. Their fixes need files that
    feed the loop identities, which the FP16 profiles are being emitted
    against.
  - **Validation:** workspace Clippy with `-D warnings`, all targets and
    all features: exit 0. Nextest over the kernel's arbiter tests and the
    identity pins: all passed, and the pins are unchanged.

### 45. The reply carries each text once

- **Step:** the user's rule of 22 November 2026, about 23:15 UTC: "The
  Answer plumbing should ignore duplicate or repeated facts. It just wastes
  space." Built after entry 44, in the main tree.
- **What was wrong:**
  - Every read's hits were sent in its own record. A part's search and each
    sub-need's search often return the same chunks, so the same text was
    sent several times, once a record.
  - Two chunks of one file that share lines were both sent whole.
  - The same text found at two places (a vendored copy) was sent twice.
  - The frame is 131,072 bytes. Repeats pushed Union into its trimming
    levels and record omissions sooner than the evidence warranted.
- **What changed:**
  - **Kernel (`answer/repeats.rs`, `answer/union.rs`):** before Union writes
    the records, it plans what each span row carries, over every row of the
    reply. A row carries its text or names the row that does.
  - **Exact repeats:** rows with the very same text. The highest-ranked one
    carries it; the others have `same_as` `{record, pointer}` and no
    `text`. This works across sub-needs, rounds and parts.
  - **Rank and tie-break:** a row with a Focus relevance before one
    without, a higher relevance before a lower one, then the earliest read,
    then the earliest hit of that read.
  - **The same text at another place:** that row is a repeat too and keeps
    its own location; the carrying row lists the place in `also_at`.
  - **Overlapping spans of one file** (equal path and content hash, line
    ranges that meet): the longer text keeps what they share. A span found
    whole in it names it with `within: true`. A span that shares its head or
    tail is carried without those bytes, and `overlaps` names the row, the
    bytes and the side. Overlaps under 32 bytes are left alone.
  - **Counts (Core `UnionSummary`):** `repeats_merged`, the rows without
    text, and `overlaps_trimmed`, the rows with `overlaps`. Both are absent
    when zero.
  - **Nothing is dropped:** every row keeps its pointer, path, lines and
    content hash, and `hits_total` still counts every hit.
- **Reply fields, with an example.** Three rows of two search records; the
  second search found the first span again, the span after it, and a copy
  of a guide the first search also found:

  ```json
  {"pointer":"/hits/2","path":"docs/guard.md","start_line":1,"end_line":12,
   "content_hash":"…","text":"# Restart guard\n…","truncated":false,
   "also_at":["vendor/guard.md:1-12"]}
  {"pointer":"/hits/0","path":"crates/k/src/guard.rs","start_line":10,
   "end_line":40,"content_hash":"…","truncated":false,
   "same_as":{"record":"<first search's record id>","pointer":"/hits/0"}}
  {"pointer":"/hits/1","path":"crates/k/src/guard.rs","start_line":30,
   "end_line":60,"content_hash":"…","text":"<lines 41 to 60>",
   "truncated":false,
   "overlaps":[{"record":"<first search's record id>","pointer":"/hits/0",
                "bytes":319,"side":"head"}]}
  ```

  and in the reply's `union`: `"repeats_merged": 9, "overlaps_trimmed": 1`.
  A reader that wants a row's text follows `same_as` to the named record and
  pointer. A reader that wants a trimmed row's whole text puts the named
  bytes back at the named side.
- **Exact only, by decision.** Every comparison is of bytes. Near-identical
  merging is not shipped: any looser rule (ignoring whitespace, a
  similarity threshold) can merge two different facts, such as two versions
  of one function, and the rule's purpose is to save space, not to judge
  equivalence. A test holds four near-identical pairs apart.
- **The budget.** The frame is measured on the reply as sent, so a repeat
  costs its reference (about 150 bytes) and not its text. The saved bytes
  are spent by the existing packing: decisions, uncited hits, excerpt bytes
  and whole records are omitted later or not at all. Retention while
  gathering is unchanged: it counts what was read, and custody keeps every
  canonical result intact.
- **Every path and transport.** Union builds the one reply, so the
  no-model path, the model path, MCP and the CLI carry the same rows.
- **What a card call shows.** Since entry 44 a call shows byte-equal text
  once and gives the freed allowance to its other spans. Not done: two
  different spans of one file whose lines overlap are still shown as two
  excerpts in a call. The excerpt windows and the trims would have to be
  solved together; it is recorded with an owner in the kernel arbiter
  contract, "What does not hold yet".
- **Wire change:** two optional counts on the public reply's `union`. The
  arbiter schema and the generated TypeScript views are regenerated through
  their owners. The Rust-emitted semantic cases are unchanged, because the
  counts are absent when zero. The span rows are JSON inside a record's
  `canonical_result` (`airk.arbiter.loop-evidence/1`); a row may now lack
  `text` and may carry `same_as`, `within`, `overlaps` and `also_at`. The
  MCP help content and `public_answer_cases.json` show no reply rows and
  did not change.
- **Identities:** none moved. `answer_loop.rs`, `union.rs` and the new
  module feed no identity, and the pinned identities pass unchanged.
- **Also in this commit:** a stale doc comment on the round allowance in
  `answer_loop.rs` (delta review L-h) is corrected.
- **Evidence (tests with hand-written expectations):**
  - Kernel `answer::repeats::tests`:
    `overlap_is_exact_and_needs_a_real_run_of_bytes`,
    `an_exact_repeat_is_carried_by_its_highest_ranked_instance`,
    `two_different_texts_are_never_merged`,
    `overlapping_spans_of_one_file_share_their_text_once`,
    `a_span_inside_a_longer_one_names_it_and_so_do_its_repeats`.
  - Kernel `answer::model::tests`, through the whole loop:
    `the_reply_carries_each_text_once_and_keeps_every_location`. Two parts
    and their sub-needs search and find the same four hits (two overlapping
    spans of one file, a guide and its copy elsewhere), once without a
    model and once with the synthetic scorer. It checks the single copies,
    every `same_as`, the trimmed overlap, `also_at`, the counts, that every
    location is kept, and that no card call shows a text in two rows.
  - MCP `arbiter_handler_tests`: the check of every projection row against
    the direct canonical read (`assert_projection_matches_direct_read`) now
    covers a row with text, a trimmed row and a row that names another, and
    the handler test asserts that no two rows of a reply carry the same
    text.
- **The parity comparator** (`crates/airk-cli/tests/common/parity/context.rs`)
  renames record ids by position, because they differ by transport. A row
  that names another row cites a record id inside a projection, where the
  comparator did not look. It now rebinds exactly those ids and refuses one
  that is not a record of the same reply.
- **Validation (main tree):**
  - Clippy with `-D warnings` over all targets: the workspace with all
    features, exit 0; `airk-mcp` and `airk-cli` again after their test
    changes, exit 0; `airk-service` with default features, exit 0;
  - nextest, all features, over the loop, compose, union and handler tests
    with the identity pins: 173 run, 171 passed. The two failures were the
    MCP handler tests that read `text` on every row; with them updated the
    MCP arbiter tests are 7 run, 7 passed;
  - the generated views are in sync with the schema, exit 0;
  - **the CLI parity binary (`combined_reads`), run once in full:** 64 run,
    62 passed. The two failures were the Answer parity tests, because of
    the record ids in the new rows. After the comparator change the
    binary's six Answer tests were run again: 5 passed. The sixth,
    `current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets`,
    failed in its Open case: one member's `outline` read had 3,557 bytes on
    one transport and 3,066 on the other, which changed that member's used
    bytes and the next member's allowance. Both transports reported the
    same rows and the same `repeats_merged`. One repeat of that test on
    unchanged inputs passed. The duplicate rule does not touch a read's
    body or its size, but why that read differed once is not established,
    and the test needs an owner (retained pair:
    `artifacts/testing/combined-reads/11d7b9b8d227b6fcb574d67b394ca04b.json`).
- **Where:** Core arbiter `CONTRACT.md` ("One text, one copy in the
  reply"), kernel arbiter `CONTRACT.md` ("The reply carries each text once"
  and "What does not hold yet").
- **After the review** (independent review of `5f971a70`, private
  `answer-loop\review-d\review-5f971a70.md`: 1 high, 2 medium, 11 low):
  - **H1, fixed (text could be delivered nowhere).** The plan ran on each
    hit's whole text, and from trimming level 2 Union then cut the carrying
    row to 768 or 384 bytes. A snippet `within` a cut chunk, or the bytes a
    head or tail trim left to a cut carrier, were in no row, and a repeat of
    a cut row said `truncated: false`. Of the reviewer's two fixes the
    simpler one is taken, because it is correct by construction: overlaps
    and `within` are planned only while every carrier is sent whole (levels
    0 and 1, `repeats::plan(.., overlaps)`); from level 2 only exact repeats
    merge, and a repeat's `truncated` includes its carrier's cut
    (`union::carrier_cut`). Cost: from level 2, overlapping spans of one
    file are each sent cut on their own, so shared lines can appear twice
    (recorded under "What does not hold yet").
  - **M1, fixed.** A span trimmed by one carrier and then found inside
    another became `within` the second and lost the first trim. A span is
    now `within` another only when nothing was trimmed from it; otherwise it
    stays a trimmed row with its `overlaps`.
  - **L2, fixed (a location list could shrink).** A carrier whose `also_at`
    listed places, later found inside a longer span, became `within` it and
    its `also_at` was lost. Such a carrier now keeps its text and its list.
  - **M2, tests added** (hand-written expectations; each states which
    locations must be present and which bytes must be recoverable):
    `repeats::tests` `a_span_trimmed_at_both_ends_by_two_carriers_rebuilds_whole`,
    `a_trimmed_span_is_never_called_part_of_another` (the M1 case),
    `a_carrier_with_other_places_keeps_them_when_it_lies_within_another`
    (L2), `a_repeat_of_a_trimmed_carrier_rebuilds_the_carriers_whole_text`,
    `a_multi_byte_tail_overlap_is_cut_on_a_character_boundary`,
    `two_indexes_with_the_same_path_and_hash_each_keep_their_text`,
    `without_overlaps_only_exact_repeats_merge`; `union::tests` through the
    row code Union sends (`span_row`, extracted from `project` without a
    change of behaviour): `level_one_still_trims_overlaps_and_delivers_every_text_whole`,
    `from_level_two_a_snippet_inside_a_cut_chunk_keeps_its_own_text` (the
    H1 case at levels 2 and 3, with a repeat of the cut chunk),
    `from_level_two_overlapping_spans_are_cut_each_on_their_own`.
  - **Contracts:** both state that a row's `path`, `start_line`, `end_line`
    and `content_hash` always describe the hit's whole original span, also
    for a trimmed or `same_as` row, and that lines are never derived from
    `text` (L3). The Core contract also states that a `same_as` row without
    `within` has the named row's whole rebuilt text (L1) and that parts are
    put back in `overlaps` order (L7).
  - **Lows left as recorded:** L4 (Core does not validate projection rows;
    recorded decision, the projection is opaque to Core), L5 (the parity
    comparator also accepts an omitted record's id; in the CLI test tree,
    not these files), L6 (an exact merge ignores `truncated`; each row keeps
    its own), L8 (cost of the overlap scan; bounded by the frame), L9, L10
    (`ffa08c04`, service files) and L11 (`window` edges, correct
    arithmetic).
  - **Identities:** none moved. `repeats.rs`, `union.rs` and the test files
    feed no identity; the retrieval pin control
    `recorded_loop_policy_identities` and the `cpu_source_transition` tests
    pass (14 run, 14 passed).
  - **Validation (main tree, commands in `answer-loop\L4\commands.tsv`,
    names `pool-dup-`):** Clippy `-D warnings` over all targets of
    `airk-kernel`, `airk-service`, `airk-mcp` and `airk-cli`, all features
    and default features, exit 0; nextest of the kernel Union, repeats and
    model tests, 40 run, 40 passed; MCP arbiter handler tests, 7 run, 7
    passed. The six Answer tests of the CLI parity binary: 5 passed, and
    `current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets`
    failed in its Open case, where one member's reads had different sizes
    on the two transports (2,878 and 3,537 bytes against 2,387 and 3,046;
    retained pair `artifacts/testing/combined-reads/56eb410e498264950ebdeb03bf6a4c18.json`).
    One repeat on unchanged inputs failed the same way (a 2,403-byte read
    on one side; `b08f6266195c85201cfc31e25704bb25.json`). This is the
    read-size instability recorded above; the duplicate rule does not touch
    a read's body or size. It is not a pass, and its cause is still not
    established.

### 46. A Context profile may declare the loop card tasks

- **Step:** a blocker found by the LG agent while re-emitting the FP16
  profiles at `936c9b94`, fixed in the main tree ahead of entry 45.
- **What was wrong:**
  - Core refused every profile that declared a loop card task.
    `Profile::validate_stage` required every declared task to pass
    `Stage::permits`, which allows only the Focus task for Focus and the
    one-turn task for Context and predates the loop.
  - `Profile::validate` calls it, and `validate` runs when a profile enters
    the evaluation catalog, is prepared for qualification, is read from the
    release catalog and when a scorer is opened. So a profile from
    `admit_loop_tasks` could not be loaded on the CPU or on CUDA, and every
    card decision would have come back not dispatched.
  - It was not seen in L4 because its tests called
    `validate_selected_policies` and `calibration_set` on the admitted
    profile, never `validate`.
  - Observed: the Compact profile, after `admit_loop_tasks`, refused with
    "Arbiter profile has unsupported stage task".
- **What changed:**
  - **Core (`stages.rs`):** `Stage::declares(task)` is the stage's
    enrollment task and, for Context, the four card tasks.
    `Profile::validate_stage` and `StageStatus::validate` use it.
  - **Core (`device/state.rs`):** the device status rows of a model and of
    its variants use it too. A variant row carries its profile's tasks, so
    a loop profile in the catalog would have made the device status
    invalid.
  - **Service (`arbiter.rs`, `supports_stage_task`):** a profile that
    declares the card tasks serves an index enrolled for the Context task.
    Before, such a profile made every enrolled index report
    `UnsupportedTask`, and a device configuration that selected it was
    refused.
- **What did not change, by the decision of entry 31:** `Stage::permits`,
  enrollment, eligibility and the persisted configuration still name only
  the enrollment task; the card tasks run under the Context enrollment
  (kernel `eligible_for`, the lane's `enrollment_task` and the service's
  card admission already check the one-turn task for them).
- **Traced and left alone:**
  - the retrieval catalog and scorer paths (`runtime.rs`): their per-task
    policy checks apply only to the tasks a profile declares, and the card
    policies are checked by `validate_selected_policies`;
  - the lane's and the service's card admission, which test the profile's
    own task list;
  - `SelectedModel::supported_tasks`, which is descriptive: a model row
    without variants still lists the one-turn task;
  - release publication (`publication.rs`), which takes only the one-turn
    task for a Context profile. Publishing loop profiles in a release
    catalog is the product rollout after L7.
- **Identities:** none moved. The changed sources are `stages.rs`,
  `device/state.rs` and the service's `arbiter.rs`; none is an input of the
  loop semantics, the card or Focus policies, the Context or Focus
  semantics digests, the CPU source transition pins or the FP16 math
  policy. The pinned identities and the transition controls pass unchanged.
- **Evidence (tests with hand-written expectations):**
  - Core `stage_tests`:
    `a_context_profile_may_declare_the_loop_card_tasks_and_nothing_else_may`
    (a loop profile passes `validate`; a Context status row may list the
    card tasks; a Focus profile with a card task, a Context profile with
    the Focus task or a task of no current stage, a Focus status row with a
    card task and an enrollment that names a card task are all refused).
  - Retrieval `runtime::loop_tasks::tests`:
    `a_loop_profile_is_a_valid_profile_and_enters_a_catalog` (a catalog
    Context profile after `admit_loop_tasks` passes `validate`, is accepted
    by `for_qualification`, enters an evaluation catalog and yields its
    execution profile).
  - Service `failure_tests`:
    `a_loop_profile_serves_an_index_enrolled_for_context`.
  - Not tested: opening a scorer on such a profile, which needs the model.
- **Validation (main tree):**
  - Clippy with `-D warnings` over all targets: the workspace with all
    features, exit 0; Core, retrieval, inference, kernel, service,
    supervisor and MCP with default features, exit 0;
  - nextest, all features, over the arbiter tests of Core, retrieval and
    service: 662 run, 661 passed (Core 172, retrieval 314, service 175).
    The one failure is the service test
    `ambiguous_or_corrupt_configuration_status_never_invents_a_legacy_digest_or_writable_config`.
    It fails the same way in L4's final run and in the integration run
    before this change, and it does not touch the changed code;
  - the service's three stage-task tests, with the new one: 3 run, 3
    passed;
  - the run was made in a tree that also held the uncommitted work of entry
    45, which this commit does not include.
- **Where:** Core arbiter `CONTRACT.md` ("Which tasks a profile may
  declare"), retrieval compiler `CONTRACT.md` ("A loop profile is a valid
  profile"), service arbiter `CONTRACT.md` ("A loop profile serves an index
  enrolled for Context").

### 49. The job table stays out of the LG gate image; the gate measures concurrency on the unqueued path

- **Step:** LG preparation. Numbered after 47 and 48, which the job agent
  used on its branch for L2c and L2d (L2c's entry is now 69; 48 was never
  written).
- **What:** the first job-table wiring (`loop/l2b-wire` at `24184339`) is not
  merged before LG. Test 4 of plan sections 12.7.1 and 12.7.2 ("throughput
  with 2 and 4 concurrent Answers, using the queueing of section 12.4.1")
  runs two and four concurrent Answers on the unqueued path, where only the
  per-index lane rule orders model work, with a distinct question per
  concurrent call. The gate report says so.
- **Why:** an independent review found two high and seven medium defects.
  A job's reads were revoked when its maker's session ended, so the
  retry-after-timeout case failed and the kept failure refused identical
  requests for ten minutes. Kept replies answered repeated and concurrent
  copies of a question with the first run's timings, which would have
  counted one run several times in the gate. Fixing both and reviewing again
  cost more than the gate gained.
- **What replaces it:** the job design the user approved on 23 November
  (plan section 12.4.2, steps L2c, L2d and L2e), built beside the gate and
  merged after it.
- **Evidence:** `review-l2b\review-24184339.md` in the private `answer-loop`
  folder; the disconnect defect was later reproduced by a failing test
  through the supervisor (`L2b\l2c-h1-repro.log`).
- **Where:** plan sections 12.4.2 and 12.8; the ledger entries of 23
  November at 00:31 and 01:18 UTC.

### 50. The Answer token pool is 380,000 by default and at most

- **Decision (user, 23 November 2026):** "we're going to have to expand the
  full Answer budget to 380,000 tokens." `input_tokens` of the Answer /3
  budget now defaults to 380,000 and accepts at most 380,000 (it was 196,608
  by default with a maximum of 262,144, entry 44).
- **Why:** at the gate's settings a full evidence call observes about 3,800
  tokens, a part's round costs about 30,200 and a part over three rounds
  about 84,900 (entry 44). With 196,608 a four-part Answer gets about 48,200
  a part and loses its card calls in round 2, disclosed as `not_dispatched`
  with reason `token_budget`. Funding four parts needs about 340,000.
- **Shape check:** no rule asks the pool to be a multiple of anything. The
  round allowance (32,768) caps each part-round and is not a divisor, and
  validation asks only `1..=380,000` and `round_input_tokens <=
  input_tokens`. After an evidence-sized Split call (3,800) each of four
  parts gets 94,050 (376,200 / 4, no remainder); with no Split charge 95,000.
  Plan plus three rounds take about 88,200 of it, 5,850 to spare. A combined
  Answer over three members gives 126,667, 126,667 and 126,666.
- **What follows the number:** `MAX_INPUT_TOKENS` and `DEFAULT_INPUT_TOKENS`
  in Core `answer_loop.rs`; everything else reads the budget: the kernel
  part shares and their hand-back (`shares.rs`), the card-call reservation
  cap in `model.rs`, the combined Answer member shares and the widest budget
  (`combined_context/answer.rs`), the native allowance clamp
  (`native_loop.rs`) and the service loop ledger. No code beyond the two
  constants changed. The Rust-emitted semantic cases
  (`fixtures/split/arbiter/answer_loop_cases.json`) were regenerated through
  their Core owner: the default budget, the `input_tokens` maximum and
  over-maximum cases and the over-pool refusal (380,001). The arbiter schema,
  the TypeScript views and the help content carry no number and did not
  change.
- **Model time:** a full four-part Answer makes 45 card calls (1 Split, 4
  Plan, 24 Coverage, 16 Follow-up). At about 1.7 s a full evidence call on
  Compact that is about 77 s of card calls; at about 4.4 s on Large about
  198 s. Focus time comes on top and is not measured here. The deadline
  (300 s by default, at most 600 s) still bounds the Answer: the model-time
  shedding of entry 44 holds a card call back when its estimate exceeds the
  part's model time, so on Large a four-part Answer may now lose late calls
  for time (reason `deadline`) where it used to lose them for tokens.
- **Identities:** none moved. `answer_loop.rs` and the test files feed no
  identity; no identity-feeding file (Core `answer_loop/cards.rs`,
  `calibration.rs`, `focus_batch.rs`, `log.rs`, `semantics.rs`,
  `native_loop.rs`; retrieval `loop_cards.rs`, the task policies,
  `compiler.rs`, `compiler/qwen.rs`, `runtime.rs`) changed, and the pin
  control `recorded_loop_policy_identities` and the `cpu_source_transition`
  tests pass unchanged.
- **Tests:** Core `default_budget_is_the_literal_recorded_decision` pins
  380,000, 95,000 a part and the refusal of 380,001; kernel
  `default_pool_funds_four_parts_through_three_evidence_rounds` runs the
  real part shares under the default budget and walks the measured costs:
  every round is funded, while the old 196,608 gives 48,202 a part and loses
  round 2. That test is at the share level: the synthetic kernel fixture
  closes its sub-needs in round 1 and cannot hold a four-part Answer open for
  three rounds, so no end-to-end loop run at gate costs is claimed.
- **Where:** Core arbiter `CONTRACT.md` ("Budget").
### 51. The private Answer routes admit a loop Context profile

- **Found by:** a real service run. The service refused to warm an FP16
  loop profile, Compact and Large: "protected MHA Answer requires the
  original Context limits and its own policy pin". No gate lifetime could
  run a Context model. The scorer-level GPU confirmation opens through
  `Scorer::open`, which does not make this check, so it passed.
- **Cause:** `open_private_protected_mha_answer` in
  `crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0/answer.rs`
  asked for `Task::ContextAction`. A loop profile declares the four card
  tasks (`admit_loop_tasks`) and not the one-turn task. The protected QKV
  route and the Large FP32 batch-view route beside it had the same test.
  This is the third defect of one class (one-turn code meeting a loop
  profile); the first two were fixed in `ffa08c04`.
- **Fix:** the three routes take one predicate, `declares_context_task`:
  the one-turn task or the loop card tasks, the rule of Core
  `Stage::declares` and `loop_tasks::declares_card_tasks`. Which tasks go
  together is still checked exactly when the scorer opens. The limits (8,192
  tokens, 8 candidates) and the protected MHA policy pin are unchanged; the
  MHA admission is the pure `protected_mha_admits`, so it is tested without
  a device.
- **Test (hand-written):**
  `protected_mha_answer_admits_a_loop_profile_under_the_same_limits_and_pin`.
  It admits a one-turn and a loop profile with the right limits and pin, and
  still refuses a wrong pin, 2,048 tokens, 16 candidates, a strict-GPU
  placement and a profile with no Context task (none, or Focus only). The
  QKV and Large routes cannot be opened without a device; their change is
  the same predicate.
- **Identities:** none moved. `layer0/answer.rs` is in no `include_bytes!`
  or `include_str!` source list: the bounded-hybrid validator hashes
  `bounded_hybrid.rs` and `cuda/graph_metadata/cpu_values.rs`; the bounded
  execution policy hashes `bounded_execution.rs`, `cpu_values.rs`,
  `graph_metadata.rs`, `placement.rs` and the platform loader sources; the
  FP16 math policy hashes `fp16.rs`, `g5.rs`, `cuda.rs`, `runtime.rs`,
  `external_data.rs` and `onnx.rs`; the CPU source transition pins
  `runtime.rs`, `compiler.rs`, `compiler/focus.rs` and CUDA sources; the
  loop policies hash `loop_cards.rs` and the card task files. The pin
  control `recorded_loop_policy_identities` and the `cpu_source_transition`
  tests pass.
- **Validation:** Clippy `-D warnings` over all targets of `airk-retrieval`,
  `airk-inference`, `airk-kernel`, `airk-service`, `airk-supervisor` and
  `airk-mcp`, all features and default features, exit 0; nextest retrieval
  arbiter selection with the pins, 315 run, 315 passed; service arbiter
  selection, 177 run, 176 passed, the one failure the known older
  `ambiguous_or_corrupt_configuration_status_never_invents_a_legacy_digest_or_writable_config`.
  Not run: a warm of a loop profile on a device.
- **Where:** retrieval CUDA `CONTRACT.md` (the protected MHA recipe).
### 52. Wave workers read under the caller's worker bindings

- **Found by:** a live evaluation service. Inside an Answer every search
  read of a wave of two or more failed `read_failed` "embedding worker is
  unavailable; native adapter not installed"; symbols reads succeeded, each
  part stopped refused at Retrieve, and Focus never ran (`focus_calls` 0,
  `focus_absent`) although its stage was ready. A one-read wave, which runs
  inline, worked.
- **Cause:** the scoped-thread wave of L3-9 (`038472de`, entries 20 and 21
  describe its reads) spawned plain `std::thread::scope` workers. An
  in-process owner keeps its execution owner and embedding connector in
  thread-local slots bound on its runtime threads; a wave worker had none.
  The MCP reader runs a plan read inline on the calling thread, so the read
  ran on the unbound worker and `embedding_worker::connector()` fell back to
  the process slot, which the service process never installs. The same
  workers also missed the `ExecutionScope` (drain and quarantine at a
  checkpoint) and the accepted grant's allowance (cancel, stall, expiry,
  pause); those failed open, which is worse than the visible connector
  failure. Kernel tests used synthetic readers that need no binding.
- **Fix:** `answer/wave.rs` captures `service_execution::Inherited` on the
  driver thread before spawning and binds it around each threaded read; the
  guard restores the worker's previous bindings when the read returns or
  unwinds. This is the mechanism the indexing helper threads in
  `current/update.rs` already use; no new mechanism or dependency edge.
- **Hunt for the same defect on the Answer path:** the wave is the only
  thread hop reachable from `Kernel::answer_with`. Card calls, batched Focus,
  `file_info` probes and Union run on the driver thread. The reader's
  inline plan read in `airk-mcp` (`execute_verified_policy`, `plan_read`)
  stays on the calling thread, now bound. Retrieval has no non-test thread
  hop on the query path (`embedding_batch::parallel_map` is test-only), nor
  does the store. The combined Answer's frontend `spawn_blocking` calls read
  only selection snapshots under the control root; member reads go to each
  member's own owner runtime, whose threads are bound for life. The store's
  connection admission and quota lock policies are set only inside accepted
  work (`service_execution::run`), never on a request's driver thread, so a
  worker matches its driver without them. `ServiceComputeScope` is per thread
  by design and is never inherited.
- **Tests (hand-written, waiting on no sleep):** kernel
  `every_wave_worker_reads_under_the_callers_scopes` binds a real
  `ConnectorScope` and an installed `ExecutionScope`, cancels an accepted
  grant, and asserts that every read of a wave of two and of four runs off
  the caller's thread, finds exactly the caller's connector through
  `embedding_worker::connector()`, sees the execution scope and stops
  canceled at its own checkpoint.
  `a_binding_left_on_a_worker_reaches_neither_its_next_read_nor_the_caller`
  has every read of an eight-read wave leak a stray binding and asserts that
  each read still saw the caller's connector (some worker ran two), that the
  caller kept its own, and that an unbound caller's workers see none. Both
  failed on the old runner (wave of two: connector, scope and canceled grant
  all absent) and pass now.
- **Gap:** no integration test runs an Answer through the real handler on an
  in-process owner against an index whose search needs the embedding
  connector; the evaluation's shakedown covers it.
- **Identities:** none moved; no file in an identity source list changed.
  The pin control `recorded_loop_policy_identities` and the
  `cpu_source_transition` tests pass.
- **Where:** kernel arbiter `CONTRACT.md` ("Answer /3 loop").

### 53. Focus pairs are counted as dispatched, one way everywhere

- **Found by:** a live evaluation service, once search reads worked (entry
  52) and Focus ran there for the first time. Every Answer failed as a whole
  after about 4 s with `read_failed` "Answer Focus work exceeds its budget",
  with the Context model and with Focus only, and with budgets of
  `focus_calls=1, focus_pairs=32` and `24, 24`.
- **Which bound:** `focus_calls <= focus_pairs`, with one call whose receipt
  scored no pair. Under the `1, 32` budget it is the only one of the four
  that can trip.
- **Cause:** two counts of one thing. The kernel's ledger (`model.rs`,
  `focus.rs` `loop_pairs`) and the service's loop ledger (`reserve_focus`)
  count the pairs a call dispatched, which is what the budget's
  `focus_pairs` reserves tokens for. The decision log summary (`log.rs`)
  counted each receipt's `pairs`, the pairs it scored. A real call can score
  fewer, or none: the service stops at a refused lane, a deadline, unsettled
  work or a score that is not two finite logits, and the kernel holds a
  failed call as unknown work with no pair. The synthetic scorer of the
  kernel tests always scored every pair, so the two counts never differed.
  (Why the live calls scored nothing is the next item of this entry.)
- **Fix:** the summary and the log's own budget check count each receipt's
  `candidate_count` (pairs dispatched); the log refuses a receipt that
  dispatched outside 1 to 32 pairs or scored more than it dispatched. The
  kernel checks that a receipt's `candidate_count` equals the batch it sent,
  and a call that was not applied (scored fewer than it dispatched) now
  discloses `ModelShare { stage: read }` for every part it served; before,
  only the parts after a blocking call were disclosed. Scored pairs stay in
  custody (each receipt's `pairs`, its fixed `reason`). The reply validation
  is unchanged in substance; each of its four Focus bounds, the card-call
  bound, the round and pool token bounds and the original work budget now
  name themselves and their figures.
- **Neighbours checked** (could the kernel's accounting and the reply's
  summary disagree in a real run?):
  - Card calls and slots: one count (distinct `card_slot`s of the log); the
    kernel allocates a slot per dispatched call, including a call the lane
    refused (kept used, zero tokens). No disagreement.
  - Round and pool input tokens: the reply's round tokens are the
    attribution of every card and Focus receipt plus the kernel's
    `unknown_tokens`; an unknown Focus call is attributed its reservation.
    A known refusal attributes zero. No disagreement found.
  - Calls, candidates, bytes, rounds, parts, sub-needs: counted from the
    rounds and records the reply carries, not from receipts. No change.
  - Model work receipts: `model_work` is copied from the summary
    (`union.rs`), so it cannot differ.
  - Combined Answer: a member's used Focus pairs come from its reply summary
    (`LoopUse::of_reply`). Counted as scored, a member whose call scored
    nothing handed back pairs the service ledger had already reserved for
    good, so a later member could be refused by the service ledger. Counted
    as dispatched, the hand-back matches the service ledger. Not tested
    further.
- **Tests (hand-written):** kernel
  `a_focus_call_that_scored_nothing_is_counted_and_disclosed_not_fatal`
  (four parts, 4,096-token rounds so a round needs two 8-pair batches,
  three rounds, a native owner that fails every batch above four pairs as
  the service did: one call of 8 dispatched pairs, no pair scored, unknown
  work and every part's unscored spans disclosed);
  `calls_that_queued_no_pair_count_their_dispatched_pairs` (two calls, 16
  pairs, none scored); `focus_stops_at_its_call_and_pair_budget_and_discloses_the_rest`
  (never reached: 8 + 8; reached exactly at 2 calls and 16 pairs; calls
  exceeded by one batch: 8, parts 3 and 4 unscored; one pair short of the
  shares: 7 + 6, parts 2 to 4 unscored) with the reply's figures equal to the
  receipts and to the batches the owner was sent. The first two failed on
  the old code with "Answer Focus work exceeds its budget". Core
  `a_focus_call_that_scored_nothing_counts_its_dispatched_pairs` and three
  new refusal rows, one per bound.
- **Identity moved (recorded decision, coordinator approval of 23 November
  2026):** `log.rs` feeds the Core loop semantics digest, which every loop
  identity hashes; there is no correct fix without it, because the summary
  that the reply is checked against is built there. See the retrieval
  compiler `CONTRACT.md`, "Sixth move". The `cpu_source_transition` tests
  did not move. The Rust-emitted cases
  `fixtures/split/arbiter/answer_loop_cases.json` were regenerated by their
  Core owner.
- **Where:** Core arbiter `CONTRACT.md` (reply `model_work` and the decision
  log), kernel arbiter `CONTRACT.md` (batched Focus).
- **Why the live Focus calls scored nothing (second commit):** the service
  refused every loop batch of more than four pairs before reserving it. Its
  admission still carried the one-turn check `score_count <=
  max_candidates`, and both live Focus profiles (CUDA and the CPU twin)
  declare `max_candidates` 4; the kernel sizes batches up to 32 pairs and
  reads only the pair token limit. The refusal came back as an error, which
  the kernel holds as unknown work. Confirmed live: with the default budget
  no pair reached native; with `max_sub_needs=1, round_input_tokens=8192`
  (batches of at most 4) 5 pairs were scored on CUDA. `max_candidates` bounds
  one native request; a loop pair is one single-candidate job, so the batch
  admission now needs one candidate to fit, 1 to 32 pairs and a sufficient
  reservation. A refusal before the reservation is a declined call (nothing
  charged), and every not-applied batch names its typed stop with scored and
  dispatched counts in its receipt reason and in the service log (service
  `CONTRACT.md`, "Batch admission and refusals"). Test
  `a_batch_wider_than_the_one_turn_candidate_cap_is_admitted` (5 and 32
  pairs on a `max_candidates` 4 profile; the old predicate refused both) and
  `a_batch_that_was_not_applied_names_its_stop_and_counts`. Gap: the stage
  owner's admission against a live resident is not unit-tested (it needs a
  native resident); the live service confirmed the reading.
- **Why no card call ran (third commit):** the private Compact and Large
  Answer routes refuse a job whose deadline is more than one operation (the
  frozen resource policy's 120 s) away, after compile and before dispatch.
  A loop card call carried the whole Answer deadline, so at 300 s every call
  more than 120 s before the end was refused and settled as unknown work.
  Confirmed live: at `deadline_ms=110000` all six card calls dispatched and
  scored; at 300 s none. A card call's and a Focus pair job's lane deadline
  is now the earlier of the Answer's and one operation from now; the limit
  itself is unchanged. Card calls that take no decision are logged with the
  code of their settled reason. Test
  `a_card_call_far_from_the_answer_deadline_gets_one_operation` (250 s and
  600 s before the Answer's end: lane deadline 120 s away and admitted by the
  route's rule, restated by hand; a nearer Answer deadline kept). Open: the
  private route's admission expiry (`graph.expires`) is set by the
  evaluation fixture and also bounds every job; and the lane's own
  post-compile pre-dispatch refusals (deadline, authority, headroom, the
  private route's deadline) are untyped errors that a card call can only
  hold as unknown work. Typing them as `KnownInputRefusal` would change the
  lane contract and is left for a decision.
- **Identities:** the second and third commits change no identity-feeding
  file.
- **An Answer failed as a whole on its verification metadata (fourth
  commit):** a munarium scenario with the default `max_sub_needs` failed with
  `verification_reply_budget` (13.8 s alone and concurrently); with
  `max_sub_needs=1` it passed with a 42,898-byte reply. Cause: the handler
  sized the receipt with `receipt_budget_for_reply`, which assumes the
  65,536-byte frame of the other tools, so an Answer reply whose escaped
  text passed about 61 KiB left less than 1,024 bytes and the whole call was
  refused, although the Answer frame is 131,072. Second, Union packed the
  plain encoding while the receipt allocation and the handler's reply
  ceiling measure the escaped text, and the verification reserve (twice the
  floor plus 2,048) left no room for the annotations added after Union.
  Fix: the handler sizes the receipt in the tool's own frame
  (`verify_returned_sources_within`, `receipt_budget_within`), Union packs
  the escaped text, and the Answer's verification reserve is twice the floor
  plus 4,096, so the receipt always gets at least its floor and trims its
  source rows with the existing disclosure. Other tools keep their 65,536
  rule; a payload above it is still refused, as before. Every transport
  reaches Answer through the same handler, so the result is the same on
  each. Tests: Core `an_answer_reply_receipt_shares_the_answer_frame` (the
  boundary: 126,976 escaped bytes leave exactly 1,024; one more is refused;
  a 60 KiB reply refused in the 65,536 frame and given the 32,768 cap in the
  Answer frame), MCP `an_answer_reply_above_the_tool_frame_keeps_its_verification`
  (a 70 KB Answer reply verified with no rows omitted; the same payload as
  `file_info` still refused), and the realistic Union test now asserts the
  escaped fit. The trimming disclosure at a 1,024-byte allocation is the
  existing kernel verification test. Gap: no test runs the live scenario's
  reply through the handler. No identity-feeding file changed.
- **The fourth commit exposed a 64 KiB worker frame (fifth commit):** on the
  image built from `07d0a51c` every default-budget Answer with verification
  on failed as a whole with `scoped_observation_unavailable` (no
  `scope_observation`, nothing in the service log); with verification off
  the same Answers passed. Cause: the project worker frames each query reply
  with `MAX_FRAME_BYTES` (65,536) and the frontend reads it with the same
  limit. The old receipt allocation, (65,536 - reply) / 2, had kept reply
  plus receipt under that frame by accident; sized in the Answer frame, a
  receipt of 10 to 23 KB pushed the rendered reply past 64 KiB, the worker
  could not frame it and dropped the stream, and the caller could only
  report an unknown observation. The 131,072-byte Answer frame of the L2
  decision had never been carried by this channel. Fix: an Answer query's
  reply uses `MAX_ANSWER_REPLY_FRAME_BYTES` (131,072 plus 16 KiB for the
  query result envelope; `airk_core::service::query_reply_frame`) at the
  worker's write and the frontend's read; other tools and native reads keep
  65,536; a reply larger than its frame is answered with a typed
  `reply_frame_exceeded` refusal that fits the default frame instead of a
  dropped stream. Test (supervisor)
  `an_answer_reply_has_the_answer_frame_and_an_oversized_one_is_refused_typed`:
  a 100 KB reply fails the old frame, fits the Answer frame, and the refusal
  is typed. Gap: no end-to-end test through a real worker stream.

### 54. A record's identity binds its read without per-read status

- **Found by:** the `combined_reads` parity tests
  `current_answer_single_index_preserves_complete_pipe_cli_and_https_mcp_results`
  and `current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets`
  failed intermittently (the second in 3 of 4 runs around `f682af51`, both
  at `cad8f5b9` and `2994c65d`), and two shakedown runs of one request
  (`s10-shakedown`, `c2-compact` a and b) gave different record ids, read
  sizes, `record_uses` order, basis digests and decision-log digests.
- **What differed:** in all six retained parity pairs and in the shakedown
  pair, exactly one structural read (`context`, `outline` or `read_file`,
  on either transport or run) had a body 491 bytes smaller in the fixture
  (588 on the shakedown index) than the same read in the other reply; read
  by position, every record, carried text and `same_as` target was
  otherwise the same. Every other difference followed from record ids:
  `full_digest`, the coverage basis digests, the decision-log digest,
  paragraph evidence ids, the order of `record_uses` (kept in id order) and
  the verification selection digest.
- **Cause:** a transport-independent, run-to-run race. The MCP render adds
  `discovery_readiness` and `semantic_readiness` to every read result after
  its handler. When that observation fails it silently puts
  `"discovery_observation":"unavailable_after_read"` in their place. With
  four reads of a wave in flight, the observation's second current reader
  met `CurrentReadContention` ("current publication read is temporarily
  busy: database is locked", SQLite code 261). A temporary diagnostic in one
  parity run logged exactly one such error, in the run whose one test
  failed; the run's other 28 logged outcomes were a deterministic `Ok(None)`
  (no readiness for that result). Nothing repeated the failed observation. Second, independent of the
  race: every read result carries its own `request_id` and the freshness
  observation time, and the record id and `full_digest` hashed the whole
  result, so no record id was ever the same in two runs; the parity
  comparator hid this by renaming ids by position.
- **Decision:** a contract amendment made by the primary agent under plan
  section 12.9, with the user's rule that Answer carries no repeated facts:
  Core contract "Record identity without per-read status".
- **Fix (kernel, Core):** Core lists the per-read status once
  (`VOLATILE_READ_FIELDS`: `request_id`, `freshness`, `discovery_readiness`,
  `semantic_readiness`, `discovery_observation`, `indexing`).
  `gather::retain` keeps the result without them (`answer::volatile::stable`)
  as the record body that the id, `full_bytes`, `full_digest`, retention
  accounting and every rule use, and the result as received for custody.
  The reply carries the first search's status once, as the optional
  `read_status` (`READ_STATUS_FIELDS`); Core validates it with or without
  it, and the schema and generated TypeScript views carry it. The
  verification selection digest leaves `read_status` out, as it leaves the
  other status fields out.
- **Fix (MCP):** the readiness observation of every read is repeated within
  two seconds while it fails with a typed `repeat_same_request` contention
  (`readiness::observe_with_retry`); if contention lasts, the read fails
  with that typed hint, which the Answer's existing `busy_retry` repeats. A
  read never again carries a shorter body because a concurrent observer
  held the publication. A terse Answer reply drops `read_status` unless
  `include_status` is true. Standalone reads and Parse are unchanged.
- **Comparator:** `tests/common/parity/context.rs` no longer renames record
  ids, `full_digest`, the basis and decision-log digests or sorts the
  id-ordered lists; it checks that each `same_as` and `overlaps` row names a
  record of the reply. It still normalizes the reply's `request_id`,
  timing (`elapsed_ms`, measured work and log timings, part deadline
  shares, freshness and verification observation times, including
  `read_status.freshness.observed_unix_ms`) and a decision's Focus `receipt`
  digest, which binds measured model work. Its own test
  (`answer_reply_comparison_normalizes_only_transport_identities_custody_and_timing`)
  now builds the two observed replies with equal ids and digests and
  requires a changed record id, `full_bytes`, `full_digest`, basis or log
  digest to make the replies differ.
- **Parity runs:** without the fix (at `c1e613c7`) the many-and-open test
  failed in 3 of 5 runs and the single-index test passed in 5 of 5; one
  more run with the diagnostic failed the many-and-open test once. With the
  fix both passed in 5 of 5 runs each on `c1e613c7` and again after the
  rebase on `de0dde02` (receipts `parity-cli-before-*`, `parity-cli-after-*`
  and `parity-cli-final-*` in the campaign's `answer-loop\repro`).
- **Tests:** kernel
  `runs_differing_only_in_read_status_give_byte_equal_replies` (two runs
  with different request ids and freshness on every read and one structural
  read without readiness give byte-equal replies but for `read_status`) and
  `decisions_are_independent_of_read_completion_order`, extended so the
  reversed run stamps an arrival-numbered request id on every read and the
  reply has `same_as` rows; Core
  `reply_validates_with_and_without_its_one_read_status`; MCP
  `answer_reads_vary_only_in_classified_status_fields` (search, context and
  outline read twice through the real handlers differ only in classified
  fields: a new volatile field fails until it is classified),
  `one_answer_twice_on_one_session_gives_byte_equal_records_and_decisions`
  (only request id, timing and the timing-bound selection digest
  normalized; `read_status` kept in full mode, dropped in terse, kept with
  `include_status`) and three `readiness` tests (repeated until it succeeds,
  the typed hint kept past the budget, other errors not repeated). The
  kernel and MCP two-run tests failed with the record identity taken over
  the received result (a temporary switch, removed) and pass now.
- **Where:** Core arbiter `CONTRACT.md` ("Record identity without per-read
  status", and the amended record rule), kernel arbiter `CONTRACT.md`.

### 55. A document hit gets no line window; the refused range is typed

- **Found by:** a live evaluation service on a private repository that holds
  `.docx` files. Three of ten scenario questions carried failed records:
  `airk_context` with `line` at a hit's first cited line, `before` 0,
  `after` 40, refused `read_failed` "range crosses document locations;
  request one page/block/slide at a time". A repository of code and
  Markdown never hit it.
- **Cause:** the loop's whole-chunk `context` read (Retrieve's Chunks,
  Literal and Documentation steps, the Follow-up candidate for a cut or
  short top span, and the /2 change-set planner) always asked for at least
  40 lines past a hit's first cited line, as if every source were line text.
  A source not read as UTF-8 text keeps each chunk at one native location
  (a DOCX paragraph is one `Block`, often one line), and the context tool
  serves such a source one location at a time. The Documentation step also
  chose its hit by path (`/docs/`), which matches `.docx` files.
- **Fix:** one owning rule, `software::whole_chunk_after`, decides from the
  hit's own citation location, never from its path or extension. A `Lines`
  hit (or a `Mapped` region, which only UTF-8 composite hosts produce) keeps
  its window unchanged. A page, block, slide, table-row or worksheet-row hit
  is read only over its own cited lines, which lie inside its location, and
  only when its text was cut (Focus narrows a cut hit's citation to the
  snippet's lines and sets `truncated`). An uncut document hit already
  carries all of those lines, so a read would return the same text: it is
  omitted rather than replaced by a neighbouring location, because the hit
  does not say where its location ends and a neighbour read would be a guess
  the tool could still refuse. Documentation takes the first documentation
  hit a read can add to.
- **The other structural reads on documents:** `outline` returns the
  document's heading facts (useful, succeeds). `imports` on a document
  returns no rows (a wasted call, not a failure; left as is). `symbols` and
  `references` are name lookups; their excerpt `read_file` reads stay inside
  the cited rows they come from, so they cannot cross a location. Governing
  probes read only `CONTRACT.md` and `README.md` after `file_info`. The
  Tests sub-need uses `references` and excerpts, as above, or a search. None
  produces a failed record.
- **Typing:** the refusal (store `text_range.rs` and the kernel's retained
  snapshot path) is now `airk_core::read_arguments::crossed_document_locations()`,
  the existing typed `ReadArgumentError`: code `invalid_arguments`, field
  `range`, not retryable. The MCP handler already renders that type as its
  receipt, so callers see `invalid_arguments` instead of `read_failed` and
  can tell an argument the source cannot serve from a read that failed. The
  message text is unchanged. The Answer loop does not match on it.
- **Tests (hand-written, no private text):** kernel
  `document_hits_get_no_line_window_and_code_hits_keep_theirs` (an uncut
  block gets only its outline, a cut page is read over lines 300 to 305
  only, an uncut slide gets nothing, Documentation passes over an uncut
  `docs/` DOCX for a Markdown hit with its 40-line window, and code hits keep
  windows of 40 and 100); `a_document_top_span_is_read_only_within_its_own_location`
  (Follow-up: none for an uncut block, lines 87 to 88 for a cut page). Store
  `native_projection_tests` now asserts the crossing refusal's type, code
  and field and that the next location alone is served.
- **Identities:** none moved; no identity-feeding file changed.
- **Where:** kernel arbiter `CONTRACT.md` ("Answer /3 loop"),
  `docs/read-interface-parity.md` (typed refusal).

### 56. Decisions for step LH: one Context model named Arbiter, and what the removal of Large does to names, state and the wire

- **Set by the user (23 November 2026):** after LG, drop Large, keep only
  the smaller Context model and call it Arbiter, and make Focus and Arbiter
  separate switches for Answer. LG is documented (report 04): Large did not
  meet its bar.
- **Decided by the primary agent under plan section 12.9,** from the LH
  inventory (private campaign folder, `answer-loop\LH\inventory.md`), for
  the user's review after L7. Each is implemented in LH and recorded in the
  owning `CONTRACT.md` where it changes a contract.

| # | Question | Decision | Why |
| --- | --- | --- | --- |
| 1 | The public value that replaces `context_compact` (configuration, device status, `airkctl` JSON, an Answer's `decisions[].model`) | `arbiter`. The stage keeps its key `context`; the models a caller can select are `focus` and `arbiter`. Labels a person sees: "Focus" and "Arbiter" | The user asked for the model to be called Arbiter, and agents read this value in every Answer. The inventory recommended `context`, to avoid the word naming both the feature and a model; the user's wording decides it. It is cheap to change before the first build with it is released and a state migration afterwards |
| 2 | The Large CPU FP32 descriptor and the `context_large` slot of the release catalog | Both are removed, in the one commit that moves the catalog binding, with a compiled transition from the old binding to the new one. `ModelChoice::ContextQwenLarge` stays decodable, because retained private profiles carry it | The user dropped Large; a slot nothing can select is a second thing to keep right |
| 3 | An installation that has Large selected | Its Context stage becomes disabled with no model selected. Enrollments, policy and precision are kept, the revision is bumped and the stage reports a reason that Large was retired. Arbiter is never selected for it automatically | Selecting a model needs the person's consent and its assets |
| 4 | Saying that Focus is off by choice | No wire change in LH. `focus_calls: 0`, a null `focus_profile`, the `focus_absent` degradation and the help text carry it. A reason on `focus_absent` is tracked with the open gap that an idle reload is not named | One wire change fewer in a step that already moves state |
| 5 | The scripts that reproduce Large's retained receipts | Kept as retired history | Deleting them would orphan the receipts |
| 6 | Where a long span's excerpt starts when Arbiter runs without Focus | Unchanged in LH (from the span's start, disclosed). A lexical anchor is a candidate for an L7 change set, where it can be measured | It changes what the model sees and needs evidence |
| 7 | The status reason `smaller_model_selected` | Kept reserved | Removing a wire word buys nothing |

- **State.** The rename and the removal need a new state contract,
  `airk.arbiter.configuration/5`, with an explicit migration from `/4`,
  retained `/4` fixtures (disabled; the smaller model selected; Large
  selected) and a test that the current build decodes each.
- **Identities.** LH moves pinned identities once (Core loop semantics, the
  loop task policies, the CUDA math policy, the bounded-hybrid validator, a
  new `cpu_source_transition` row), and the FP16 CUDA profiles are emitted
  again once from retained measurements.
- **Internal names.** Rust identifiers that say Compact change only inside
  that one identity move. Domain-separation strings and profile ids stay
  verbatim: they are inside digests of retained evidence.
- **Not changed by these decisions:** the stage name `context`, the task
  names `airk.context_*`, the degradation `context_absent`, the name
  "Arbiter" for the whole subsystem in crate, module and schema names.

### 57. The evaluation fixture's dispatch census is set per lifetime

- **Step LH, commit 0 (23 November 2026).** The fixture's native dispatch
  recorder refused the 1,025th dispatch of a lifetime ("private native
  dispatch census bound exceeded"). Each of the LG gate's three arm
  lifetimes passed 1,024 dispatches (arm C: 1,025 admitted, 3,073 records,
  4.45 MB), so none wrote a settled close (`closed.json`) and step 16 of the
  gate plan closed nothing cleanly. The replies were not affected.
- **What changed.** The bound stays, because the recorder keeps one binding
  per dispatch in memory, but a lifetime may set it:
  `AIRK_NATIVE_DISPATCH_CENSUS_BOUND`, a whole number from 1 to 65,536;
  absent keeps 1,024. The record and byte bounds follow it (4 records and
  4 KiB per record per dispatch, never below the former 8,192 records and
  64 MiB). A dispatch past the bound fails with a message that names the
  bound and the variable. The harness passes it as `--dispatch-census N` on
  `fixture.py start` and `gpu_fixture.py start`; `make_loop_spec.py gate`
  sets N for every gate lifetime from its plan: the Answers the plan sends it
  (smoke, two per scenario for replays, twelve per session for the first and
  second sessions, and the concurrency runs) times the budget's
  `focus_calls + context_calls`. For the LG shape (20 scenarios, 2
  repositories, default budget) that is 528 × 72 = 38,016. A lifetime row
  that names a smaller census, or turns the observer off, is refused.
- **Evidence.** The Rust unit test
  `execution_observer::census_default_keeps_the_old_bound_and_a_lifetime_may_raise_it`
  admits dispatch 1,025 under a raised bound (the old code refused it) and
  still refuses 1,025 by default; harness tests `DispatchCensus` (fixture)
  and `test_gate_lifetimes_carry_a_dispatch_census_from_the_plan` (spec).
- **Where.** `crates/airk-service/src/service_runtime/evaluation_fixture_tests/execution_observer.rs`;
  the service Arbiter `CONTRACT.md` (dispatch observer paragraph);
  `scripts/arbiter-answer-eval/{fixture.py, gpu_fixture.py, make_loop_spec.py, FIXTURE.md}`.

### 58. The harness gets an Arbiter-only arm; Large leaves the L7 plan

- **Step LH, commit 1 (23 November 2026).** Arm A runs the remaining
  Context model (Arbiter, the former Compact) with Focus switched off. It is
  an L7 arm, not an LG arm: `make_loop_spec.py gate` builds F, C and L as the
  series ran, so LG stays reproducible. `make_loop_spec.py l7` (no `--keep`
  any more) writes Search, Parse, F, C and A; the L7 template is regenerated
  without Large.
- **Arm check.** An arm's `focus` requirement is now three-valued: true
  (Focus staged, must take part), false (arm A: no Focus call, no Focus
  profile in `model_work`, and `focus_absent` disclosed whenever the reply
  holds evidence), absent (not checked).
- **Lifetimes and switches.** `lg_lifetimes.py` accepts a Context-only pair
  for arm A and starts it with `--seed-cpu none`, so no Focus is staged; a
  lifetimes file names C, L and F (LG) or arms of C, F and A (no Large).
  `run_plan.py switch_context` refuses a switch to no Focus while the live
  stages are unknown (the plan's first switch), because a Focus seeded at
  start could not be released; after a known switch it releases Focus.
  The dispatch census allowance per session is now `common.TURN_CAP` (12,
  the value entry 57 used).
- **Evidence.** Harness tests `test_an_arbiter_only_arm_refuses_any_focus_work`,
  `test_l7_spec_has_no_cpu_cells_and_the_template_is_current`,
  `test_an_arbiter_only_switch_releases_focus_and_needs_known_stages`,
  `test_an_arbiter_only_arm_needs_no_focus_entry_and_seeds_none`; suite 237
  tests OK.
- **Where.** `scripts/arbiter-answer-eval/{loop_reply.py, make_loop_spec.py,
  lg_lifetimes.py, run_plan.py, common.py, grade.py, l7_plan_template.json,
  README.md}` and their tests.

### 59. Focus and Arbiter are separate switches: the kernel proof

- **Step LH, commit 2 (23 November 2026).** No behaviour changed: the
  inventory found no code that couples the Context stage to Focus (one stage
  owner per stage in the service; the kernel loop reads `focus_application`
  and `card_model` separately). This commit adds the proof that all four
  combinations run.
- **Evidence.** Kernel test
  `arbiter::answer::model::tests::focus_and_arbiter_are_separate_switches`
  runs a compound question (two parts, so Union runs) with neither stage,
  Focus only, Arbiter only and both, and checks for each: `focus_absent`
  exactly when Focus is off, `context_absent` exactly when Arbiter is off,
  Focus calls and the `model_work.focus_profile` only with Focus, card calls
  and model-sourced decisions only with Arbiter, every decision's model the
  Context model, and, for Arbiter only, every evidence row unscored and
  shown from byte 0 (entry 56, decision 6). 1 test, passed; Clippy
  `-D warnings` on `airk-kernel` default and all features passed.
- **Not covered here.** The same four combinations on a real CUDA service
  are LH's acceptance run, after review (the harness arm A of entry 58
  checks those replies).
- **Where.** `crates/airk-kernel/src/arbiter/answer/model/tests.rs` (the
  synthetic scorer gained a `card` switch; the file was also run through
  rustfmt).

### 60. Retained `configuration/4` fixtures for the state move

- **Step LH, commit 3 (23 November 2026).** Before Large leaves the
  catalog, the build as it stood wrote the documents a `/4` installation
  can hold, so the `/4` -> `/5` migration is tested against real bytes:
  disabled, Context enabled with `context_compact`, Context enabled with
  `context_large`, plus the compiled `DeviceCatalogBinding`, the compiled
  `DeviceCatalog` and a `DeviceStatus` of a real owner.
- **How.** The ignored emitter
  `device_management::retained_tests::emit_retained_configuration4`
  (`airk-service`, all features) wrote them into a fresh private folder
  (`L5C\scratch-lh\configuration4`), and they were copied unchanged to
  `fixtures/arbiter/configuration4/` with a provenance README. Their SHA-256
  digests are pinned in the test, apart from the emitter.
- **Evidence.** `retained_configuration4_documents_are_pinned_and_decode`
  (the current build decodes each under the retained binding, with the
  expected selection) and `retained_configuration4_binding_is_the_compiled_binding`
  (the retained binding is the compiled one, recomputed from the retained
  catalog, and the status carries it): 2 tests passed. Clippy `-D warnings`
  on `airk-service` default and all features passed.
- **Binding at this point:** catalog
  `514233c3512271e9dd0bca1075bd20947f70c44612b847b6aef6fc1907031366`,
  source `c7cf9d32afe55ee2f21531ab1dbc0171bb12a1fb10a4cecb78ef223736cc301c`.

### 61. Large-only private consumers removed outside the identity files

- **Step LH, commit 4 (23 November 2026).** Mechanical: the private
  qualification runners and diagnostics that exist only for Large are
  removed. No product path, wire value, state or pinned identity changes;
  none of the edited files is an input of a pinned digest.
- **Removed.** Inference: `cuda_qualification/fp16_large.rs` (+ `cal.rs`,
  `compensated.rs`), `batch_view.rs`, the Large final-graph metadata
  diagnostic, the Large bounded-hybrid profiling, T32, tap and Resource20
  selectors with their Large-only helpers (`equivalent_predecessor`, the
  layer17, hidden and projection tap encoders), the Large T32 profile
  trace, `TimingModel::Large`, the Large attention, dense and synthetic CPU
  diagnostics in `software_v2/attention_diagnostic.rs`, and the Large arm
  of the Linux GNU CUTLASS observer. Retrieval:
  `cuda/diagnostic/cutlass/{large.rs, large/cal.rs, large_routes.rs}`, the
  Large pressure namespace, `Scorer::diagnose_large_fp32_qkv_graph`, and the
  `large` arm of `tests/cuda_l5a_diagnostics.rs` `l5a_timing`.
- **Evidence.** Each entry point removed is an ignored test or selector
  whose only subject is Large (the user dropped Large, entry 56). Every
  helper removed then had no caller: the shared runners
  (`profile_context`, `resource20::run`, `observe_cuda_graph_metadata`,
  `protected_timing`, `diagnostic_attention_identity`) were folded to the
  only experiment left (Compact), keeping their Compact bounds, receipts,
  schemas and checks byte for byte. Clippy `-D warnings` on `airk-inference`
  and `airk-retrieval`, all targets: default features, all features, and
  `gpu-cuda,test-support` for `airk-inference`, all passed. The Compact
  unit assertions of two mixed Large/Compact tests were kept as two
  Compact-only tests. Nextest: inference 8 of 8 selected passed, retrieval
  20 of 20 selected passed.
- **Kept on purpose.** `ModelChoice::ContextQwenLarge` match arms (retained
  profiles still decode it); the L5C switch test (profile-driven, reproduces
  its receipt); the `fp16_compact::cal` chronological two-compiler batch
  (one retained two-model receipt); the service's
  `actual_large_finalization_and_cached_verification_keep_native_control_and_lease_custody`
  (it proves asset-store custody with the Large closure as input; it needs
  a new input once Large leaves the catalog: open item for entry 62); the
  Large policies and tap cuts inside identity files and the public items
  that now have no caller (`open_large_*`, `score_large_projection_taps`,
  `synthetic_large_dense_gqa_profile`, `SyntheticDenseMask`,
  `LARGE_DENSE_DIAGNOSTIC_PROFILE_ID` and its regeneration users) leave in
  the identity move.
- **Not verified here.** `gnu_cutlass.rs` is Linux-only
  (`target_os = "linux"`, GNU); it is compiled in step LX.
- **Where.** The two `CONTRACT.md` files of
  `airk-inference/.../cuda_qualification` and `airk-retrieval/.../cuda`;
  retirement notes in `scripts/arbiter-fp16-execution/CONTRACT.md`,
  `scripts/arbiter/evaluation/linux_cuda/FP16_QUALIFICATION.md` and
  `scripts/cuda-component-execution/README.md`.

### 62. Large leaves Core, kernel, retrieval and service; the model value is `arbiter`; configuration/5

- **Step LH, commit 5 (23 November 2026): the one identity, state and wire
  move of LH**, under entry 56's decisions.
- **Wire.** `SelectedModel` is `focus` | `arbiter` (Rust `Focus`, `Arbiter`);
  `context_compact` and `context_large` are gone from configuration, device
  status, `airkctl` JSON and an Answer's `decisions[].model`.
  `airk.arbiter.device-catalog/2` has the slots `focus` and `arbiter`.
  The Context stage of device status lists one model. `Degradation::Model`
  keeps its shape; only a step to no model is valid. The schema capture and
  the generated TypeScript were regenerated (`arbiter_device_contract` 3).
- **State.** `airk.arbiter.configuration/5` adds `context_retired`. The
  service's one compiled transition (old binding: catalog `514233c3…1366`,
  source `c7cf9d32…301c`) migrates a stored /4 document on the first
  management read, retaining the exact /4 bytes; evidence reads never
  migrate. A /4 Large selection becomes a disabled Context stage with no
  model, its enrollments, policy and precision kept, and the reason "The
  Large Context model was retired, …; Select Arbiter …". Arbiter is never
  selected for it. assets/4 receipts keep naming configuration/4, so
  downloaded assets keep decoding.
- **Catalog.** The release catalog lost the Large CPU FP32 descriptor
  (`unqualified-local-test-context-qwen3-4b-fp32-r1`) and the `context_large`
  slot. `ModelChoice::ContextQwenLarge` stays decodable (retained private
  profiles); `SelectedModel::from_model` refuses it.
- **Removed.** Core `device::ladder`; the Large-on-CPU rule in `cards.rs`
  and `card_model.rs`; the Large loop calibration admission; kernel Large
  model admission and the dead Large CPU device-fallback branch; retrieval
  Large bounded-hybrid policies, `open_large_*`, the Large tap cuts and T32
  checks, the Large Answer recipe, the Large FP32 probe, the Large FP16
  efficient probe and protected-MHA geometry, the Large G5 arms, the Large
  bounded-execution scope and payload; service Large private CUDA admission,
  the Large host/device envelope and the Large CPU guard; the estimator's
  Large rung and model step (answer_jobs, L2c's module: one rung now).
- **Review items closed.** L-a (rule sentence on a repeat row's relevance),
  L-b (`CardEvidence.same_as` is the row position; the view shows
  `s<segment> row <place>`), L-c (segments are judged with their own
  evidence, a named row in another segment counts as theirs; Follow-up has
  the empty-evidence sentence), the `CardEvidence.excerpt` comment, and the
  one-turn task check of the Compact Answer route in `bounded_hybrid.rs`
  (`declares_context_task`, entry 51's follow-up).
- **Identities that moved** (old -> new; pinned in
  `loop_cards::tests::recorded_loop_policy_identities` and the
  `cpu_source_transition` tests, recorded in the compiler and
  `cpu_source_transition` CONTRACTs): Core loop semantics `99b87f98…` ->
  `0a7130f7…`; split `32fcf95c…` -> `ec185b65…`; plan `e180b6be…` ->
  `b1800b6b…`; coverage `f71c7c7e…` -> `4655560b…`; follow-up `fe85b1e0…` ->
  `21704226…`; `runtime.rs` `c9008eef…` -> `66b02a58…` (inverse replay:
  `scripts/arbiter/evaluation/runtime_row_replay`, 9 checks); `cuda.rs`
  `3526ef91…` -> `ddb31240…`; `cuda/diagnostic.rs` `b784131c…` ->
  `0efbf86c…`; CPU Focus row `e0575e94…` -> `ee9b14de…`. The CUDA math
  policy, the bounded-hybrid validator and the bounded-execution policy move
  with their sources (not literal pins); every FP16 CUDA profile is emitted
  again (entry 63). The batched Focus identity over the release pair policy
  binds the loop semantics and moved too: `839af2a5…` -> `31ff048b…`.
- **Evidence.** Clippy `-D warnings`, all targets, default and all
  features, on core, kernel, retrieval, store, service, mcp, ipc, https,
  airkctl and inference; the Nextest selections and harness suite listed in
  the LH hand-back. Tests: `device::migration::tests` (6), service
  `retained_tests` (4) on the retained /4 bytes, the store migration test,
  `focus_and_arbiter_are_separate_switches`, the rewritten estimator tests.
- **Kept on purpose.** Historical `ModelChoice::ContextQwenLarge` arms in
  CPU qualification, regeneration and final-distribution code that
  reproduce retained receipts; the `Qwen3-4B` download allow-list (retained
  profiles carry those URLs); the asset-store custody test that used the
  Large closure (it needs a new input; owner: the primary).
- **Not verified here.** Linux-only `cuda/linux/product.rs` and
  `gnu_cutlass.rs` (step LX).

### 63. The Arbiter FP16 loop profile emitted once from `360178c5`

- **After LH commit 5 (23 November 2026).** Commit 5 moved the loop task
  policies, the CUDA math-policy inputs and the bounded-hybrid validator, so
  the FP16 profile that scores loop cards no longer opened. It was emitted
  again, once, from retained measurements with
  `scripts/arbiter-answer-eval/fp16_loop_emit.py` (inputs
  `lg-inputs/fp16-loop-emit-inputs.json`, unchanged) from the
  `cuda_l5a_diagnostics` image of `360178c5` (staged as
  `l5c-cuda-diagnostics-r7-360178c5-inference.exe`, SHA-256
  `3659ec4a…76f0`). Large is not emitted.
- **Result** (private, `L5C\reemit-360178c5`): profile
  `fd4a1f31687535ff8360b7917140a1d2dc8fe6ddeb30ffb1122cc7ea8d21cfcb`, manifest
  `e90b7f753315698f01a744edaf762de6afdbed7ba37897043de64e4d3755ff79`, bounded
  policy `2ad7ad8c1abe875b773a0e24f245e69ce7fdd03d37643c9a60052a59ea7aaace`
  (was `4ac58019…`), math policy `c08c51a0…`, the four task policies of entry
  62. Confirm (strict, 10 warm calls): cold open 7.34 s; warm median 56 ms at
  32 tokens, 203 ms at 1,024, 1,701 ms at 8,192; device settled. The CUDA
  Focus 1024 entry (profile `ef232291…`, manifest `a3157dd1…`) is unchanged
  and still opens and scores (cold open 2.93 s, warm 118 ms at 1,024).
- **Pairs** (`L5C\lg-inputs`, commands in `rebuild-360178c5.log`):
  `pair-C-r4` (Arbiter + Focus)
  `6045d57f37425543fe84aed9e968b04c86a35d0e20efe1cdc231b2d726b7f6d8`;
  `pair-A-r4` (Arbiter only)
  `df1bb1c7e672d831ca6a3780225ad877500dcc7ddcea7bf863afcef0f00e1b9b`. The r3
  pairs are withdrawn (`WITHDRAWN.txt`).
- **Not done here:** the LH acceptance (four combinations on a real CUDA
  service) and any Linux CUDA warm of this profile (step LX).

### 64. The names Focus and Arbiter in what people read

- **Step LH, commit 6 (23 November 2026).** Text only, no identity move:
  the Admin stage titles are "Focus" and "Arbiter", the panel is "Optional
  local models for Answer" and says the two are separate switches; MCP Help
  (topic `arbiter`), its Rust-emitted content and the shared help manifest,
  VS Code help, README, the user guide, admin pages, agent integration and
  known issues say Focus and Arbiter and that Large was retired. The Admin
  assets were rebuilt from source (`npm run build` in `packages/admin`).
  Dated history and earlier change-log entries keep their words.

### 65. A reader open waits out WAL-index recovery instead of refusing the read

Numbered 65 because the unmerged `loop/lh` already holds entries 57 to 64.

- **Found by:** the independent review of `loop/l2b-wire` (ledger, 23
  November 2026). `admin_sign_in_reads_and_cross_plane_refusals` failed 7
  of 20 stress iterations on `705ab312` and 7 of 20 on the branch; the
  branch's job test failed up to 4 of 20. Every captured refusal was
  `CurrentReadContention` with SQLite extended code 261
  (`SQLITE_BUSY_RECOVERY`).
- **Cause:** a read-only current-database open has `busy_timeout(0)`, so a
  reader that meets another connection rebuilding the WAL index this instant
  is refused at once. Query threads have no open admission. The Answer
  loop's canonical reads repeat such a refusal (`BUSY_REPEATS`), but
  `Kernel::arbiter_publication` (called by `focus_pair_tokens` in
  `Loop::rounds`) does not, so the whole Answer failed with
  `publication_busy`. Status reads in a readiness poll failed the same way.
  Two Answers at once on one service were enough.
- **Fix (owner: store):** the open is repeated every 5 ms for at most
  500 ms while the error is exactly `SQLITE_BUSY_RECOVERY`. Recovery holds
  no publication, and no snapshot exists before the open returns, so nothing
  of the caller's work is repeated. Every other contention still refuses at
  once, and a longer recovery still becomes `publication_busy`.
- **Tests:** store `database::recovery_tests` (injected clock and sleep):
  recovery waited out then opened; BUSY, LOCKED, BUSY_SNAPSHOT, CORRUPT and
  IOERR returned at once without waiting; recovery past 500 ms returned
  after 100 waits. Stress reproduction, the same 17-test group
  (`combined_reads` Answer tests and `admin_plane`), `--stress-count 20`:
  `publication_busy` failures went from 3 of 20 on `705ab312` (and 6 of 20
  on `loop/l2b-wire`) to 0 of 20 with this change.
- **Not fixed here, a second failure class of the same test:** right after
  its Index job, the test's `airk_diagnostics` readiness read is refused
  with `scoped_read_unavailable` (not retryable): 4 of 20 on `705ab312`, 1
  of 20 on the branch, 5 of 20 with this change. The reviewer counted both
  classes together. Its cause is not yet diagnosed.
- **Identities:** none moved; no identity-feeding file changed. No wire,
  digest input or persisted format changed.
- **Where:** store `current/CONTRACT.md` ("Reader open and WAL-index
  recovery").

### 66. Diagnostics observe again when a publication lands during the device read

- **Found by:** the second failure class of
  `admin_sign_in_reads_and_cross_plane_refusals` (entry 65): right after its
  Index job succeeds, the test's `airk_diagnostics` readiness read was
  refused with `scoped_read_unavailable`, not retryable. Stress counts: 4 of
  20 on `705ab312`, 5 of 20 with entry 65, 8 of 25 on the test alone.
- **Diagnosis:** a temporary capture at the MCP refusal render points (never
  committed), built with line tables and run with `_NT_SYMBOL_PATH` pointing
  at the build's PDBs so the installed fixture binary resolved symbols. All
  8 captures were created at `Kernel::scoped_device_status` (kernel
  `arbiter.rs`), called by `airk_diagnostics`. After the native device read,
  it checked that authority **and** publication were unchanged and typed
  either change as `ScopeReadRefused`. The Index's semantic publication
  lands just after its text publication, so a perfectly authorized read
  that overlapped it was refused as if it were outside its authority.
- **Fix (owner: kernel arbiter):** the two outcomes are kept apart. A
  changed authority is still `ScopeReadRefused`. A newer publication under
  the same authority means the device status is observed again against
  it, at most three observations within the original deadline. After that
  it is the retryable `publication_busy`
  (`CurrentGateContention::Applying`). This is the optimistic read already
  used between readers and the writer (snapshot, read, verify, repeat), not
  a lock: holding the writer back for a native observation would stall
  indexing behind diagnostics.
- **Tests:** `airk-mcp`
  `diagnostics_observes_again_when_a_publication_lands_during_the_device_read`
  (the owner republishes the index from inside its device read; the reply
  succeeds after two observations and names the publication that landed)
  and `diagnostics_with_a_publication_moving_on_every_read_is_retryable_busy`
  (three observations, then `publication_busy` with `repeat_same_request`).
  Both fail on the old code with `scoped_read_unavailable`.
- **Test-side change (`airk-cli` `admin_plane.rs`):** with this fix the
  test still failed 1 of 20, now on a `publication_busy` /
  `repeat_same_request` readiness reply. That reply is the documented signal
  to send the same read again, and the test's readiness loop already
  re-sends the same request until the publication settles. It now treats
  exactly that typed reply as "not yet settled", within its original 30 s
  deadline and at most 50 repeats; any other failed read still fails and is
  retained as before. Nothing else in the test is relaxed.
- **Stress, the same 17-test group, `--stress-count 20`:** entry 65 alone,
  15 of 20 passed (5 `scoped_read_unavailable`); with this kernel change, 19
  of 20 (1 `publication_busy`); with the test change, 20 of 20.
- **Not changed:** the Answer and joint delivery fences (kernel `arbiter.rs`,
  `answer.rs`, `joint.rs`) also compare the publication before delivery and
  fail with an untyped message when it moved. No test failure points at them
  yet, so they are left as they are. Focus keeps its typed stale receipts.
- **Identities:** none moved; no identity-feeding file changed. No wire,
  digest input or persisted format changed.
- **Where:** kernel arbiter `CONTRACT.md` ("G4 scoped device diagnostics").

### 67. Fences become consistency checks (local trust plan, phase A)

- **Why:** the user's [local trust plan](../simplify/local-trust-plan.md)
  (23 November 2026), decision 4 and phase A. Scope, admission and delivery
  fences reported harmless races as permanent refusals (entries 65 and 66,
  and the `scoped_connection_unavailable` in LH's acceptance). Entry 66's
  pattern now applies everywhere: a changed authority is still a refusal; a
  publication that moved under the same authority is read again, or
  answered with the retryable `publication_busy`; work that already spent
  model tokens is never replayed and reports the typed, non-replayable
  `publication_advanced`.
- **New codes:**
  - `publication_advanced` (Core `scoped_reads::PublicationAdvanced`;
    `retryable: false`, `query_replay_safe: false`, no `action`). Recorded
    in kernel arbiter `CONTRACT.md` ("Fences as consistency checks") and the
    combined Answer `CONTRACT.md`.
  - `scope_reconcile_pending` (kernel `retrieval_retry::ScopeReconcilePending`;
    retryable, `retry_after_ms: 250`, `repeat_same_request`). Recorded in
    kernel arbiter `CONTRACT.md`.
  - Catalog-control `CatalogAdmissionPending` is a new type, rendered with
    the existing `catalog_connection_busy`. Recorded in service runtime
    `CONTRACT.md`.

| Site | What it detects | Old | New | Test (fails on the old code) |
|---|---|---|---|---|
| kernel `arbiter.rs` v1 delivery fences (2) | authority or publication changed after the gathering | untyped `read_failed` | fingerprint: `ScopeReadRefused`; publication: `publication_advanced` | `arbiter::focus::tests::v1_delivery_after_a_publication_landed_is_publication_advanced` (yes) |
| kernel `arbiter/answer.rs` `finish` | the same, Answer /3 | untyped `read_failed` | the same split | `airk-mcp` `publication_landing_after_model_work_is_typed_publication_advanced_without_replay` (yes: `read_failed`) |
| kernel `arbiter/joint.rs` (2) | the same, joint | untyped `read_failed` | the same split | `joint_delivery_after_a_publication_landed_is_publication_advanced` (yes) |
| kernel `arbiter/focus.rs` Focus fence | authority, publication or Focus policy changed after ranking | `FocusPostExecutionStale` for all three | fingerprint: refusal; publication: `publication_advanced`; policy: `FocusPostExecutionStale` | `focus_publication_moved_after_native_work_is_publication_advanced` (yes) |
| `airk-mcp` `response.rs` | `FocusPostExecutionStale` rendering | `read_failed` | `arbiter_context_action_stale`, not retryable, not replay-safe | `focus_policy_change_after_native_work_renders_typed_stale_refusal` (yes) |
| kernel `scoped_index_metadata` (token and embedded publication) | publication landed during the read | `ScopeReadRefused` | read again, at most 3 observations, then `publication_busy` | `scoped_metadata_reads_again_when_a_publication_lands` (yes) |
| kernel `scoped_source_projection` (2) | expected publication replaced | `ScopeReadRefused` | `publication_busy` | `source_projection_of_a_replaced_publication_is_publication_busy` (yes) |
| kernel `SessionScope::attach` (`compile_atoms` on a new publication) | a granted root configured but not yet published | `ScopeReadRefused` | `scope_reconcile_pending`; unconfigured roots still refuse | `configuration_ahead_of_the_publication_is_pending_reconcile_not_refusal` (yes) |
| kernel `require_index_metadata` roots comparison | configuration ahead of the publication | `ScopeReadRefused` | `scope_reconcile_pending` when the authority resolves against the configuration; a real partial scope still refuses | `scoped_metadata_with_configuration_ahead_is_pending_reconcile` (yes) |
| kernel `require_bootstrap_authority` | genesis, multi-root scope | `ScopeReadRefused` | checked against the configuration: `BootstrapPublicationPending` (`publication_busy`); unconfigured still refuses | `genesis_multi_root_scope_with_configured_roots_is_pending` (yes) |
| store `source_policy.rs` `require_source_policy_live` | sources not frozen yet | `ScopeReadRefused` | `SourcePolicyReadUnavailable`; reserved roots still refuse | `unfrozen_sources_are_policy_unavailable_and_reserved_roots_refuse` (yes) |
| frontend `combined_frontend.rs` `read_member` | member projection refused because the publication moved | `source_projection_expired` | read-only member read again (3), model work `publication_advanced`, still moving `publication_busy` | `moved_member_publication_is_classified_by_its_typed_native_receipt` (classifier only, new functions) |
| frontend `combined_frontend/answer.rs` | the same, Answer member | `source_projection_expired` | `publication_advanced` | the same classifier test |
| frontend `usage_client.rs` pagination | publication moved between pages, same scope | untyped failure | collected again (3), then `publication_busy` | `a_newer_publication_during_pagination_is_the_typed_restart` (header comparison only, new function) |
| frontend `project_client.rs` `setup_retryable` | typed in-process setup transients | not retryable | retried and `retryable: true` | `typed_in_process_setup_transients_are_retryable` (yes) |
| catalog-control `begin_issue`, `capture_current_issue` | fence in flight; moved revision; clock behind | `ScopeReadRefused` | `CatalogAdmissionPending`, `CatalogRevisionChanged`, `CatalogClockPending`; namespace and delivery still refuse | `begin_side_transients_are_typed_and_delivery_still_refuses` (yes) |
| catalog-control `require_worker_start` | worker start during a census fence | `ScopeReadRefused` | `CatalogAdmissionPending` | `worker_start_during_a_census_fence_is_admission_pending` (yes) |
| service `connection_failure_code` | `CatalogAdmissionPending` | `scoped_connection_unavailable` | `catalog_connection_busy` | `begin_side_catalog_admission_pending_is_the_busy_code` (yes) |
| service `tools_call.rs` (4 `map_err`) | any setup failure | `scoped_read_unavailable` | typed transients keep their code; the rest refuse, no text | `selection_setup_keeps_typed_transients_and_refuses_the_rest` (new function) |

- **Changed test expectation:** kernel
  `current_metadata_handlers_accept_full_source_and_refuse_subset_or_new_alias`
  asserted that `configured_roots()` and `roots()` refuse once a root is
  configured but not published. That is the refusal this entry turns into
  `scope_reconcile_pending`; it now asserts `ScopeReconcilePending`. Its
  later assertions (the published root makes the scope partial and refused)
  are unchanged.
- **Left, with reasons:** the service arbiter `catalog_clock()` checks (the clock is `None`
  only when the gate's lock is poisoned, so there is no pending state to
  split); `collection_state()` being `None` after genesis (not reachable:
  genesis is handled first); the Supervisor's untyped draining, retiring,
  exchange-pending and outcome-unknown messages (no Supervisor unit fixture;
  and the outcome-unknown ones contradict the documented rule that unknown
  outcomes never construct `ConnectionSetupPending`); the combined cursor
  projection (`combined_cursor_expired` is a true expiry); and the batched
  Focus fence (already a typed non-replayable refusal).
- **Identities:** none moved; no identity-feeding file changed. No wire
  shape changed beyond the new error codes; no persisted format changed.
- **Addendum (found during phase B, 23 November 2026):** `refusal_or_pending`
  answered `scope_reconcile_pending` whenever the configuration differed from
  the published roots and the authority compiled against it. Compiling
  ignores configured roots outside the authority, so a request that the
  reconciled configuration would still refuse became retryable, and a client
  would retry until the configuration settled and still be refused. Two
  `airk-mcp` tests showed it (they failed on entry 67's code and on phase B's
  first commit, which does not touch them):
  - `native_settings_refuse_prefix_pending_sources_revocation_and_arbitrary_inputs`
    adds an ungranted root (`pending`) to the configuration of a full-index
    grant; the published roots stay empty. The settings read must refuse:
    once published, that root is outside the grant, so the scope is partial.
  - `recovery_details_outside_frozen_project_scope_are_explicitly_unavailable_without_changing_status`
    passes a rebuild observation's saved sources (`secret`) as the
    "configured" roots of the recovery-roster check; they are not a pending
    configuration at all, and the grant does not cover them, so the status
    must report `rebuild_unavailable` with `partial_project_scope`.
  - **Fix:** pending only when the same request would pass once the
    configuration is published. Evidence reads (`attach`) keep the compile
    check; index metadata (`require_index_metadata` with a configuration:
    settings, roots, the rebuild roster) also requires
    `covers_sources(primary, configured)`, the same full-coverage proof as
    the published path. Otherwise `ScopeReadRefused`. The two `airk-mcp`
    tests keep their expectations: both assert the refusal, which is correct.
  - **Kernel expectations corrected.** Two of entry 67's own kernel tests
    asserted the wrong outcome:
    - `current_metadata_handlers_accept_full_source_and_refuse_subset_or_new_alias`
      expected `configured_roots()` and `roots()` to be pending after a root
      was configured outside its primary-only authority. The same test
      then shows the published state refusing, so pending could never lift.
      They now assert the refusal.
    - `scoped_metadata_with_configuration_ahead_is_pending_reconcile`
      likewise expected pending for a primary-only authority. It now uses an
      authority that covers the configured root in full (pending, then the
      same request passes once published). The refusal case moved to the
      new `scoped_metadata_with_a_configured_root_outside_the_authority_refuses`.
    - The attach case (`configuration_ahead_of_the_publication_is_pending_reconcile_not_refusal`)
      is unchanged: an evidence read only needs the authority to resolve.
  - Evidence: both failed before (`trust-b/logs/base-mcp-settings.log`,
    `debug-rp.log`, which printed the two configurations) and pass after
    (`f1-mcp-after.log`, `native_scoped_settings_tests` 7 of 7; kernel and MCP
    `scoped`/`selection` tests 66 of 66 in `f1-after-2.log`; `airk-kernel`, `airk-mcp`, `airk-store` lib 1,679 of 1,679 in
    `f1-kernel-mcp-store-after.log`).

### 68. Credential, enrollment and connection removal (local trust plan, phase B, part 1)

- **Why:** the user's [local trust plan](../simplify/local-trust-plan.md)
  (23 November 2026), decision 3 and phase B. Since U110 every `/mcp`
  request was the built-in local agent session, yet the gateway still built a
  synthetic credential per request, startup still created the credential
  ledger, and the enrollment, revocation, connection-read and agent-choice
  surfaces stayed compiled. This entry removes them. Reviewed grants (B5) and
  the catalog-grant authority arm (B6) are **not** in it; see "Left".
- **Removed** (branch `loop/trust-b`; compiler or call-site evidence for
  each; the plan is the recorded contract decision for the wire changes):

| Item | Evidence | Commit |
|---|---|---|
| Gateway `CredentialDelivery`, its registry and authentication epoch, `ConnectionFence::bind_credential`, `Gateway::revoke`, the 1 s credential expiry sweep, the per-id service map | B1: the session replaces the credential; `ValidateRequest` now takes only the read context | `b23ffa75` |
| Non-Open `HttpSelection` arms in the gateway (`ConnectionFence::capture` takes the session's namespace; its ticket follows the current issue, U106(f)) | B1 | `b23ffa75` |
| Startup `CredentialStore::open` and `initialize_empty_owned`; the HTTPS owner's credential store, enrollment, `open_state`, `set_open_defaults` and `revoke` | B2/B3: no caller after the control methods below were removed | `c1e75342` |
| Control methods `HttpEnroll`, `HttpRevoke`, `AgentConnectionInfo`, `AgentConnectionOwnership`, `AgentConnectionVerify`, `AgentConnectionDefaults` and replies `HttpConnection`, `HttpRevoked`, `AgentConnection*` | B3; the I1 fixture now holds their bytes as `rejected_*` cases | `c1e75342` |
| `connection_id` in M2 `ToolsCatalog` and M3 `ToolSelection::Open`; `open_reads::catalog`, `defaults`, `connection_state`; agent-plane `revoke_connection` | B4; `revoke_connection` had no caller after a clean Clippy (`-D dead-code`) | `c1e75342` |
| Agent choice: `agent_scope` `Requested::Narrowed`, `list_allowed`, `outside_assigned_set`, `outside_message`; `airk_mcp::open::catalog(choice)` (now the private choice-free base of `local_catalog`); the HTTPS open backend's `OpenChoice` hook | B4; the local catalog bytes are unchanged (help content and pilot cases pass without rewriting) | `c1e75342` |
| `airkctl agents info`, `credential`, `revoke`, `verify`, `launch`, `default`; `agents list --connection-id`; `tools list|help|call --connection`; the `launch` module | B3; `Client::executable_name` and `label` had no caller after the removal (Clippy) | `c1e75342` |
| `airk runtime http-connection`, `http-connection-info`, `http-credential`, `http-revoke`, `http-legacy-connection`, `http-legacy-union-connection`; the non-cleanup `lookup-http-connection`; `runtime_http_commands` enrollment, credential, ownership and `/mcp` probe code | B3 (their only service path was `HttpEnroll`/`HttpRevoke` or the ledger) | `c1e75342` |
| The interim `airk setup` enrollment (client, connection, scope review, reviewed grant draft, launch; the global `--catalog-session`, `--combined-session`, `--project-ids`, `--index-ids`, `--member-availability` of `airk`, which only setup used) | B3 "interim setup enrollment"; `setup` now installs a reviewed payload and names `airkctl agents connect` | `c1e75342` |
| Admin rows `agents.connection`, `.ownership`, `.verify`, `.revoke`, `.connection_defaults` and the `credential_enrollment` never-over-HTTP item | B3; I3 regenerated by `fixtures/split/i3/admission_cases.py`; Admin bundle rebuilt | `c1e75342`, `8063f0fa` |
| The ignored opt-in `owned_combined_benchmark_replays_one_two_four_and_thirty_two_indexes` and `legacy_agent_credentials_keep_complete_current_native_results` tests | they exercised `HttpEnroll` and the credential helpers | `c1e75342` |

- **Compatibility rows kept** (recorded in the owning `CONTRACT.md`):
  - installer predecessor cleanup: `airk runtime lookup-http-connection
    --purpose predecessor-cleanup --predecessor …` reads the retained ledger
    read-only through `CredentialStore::read_existing` and `cleanup_owners`
    (`airk-cli` runtime-commands `CONTRACT.md`);
  - `airkctl` recognition of `Bearer`, `headersHelper`,
    `http_headers_helper` and `bearer_token_env_var` entries, including the
    retired helper command forms, and `agents repair` stripping them
    (agents-config `CONTRACT.md`);
  - VS Code `airk.httpCredential`: validates an old input and answers the
    fixed placeholder `airk-local-session`; nothing is read or issued and the
    endpoint strips the header unread (agents-config `CONTRACT.md`).
- **Retained data:** `http-1/credentials.json`, the `credential-*.token`
  files and the grant tables in `service-1.4.sqlite3` stay on disk; nothing
  writes them. No `airk-state/1` schema or persisted format changed.
- **Wire changes** (the plan's decision): the six control methods and their
  replies no longer decode; `ToolsCatalog` and `ToolSelection::Open` refuse
  `connection_id` (`deny_unknown_fields`); fixtures
  `fixtures/split/i1/control_methods.json` and
  `fixtures/split/agents/open_access.json` move those bytes to their rejected
  cases.
- **Regenerated by their owners:** `crates/airk-mcp/src/help/cli_commands.json`
  and `docs/cli-reference.md` (`AIRK_WRITE_CLI_HELP`, `AIRK_WRITE_CLI_REFERENCE`),
  `crates/airk-mcp/src/help/content.json` (`AIRK_WRITE_HELP_CONTENT`),
  `fixtures/split/ui_pilot/native_reads.json` (`AIRK_WRITE_UI_PILOT_HELP`,
  `AIRK_WRITE_UI_PILOT_CLI`), `fixtures/split/i3/admin_operations.json`
  (`admission_cases.py`), `crates/airk-cli/admin-assets/*` (`npm run build`
  in `packages/admin`), `packages/ui/generated/help-manifest.json`
  (`build-explorer.cjs --write-help-manifest`). `contract_types.py i2|i3|arbiter
  --check` reports no TypeScript drift (the I3 schema names no operation).
- **Left, with reasons:**
  - **B4, the rest:** `CredentialStore`'s write and authenticate paths,
    `AuthenticatedCredential`, token issuance and `service_http_credentials`
    token helpers, `agent_connection.rs` and the ledger types. The ledger
    types and `OpenConnectionState`/`AgentChoice` cannot go while the
    predecessor-cleanup row decodes the retained ledger (`deny_unknown_fields`
    records with a typed `HttpSelection` and `open` state); removing them needs
    a lenient cleanup reader, a decision for the plan owner. The other items
    are `pub` and so not flagged by the compiler; they need call-site review.
    Also `http_backend::factory_attached`, `LazyBackend`, `imported_legacy`,
    the installer `http_legacy` resolvers and the endpoint selection and
    registration file readers, which lost their last service caller here.
  - **B5 reviewed grants** (Admin Grants tab, VS Code grant UI,
    `airkctl --catalog-session`/`--combined-session`, `CatalogGrantDraft`,
    `CreateGrant`/`RevokeGrant`) and **B6** (`Authority::Catalog` and the grant
    halves of `ScopedReadRef` and `ScopeObservation`, with the I3
    regeneration): not started. The reviewed-service integration fixtures
    (`combined_reads`, `native_read_retry`, `tools`) are built on reviewed
    grants and catalog sessions, so B5 rewrites them onto open reads.
  - The module-level `#![allow(dead_code)]` in `catalog_admission.rs`,
    `catalog_authority.rs` and `catalog_coordinator.rs` belongs to B5/B6 and
    stays.
  - Harness scripts that drive the removed commands:
    `scripts/http_fixture.py`, `scripts/setup-local-test.py`,
    `scripts/check-container.py`, `scripts/check-vsix-upgrade-editor.cjs`
    and their unit tests (`test_http_enrollment.py`,
    `test_local_test_setup.py`, `test_container_check.py`). Their mocked
    unit tests still pass; the harnesses fail against a B build. Phase C
    rewrites `http_fixture.py` and its users.
- **Validated** (Windows, target `target-l2b`, incremental):
  - Clippy `-D warnings`, workspace, `--all-targets`, with `--all-features`
    and with default features: clean.
  - Nextest `--lib --bins` of `airk-core`, `airk-https`, `airk-service`,
    `airkctl`, `airk-installer`, `airk-endpoint`, `airk-mcp`, `airk-cli`:
    1,822 of 1,824 passed. The two failures,
    `airk-mcp` `native_scoped_settings_tests::recovery_details_outside_frozen_project_scope_are_explicitly_unavailable_without_changing_status`
    and `::native_settings_refuse_prefix_pending_sources_revocation_and_arbitrary_inputs`,
    also fail on the B1 commit, which does not touch `airk-mcp` or its
    dependencies: they expect `ScopeReadRefused` where entry 67 now answers
    `scope_reconcile_pending`. They belong to entry 67's follow-up.
  - `airk-cli` integration (`admin_plane`, `combined_reads`, `cli_commands`,
    `tools`, `mcp`, `service_runtime`, default features): 139 of 184 passed.
    44 failed before any changed code ran: the reviewed-service fixture's
    `native runtime self-test failed` (a default-feature build has no
    bundled models; this shell has no static ONNX Runtime for
    `local-inference`). One, `capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame`,
    compared `payload_bytes` that differed by two bytes because two
    `elapsed_ms` values had different digit counts.
  - TypeScript: `packages/vscode` `npm test` 1,139 of 1,147 passed, 2
    skipped, 6 failed: the installer-download tests need the repository's
    ignored `artifacts/` folder; with it, `installerDownload.test.js` passes
    7 of 7; `packages/admin` `npm
    test` 303 passed, 2 skipped, and `npm run check`; shared UI contract,
    pilot and help tests 93 of 93.
  - Python: `test_http_enrollment`, `test_local_test_setup`,
    `test_container_check`, `test_check_self_spawn` 70 passed, 3 skipped.
  - `scripts/check-repo.ps1` and `git diff --check`: clean.
- **Not verified:** the reviewed-fixture integration tests above (they need
  the `local-inference` build), any installed journey, the extension in a
  real editor, and the Linux and macOS builds.
- **Part 2 (same day, after review):**
  - **Entry 67 regression** fixed and recorded in entry 67's addendum
    (`58c0b9bf` here; `9285a1a6` on `loop/trust-b-entry67` from `4d462202`,
    for cherry-picking to `feat/arbiter-final`).
  - **B4 remainder** (`85ba0ad6`). Evidence for each removal: no caller after
    the removal of enrollment, or a compiler dead-code error under Clippy.

| Item | Evidence |
|---|---|
| `CredentialStore` (write, authenticate, ownership, attachment retirement, open defaults), `IssuedCredential`, `AuthenticatedCredential`, `read_client_token`, `authority_digest`, `ConnectionOwnership`, `OpenDefaults*`, `AttachmentRetirementProof` | Only enrollment, revocation, connection reads and the credential helper used them (all removed in part 1); predecessor cleanup moved to the lenient reader |
| Strict ledger types `Ledger`, `Record`, `RevocationReason`; `token_id`, `client_token`, `validate_attachment`, `same_authority` | Replaced by the untyped reader; no other caller |
| `agent_connection.rs` (`OpenConnectionState`, `AgentChoice`) | Only the strict ledger and `HttpConnectionInfo.open` used them |
| `HttpConnectionInfo`, `MAX_CONNECTION_BYTES`, `MAX_HTTP_SELECTION_BYTES`, `MAX_IMPORTED_HTTP_MEMBER_BYTES` | No caller after enrollment's removal (`git grep`: definition only) |
| `HttpSelection::ImportedLegacy`/`ImportedLegacyUnion`, `NativeImportedHttpSelection`/`Union`, `CatalogLegacyAuthority::admits_configuration` | Only imported-legacy enrollment and the strict ledger used them |
| `http_backend::validate`/`validate_details`/`factory`/`factory_attached`, `LazyBackend`, `imported_legacy` | The gateway serves only the local session; replaced by `validate_open` |
| Installer `http_legacy` (`resolve`, `resolve_claim`, `validate`, `validate_registry`) | Its only callers were `enroll_legacy*` and the imported-legacy backend |
| Endpoint `read_selection_file`, `read_registration_file`, `read_registrations_file` | Inputs of the removed enrollment commands |
| `explorer_assistance::window_retired` | Clippy `dead-code` after the store's removal |
| `open_refusal::AGENT_SCOPE_CHOICE_DISABLED`, `AGENT_CONNECTION_NOT_OPEN`, `AGENT_DEFAULTS_CONFLICT`, `LOCAL_AGENT_CONNECTION` | No producer or reader left (`git grep`: definition only) |

  - **Compatibility row kept:** predecessor cleanup reads the retained
    ledger through `RetainedLedger`. It is lenient and read-only: untyped
    JSON, only `schema`, `binding` and per record `id`, `revoked`,
    `expires_unix_ms`, `verifier`, plus retirement markers. It never writes,
    and it refuses if the bytes change during capture. A current
    installation with no ledger and no credential file owns nothing; that is
    every installation since B. Tests: `retained_ledger::tests`
    (hand-written old-install ledger, byte-identical afterwards; the
    captured v1 fixture) and the installer `clean_break_*` lookups.
  - **Harness scripts** (`3d22b08c`): `http_fixture.py` loses
    `enroll_native`; `check-service-clients.py` loses its unused
    `enroll_http`; `check-vsix-upgrade-editor.cjs` reads `runtime
    http-endpoint`. `check-container.py` keeps calling `runtime
    http-connection` only on its pre-U110 predecessor-image branch (the
    current-build branch already reads the endpoint), and
    `validate_enrollment` stays for it.
  - **Left, needing a decision:**
    - **B5/B6 (reviewed grants and the catalog-grant authority arm).**
      Removing `CatalogOperation::CreateGrant`/`RevokeGrant` changes a
      persisted format. `FenceIntent` stores the whole `CatalogRequest`
      in `index_operation_receipts`, and `RevokeGrant` requires an
      authority fence (`catalog_management.rs`
      `requires_authority_fence`). The service decodes stored intents at
      startup (`airk-store` `runtime_contract.rs`, `fences.rs`,
      `source_change.rs`, `split.rs`). A retained intent naming
      `RevokeGrant` would then fail to decode. The first brief says to stop
      such a step, so it was not started. Options for the user:
      - keep the two variants as decode-only (refused on new requests);
      - or approve a migration of retained intents.

      The Admin Grants tab, the VS Code grant UI, `--catalog-session`,
      `--combined-session` and `CatalogGrantDraft` do not touch persisted
      formats. They can go once that choice is made, together with
      rewriting the reviewed-grant integration fixtures onto open reads.
    - **`scripts/setup-local-test.py`.** Its whole flow is pre-U110
      enrollment: a selection or registration file, `runtime
      http-connection`/`http-legacy-connection`, `setup
      --existing-connection-id` and a bearer environment variable in the
      Codex entry. It needs a redesign around `airkctl agents connect`,
      not an edit. Its unit tests mock the old calls and still pass.
  - **Integration environment** (from the L3 and trust-a builds): static
    CPU ONNX Runtime (`ORT_LIB_LOCATION` = `ort130-x64-owned-r5\aggregate`,
    `ORT_SKIP_DOWNLOAD=1`, `ORT_PREFER_DYNAMIC_LINK=0`,
    `ORT_LIB_PROFILE=Release`, `LIBONNXRUNTIME_NO_PKG_CONFIG=1`), the
    bundled model directory, the offline `CARGO_HOME`,
    `CARGO_PROFILE_DEV_OPT_LEVEL=2`, `--all-features --profile ci`, and
    `TEMP`/`TMP` on D: in the trust-b folder. With it the reviewed-fixture
    self-test passes. `admin_plane`, `combined_reads`, `cli_commands`,
    `tools`, `mcp` and `service_runtime` pass 196 of 196 (2 skipped).
  - **The `payload_bytes` failure** of
    `capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame`
    did not reproduce. It passed once on base `4d462202` and once on
    `loop/trust-b` in that environment. The part-1 failure compared
    `payload_bytes` 123934 against 123932, and the only other differences
    were timing and identity fields. Two `elapsed_ms` values of different
    digit counts (112 and 93) account for the two bytes. The test is
    therefore timing-dependent. It was not retried and not loosened.
  - **Pre-existing:** `airk-service`
    `arbiter::failure_tests::ambiguous_or_corrupt_configuration_status_never_invents_a_legacy_digest_or_writable_config`
    fails with `--all-features` on base `4d462202` as well. That is
    `logs/base-items-3b.log`; the arbiter configuration status is not
    touched by B.
  - **Validated (part 2)**, Windows, target `target-l2b`, the integration
    environment above:
    - Clippy `-D warnings`, workspace, `--all-targets`, with all features
      and with default features: clean.
    - Nextest `--lib --bins --all-features --profile ci` of `airk-core`,
      `-endpoint`, `-installer`, `-https`, `-service`, `airkctl`,
      `airk-mcp`, `-kernel`, `-store` and `airk-cli`: 3,414 of 3,415
      passed. The one failure is the `airk-service` arbiter test above,
      which also fails on base.
    - `airk-cli` integration (six binaries): 196 of 196.
    - `packages/vscode` 1,145 of 1,147 (2 skipped); `packages/admin` 303 of
      305 (2 skipped); shared UI 93 of 93.
    - `contract_types.py i2|i3|arbiter --check`; `check-repo.ps1`;
      `git diff --check`.
    - Python `unittest discover` over `scripts/tests`: 2,331 run, 46 not
      passing. Rerunning the same modules on base `4d462202` failed 55, so
      none is new. One rerun individually here
      (`test_package_extension...test_actual_compiled_runtime_imports_are_all_staged`)
      fails on `mcpStdioSession.js -> ./answerDeadline`, which phase B did
      not touch.
- **Part 3: reviewed grants removed (B5, B6; branch `loop/trust-b2`).**
  - **Why:** the user's decisions of 23 November 2026: reviewed catalog
    grants go entirely, and because the build that wrote them was never
    deployed beyond its author's machine, their records are deleted on
    upgrade rather than kept readable.
  - **Authority now:** a read is authorized only by an in-memory selection:
    a window's W4 grant or the Admin session's A2 grant
    (`SelectionReadAuthority`), or a U105 open agent read of whole indexes
    (`OpenReadAuthority`). Both live in the service process only, are
    registered by nonce in the native gate, are never durable fence targets
    (`FencePlan` targets are now always empty) and never leave the private
    host/worker path. There is no persisted bearer to review, revoke or
    import, so nothing durable grants read authority any more.
    `ScopedReadRef.selection` is required; its bytes equal the former
    optional field.
  - **Store upgrade step:** `catalog::grant_removal::upgrade` in the
    read-write `ServiceRegistry::open`, described in the registry
    `CONTRACT.md`. Regressions in `grant_removal/tests.rs`: an old database
    with grant tables, fenced grant receipts, a completed grant-scoped fence
    and a reference refresh job upgrades in one open, the second open does
    nothing, other rows are unchanged; an unfinished grant-scoped fence
    refuses with `CatalogUpgradeBlocked` and leaves the file unchanged; a
    read-only open refuses typed; every predecessor maps to its terminal
    layout. Terminal schemas: `fixtures/schema/airk-state-1-terminal/`.
  - **Removed:**

| Item | Evidence |
|---|---|
| Core `GrantId`, `ReviewedGrant`, `GrantProvenance` (module), `catalog_legacy_authority`, `catalog_refresh`, `CatalogOperation::CreateGrant/RevokeGrant`, `CatalogKind::Grants`, `CatalogEntity/CatalogItem::Grant`, `ScopeGrantObservation`, the grant halves of `ScopedReadRef`, `ScopeObservation`, `ResolvedSelection`, `QuerySelection::resolve(grant)` | B5/B6; no caller after the removals below (Clippy `-D warnings`) |
| Control methods `ConnectScoped`, `CatalogGrantDraft`, `ValidateCombinedDelivery`, `ReferenceRefreshReceipt`; replies `CatalogGrantDraft`, `ReferenceRefreshReviewRequired`, `ReferenceRefreshReceipt`, `CombinedDeliveryValidated`; `ScopedConnectRequest`, `CombinedConnectRequest`; `ResolveTarget::Catalog`; `ToolSelection::Catalog/Combined`; `ToolsCatalog.combined`; `UsageDocument.catalog`; `HttpSelection::Scoped`; `ProjectConfiguration::RefreshReferences` | B5; I1 fixtures hold the old bytes as rejected cases |
| Kernel `Authority::Catalog`, `ScopedReadAuthority`, `with_scoped_reads`, `service_reference_refresh` and the reviewed-refresh indexing plumbing | B6 |
| Catalog control `catalog_grant_draft`, `reference_refresh_control`, the catalog reserve and renew paths of `catalog_admission`, `build_catalog_read_authority`, `build_legacy_catalog_read_authority`, `native_scoped_selection`, legacy binding review; the module-level `#![allow(dead_code)]` of `catalog_admission`, `catalog_authority` and `catalog_coordinator` lifted | B6; what became dead after the lift was deleted |
| Store grant tables and code, `legacy_index_projects`, `index_context_imports`, the adopted-authority tables and `adopted_authority.rs` (adoption already refused: `STANDALONE_ADOPTION_UNSUPPORTED`, no inserts), `historical_inactive_retirement`, refresh admission and jobs | the user's record cleanups of 23 November 2026, item 4 |
| Supervisor `ConnectionSelection::Reviewed/Legacy`, `NativeLegacyAuthoritySelection`, `issue_scoped`, `issue_legacy`, the pipe `ScopedConnectionDelivery::deliver`, worker `InstallScope`/`RenewScopeLease` and `Reply::ScopeInstalled`, the catalog ledger of `project_worker::scopes`, the `refresh_references` hook | B6; selections and open reads are handed over in process (`take_in_process`) |
| Frontend `combined_frontend::resolve`, `validate_delivery*`, `validate_http_selection`, the catalog `Session` impl; `project_client::scoped_tools`, `Connection::connect_scoped`; `service_frontend::resolve_scoped`; `unified_mcp::serve_scoped/serve_combined`; the `*_selected` usage readers; `RemoteTools::scoped_selection/combined_selection` | B5 |
| Service connect/legacy/grant-draft/refresh/combined-delivery routes; `ControlMethod::Connect{project}` under an activated catalog now refuses `scoped_read_unavailable` (its only authority was an imported legacy grant, which nothing writes) | B5/B6 |
| `airkctl --catalog-session`, `--combined-session` and their `--project-ids/--index-ids/--member-availability`; `library show --grant`; `agents connect --connection-id/--open/--selection-file` (no reviewed test or contract needed a compatibility refusal: they are now unknown-argument usage errors) | B5; CLI help and reference regenerated |
| Admin Grants tab and grant operations; VS Code grant UI, `catalogSession`, `scopeSelection`, `referenceRefresh`, `workbenchReferenceRefresh` (and their tests), removed from `scripts/package-extension.py` | B5 |
| Evaluation fixture `readable_selection.rs` and the optional `reviewed_selections` input (refused if present); no live script supplied it | B5 |
| `airk-cli` test `combined_reads/reference_refresh.rs` | reference refresh removed |

  - **Integration fixtures rewritten** onto window selections (where
    membership scopes matter) and open reads: `tests/common/{mod,reviewed,
    read_retry,ctl,parity,pipe}.rs` (new `pipe.rs`: W1/W4 window helpers and a
    `PipeTools` adapter over M3 `ToolsCall`), `combined_reads` and its
    submodules, `tools`, `mcp`, `service_runtime`, `native_read_retry`,
    `admin_plane`, `cli_commands`, `helper_outage_reads`. Removed grant-only
    tests: the two window-versus-catalog parity tests, the three reference
    refresh tests, the catalog setup-retry and JSON-selector capture tests of
    `native_read_retry`, and `catalog_session_bodies_keep_their_request_and_grant`
    (frontend). Grant-only unit tests removed from `catalog_admission`
    (14, built on catalog installs and durable targets),
    `project_worker::scopes` (6; the redemption test was converted), the
    supervisor's pipe-delivery tests (2), and `combined_defaults`.
  - **`airkctl tools shell`** now reads through a session-routed window, as
    `tools call` does; an explicit project selection keeps precedence.
  - **Wire fixtures regenerated by their owners:**
    `fixtures/wire/read_retry_decisions.json` and `read_retry_envelopes.json`
    (`read_retry_*.py`; the catalog cases removed, the removed grant fields
    kept as refusals), `fixtures/split/i1/selection_authority.json`.
  - **Validated** (Windows, target `target-l2b`; integration tests in
    `target-l2b-cli`; the integration environment of part 2):
    - Clippy `-D warnings`, workspace, `--all-targets`, all features and
      default features: clean (`logs/ws-all2`, `ws-all3-default`, `ws-all4`).
    - Nextest `--workspace --lib --bins --all-features --profile ci`: 4,725
      run, 4,694 passed, 31 failed (`logs/nt-ws-lib1`). 22 of the 31 were
      stale fixtures or expectations, fixed above and rerun green
      (`nt-fail2`, `nt-fail3`). Nine remain, listed under "Left".
    - `airk-cli` integration, all features: 476 run, 467 passed
      (`logs-cli/nt-09-full`). Fixed since: `native_read_retry` (all pass,
      `nt-p-cli1`), the `tools shell` and help-text tests (`nt-p-cli2`).
      The rest are listed under "Left". The full suite was not rerun after
      those fixes, and the 20-run stress of `combined_reads` Answer and
      `admin_plane` was not run.
    - `packages/vscode` 1,024 of 1,026 (2 skipped); `packages/admin` 303 of
      305 (2 skipped; the four R22 picker tests failed once on `EBUSY` from
      Edge under load and pass rerun alone); shared UI 215 of 216 (the one
      failure is the shared-editor Service test reported before this part).
    - Python `pytest scripts/tests`: 2,298 passed, 28 failed, 4 collection
      errors (missing `pyarrow`), all in packaging, research-notice, supply
      chain and runtime-upgrade modules this part does not touch; the VSIX
      staging failure is the `mcpStdioSession.js -> ./answerDeadline` one of
      part 2. Not compared against a base run.
    - `contract_types.py i2|i3|arbiter --check`, `check-repo.ps1`,
      `git diff --check`: clean.
  - **Left, with reasons:**
    - **Eight Focus tests** (`airk-kernel` `arbiter::focus::tests` six,
      `airk-mcp` `focus_handler_tests` two) exercised Focus on an ordinary
      unpartitioned single-index catalog-scoped read. Every installed read is
      now a selection, and an unpartitioned selection read never Focuses
      (the existing rule); production single-index reads Focus through the
      singleton partition of `tools_call::singleton_focus`. The tests need
      rewriting onto that partition; not done.
    - **A pipe float defect**, found by the rewritten
      `permanent_trim_keeps_allowed_occurrences_of_a_shared_native_vector`:
      across the service pipe a structured float can change by one ULP
      (the workspace `serde_json` lacks `float_roundtrip`), so the delivery
      digest refuses. Enabling the feature changes every crate's parsing and
      rebuilds everything; it is left for a decision.
    - `combined_reads` `deleted_source_recovery` (three tests): the first
      native read of a missing-storage member is `retryable: false`, the CLI
      read right after it `retryable: true` with fresh brokerage; order
      dependent setup state in `project_client::setup_retryable`. Not
      diagnosed further.
    - `tools` `airkctl_registration_matches_airk`: `fencing_generation` 0
      against 1, passed in one run and failed in two; worker-start timing,
      unrelated to grants.
    - `airk-inference` `context_8192_selector_requires_all_live_original_eight_option_row`
      fails on a missing catalog profile; this part does not touch
      `airk-inference` or the model catalog. Not checked on base.
    - **Not done in this part:** the `RetainedLedger` removal and credential
      file deletion with installer and agent-config cleanup (piece 5), and
      the user's record cleanups of 23 November 2026 items 1 (operation
      receipt pruning), 2 (finished split pruning), 3 (tombstoned index
      definitions) and 5 (`index-catalog-upgrade.json`, `rebuild.json`,
      clean-break journal). Item 4 (dead tables) is done above.
- **Part 4: the credential ledger and operation records (B, pieces 5 and the
  record cleanups; branch `loop/trust-b3` from `7dfc5fcc`).**
  - **Why:** the user's decisions of 23 November 2026 (local trust plan,
    decision 3): the credential ledger is removed on upgrade, predecessor
    cleanup recognises AIRK's agent-config entries by their known shapes, and
    the operation records are bounded. This build was never deployed beyond
    its author's machine, so no backward compatibility is owed for these
    application records. No table schema changes; only rows and dead files
    are deleted.
  - **Removed:**

| Item | Evidence |
|---|---|
| `airk-endpoint` `RetainedLedger`, `holds_no_credentials` and their tests; Core `MAX_STORE`, `MAX_CREDENTIALS`, `MAX_TOKEN`; `fixtures/split/agents/credentials_ledger_v1.json` (and its `Dockerfile`, `.dockerignore` and `container-inputs.json` lines) | Their only caller was installer predecessor cleanup, which now reads no ledger (Clippy `-D warnings`, `git grep`) |
| Core `HttpCleanupOwner`, `HttpCleanupOwnership`, `MAX_HTTP_CLEANUP_OWNERS`; the snapshot's `owners`; the lookup's `ownership`, `credential_file`, `credential_sha256`, `credential_digest_algorithm`; the `not_owned` lookup status and the `owned` current-namespace status | Ledger-derived; no producer once the ledger is not read |
| The `index-catalog-upgrade.json` refusal in Genesis admission | Nothing writes or reads the file; the owner receipt's `catalog_upgrade` fence is what admitted an in-place upgrade, and it still refuses on its own |

  - **Credential files deleted on upgrade:** the service's HTTP owner calls
    `retired_credentials::remove` on `http-1` before preparing it. It deletes
    `credentials.json` and the canonical `credential-<id>.token` files,
    exact names inside that folder only, inspected with `symlink_metadata`
    and never followed; a link, directory or look-alike name stays, and the
    folder goes only when it is then empty. Tests `retired_credentials::tests`:
    an old folder with a ledger, two tokens, a marker and look-alikes keeps
    only the non-credential entries; a folder of only credentials is removed;
    the same names one level up and in a sibling folder and a link's target
    stay byte-identical (the link case runs only where a link can be created).
  - **Predecessor cleanup by shape:** `http_cleanup::capture` records the
    predecessor's endpoint and CA digest and reads no ledger;
    `current_namespace_ownership` no longer reads the current installation's
    ledger. `lookup` answers `owned` for the connection id an AIRK-shaped
    entry names when the predecessor is identified and no newer installation
    can claim it, else `unavailable`. One guard replaces the ledger collision
    check: a newer installation that serves the predecessor's exact endpoint
    URL makes the lookup `unavailable`, since an entry there may be its own.
    The shapes are the `Bearer`, `headersHelper`, `http_headers_helper` and
    `bearer_token_env_var` forms `airkctl agents repair` recognises
    (`agents::config::recognize`; its `legacy_*` tests repair each form and
    leave foreign servers byte-identical). Installer tests:
    `clean_break_native_http_lookup_identifies_the_predecessor_without_a_ledger`
    (with and without an old ledger, which stays byte-identical),
    `clean_break_http_snapshot_survives_headless_old_control_and_payload_retirement`,
    `clean_break_native_http_lookup_refuses_missing_or_foreign_metadata`.
  - **Record cleanups** (registry and split `CONTRACT.md`):
    1. `index_operation_receipts` is pruned in the request-epoch pass
       (`catalog::record_pruning`, run inside `prune_job_receipts`, so also by
       `maintain_job_receipts`). From 896 of its 1,024 rows it keeps every
       unfinished receipt (the checkpoint's terminal classifier), every
       terminal receipt a live reader needs (a retained tombstone's
       `RetireIndex` fence, a source-change begin until its completion is
       applied, the current policy's fence, a retained split's fences and
       final receipts) and the newest 256 other terminal receipts; the
       request epoch rotates when any is deleted. N = 256: a retry comes from
       its own caller within seconds, and 256 leaves room under the cap.
       Readers checked: `runtime_contract::required_capabilities`,
       `retirement_evidence`, `source_change::discovery` and recovery,
       `checkpoint::receipts`, the policy parent read and the split readers;
       `adopted_authority.rs` was removed in part 3. Test:
       `more_than_1024_catalog_mutations_succeed_and_unfinished_receipts_survive`
       (1,101 mutations; two unfinished receipts survive; the newest key
       still answers; the first key expires and replays nothing).
    2. Finished splits (complete with the final acknowledgment, or canceled
       before the fence with cleanup done and any final plan acknowledged) are pruned from 48 of 64, keeping
       the newest 8, children first (`ON DELETE RESTRICT`), with their final
       receipts; the epoch rotates. Test:
       `more_than_64_splits_succeed_and_an_unfinished_split_survives` (70
       splits; an accepted split survives a pass that keeps none).
    3. A tombstoned `index_definitions` row is deleted once its `RetireIndex`
       fence is applied, no fence is pending, its registration is gone or
       unregistered with the cleanup guard cleared, and
       `service_removed_projects` holds its cleanup identity; its sources,
       aliases, description and retained registration go with it. Test:
       `acknowledged_retirement_cleanup_deletes_the_tombstone_and_frees_its_fence`.
    5. `index-catalog-upgrade.json` is deleted by the read-write open and
       Genesis admission when no owner fence names an upgrade (genesis test
       `competing_intents_unexplained_fences_and_missing_owned_database_refuse`).
       The finished `clean-break-cleanup-<sha>.json` journal is deleted after
       the rebuild journal durably records `cleanup_complete`, and on a later
       continuation tick if a crash left it (`retire_journal`; test
       `clean_break_finished_cleanup_journal_is_retired_and_others_stay`).
       `index-catalog-genesis.json` and the schema-11 backups are kept.
  - **Left, with reasons:**
    - **`rebuild.json` is not deleted.** After `Complete` and
      `cleanup_complete` it is still read: `default_project_data` takes the
      successor's default data root (`rebuild-default-<operation>`) from it,
      so deleting it would move where every new index stores its data. That
      is a behaviour change the decision did not cover. Deleting it needs that
      root recorded elsewhere first, or the user's choice.
    - **The extension's predecessor-proof validators**
      (`packages/vscode/src/serviceHttp.ts` `validatePredecessorHttpLookup`,
      `validateOwnedPredecessorHttpProof`) still expect the removed
      ledger-derived fields. They have no production caller
      (`captureCleanBreakClientCleanup` is called only by tests), and
      `packages/vscode` belongs to another agent in this round.
    - A tombstone that a source policy still names stays, with its fence: its
      policy job keeps the registration through its foreign key.
  - **Validated** (Windows, target `target-l2b`, incremental on, the
    integration environment of part 2; logs in `trust-b3/logs`):
    - Clippy `-D warnings --workspace --all-targets`, all features
      (`cl-ws-all`, 1 min 22 s) and default features (`cl-ws-default`,
      1 min 6 s): clean.
    - Nextest `--profile ci --all-features --lib --bins` of `airk-core`,
      `airk-endpoint`, `airk-store`, `airk-installer`, `airk-service`: 2,300 of
      2,300 passed, 40 skipped (`nt-final`, 7 min 8 s). The first run
      (`nt-b1`) failed only the new split test: a canceled split also runs
      a final plan, which `finished` now requires to be acknowledged.
    - `airk-cli` integration, all features: `catalog_upgrade`,
      `cli_commands`, `service_runtime`, `unregister_native`,
      `native_owner_recovery`, `service_only`: 96 of 96, 1 skipped (`nt-cli`,
      8 min 44 s).
    - `rustfmt --check` of the changed files, `scripts/check-repo.ps1`
      (1 min 23 s) and `git diff --check`: clean. `boundary_check.py` reports
      the same three failures as base `7dfc5fcc` (`airk-core` and
      `airk-retrieval` module cycles, `airk-mcp` `explorer_offline.rs`), none
      new.
  - **Not verified:** `airkctl` tests (its code is unchanged), the extension,
    an installed upgrade, Linux and macOS.
- **Part 5: phase B leftovers (branch `loop/trust-b4` from `7dfc5fcc`).**
  The failures part 3 left, and its TypeScript leftovers. Base: `7dfc5fcc`.
  - **Focus on one-index reads (eight tests).** Checked from source first:
    every production one-index read with Focus on is a selection read
    (`tools_call::selection_read`, W4 and A2) or an open read
    (`open_reads`), and both call `tools_call::singleton_focus::admit`
    before the worker, which captures the Focus-only singleton
    `PartitionAllowance`; the MCP local owner installs it with
    `with_context_partition_for` and the original deadline. So Focus still
    applies wherever it applied before grant removal; the unpartitioned
    branch is reached only by a combined member read, which never Focused.
    No code change. The six kernel `arbiter::focus::tests` that exercise an
    ordinary search now build that partition the way `capture` does
    (`ordinary_search_allocation`, selection identity, empty assessment
    ordinals; fixture `singleton`), as do `ties`, `off_and_lexical` and
    `reserves_actual_receipt_space`, which had passed vacuously on the
    Constrained notice. Each keeps its assertions. New
    `unpartitioned_ordinary_search_keeps_canonical_order_without_native_work`
    pins the combined-member rule. The two `airk-mcp` `focus_handler_tests`
    use `singleton_attempt`, built the same way.
  - **Pipe float defect: `float_roundtrip` enabled** in the workspace
    `serde_json`. A real product defect: every read crosses the service pipe
    as JSON text (worker to service, service to client), and without the
    feature `serde_json` parses some `f64` one ULP off. The reciprocal-rank
    score 1/65 written as `0.015384615384615385` came back as
    `0.015384615384615384`, so a client re-serialising the body (the
    `combined-delivery-result/1` digest that `airk-mcp` `remote.rs`
    rechecks for parse, status and diagnostics, and the integration tests)
    saw a different body, and every hop could move the value. Identities:
    the feature changes parsing only; serialisation (`ryu`, shortest
    round-trip) is unchanged. A stored digest over a Rust-written value is
    over `ryu` text, which correct parsing reproduces exactly, so no
    persisted digest, `airk-state/1` record or pinned fixture moves; the full
    workspace and `airk-cli` suites (below) pass with every pinned fixture
    unchanged. The evaluation identities are computed by the Python
    harness over file bytes and are not affected; no identity-feeding file
    changed. `Cargo.lock` is unchanged. Test:
    `combined_frontend::tests::delivery_digest_survives_the_text_round_trip_of_rank_scores`
    fails without the feature (`1/65 changed across the text round trip`,
    `logs/t1-prefix.log`) and passes with it; the integration test
    `permanent_trim_keeps_allowed_occurrences_of_a_shared_native_vector`
    passes.
  - **`deleted_source_recovery` (three tests): restart backoff.** Cause,
    from a diagnostic of the setup error chains: the first read of a
    missing-storage member starts its worker, which fails (`project
    startup failed or timed out`), refused `retryable: false`; the
    supervisor then holds a restart backoff, and every later read during it
    got `ConnectionSetupPending::RestartBackoff`, which entry 67 had made a
    retryable transient (`retryable: true`, `fresh_brokerage_required`).
    Decision from the contracts: a backoff exists only after the project's
    last start failed; the trust plan's transients are a worker busy,
    starting, draining or restarting and a catalog exchange pending, not a
    failed start (decision 4); `service-protocol.md` gives the retry hint
    only to recognised transient setup failures; and the missing-storage
    contract of `deleted_source_recovery` is `scoped_connection_unavailable`,
    `retryable: false`. So the final answer is right, and `setup_retryable`
    no longer treats `RestartBackoff` as retryable; `Queue`, `Draining` and
    `ResidentCapacity` still are. The supervisor's three-attempt restart
    budget and the pipe code `project_restart_backoff` are unchanged.
    Recorded in `project_client/CONTRACT.md`. Test:
    `a_restart_backoff_after_a_failed_start_is_final_like_the_failed_start`;
    the three integration tests pass.
  - **`airkctl_registration_matches_airk`: `fencing_generation`.** It is
    the project's worker admission count in the registry
    (`admit_worker`), not a documented transport or timing identity, so the
    comparison is unchanged. Cause: the old side is a recorded fixture
    (`fixtures/split/parity/airkctl_registration_matches_airk.json`,
    captured after the reviewed service had admitted the project's first
    worker, generation 1); the live `airkctl init` raced that asynchronous
    admission and answered 0. The test now waits, under a 60 s deadline,
    until the service lists the project at generation 1 or more, then runs
    both sides once.
  - **`admin_plane` stress race.** Confirmed by elimination: the two pages
    differed only in `revision` (2 against 3), with the same projects and
    `fencing_generation`, so the bump came from a registry writer that does
    not change the page's rows: the source watcher's roots and configuration
    observations in `airk-store` `service/registry/watch.rs` (worker
    admission would also have changed the generation). Not a parity bug. The
    test now reads the pipe page at the revision Admin answered
    (`Projects { revision: Some(r) }`); a moved revision refuses that cursor
    and both are read again under a 60 s deadline; the pages are then
    compared once, in full. The revision is not normalised.
  - **`context_8192_selector_requires_all_live_original_eight_option_row`:**
    fails with `Option::unwrap()` on `None` at `tests.rs:139` (no catalog
    profile for `ContextQwenLarge`, which LH dropped). Not caused by grant
    removal: this part and part 3 do not touch `airk-inference` or the model
    catalog. **Pre-existing:** it fails the same way on `feat/arbiter-final`
    at `1a657010` (checked out in this worktree, same target; `nextest -p
    airk-inference --lib --all-features --profile ci`, same panic at
    `tests.rs:139:66`; `logs/base-inference-1.log`). Not fixed here.
  - **The two `http_gateway` tests:** `catalog_close_stops_actual_partial_socket_writes_and_buffered_flush` and
    `tls_read_driven_alert_cannot_flush_ciphertext_after_native_fence` pass in
    the workspace run below (`logs/nt-ws-1.log`); no change.
  - **TypeScript leftovers:**
    - VS Code journals: a saved `reference_refresh` intent made
      `IntentJournal.list()` throw (`invalid_mutation_intent`). It is now
      skipped on load and discarded by `ready()` (`retiredIntentRecord`).
      Test: `a journal saved before reference refresh was removed loads,
      skips and discards its refresh intents` (fails before the fix with
      `invalid_mutation_intent`).
    - Fence validation (`serviceClient.validateFenceTarget`) no longer reads
      `grant_id` or `grant_revision`; it validates the native `ScopedReadRef`
      (`selection` with its owner, nonce, service instance, scope identity
      and catalog revision) and refuses a grant-shaped target. Test: the
      catalog fence wrapper test uses the native shape and refuses the old
      one.
    - The `imported_legacy` and `imported_legacy_union` HTTP selections, the
      `imported_snapshot` legacy-binding eligibility and its grant
      provenance validators had no producer (native
      `NativeLegacyBindingEligibility` is `authority_zero` or `unavailable`;
      `HttpSelection::ImportedLegacy*` went in part 2). Removed:
      `ImportedHttpSelection`, `matchesImportedHttpOrigin`,
      `standaloneHttpServerKey`, `preservedImportedHttpServerKey`,
      `validatePersistedGrant`, `validateStandaloneAdoption`, the
      standalone reattachment branch of `refreshHttpAgentConfig` (its
      `previous` parameter had no caller) and the preserved-alias arm of the
      VS Code manual-configuration checks. Tests rewritten onto the open
      kind; both removed kinds are refused.
    - `packages/ui/generated/help-manifest.json` was stale after part 3 (it
      still listed `--catalog-session`); regenerated by `build-explorer.cjs
      --write-help-manifest`.
    - `contract_types.py i2|i3|arbiter --check`: no drift; no contract
      changed, so nothing was regenerated.
  - **Validated** (Windows, target `target-l2b-cli`, CPU integration
    environment of part 2, incremental):
    - Clippy `-D warnings`, `--workspace --all-targets`: `--all-features`
      clean in 2 min 3 s (`logs/cl-all-1.log`), default features clean in
      1 min 25 s (`logs/cl-default-1.log`).
    - Nextest `--workspace --lib --bins --all-features --profile ci`: 4,728
      run, 4,727 passed, 202 skipped, 1 failed, the `airk-inference` selector
      test above; 19 min 5 s (`logs/nt-ws-1.log`).
    - `airk-cli` `--tests --all-features --profile ci`: 476 of 476 passed, 6
      skipped; 20 min 14 s (`logs/nt-cli-full-1.log`).
    - Stress `-p airk-cli --stress-count 20 -E '(binary(combined_reads) &
      test(/answer/)) | binary(admin_plane)'`: 20 of 20 iterations, 17 tests
      each; 5 min 34 s (`logs/stress-1.log`).
    - `packages/vscode` `npm test`: 1,023 of 1,025, 2 skipped
      (`logs/npm-vscode-5.log`); `packages/admin` `npm test`: 303 of 305, 2
      skipped (`logs/npm-admin-1.log`); shared UI `node --test
      ../ui/test/*.test.mjs`: 215 of 216, the one failure being the
      shared-editor Service test part 3 already reported (`logs/ui-1.log`).
    - `contract_types.py i2|i3|arbiter --check`, `check-repo.ps1`,
      `git diff --check`: clean.
  - **Predecessor lookup in VS Code (coordinator addition).** Phase B part 4
    (`loop/trust-b3`, `b0482b9b`) cut the predecessor lookup to `owned` or
    `unavailable` with no ledger fields. The extension's validators of that
    output (`validatePredecessorHttpSnapshot`, `validatePredecessorHttpLookup`,
    `validateOwnedPredecessorHttpProof`, `predecessorHttpProof`) and the
    clean-break client cleanup that consumed them (`captureCleanBreakClientCleanup`,
    `applyCleanBreakClientCleanup`, `validateCleanBreakClientPlan` and their
    helpers in `agentMigration.ts`) had no production caller: `git grep`
    finds only their definitions, no test imported them, and the fixture
    `test/fixtures/predecessorHttpFixture.ts` was imported by nothing. They
    are removed rather than updated. The installer's native predecessor
    cleanup (`airk-installer` `http_cleanup.rs`) owns that duty. Not touched:
    `agentConfigHealth.ts` `HttpOwnership` (`owned`, `not_owned`,
    `unavailable`), an injected health-check interface with no production
    implementation, which is not the lookup output.

### 69. Answer jobs owned by the service (L2c)

Numbered 47 on `loop/l2b-wire` until 24 November 2026; renumbered 69 when
the branch was brought onto entries 49 to 68 as `loop/l2c`. Its commits
before that move, and the ledger entries of 23 November, cite it as entry
47.

Plan section 12.4.2, step L2c, built on `loop/l2b-wire` after the review of
`24184339` (H1, H2, M1 to M7). Not in the LG gate image. Recorded contract
decisions: service `answer_jobs/CONTRACT.md` ("Service-owned jobs"), kernel
arbiter `CONTRACT.md` ("Answer job kernel"), supervisor
`project_worker/CONTRACT.md` ("Answer job authority"), Core arbiter
`CONTRACT.md` ("Answer job options and delivery record").

- **What changed:**
  - a job has its own read kernel (`Kernel::answer_job_kernel`: the maker's
    frozen authority with its own revocation flag), registered by the owner
    on the grant's installed lease (`Scopes::attach_job`), and its own
    native attempt and read context minted at dispatch
    (`AnswerJobAuthority::attempt`);
  - the caller's wait (`wait_ms`, the transport attempt) is apart from the
    work budget (`deadline_ms`); reuse by digest needs the same deadline;
  - `fresh`, `invocation_key` and `interactive` beside the request
    (`AnswerJobOptions`), and a delivery record beside every job reply
    (`AnswerDeliveryRecord`); `elapsed_ms` is the caller's own wait;
  - failures belong to their invocation; attended jobs dispatch first and
    unattended ones expire at their due time; a supervised deadline with a
    5 s grace quarantines the owner and holds the slot until the work
    settles.
- **Review findings:**
  - **H1** (session teardown revoked the job): fixed by the job kernel.
    Reproduced first on the old code (`answer_job_tests`, 1 failed), passing
    after the fix; decisive test through the real service below.
  - **H2** (kept replies stand in for fresh runs): `fresh` always runs; the
    delivery record says `reused`; the harness sends `fresh`.
  - **M1, M4** (a run cut for another caller's limits, an invalid reused
    reply): reuse only of work asked with the same deadline; each delivery
    is validated against the caller's own budget.
  - **M2** (no reap boundary): supervised deadline and settling capacity.
  - **M3** (`include_status` headroom): folded into the scope.
  - **M5** (transient failures pinned): failures never reach a call by
    digest.
  - **M6** (missing tests): added, below.
  - **M7** (transport parity): the parity tests still compare one kept
    reply through each transport; the delivery record and `elapsed_ms` are
    per delivery. Not changed here; for the parity owner.
- **Residual (recorded):** the lease in force at submission bounds a job
  (at most 300 s); nothing renews it for a job whose maker has gone, and a
  renewal fences it. See the service contract.
- **Test support:** a `test-support`-only job gate
  (`AIRK_TEST_ANSWER_JOB_GATE`), never in product builds.
- **Validation** (target `answer-loop/target-l2b`, `--locked --offline`,
  commands and logs in the L2b notes, `commands.tsv`):
  - Clippy `-D warnings`, all targets, airk-core, -kernel, -mcp,
    -supervisor, -service, -https, -frontend and -cli: exit 0 with default
    features (`l2c-clippy-8`, then -service `l2c-clippy-9` and -cli
    `l2c-clippy-cli-default` after later edits) and with all features
    (`l2c-clippy-all-1`, -cli `l2c-clippy-cli-all`);
  - focused nextest, default features: 280 run, 278 passed
    (`l2c-nextest-2`); the two service failures were tests asserting the old
    attach rules, updated, then the job-table set 48 of 48
    (`l2c-nextest-4`);
  - broad nextest, all features (airk-mcp, -supervisor, -frontend, -https
    whole; kernel, core and service Answer and arbiter sets): 1,267 run,
    1,264 passed (`l2c-nextest-all-1`). Two failures were the advertised
    Answer schema and Help captures, regenerated through their owners
    (`l2c-regen-*`) and passing (`l2c-rerun-1`); the third is the known base
    failure of entry 40;
  - airk-cli `combined_reads` and `admin_plane`, all features: 78 run, 75
    passed (`l2c-cli-all-1`); after the parity normalizer learned the
    delivery record, one of the three passes (`l2c-rerun-1`). **Two remain
    failing:** `current_answer_single_index_preserves_complete_pipe_cli_and_https_mcp_results`
    and `admin_sign_in_reads_and_cross_plane_refusals`. They compare one kept
    reply through two transports; it is now rendered per delivery, so
    `elapsed_ms`, the delivery record and the verification receipt's
    `receipt_bytes_budget` (frame minus reply bytes) differ by a few bytes.
    The normalizers belong to the parity owner (review M7); not changed
    here;
  - the decisive test and the injection test: 2 of 2
    (`l2c-cli-decisive-1`); the mutation restoring the shared flag fails it
    (`l2c-cli-mutation-shared-flag`, exit 100);
  - `scripts/check-repo.ps1` exit 0; `git diff --check` clean.
- **Independent review after the rebase onto `705ab312` (23 November
  2026).** The branch was rebased onto entries 49 to 55; only this entry and
  the parity normalizer's comment conflicted. Three defects were closed, each
  with a test or build that failed on the old code:
  - the ticket reply lacked entry 54's optional `read_status` and did not
    compile on the new base (a ticket ran no read, so it carries none);
  - an invocation key recovered its job without the publication check the
    digest and continuation paths make, so after a new publication a keyed
    call got the old publication's result
    (`an_invocation_key_never_recovers_a_result_of_an_older_publication`);
  - an owner counted a job only while it ran, so an owner whose only work was
    a queued job left as idle after 15 s and the job was dropped unrun,
    against "What a ticket means" (plan 12.4.2)
    (`a_queued_job_keeps_its_owner_from_leaving_as_idle_until_its_work_ends`).
  - On this base the two airk-cli parity tests that failed above no longer
    fail on a comparison difference (entry 54 compares record ids and digests
    as sent): `combined_reads` Answer tests and `admin_plane`, 19 of 19 in one
    batch. In another batch `admin_sign_in_reads_and_cross_plane_refusals`
    failed once on a typed `publication_busy` Answer refusal (no replay
    allowed); it passed alone and in the repeat, and the same batch on
    `705ab312` passed once. Not attributed; open. (Diagnosed below.)
- **The coordinator's four follow-up items (23 November 2026).**
  - **Item 1, the intermittent `publication_busy`: a fault of main, not of
    the job path.** Stress runs of the same group (`combined_reads` Answer
    tests and `admin_plane`): `admin_sign_in_reads_and_cross_plane_refusals`
    failed 7 of 20 on the branch and 7 of 20 on `705ab312`; 5 of 15 with the
    `admin_plane` binary alone. Every failure is on its readiness
    `airk_diagnostics` poll after the Index job, before the test's first
    Answer. A temporary render-site capture (never committed) showed each as
    `CurrentReadContention` from SQLite extended code 261
    (`SQLITE_BUSY_RECOVERY`, "database is locked"), never the gate's
    "applying an update": a read-only current-database open, which has a
    zero busy timeout (`airk-store` `current/database.rs`, `open_reader_budget`)
    and no open admission on query threads (`current/admission.rs`: only
    service work installs `with_work_admission`), met another connection
    rebuilding the WAL index. The test treats the typed `repeat_same_request`
    as fatal. The branch's new decisive test
    (`answer_jobs_outlive_their_makers_session_and_are_recovered_by_their_handle`)
    failed 4 of 20, then 0 of 10 and 1 of 10, on the same code: with origin
    backtraces, the whole-Answer refusal comes from
    `Kernel::arbiter_publication` called by `focus_pair_tokens` in
    `Loop::rounds` (kernel `arbiter/answer.rs`), a current-database open the
    loop does not repeat, while its canonical reads absorb the same 261
    (about 70 captured, all recovered by `BUSY_REPEATS`). The job's blocking
    thread and the direct handler's thread have the same (no) open admission;
    the test drives two Answers at once in one service, which the direct
    path also allows. Owners: the current reader's open admission (store) or
    the loop's non-canonical publication reads (kernel arbiter); neither is
    changed here, and no test limit or retry was added.
  - **Item 2:** a reply packed to `reply_limit - reserve`, with the largest
    valid delivery record and status, renders within 131,072 bytes with and
    without verification, and the largest rendered Answer result fits the
    147,456-byte worker frame; the reserve is unchanged (`0d52546c`).
  - **Item 3:** `answer_job_stuck` and `answer_authority_ended` say whether
    the job's reads ran (`query_submitted`), recorded in the service
    `answer_jobs` CONTRACT (`db314433`).
  - **Item 4:** the combined frontend bounds a combined Answer's job options
    and refuses `invocation_key`, `wait_ms` and `interactive` with the typed
    `invalid_native_arguments` (`02f46ebc`).
- **Rebase onto LH and entries 65 to 68 (24 November 2026, branch
  `loop/l2c` from `e2ca0e47`).** The 14 commits of `loop/l2b-wire`
  (`a0d04ad7` to `1bf80aed`) were cherry-picked in order; none was dropped.
  Two textual conflicts, and four places where the text merged but the code
  assumed what LH removed:
  - `airk-mcp` `response.rs`: entry 67's `FocusPostExecutionStale` and
    `PublicationAdvanced` renders and L2b's `AnswerQueueBusy` render all
    kept. A job failure keeps its typed receipt through
    `PreservedReadFailure`, so a job that meets a newer publication after
    model work is the typed `publication_advanced` of entry 67, kept for its
    invocation only and never reused by digest (M5 above).
  - `answer_jobs/estimate.rs` (LH wins): LH left one rung, the Arbiter.
    `CostTable::only` no longer names a Large rung. With one rung the ladder
    never climbs, so `configured_rung_only` changes no plan today; it is kept
    (service `answer_jobs/CONTRACT.md`, "One rung per job").
  - `answer_jobs/wiring.rs` (LH wins): the kernel's Context model maps
    `SelectedModel::Arbiter` to the one rung; `ContextLarge` and
    `ContextCompact` are gone.
  - Tests (LH wins): the kernel admission tests disclose
    `"requested":"arbiter"`. The wiring rung test now shows an Arbiter job
    priced past its deadline dropping to the no-model loop
    (`a_job_is_fitted_on_its_own_rung_and_only_drops_to_the_no_model_loop`;
    the Large-to-Compact case it also showed left with Large). The
    unmeasured-costs test prices the Arbiter as Large was (8 s, 4 s, 8 s), as
    LH did in `answer_jobs/tests.rs`, so its 408 s arithmetic stands.
  - `scoped_reads.rs`: the job kernel's copy of the session scope gets
    entry 67's empty `metadata_observed` test seam. Commit `01bd1020` alone
    does not build its kernel test target; the follow-up commit fixes it.
  - Entry 68 removed no type L2c uses; the job authority stays on the
    installed grant lease. The grant removal (local trust plan B5/B6) will
    meet `Scopes::attach_job`, `detach_job` and `SessionJobAuthority` in
    `airk-supervisor` `project_worker/scopes.rs`; a second rebase is expected.
  - The L2b wiring notes above that name Large (the 408 s placeholder, a
    Large job planned on Compact) are history from before LH.
- **Validation of the rebase** (target `answer-loop/target-l2c`, CPU
  environment of `L3/build-env.json`, incremental on, `--locked --offline
  --timings`, logs in the private `answer-loop/L2c/logs`):
  - workspace Clippy `-D warnings --all-targets`, all features: exit 0
    (44 s, warm after a first run that found the missing seam); default
    features: exit 0 (79 s);
  - Nextest `--profile ci --all-features --lib --bins` over airk-core,
    -kernel, -mcp, -supervisor, -service, -https, -frontend and -cli: 2,174
    of 2,174 (734 s with the build);
  - L2c's own tests by name (kernel `answer::admission::`, MCP and service
    `answer_jobs::`, supervisor `answer_job_tests::` and `response_tests::`,
    frontend `combined_frontend::answer`, and the three `combined_reads` job
    tests): 76 of 76;
  - `python scripts/dev/contract_types.py --check arbiter`: exit 0;
  - the stress group (`combined_reads` Answer tests and `admin_plane`), 20
    iterations: **19 of 20**. The one failure is
    `admin_sign_in_reads_and_cross_plane_refusals` at `admin_plane.rs:445`:
    the Admin `indexes.list` page and the pipe's `Projects` page differ only
    in the registry revision (2 and 3). **A fault of main, not of L2c:** the
    same test alone on unchanged `e2ca0e47`, 40 iterations, failed once in
    the same way (39 of 40). By reading, not confirmed: the registry revision
    is bumped by the source watcher recording or invalidating its roots
    (`airk-store` `service/registry/watch.rs`), which can land between the
    two reads. No assertion was changed. Open for the primary agent.

### 70. Phase B and L2c integrated: Answer jobs on selection authority; two fixes

- **Where (24 November 2026):** branch `loop/trust-int` from `loop/trust-b4`
  (`b3be2f4c`), with merge commits of `loop/trust-b3` (`b0482b9b`, as
  `caf98910`), `feat/arbiter-final` (`1a657010`, as `46cd7b59`) and
  `loop/l2c` (`1ec685ed`, as `38166936`). Change-log conflicts: entry 68
  keeps parts 1 to 5 in order (part 4 before part 5), and entry 69 follows.
- **L2c on selection authority.** Entry 69 left a job's authority on the
  reviewed-grant lease, which entry 68 part 3 removed. A job now holds the
  read authority its caller redeemed: a W4 window or A2 Admin selection
  (`SelectionReadAuthority`) or a U105 open read (`OpenReadAuthority`), both
  installed in the worker's selection ledger. `Scopes::attach_job` registers
  the job kernel (`Kernel::answer_job_kernel`, the maker's frozen scope with
  its own revocation flag, never wider) on that ledger entry beside its
  sessions; `detach_job` and `SessionJobAuthority` take the same slot. No
  design decision was needed: the selection ledger has the same install,
  renewal and revocation shape the grant ledger had.
  - **Guarantees kept.** A queued job keeps its owner from leaving as idle
    (unchanged `answer_jobs_in_flight`). The job outlives its maker's query
    session (`SessionFence::drop` revokes only the session kernel). Its
    authority ends with the selection: the host's `RevokeScope`, sent when
    the window, Admin or agent selection that owns it ends, revokes it, as do
    lease expiry, lease renewal and the owner's close; the job's refusal is
    then `answer_authority_ended` with `query_submitted` saying whether its
    reads ran. A newer catalog revision is a new selection with its own
    nonce; there is no grant revision to fence any more.
  - **Entry 67's fences are unchanged:** the job reads through the same
    scoped kernel paths (publication moved under the same authority is read
    again or `publication_busy`; after model work `publication_advanced`).
  - **Tests moved from grants to selections:** supervisor `answer_job_tests`
    now runs each case for a selection and for an open read, and adds
    `a_revoked_selection_ends_only_its_own_jobs`; the MCP `answer_jobs` and
    service `wiring` fixtures use a W4 selection (`other_grant` became
    `other_selection`, another nonce); the three `combined_reads` job tests
    read one index, or both, through `window_for`. Contracts updated:
    supervisor `project_worker/CONTRACT.md` ("Answer job authority"),
    service `answer_jobs/CONTRACT.md`, kernel arbiter `CONTRACT.md`.
  - **Shared Help manifest** regenerated (`f649fe40`): part 5 regenerated it
    before L2c's `fresh`, `interactive`, `invocation_key` and `wait_ms`
    arrived, so the merged VS Code build refused it as stale.
- **Fix: `context_8192_selector_requires_all_live_original_eight_option_row`**
  (`6737649a`). It unwrapped a catalog profile for `ContextQwenLarge`, which
  LH removed, and panicked at `tests.rs:139` on every tree since LH (entry
  68 part 5). The selector checks now run for the catalog's one Context
  profile, the Arbiter, with every assertion unchanged, and the test asserts
  that no Large profile is left. Chosen over deleting the test because the
  8192-token eight-option rule still applies to the Arbiter. Before:
  `logs/nt-fix1-before.log` (fails); after: `nt-fix1-after.log`.
- **Fix: a typed refusal for an unresolvable configuration/3** (`043e4c04`).
  After a legacy Configure saved a /3 naming a profile the release catalog
  cannot resolve, `airkctl arbiter status` refused with the catch-all
  `arbiter_unavailable`. The refusal and the saved bytes are kept; Core now
  returns `LegacyProfileUnresolved` (its message points to `airkctl arbiter
  status-legacy`) and `failure()` renders `arbiter_configuration_migration_refused`,
  the code Core's `initialization_unresolved_current3` case already names, so
  no wire code is new. Test
  `failure_tests::status_device_of_an_unresolvable_configuration3_is_the_typed_migration_refusal`
  failed before with `arbiter_unavailable` (`nt-fix2-before.log`). Recorded
  in the service arbiter `CONTRACT.md`.
- **Validated** (Windows, target `target-l2b-cli`, the CPU environment of
  `trust-b4/build-env.json`, incremental on, `--locked --offline`; logs in
  the private `answer-loop/trust-int/logs`):
  - Clippy `-D warnings --workspace --all-targets`: all features clean in
    1 min 35 s (`cl-all-final`), default features clean in 1 min 5 s
    (`cl-default-final`).
  - Nextest `--workspace --lib --bins --all-features --profile ci`: 4,772 of
    4,772 passed, 202 skipped, 15 min 1 s (`nt-ws-final`). The selector test
    above now passes; no failure remains.
  - `airk-cli --tests --all-features --profile ci`: 479 of 479, 6 skipped,
    20 min 8 s (`nt-cli-final`).
  - Stress `-p airk-cli --stress-count 20 -E '(binary(combined_reads) &
    test(/answer/)) | binary(admin_plane)'`: 20 of 20 iterations, 20 tests
    each, 11 min 53 s (`stress-final`).
  - L2c's named tests: kernel `answer::admission::`, MCP and service
    `answer_jobs::`, supervisor `answer_job_tests::` and `response_tests::`,
    frontend `combined_frontend::answer`: 74 of 74 (`nt-l2c-lib-1`); the
    three `combined_reads` job tests: 3 of 3 (`nt-l2c-cli-1`).
  - `packages/vscode` `npm test` 1,023 of 1,025, 2 skipped
    (`npm-vscode-final-2`); `packages/admin` 303 of 305, 2 skipped; shared UI
    215 of 216, the one failure the shared-editor Service test part 3 and
    part 5 already reported (`ui-final`); `contract_types.py i2|i3|arbiter
    --check` clean; harness `unittest discover -s scripts/arbiter-answer-eval`
    252 OK; `check-repo.ps1` and `git diff --check` clean.
- **Not verified:** Linux and macOS, an installed service, and revoking a
  selection end to end while a job's maker is gone (covered at the
  supervisor level only, as entry 69 recorded for grants).
- **Identities:** none moved; no wire shape, digest input or persisted format
  changed.

### 71. Local trust plan C: loopback over plain HTTP

- **Where (24 November 2026):** branch `loop/trust-c` from
  `feat/arbiter-final` (`0d8aef9e`), in the `trust-b4` worktree. Decided by
  the user in the [local trust plan](../simplify/local-trust-plan.md)
  (decisions 1, 2 and 5, section C) and the phase C brief; map
  [local-trust-phase-c-map.md](../simplify/local-trust-phase-c-map.md).
  Commits:
  1. `19b15522` the remove-only legacy trust retirer
     (`http_endpoint::legacy_trust`, `cert_trust::retire_exact`) and the
     tolerant legacy readers (`LegacyEndpoint`, `SavedEndpoint`,
     `LoopbackMcpUrl::legacy_https_port`);
  2. `b4c2ccbc` the consumers removed: `airkctl service trust` and
     `issuer-rotation`, `runtime http-trust` and `http-issuer-rotation`,
     M18, the installer rotation journal and its guards, the
     `https.trust_status`/`https.rotation_status` Admin rows, setup's
     `ApplyTrust`/`RemoveTrust`, `TrustChoice`, `SetupOptions.trust` and the
     `trust_*` phases (one `RetireLegacyTrust` step, phase
     `legacy_trust_retired`), Inno `/TRUST` and the shell wrappers' trust
     option; I1/I3/I4 fixtures, CLI reference, help and pilots regenerated;
  3. `1683c9b2` the switch: gateway on plain TCP with
     `FlushTrackedIo<FencedIo<TcpStream>>`, `tls.rs` gone, descriptor without
     `ca_sha256`/`certificate_sha256`, `http://` URL and origins, cookie
     `airk_admin` (`HttpOnly; SameSite=Strict; Path=/`), `rcgen` and
     `x509-parser` out of the workspace, `rustls`/`tokio-rustls` only in
     `airk-service`; the four rustls-buffer fence tests rewritten over a
     counting duplex writer, the TLS-only test deleted;
  4. `e5c4b58d` clients: `airkctl` recognition (an `https://` entry is a
     legacy `https_endpoint_mismatch`, repaired to `http://`;
     `https_trust_missing` removed), VS Code, the Admin UI and its rebuilt
     bundle, the I2 TypeScript;
  5. `90f03d40` scripts, examples (`local_http_client.*`), installer
     READMEs, `docs/local-http-api.md` (replacing `local-https-api.md`) and
     the other docs, and the owning CONTRACTs;
  6. `af21ea1d` the persisted-record fix below and the end-to-end upgrade
     test.
- **Migration as built.**
  - **Endpoint record.** `http_endpoint::load_saved` decodes the current
    descriptor, else an HTTPS build's record as `SavedEndpoint::Legacy`
    (port, generation, binding, executable, and the framed digest of its
    exact bytes); `load_descriptor` never returns a legacy record as current.
    The service binds the legacy port, publishes generation + 1 over
    `http://` and chains `predecessor_digest` to the legacy digest;
    `publish_descriptor` accepts a legacy predecessor only with a higher
    generation.
  - **OS trust.** Setup's `retire_legacy_trust` step (before the start for
    placing kinds, where trust removal ran for removals, in the user's
    session) calls `legacy_trust::retire`. Only a receipt
    `http-1/user-trust-1.json` in phase `installed` whose binding names the
    current user acts: the exact certificate is removed by DER SHA-256 from
    the store the receipt names (Windows CurrentUser\Root through PowerShell,
    which selects by `ComputeHash(RawData)` and proves absence afterwards;
    macOS login keychain through `security find-certificate -a -Z` and
    `delete-certificate -t -Z`). Another store is refused. `prepared`,
    `remove_prepared`, another user's or an unreadable receipt is residue:
    it and `ca.pem` are kept and reported as a setup leftover. Linux has no
    store: files only. Then `ca.pem`, `tls-identity.json`,
    `retired-private-key.json`, `user-trust-1.json`, `user-trust-1.lock`,
    `issuer-rotation.json`, `issuer-rotation.lock` and
    `rotation-user-trust-preimage.json` are deleted by exact name, never
    following links (a pending rotation journal is discarded; the
    `issuer-rotations/` history is reported, not deleted). A failed removal
    is recorded `left` and keeps the CA and receipt; it never fails setup.
    Predecessor retirement and uninstall of an old runtime layout use the
    same retirer.
  - **Agent configs.** `airkctl` recognises `https://127.0.0.1:<port>/mcp`
    entries as AIRK's, always `https_endpoint_mismatch` after stale
    credential fields; `repair` writes the live `http://` URL and strips
    `headersHelper` and bearer fields. VS Code's recognizer and health check
    follow (`legacyHttpsUrl`).
- **Gap found and closed: the setup records of an HTTPS build.** Dropping
  `SetupOptions.trust` outright broke every service start on this machine:
  `setup_role::start_gate` reads the settled
  `%LOCALAPPDATA%\AIRK\install-pointer\active-setup.json`, which embeds the
  user's HTTPS-build operation with `"trust":"ask"` and binds those bytes by
  digest (`unknown field trust`, then `setup_owner_refused`; every
  `airk-cli` reviewed fixture failed). The same file blocks the user's own
  upgrade. `SetupOptions.trust` is now `Option<LegacyTrustChoice>`: decoded
  and re-encoded only when present, so old records verify byte for byte,
  never written, never read for a decision; the setup role refuses to run an
  operation naming it. This keeps a read-only legacy field the brief said to
  drop; it is the narrowest form the required automatic migration allows and
  is flagged for the user. Recorded in `install_record/CONTRACT.md`.
- **Tests.** Legacy retirer: four `legacy_trust::tests` (installed removes
  by digest then the files; interrupted receipts are residue; another user,
  a failed removal and another store keep the trust files; no store or no
  folder), `cert_trust::tests::exact_retirement_selects_by_digest_and_proves_absence`,
  `service_http::tests::an_old_https_endpoint_record_decodes_as_legacy`,
  `http_endpoint::tests::a_legacy_https_record_is_carried_forward_by_the_next_http_generation`,
  `install_record::tests::an_https_builds_trust_option_is_read_from_persisted_records_with_exact_bytes`.
  Fence tests (`http_gateway::tests`):
  `request_deadline_and_cancellation_fence_actual_bytes_until_close`,
  `real_http_scope_refusal_preserves_fresh_403_and_fences_old_unsent_evidence`,
  `native_policy_fences_unsent_bytes_at_actual_write_poll` and
  `a_live_http_connection_keeps_an_answer_open_past_120_seconds_to_its_result`
  assert that no byte reaches the socket after a fence, a deadline, a
  cancellation or a policy change (with a positive control where the old
  test had one); `tls_read_driven_alert_cannot_flush_ciphertext_after_native_fence`
  and the `read_driven` branches are deleted as TLS-only. End to end:
  `airk-cli` `trust_upgrade` (an HTTPS install shape on a real reviewed
  service: CA removed by digest through an injected runner, TLS files gone,
  the same port serving `http://` as the next generation, an `https://`
  agent entry repaired, a `prepared` receipt reported and not acted on) and
  the live `airkctl` agents test's new legacy round trip.
- **Validated** (Windows, target `target-l2b-cli`, the CPU environment of
  `trust-b4/build-env.json`, incremental on, `--locked`, offline; logs in the
  private `answer-loop/trust-c/logs`):
  - Clippy `-D warnings --workspace --all-targets`: all features clean
    (`final-clippy-all`, and 45 s after the last source change in
    `s6c-clippy`), default features clean in 1 min 9 s
    (`final-clippy-default`).
  - `cargo tree -i`: `rcgen` and `x509-parser` match no package; `rustls`
    and `tokio-rustls` are reached only through `airk-service`.
  - Nextest `--workspace --lib --bins --all-features --profile ci`: 4,743 of
    4,743 passed, 202 skipped, 15 min 54 s (`final-ws-lib`).
  - `airk-cli --tests --all-features --profile ci`: 487 of 487, 6 skipped,
    13 min 31 s (`final-cli`), including `trust_upgrade`, `admin_plane`,
    the gateway-backed `combined_reads` and the live `tools` agents test.
  - Stress `--stress-count 20 -E '(binary(combined_reads) & test(/answer/))
    | binary(admin_plane)'`: 20 of 20, 11 min 53 s (`final-stress`).
  - `packages/vscode` `npm test` 1,024 of 1,026, 2 skipped; `packages/admin`
    302 of 304, 2 skipped, `npm run check` up to date; shared UI 215 of 216,
    the one failure the shared-editor Service test entry 70 already
    reported; `contract_types.py i2|i3|arbiter --check` clean; harness
    `unittest discover -s scripts/arbiter-answer-eval` 252 OK; touched
    Python tests 249 run with two failures and one import error that the base
    run already had (`test_runtime_upgrade` preflight,
    `test_package_standalone_linux` CUDA layout; `test_local_test_setup`
    passes 33 of 33 under `discover`); the whole `scripts/tests` suite has no
    new failure against the base (2,354 then 2,346 run, failures 10 and 10,
    errors 29 then 27); `check-repo.ps1` and `git diff --check` clean.
- **Not verified:** macOS keychain removal (`security` output parsing,
  whether it prompts under `--silent`) and Safari's cookie handling, Linux,
  an installed service upgrade on this machine (the user's installation was
  not touched; the end-to-end test uses a reviewed fixture and an injected
  trust runner), and Edge or Chrome opening Admin without a certificate step.
- **Left:** the `fixtures/cli-golden/windows/airk.json` capture of the old
  `airk` binary still lists `runtime http-trust` and `http-issuer-rotation`
  (a dated capture that needs a rebuild to recapture); the release-manifest
  field `certificate_trust: "skip"` (`release.ts`, publication receipts) is
  unchanged release wire shape; `scripts/dev/split_u90b_cert_trust.py` is a
  completed one-off migration kept because CONTRACTs cite it; several
  benchmark/harness receipt tokens still say `https`; README's "Service
  lifecycle" paragraph still names the `airkctl agents credential` helper
  that plan B removed.
- **Identities:** the endpoint descriptor, `LoopbackMcpUrl`, the Admin
  cookie and origin, `SetupOptions`/`SetupPhase` and the I1/I2/I3/I4
  fixtures changed under the plan's recorded decision; `SetupOptions.trust`
  stays as a read-only legacy field (see the gap above).


### 72. Gather Context development comparison

Record the Decision 6 comparison on the pinned Windows publications. Start
with Focus and Arbiter disabled and prove that state from the live runtime.
Use the same three rounds, 132 calls, 132 candidates, 262,144 retained bytes
and 300,000 ms deadline as the recorded control. Keep the canonical reply
ceiling and the existing per-read bounds. The control retains the existing
reader roster and one follow-up per open need per later round. The reader
comparison introduces the scoped retained-text literal scan and the approved
existing facts and relationship readers through their owning paths. Compare
one versus two follow-ups and fixed versus need-based excerpt sizes in
separate runs. Count census work against round one and the same total
budgets. Reports distinguish scanned bytes, retained bytes and delivered bytes.

Recognize an explicit relative path and line or inclusive line range in the
task as a known address. Use the canonical first-round Read File for that
sub-need, under the existing per-read byte and span bounds. The broad read
for the parent part remains required. Plain paths, identifiers and
unanchored prose still use their existing starting reads. The request-local
address does not change the public loop types, on-disk data or source
authority. Scope and range refusals remain canonical.

The Windows complete-evidence failure remains recorded. The offline and
fresh diagnostic comparison measures retrieval and delivery separately from
the paused judged baseline.

### 73. Caller-stated needs and anchors on the version 3 request

The user decided on 26 November 2026 that the calling agent states the needs.
A probe of the published Qwen3-Reranker-4B outside AIRK had shown that it
finds the required passages when the query states the need and not from the
task alone.

**Contract.** A version 3 request may carry `needs`, `anchors`,
`deliverable` and `known`. Each is omitted when absent or empty, so a request
without them keeps its bytes and digest. The single part of a request with
caller needs holds up to `MAX_NEEDS` (8) sub-needs. Kernel-derived sub-needs
keep `MAX_SUB_NEEDS` (4), with `MAX_CARD_SEGMENTS` and `MAX_DECISIONS`
unchanged. A first draft renamed that limit inside the two card policy
sources, which changed every profile's task policy identity and made the
fixture refuse its installed models; the sources were restored byte for byte. A reply sub-need gains `origin`, present only when
the caller stated it. Round one may hold one exact lookup per anchor, and the
reply check allows 16.
[Core contract](../../crates/airk-core/src/arbiter/CONTRACT.md#optional-caller-inputs-user-decision-26-november-2026)

**Kernel.** A request with needs is one part with no Split and no Plan
decision. The caller's statements are its sub-needs in order, each searched
in its own words. Each anchor gets one exact lexical search in round one,
and a failed lookup does not refuse the request.
[Kernel contract](../../crates/airk-kernel/src/arbiter/CONTRACT.md#caller-needs-and-anchors-26-november-2026)

**Generated.** The reviewed schema capture gained 88 lines and lost none.
The TypeScript types were regenerated from it. One Rust-emitted refusal case
changed its boundary value from 8 to 24 round-one reads.

The public Answer cases fixture gained 70 lines, because the public tool's
schema is generated from the request type and now advertises the four fields.

**Not done.** `deliverable` and `known` do not change gathering yet. The
public tool's description does not ask callers to fill the new fields, and
`task` is not implemented. Needs do not yet run as
separate jobs with a union by need, and no model judges against a need's own
statement. Nothing here was run on the real indexes. The `task` argument and
the description followed in entry 74.

### 74. The public Answer call takes `task`; guidance and descriptions say when to use each tool

The user chose option A on 25 November 2026 (completion plan, "Approved
public Answer call for the Stage A baseline"), and the guidance rewrite is
the first row of Step 3 of the agent-facing tool surface, which needs no
further decision.

**Contract.** The public Answer tool advertises `task` as its only required
argument. Its text becomes the canonical question verbatim, the service
supplies the request schema, and an omitted budget is the complete default.
An explicit valid budget passes through with the caller's values. The
existing full `schema`, `question` and `budget` request is accepted unchanged
through the 1.9 line, with the same request bytes and digest. `task` with
`question` is refused as `answer_task_and_question`, neither as
`answer_task_required`; a full request without its schema or budget keeps its
former refusal text under `answer_request_incomplete`. `needs`, `anchors`,
`deliverable` and `known` are advertised beside `task` and pass through.
[Core contract](../../crates/airk-core/src/arbiter/CONTRACT.md#public-answer-call-user-decision-25-november-2026)

**One conversion.** Core `answer_loop::AnswerCallV3::into_request` is the only
mapping. MCP `AnswerArgs` deserializes through it (`#[serde(try_from)]`), so
the native handler, the public and combined entries, the CLI/session path,
the supervisor's Admin partition check and the frontend combined Answer all
receive the canonical `ContextRequestV3`, and the service job's request
digest is the same for a `task` call and its full request.
[Frontend contract](../../crates/airk-frontend/src/combined_frontend/answer/CONTRACT.md)

**Guidance and descriptions.** Both server guidance strings now send a task
that needs evidence from several places to Answer first, with task and needs,
a known identifier to Symbols, a known path to Read File and a concept to
Search, inside the 512-byte instruction budget. The concurrency note
("Evidence default 16; Management Service:4/8/16/32/64 ...") was dropped to
fit; Status still reports the configured query concurrency. Each evidence
tool's description says when to use it, what it returns and what to call
next. Answer's asks for `task` and `needs`, describes `anchors`,
`deliverable` and `known` with their bounds, and says that `deliverable` and
`known` are checked but do not change gathering yet. Its statements about
enrolled indexes and an admitted connection were removed.

**Generated.** The reviewed schema capture gained the `AnswerCallV3` root (70
lines added, none removed) and the TypeScript types were regenerated from it
(50 lines each in `arbiter.ts` and `arbiter.d.ts`). The public Answer cases
fixture changed 52 lines in and 8 out: `required` is `["task"]`, the new
property descriptions, and three examples (`task_with_budget`, `task_only`,
`task_and_question`). The native Help content changed 95 lines in and 20
out: the new descriptions and the Answer schema, which had also not been
regenerated after entry 73.

**Not regenerated.** `packages/ui/generated/help-manifest.json` and the
Admin bundle in `crates/airk-cli/admin-assets/` embed the Help content; they
need `node scripts/build-explorer.cjs --write-help-manifest` in
`packages/vscode` and `npm run build` in `packages/admin`, which need the
packages' installed dependencies. The evaluation harness still supplies
Answer's `schema` and `budget` and sends `question`; frozen offers and
retained fixtures are unchanged.

### 75. The gathering rules a prototype proved, built into the kernel loop

The user approved building the proven gathering rules into the kernel on 26
November 2026. On four real tasks with 21 required passages the kernel loop
without a model delivered 1; a prototype outside the runtime that changed
what is fetched and how it is offered delivered 14
([prototype tool](../../scripts/arbiter-answer-eval/README.md#single-stage-arbiter-loop-prototype);
its results page, `docs/arbiter/gather-context-loop.md`, is not on this
branch yet).

**Kernel.** Seven rules, each a switch (`GatheringRules`), all on in a
product build; a test-support build may bind another set once for its
process, and the private evaluation fixture reads it from
`AIRK_ARBITER_EVALUATION_ANSWER_GATHERING` (`all`, `none`, or a list with
`-name` removals). They apply only to a one-part request with caller needs
or a deliverable that brings standing sub-needs; every other request makes
the same reads as before.

1. Search lanes: each sub-need's round-one search also runs on source code
   only, and the test-command and build-file standing sub-needs also on
   configuration only.
2. Standing sub-needs by deliverable: project rules, the test command, the
   build file and, for a code change, the error path, after the caller's or
   derived sub-needs.
3. Project guidance and cited files: one listing per guidance folder, head
   reads of `CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md` and `README.md`
   found there, then the files they and the best passages name by path.
4. Structure following from code facts (line-range and minimal rows):
   enclosing definitions, callees (same file first, then by name), call
   sites and users of inherited types, read once per file where ranges are
   near.
5. Every passage offered to every sub-need, after a cheap rare-word cut.
6. Delivery by need inside one byte budget, half the reply frame.
7. Per-stage counts and timings in every reply.
[Kernel contract](../../crates/airk-kernel/src/arbiter/CONTRACT.md#gathering-rules-user-decision-26-november-2026)

**Contract.** The request is unchanged. `SubNeedOrigin` gains `standing`
(at most `MAX_STANDING_NEEDS`, 4, after the others, one-part replies only).
The round-one bound was 1 + 16 + sub-needs x 6 and is now 1 + 16 + 48
(`GATHERING_READS_PER_PART`) + sub-needs x (6 + 2,
`SEARCH_LANES_PER_SUB_NEED`). The reply gains the optional `gathering`
report, whose reads and lookups equal `work.calls`. `MAX_SUB_NEEDS`,
`MAX_NEEDS`, `MAX_DECISIONS`, the default and maximum calls and every card
policy and digest source are unchanged.
[Core contract](../../crates/airk-core/src/arbiter/CONTRACT.md#gathering-rules-on-the-wire-user-decision-26-november-2026)

**Generated.** The reviewed schema capture gained 175 lines and lost none;
`arbiter.ts` and `arbiter.d.ts` were regenerated from it (73 lines added and
one changed in each: the `SubNeedOrigin` union). The Rust-emitted cases
(46 lines added, one changed) gained two
refusals (`standing_need_in_a_two_part_reply`,
`gathering_calls_differ_from_the_work`), and `round_reads_over_structure`
moved its boundary from 24 to 74 reads.

**Behaviour change for caller needs.** A request with caller needs and no
deliverable now also carries three standing sub-needs, so its part holds
up to eleven sub-needs; the caller test was updated to say so.

**Not done.** `known` still does not change gathering. The public Answer
tool's description still says `deliverable` does not change gathering; it
belongs to the public tool surface and was left to that change. The rules
were run only on canned reads in kernel tests, not on the real indexes, and
nothing was measured on the four tasks.

### 76. Round one leaves half the retention for the gathering rules

**Measured fault it closes.** First kernel baseline (27 November 2026, real
indexes, no model, caller needs, anchors and a deliverable, every rule on):
at the default retention budget (262,144 bytes) every request stopped with
part stop `budget` at stage `retrieve` in round one, after 40 calls that
retained 257,684 bytes. Guidance, cited and structure never ran and 1 of 21
required passages was delivered.

**Cause.** Nothing bounded round one's retention when rules 3 and 4 follow
it. Each sub-need's overview recipe (a search, whole chunks of its kept hits
and their outlines) and its code lane (whole chunks, `full_chunks: true`)
were retained in full; a read that no longer fit stopped the whole Answer
on `budget` (`Gathering::retain`), and when the budget held, the gathering
stages' own check (`bytes_fit`, a quarter of the budget free beyond each
planned read) left them nothing.

**Change.** (1) When rule 3 or 4 is on, round one's recipe reads leave half
the retention bytes free (`ROUND_ONE_KEEPS` 2, `Gathering::recipe_floor`):
a recipe read whose `max_bytes` bound, with the bounds of the reads in
flight and what is already retained, would leave less is not dispatched.
Its nominations' byte allowance comes from above the same floor. Searches
are never held back. (2) Lane searches return bounded snippets
(`full_chunks: false`, `max_bytes` 4,096, was whole chunks at 4,608); whole
text comes from the recipe's whole chunks and rule 4's definition reads,
which serves the small-reply goal. The default and maximum budgets are
unchanged, and a request without caller inputs makes the same reads.

**Arithmetic** (test
`the_default_retention_leaves_round_one_room_for_guidance_cited_and_structure`:
six caller needs and four standing ones, two anchors, default budget, every
evidence read filled to nine tenths of its bound and a snippet search to
512 bytes a hit). Before: retrieve 47 reads, 188,632 bytes; guidance 0,
cited 0, structure 0 reads (6 lookups, 3,325 bytes); the stages found under
a quarter of the budget free and made no read. After: retrieve 33 reads,
129,357 bytes; guidance 3 reads, 16,593 bytes (2,354 lookup bytes); cited 2
reads, 11,062 bytes; structure 3 reads, 20,279 bytes (9,147 lookup bytes);
177,291 bytes in all.
[Kernel contract](../../crates/airk-kernel/src/arbiter/CONTRACT.md#gathering-rules-user-decision-26-november-2026)

**Not measured.** Canned reads only; the real indexes were not rerun.
