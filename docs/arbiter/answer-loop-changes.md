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
     never waited is never `requeued` (review C4).
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
      reached their deadline (review C5).
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
- **Wiring, a follow-up:** the table is not yet on the live call path. The
  L3 agent is switching the MCP Answer handler to v3 and deleting v1 and v2
  at the same sites. The remaining steps are:
  1. **Kernel (L3 owner).** Accept an admission plan in `AnswerEnv`
     (`crates/airk-kernel/src/arbiter/answer.rs`, `Kernel::answer_with`):
     the applied shape bounds `Gathering`, and the degradations join the
     reply.
  2. **Core or kernel.** Add a builder for the ticket reply (a ticket,
     partial `admission`/`queued`, and no work), and one for the running
     partial with its continuation.
  3. **Service.** Call `AnswerJobTable::call` for Answer /3 where the
     service admits Answer: at `tools_call::admit_context` and its callers,
     which are `tools_call::response`, `tools_call::window_response`,
     `tools_call::selection_read` and `open_reads::response`. The runner
     reopens its own backend and runs to `run_ms`, so the job does not
     depend on the caller's connection. The scope is built from the
     selection's verified authority, never from client input.
  4. **MCP stdio.** In the local path (`crates/airk-mcp/src/lib.rs`, the
     `answer` tool), the kernel runs in the MCP process. The table needs a
     trait in `airk-mcp` that the service implements, because `airk-mcp`
     cannot depend on `airk-service`.
  5. **Status.** Expose `QueueStatus` on the service status surface.
- **Evidence:** the `answer_jobs::` tests in `airk-service` (25 tests),
  listed in the contract's focused checks.

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
  `docs/agent-current-work.md`; the ledger entries of 22 November at 21:45 and
  22:10 UTC; the harness changes on `loop/l5c`.
