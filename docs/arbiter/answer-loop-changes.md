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
