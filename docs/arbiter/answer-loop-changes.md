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
