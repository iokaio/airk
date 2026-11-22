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

Further entries are added as implementation proceeds.
