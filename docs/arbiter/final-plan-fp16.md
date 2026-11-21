# AIRK FP16 delivery plan

Prepared 20 November 2026, America/Denver, and rewritten that evening around
the priorities the user stated. A later revision the same evening added the
test hardware the user named and the parts of an outside review that save
time or close a gap. Its design follows from the priorities. Six items amend
recorded decisions, and the user approved all six that evening (section 3). A
few smaller choices remain open and are listed there.
[Release precision decision](final-plan.md#fp16-release-precision-decision-20-november-2026-americadenver)

## 1. Priorities

The user stated these on 20 November 2026. They govern every choice below.

| ID | Priority | What it decides here |
| --- | --- | --- |
| P1 | Support Windows, Linux and macOS. | The CPU route ships on all three. Each GPU route names its operating systems. |
| P2 | Support NVIDIA and Radeon GPUs. | Two GPU routes: CUDA for NVIDIA and one portable route for Radeon. |
| P3 | Use FP16 to keep downloads smaller and resource requirements reasonable. | FP16 is a storage and memory decision. It does not require half arithmetic. |
| P4 | Prefer simpler and good over maximal optimization. | FP32 arithmetic everywhere outside the finished CUDA candidates. No new half-arithmetic kernels. |
| P5 | Choose a solution that can support all platforms. | One weight file, one scoring contract and one calibrated reference per model. |
| P6 | Enhance retrieval so a more capable agent receives the information it needs. An advanced answer is not required. | Judge by the evidence delivered. Terminal classes stay unavailable. |
| P7 | Specialize the selected models' weights without limiting what they can already do. | Specialization replaces a weight file only. Original weights stay qualified. |
| P8 | Examine how MiniLM serves semantic indexing, at low priority. | No MiniLM change in this release. |
| P9 | CPU inference must be available whenever the GPU cannot be used. | A CPU ladder that always ends in a working route. |
| P10 | Explore faster, lower-quality options for CPU fallback. | A reduced-budget tier first and integer weights second. |

## 2. Design

### One weight rule

No weight file larger than FP16 ships. Floating-point weights ship as one
FP16 file per model, and every route reads that same file. Smaller integer
artifacts that already pass their gates stay: the bundled MiniLM INT8 encoder
and the retained Focus CPU export. No FP32 weight file and no FP32 GPU variant
ships.

### One arithmetic reference

The reference for every route is FP32 arithmetic on the half-rounded weights.
The retained evidence already points there. Recipes with half operands missed
the 0.007 limit by a wide margin, and the recipes that pass compute in FP32 on
half-stored weights. An x86-64 CPU has no half arithmetic to offer in the
pinned runtime (section 7). One independent reference per model serves every
route, so no route needs its own parity investigation.

### Three routes

| Route | Covers | Graph | State |
| --- | --- | --- | --- |
| CPU | Windows, Linux and macOS on any hardware | The portable graph: the model's FP32 graph with its learned weights cast from the FP16 file. | Compact and Large are exported, pass the independent graph proof and are scored on the published runtime build. Compact keeps every winner on the 100 frozen rows. AIRK's static CPU runtime loads and scores Compact on Windows at 8 cores with every reference winner. Linux and macOS have not run it. |
| CUDA | NVIDIA on Windows and Linux | The existing Compact and Large candidates. | Both pass the four development widths on Windows. Compact's fit is predicted to find no safe cutoff (step 4 detail). Linux links and has not scored a model. |
| WebGPU | Radeon on Windows and Linux, Apple GPUs on macOS, and other adapters | The portable graph, with FP32 arithmetic on the GPU. | Runs on the A6000 through the official prebuilt plugin: both exported graphs are correct and inside the speed line. With inputs padded on the right to a ladder of five fixed lengths, both meet their memory targets for varying lengths on the A6000, with no winner change. A real 16 GB or 24 GB card and other adapters are unmeasured. Continues through the staged probe (steps 11 and 12). |

Every route shares the weight file and the scoring contract. The CPU and
WebGPU routes start from the same portable graph. A route may use a small
provider-specific graph when that is what makes its fused attention kernel
dispatch. On a GPU the weights stay resident in half and are cast where
used, when that is needed to meet the memory target. The CUDA candidates stay
because they are finished and fast. WebGPU replaces the separate Core ML,
DirectML and ROCm recipes of the parent plan. If WebGPU fails on a platform,
that platform runs on the CPU ladder. The user dropped the pursuit of
specialized code for Radeon.

| Operating system | NVIDIA | Radeon | Apple GPU | GPU unusable |
| --- | --- | --- | --- | --- |
| Windows | CUDA | WebGPU on Direct3D 12 | not applicable | CPU ladder |
| Linux | CUDA | WebGPU on Vulkan | not applicable | CPU ladder |
| macOS | not applicable | not applicable | WebGPU on Metal | CPU ladder |

### GPU use is opt-in

GPU use is opt-in on every platform. The user chooses it in the Service Admin
configuration, through the one service-wide setting. Until that choice is
made, no GPU route loads and Auto uses the CPU ladder, so CPU is the default
route everywhere. With GPU use on, Auto prefers a qualified GPU route for the
adapter and falls back to the ladder. This one setting covers every GPU
route: CUDA, and WebGPU on Radeon and macOS. The source now defaults the
setting to off (commit `0b5fe97c`, 21 November). An enabled value stored
under the earlier default reads as off with a reset notice pending, and a
value the user sets afterwards is kept. The decision is recorded in the Core
Arbiter contract. Service Admin shows the notice with two answers, "Allow GPU
use" and "Keep GPU off", and the VS Code extension shows the reset state and
points to Service Admin. Neither has been exercised against an installed
service that carries an old enabled setting.

### CPU ladder

No CPU tier is guaranteed. The user set a target instead: processors with at
least 8 cores that are no more than six years old. Tiers are measured and
admitted for that target class, at 8 cores. On a machine below the target,
Auto still applies the ladder by measured fit and reports the result, and
retrieval alone remains available. A model enters Auto on a system only when
a qualified tier below it exists there.

Auto selects the first tier that fits the machine's measured memory and CPU
class. It reports the device, the tier and the reason on every surface. Each
result must also let a downstream agent tell which route, model, tier and
token budget produced its evidence. No downgrade is silent. GPU use that is
not opted in or is turned off, a missing or unsupported GPU, insufficient
device memory and a settled GPU failure all enter the ladder.

A tier is admitted on the parent protocol for the model forward: one cold
load, one warmup and 20 warm samples with nearest-rank p95, on a named CPU
class. The target class is also measured end to end (step 2). Every latency
figure states which of the two it is.

1. **Full.** The selected model at the 8,192-token budget, where its measured
   p95 meets 120 seconds. The FP32 Compact graph measured a model-forward p95
   of 86.049136 s over 20 samples at the CPU4 class, which is below the
   8-core target. At eight threads the same graph took 43.1 to 43.4 s per
   forward in a probe run on the published runtime build, which is not the
   static runtime.
2. **Fast.** The selected model at a 2,048-token budget. At the earlier
   2,048-token limits Compact measured a p95 of about 19.5 s and Large about
   23.1 s. Those are historical figures on earlier artifacts. In the probe
   runs at eight threads, the portable Large graph took 20.3 s at 2,048
   tokens and the portable Compact graph 6.0 s at 1,588 tokens.
3. **Smaller.** The next smaller model when memory or time still does not fit:
   Large to Compact, then Compact to Focus only.
4. **Retrieval only.** A typed unavailable result for the optional stages.
   MiniLM retrieval keeps working.

After the ladder works, explore integer weights for Context on CPU as a
faster variant: first the retained 8-bit Compact export, then 4-bit weights
through the pinned runtime's `MatMulNBits` tooling. Measure speed and
agreement with the calibrated identity, then bring an admission rule to
review. The retained 8-bit Compact export missed the 0.25 approximate-export
limit at 0.290778875, so admitting it would need a rule the user approves.

### Acceptance

1. **Calibrate once per model.** Fit on the identity that holds the complete
   100-row panel. For Compact that is the Windows CUDA candidate.
2. **Admit every other route by drift, then confirm.** Score the same 100
   frozen compiled inputs, the width fixtures and the retained hard inputs
   offline. For Compact the hard inputs are the 1,014-token development input
   and the TRAIN_Q543 close call. For Large they are its retained near-tie
   controls. Set each route's drift limit before its panel is scored: no
   looser than 0.007, and tightened to a round value above the worst drift
   seen on the fixtures and hard inputs. Require finite scores, the same
   winner on every row, normalized-mass error within its limit, and a maximum
   raw-score difference within the route's limit. Apply the fitted threshold
   with a guard band of twice that limit, and report accepted coverage on the
   panel with and without the band. A tighter limit keeps more accepted
   decisions, and the portable routes should drift little because they
   compute in FP32 on the same weights.
   Then score a disjoint confirmation set the same way and require the same
   accept and abstain decisions as the calibrated identity, including the
   near ties and the 8,192-token cases. The confirmation set is the
   calibrated identity's policy-confirmation capture, reused as frozen
   inputs. No such capture is recorded yet, so one live capture is owed for
   each model. The guard band protects only while drift stays within the limit,
   and that limit is an observation on these rows. A route that breaks it on
   the confirmation set is not admitted.
3. **Measure task quality on every iteration.** Run the paired comparison
   against FP32 on the calibrated identity and report the result each time.
   The 98% figure is a reference point for the review. It does not stop the
   work. Other routes inherit the result through rule 2.
4. **Measure what each stage contributes.** On the same tasks and the same
   final evidence budget, compare retrieval alone, retrieval with Focus, and
   retrieval with each Context model. Record whether the delivered spans
   contain the evidence the task needs, which required evidence is missing,
   duplicates and latency. Run the first two arms first (step 1). Compact and
   Large each earn further work by delivering evidence the cheaper stage
   misses.

Rule 2 replaces a live three-hour capture per identity with offline scoring
of frozen rows. The last measured batches cost 270.392 s per eight rows on
CPU and about 82 s per eight rows on the A6000.
[Policy confirmation](final-plan.md#step-13--collect-exact-artifact-calibration-and-custody-evidence)

## 3. Approved changes to recorded decisions

The user approved A1 to A6 on the evening of 20 November 2026
(America/Denver). Each row amends a recorded decision.

| ID | Change | Replaces or amends | Why |
| --- | --- | --- | --- |
| A1 | Adopt the weight rule: nothing larger than FP16 ships, and passing integer artifacts stay. | The FP16-only wording of 20 November. | P3 names the purpose. An integer artifact serves it better than FP16. The committed catalog names the Focus profile as an 8-bit export, so A1 covers it. Read the 1,024-token export's receipt before step 2. |
| A2 | Adopt the CPU ladder, including the 2,048-token tier and reported model downgrades. The user later set a CPU target in place of a promised tier (below). | D08's same-model and same-width fallback. | P9 and P10. Large at full width cannot meet the deadline on CPU. |
| A3 | Make WebGPU the GPU route for Radeon and macOS when the probe passes on the Azure Radeon VM and the rented Mac. | The Core ML, DirectML and ROCm sequence, and the deferral of WebGPU. | P1, P2, P4 and P5. One provider instead of three. |
| A4 | Adopt calibration transfer with a guard band and a disjoint confirmation (acceptance rule 2). | Separate calibration and task evidence for every promoted identity. | P4 and P5. Without it each of the routes above needs its own capture. |
| A5 | Allow evaluation-only scoring of the frozen FP32 artifact, once per model. | The stop on new FP32 runs. | The approved 98% rule names an FP32 arm that does not exist for the held-out cohort. |
| A6 | Judge device memory fit relative to a model-free control run on every platform and provider, with the approved 2 GiB reserve. Keep absolute memory and any spill visible. | The dedicated-only reading of the memory rule. | A bare CUDA context on Windows already shows about 77 MB of shared memory. |

The user decided three further points at 7:38 p.m. Denver time the same
evening.

- If WebGPU fails on a platform, that platform falls back to CPU. Specialized
  code for Radeon is dropped, so the DirectML and Linux Radeon routes of the
  parent plan are not pursued, and Core ML is no longer a fallback.
- The A6000 smoke test carries a speed line: WebGPU within five times CUDA.
- The A6000 smoke test stays early.

The user answered the remaining choices at 7:43 p.m. Denver time, and
confirmed the three points above.

- No CPU tier is guaranteed. The target is a processor with at least 8 cores
  that is no more than six years old.
- The memory goal is an 8 GB card for Compact and a 16 GB card for Large. If
  the goal cannot be met, the stated requirement is a 16 GB card for Compact
  and a 24 GB card for Large.

The user clarified one more point at 7:51 p.m. Denver time.

- GPU use is opt-in on all platforms, chosen in the Service Admin
  configuration. This amends the earlier approval under which the
  service-wide setting defaults to enabled.

One choice followed from that clarification: how to treat a configuration
that already stores GPU use as enabled. The user answered it at 9:17 p.m.,
below.

The user gave four directions at 8:29 p.m. Denver time.

- Do not assume the existing GPU work was done right. Fix or improve it
  where it is not.
- Measure the paired comparison against FP32 on each iteration and report
  it. Do not stop at 98%.
- Finish the work. A joint review of the results then decides whether to
  proceed or try again.
- Pushing `feat/arbiter-final` to `origin` is authorized, so that nothing
  exists only on one machine.

The user decided four more points at 9:17 p.m. Denver time, after the first
three measured iterations.

- The memory targets are a 16 GB card for Compact and a 24 GB card for
  Large. They replace the earlier 8 GB and 16 GB goals.
- A configuration that already stores GPU use as enabled is reset to off
  once, and the user is told. That value was written as a default and was
  not chosen.
- A person reviews the discordant cases of the paired comparison. The
  reviewer is the user unless the user names someone else.
- Carry the whole plan to completion, using multiple agents where that is
  faster.

No choice in this section remains open.

[D08](final-plan.md#11-automatic-gpu-execution-and-cpu-fallback-d08) ·
[Platform order](final-plan.md#verified-starting-point-and-backend-order) ·
[Calibration by provider](final-plan.md#numerical-limits-and-calibration-by-provider) ·
[98% rule](final-plan.md#accepted-context-fp16-task-quality-qualification) ·
[Memory fit](final-plan.md#developer-memory-fit-and-shipping-auto)

## 4. Work order

Three milestones, each a working product. The first is CPU inference
installed on Windows and Linux and measured for the target CPU class. The second adds CUDA for
NVIDIA on both systems and the Context tiers on CPU. The third adds macOS and
Radeon: the CPU route on macOS, and WebGPU on Metal, Direct3D 12 and Vulkan.
The user directed that Mac work start with a rehearsal on the Mac Neo after
the Windows and Linux CUDA work is complete (step 11). Steps 1 to 5 start now
and run in parallel within the one-Cargo-owner rule. The user authorized
multiple agents. At any time one agent owns Cargo on a target and one owns
the GPU, each agent owns named files, and the primary agent reviews and
commits every result.

### Test hardware

| Host | Used for |
| --- | --- |
| This workstation: Threadripper PRO 5975WX and RTX A6000 | Windows CPU and CUDA. Linux CPU and CUDA under WSL2. The WebGPU smoke test on Direct3D 12. |
| Mac Neo, 8 GiB | The macOS rehearsal of step 11, after the Windows and Linux CUDA work: the arm64 build, WebGPU start-up on Metal with small graphs, Focus on CPU, and a dry run of the rented-Mac batch. It cannot hold a Context model. |
| Rented Mac with a 32 GB GPU and 128 GB of RAM, as the user specified | macOS CPU tiers for Compact and Large. WebGPU on Metal. |
| Azure V620 VM with a 32 GB Radeon card, under Windows and under Linux | WebGPU on Direct3D 12 and on Vulkan. The CPU route on native Linux and on a second Windows machine. |

Booking stays a manual user action. Each rented host receives one rehearsed
batch. A 32 GB device proves the route. It does not prove fit on a smaller
card, so each claim states the smallest memory actually tested.

### Step 0 — Confidence refinement

The user added this step before the numbered work. It qualifies nothing. Each
check settles one uncertainty behind the confidence figures and is capped at
one attempt and one corrective attempt. Its results update the confidence
column below.

| Check | What it settles |
| --- | --- |
| 0a. Compile the Rust preserved in `c41b4fe6` and `4853220b` with a scoped Clippy. | Whether steps 4 and 10 start from code that builds. |
| 0b. Run a WebGPU smoke test on the A6000 with the documented prebuilt provider package: provider start-up, placement, then the retained FP32 Compact CPU graph at 32 and 8,192 tokens against the reference scores and the CUDA speed line. | Whether the Radeon and macOS route is plausible before any build or rental. |
| 0c. Count the cohort cases that already carry publisher labels and native-visible spans. | Whether steps 1 and 8 can run without new human judgments. |
| 0d. Identify the graph behind the retained Compact CPU figure and the format of the 1,024-token Focus export. | Whether step 3 starts from the graph the plan assumes, and whether A1 covers Focus. |
| 0e. Read how a profile carries its token ceiling in Core. | How large the budget-tier part of step 5 is. |
| 0f. Read whether the pinned Rust binding can load a provider plugin library. | Whether WebGPU can enter the product without a new runtime build path. |

Results on 21 November 2026 UTC. The receipts are in the private
`confidence-refinement-r1` folder of the campaign root, and the ledger holds
their digests.

| Check | Result |
| --- | --- |
| 0a | Failed, then fixed. The Large calibration selector called a private function in the Compact module. A two-line visibility change lets the retrieval and inference crates pass scoped Clippy with the CUDA and test-support features. The kernel examples and the airkctl test passed unchanged. Separately, `cargo fmt --all -- --check` fails in 11 files at the pinned toolchain, which the parent plan's format gate will meet. |
| 0b | Correctness and speed pass. Memory fails as configured. The official plugin 0.6.0 ran the FP32 Compact attention graph on the A6000 through Direct3D 12. At 32 tokens the scores were within 0.0000668 of the FP32 reference. At 8,192 tokens they were within 0.004486, with the same winner both times, and warm forwards took 5.12 to 5.23 s against 1.7721 s for CUDA, a factor of about 2.9. All 28 attention kernels ran on WebGPU. Adapter memory peaked near 21 GB above its baseline. |
| 0c | 142 cases carry publisher coordinates: 76 TechQA, 41 AWS and 25 IBM Cloud. The 76 TechQA cases also have complete ordered menus, so step 1 can start with them and needs no new judgment. Neither native token budgets nor delivered evidence have been measured for any of them. |
| 0d | The 86-second figure came from the Compact CPU component at the CPU4 class, four cores and four threads, under a 16 GiB reservation. Two graph forms are retained: the original export and an attention-kernel variant, which is the one that ran on WebGPU. Which form produced the figure was not resolved. The committed catalog names the Focus profile as an 8-bit export. |
| 0e | Smaller than feared. Core validates a `max_tokens` ceiling on every profile, and the committed catalog already holds Context profiles at 2,048 tokens. A fast tier is a profile, not a compiler change. |
| 0f | Yes. The pinned `ort` crate has a WebGPU provider module, and its system bindings expose the provider-library registration and device-selection calls. |

[WebGPU probe script](../../scripts/arbiter-webgpu-probe/README.md)

### What the measurements say about the Context stage, 21 November

The Context stage, as designed, has no measured way to add evidence on the
100-row calibration panel. This is about the task design and the
unspecialized models. It is not about FP16, CUDA or WebGPU: the FP32 CPU
references agree with every winner.

- **Compact has no safe cutoff.** The native study on the 100 rows returned
  none. Only two certified Read cards win, and four uncertified ones win by
  more.
- **Compact follows position, not content.** The seven option lines are the
  same on every row: five generic Read cards in queue order, then Sufficient,
  then Clarify. A Read option says only `read r<n>`. One fixed per-slot score
  profile explains 91% of the variation. With the reads rotated behind the
  options, the best Read stayed on the fourth line in 112 of 112 forwards.
  Which read a card names explains about 0.5% of the variation. No compile or
  prompt defect was found.
- **Large reads the cards and says Sufficient.** Its top card follows the
  text when cards are rotated, and it is Sufficient on 99 of 100 rows. Among
  the Read cards it is still mostly position-driven: 80% slot, 7% read.
- **Stating the read in the option line helps the models see it, not choose
  well.** The read's share rises to 19% on Compact and 46% on Large. What
  they then prefer is the first context-expansion read, and the certified
  reads stay at chance rank.
- **Asking about one read at a time does not help.** Neither model scores a
  Read above Sufficient in any of 140 pairs each.

The certificates cover 12 reads on 11 rows, so the sample can rule out a
large effect and cannot detect a small one. The two prompt forms were probes
the product never compiles.

What follows for this plan:

- Steps that only matter if Context acts are on hold: Compact catalog rows,
  the confirmation capture and the Context arms of step 8. Large's CUDA panel
  is deferred, because Large's top card is a Read on 1 of 100 rows and its
  fit cannot find a certified admission either.
- Work that does not depend on Context acting continues: the Focus
  comparison, the CPU and WebGPU routes as engineering, the opt-in GPU
  setting, typed reasons and truthful status, and the Linux and macOS CPU
  paths.
- Making Context add evidence needs either weights specialized for this task
  (priority 7) or a redesigned task. That is a new decision for the user and
  is not part of this plan's approved steps.

[Slot diagnosis](../../scripts/arbiter-context-slot-diagnosis/README.md) ·
[Study report script](../../scripts/arbiter/evaluation/selected_goal/cal_raw/r8_cutlass_study_report.py)

### Steps

| Step | Work | Done when | Needs | Confidence |
| --- | --- | --- | --- | --- |
| 1 | Freeze the judged cohort. Run the first two stage arms on Windows CPU at the same evidence budget: retrieval alone and retrieval with Focus. | Focus's contribution to delivered evidence is measured, with missing evidence, duplicates and latency. | Nothing | Medium to high. Start with the 76 TechQA cases, which carry publisher coordinates and complete menus. In progress on 21 November. The runner and scorer exist, and the run uses the private evaluation fixture because the Focus profile is unqualified. No pass line is set; the paired counts are reported. Retrieval alone is measured: it delivers the publisher's span on 38 of 75 judged cases. The Focus arm is running as two fixture lifetimes of 38 cases, because the fixture refuses to configure Focus over more than 64 indexes. |
| 2 | Deliver the CPU milestone. Install the product on Windows and on Linux under WSL2 and measure the ladder tiers end to end at 8 cores, the target class. Start with retrieval and Focus, and add the Compact tiers when step 3 delivers them. Record the four memory figures. macOS joins at step 11. | Each tier has an end-to-end figure at 8 cores on both systems, and the tiers that meet 120 seconds are admitted for the target class. | Nothing | Medium. The runtime build on each system is the risk. |
| 3 | Export the portable graph for Compact. Record the recipe in the exporter contract first. Verify shared weight bytes. Load on the static runtime with zero forwards. Score the four widths and 2,048 tokens against the reference. Time each width and record the four memory figures. | Compact has measured CPU numbers for the full and fast tiers. | Nothing | High. The exporter's portable recipe exists, Compact and Large are exported and scored on the published runtime, and both pass the independent graph proof. The static runtime loads and scores Compact on Windows. A quiet-machine timing, Large on the static runtime and a learned-byte check for the portable recipe remain. |
| 4 | Finish Windows CUDA. Complete Compact matching references, fit calibration, add the Large selector and score its panel. Run capacity under A6. | Both candidates hold a calibrated panel and a memory figure. | Nothing | Not reachable as written. The native study found no safe cutoff for Compact, and the cause is the task design (see the Context section above). The Large tooling exists and its panel is deferred. Capacity under A6 is implemented and replayed offline; live trials wait for the desktop's GPU memory state to recover. See the step 4 detail. |
| 5 | Record the Core contract decision, then implement: the opt-in GPU default, Auto order, the ladder tiers and the CPU target class, typed reasons, truthful status, per-result disclosure of route, model, tier and budget, the weight rule and a WebGPU provider value. Test with injected capabilities. | Reviewed Core changes with qualification flags false. | Nothing | Medium to high. Profiles already carry a token ceiling, so a tier is a profile. The opt-in GPU default is done (commit `0b5fe97c`). The other slices and the Admin notice remain. |
| 6 | Run the WebGPU smoke test on the A6000 through Direct3D 12 as soon as step 3 has a graph. It stays early by the user's decision and includes the speed line. It takes the GPU only when no CUDA run holds it. | A go or no-go for the rented runs. | Step 3 | Done on 21 November: go, with a memory caveat found afterwards. The exported Compact and Large graphs pass correctness and the speed line on the A6000, and their memory targets for one input length. With varying lengths the targets are not met by either buffer cache mode as measured (fourth iteration below). Large's 8,192-token check was against the CPU provider and still needs an independent reference. |
| 7 | Score the portable graph on the 100 frozen Compact rows and the confirmation set on CPU, and apply acceptance rule 2. | The CPU route is admitted or its first failing row is named. | Steps 3 and 4, and one policy-confirmation capture on the calibrated identity | High for the scoring. The capture is the cost. The 100 rows are scored on CPU and WebGPU as a development measurement: no winner changed and the largest raw difference was 0.000519 on CPU. No confirmation capture exists, and none is made: the fit found no safe cutoff, so there is no policy to confirm. |
| 8 | Complete the stage comparison and the 98% comparison: the FP32 arm, the calibrated identity, and the Compact and Large arms. Compare delivered evidence first. A person reviews the discordant pairs from a blinded packet. | The 98% bound and each Context model's added evidence are measured. | Nothing | The retrieval and Focus arms are being measured in step 1. The Context arms are on hold: with no policy, Context accepts no action in either precision, so the paired comparison would be equal by construction and would say nothing. |
| 9 | Decide Large's expansion from step 8. Extend Large to the CPU ladder, Linux and WebGPU only if it delivers evidence Compact misses. Until then its CPU fallback is the Compact downgrade. | A recorded disposition for Large. | Step 8 | Decided by the diagnosis instead: no expansion. Large picks Sufficient on 99 of 100 rows, so it delivers no evidence that Compact misses under the current task. |
| 10 | Port the CUDA candidates to Linux on the existing WSL2 instance. Score widths and the frozen rows. | Linux CUDA is admitted by rule 2. | Step 4 | Medium. |
| 11 | After the Windows and Linux CUDA work is complete, rehearse macOS on the Mac Neo. Build the arm64 runtime with the CPU and WebGPU providers from pinned inputs. Confirm that WebGPU starts on Metal. Run synthetic graphs and reduced fixtures, Focus on CPU, and a dry run of the rented-Mac batch. | The build is reproducible, Metal support is confirmed or refuted, and the batch runs end to end on small inputs. | Steps 4 and 10 | Medium. Memory and disk on the Neo are the risk. |
| 12 | Run one rehearsed batch on each rented host: the Azure VM under Windows, the Azure VM under Linux, and the rented Mac. | WebGPU is adopted under A3, or the failing platform stays on the CPU ladder. Native Linux and macOS hold CPU evidence for the Context tiers. | Steps 6 and 7. Step 11 for the Mac batch. | Medium for WebGPU. Memory and the Radeon and Metal drivers are the risk. Medium for CPU. |
| 13 | Package and run the installed journeys of the parent plan for each milestone. | Parent steps 16 and 17 for that milestone. | The milestone's steps | Parent plan. |

Step 2 detail. The retained Focus figure is per pair: 4.280648 s warm p95 at
1,024 tokens. A request reranks many pairs, and an engineering run scored 14
pairs in 59.14 s. Measure the installed path end to end: cold load, a full
rerank of one request's candidates, evidence assembly and total latency. Cap
the run at 8 cores to stand for the target class, and state the processor
model with each figure. The four memory figures are download size, peak load memory, warm residency and
memory after unload. Record them separately on every route, because runtime
casts and weight prepacking can undo the storage saving in memory.

Step 3 detail. The portable graph keeps the attention form of the retained
FP32 CPU graph. Its embedding and seven learned projections per layer read
the FP16 file through one explicit cast each, and candidate rows are gathered
in half before the cast. Observe where the runtime evaluates the weight casts
and bind the optimizer settings and the optimized graph into the identity. The
pinned constant folding refuses a folded output above 1 GiB. Carry the three
changes of the first measured iteration into the exporter: no explicit
attention bias, since Context inputs are compiled without padding and the
kernel applies causality itself, half weights behind casts, and for GPU
routes the two disabled passes that keep those weights resident in half. The
CPU forward of the attention-kernel variant reproduced the retained 0.000191
error figure, which indicates that the retained CPU figure used that variant.

The second measured iteration, on 21 November, did this for Compact. The
exporter has a portable recipe, and its CLI takes `--recipe portable`. The
exported graph's `weights.bin` is byte-identical to the retained Compact GPU
candidate's, so one weight file serves both. The graph was then scored
against the FP32 reference on the published runtime build, which is not the
static runtime. Every case kept the reference winner.

| Case | WebGPU on the A6000: warm forward, difference | CPU at eight threads: warm forward, difference |
| --- | --- | --- |
| 32 tokens | 0.064 s, 0.0000954 | 0.177 s, 0.0000467 |
| 128 tokens | 0.079 s, 0.0000360 | 0.500 s, 0.0000064 |
| 1,024 tokens | 0.400 s, 0.000328 | 3.76 s, 0.000282 |
| 1,588 tokens, the TRAIN_Q543 close call | 0.622 s, 0.000370 | 5.99 s, 0.0000725 |
| 8,192 tokens | 4.37 s, 0.004494 | 41.4 to 44.3 s, 0.000229 |

On WebGPU the weights stayed resident in half and the peak was 7,168 MiB
above baseline for the 8,192-token case alone, and 8,385 MiB when all five
cases ran in one session.

The third measured iteration exported Large the same way. Its `weights.bin`
is also byte-identical to the retained Large GPU candidate's. Large's
retained numeric fixture has reference scores at 17, 128 and 2,048 tokens. No
retained 8,192-token reference was in a form the probe reads, so that width
used Compact's 8,192-token input and compared the two providers with each
other.

| Case | WebGPU on the A6000 | CPU at eight threads |
| --- | --- | --- |
| 17 tokens | 0.115 s warm, 0.0000324 from FP32 | 0.334 s, 0.0000095 from FP32 |
| 128 tokens | 0.172 s warm, 0.0000445 from FP32 | 1.16 s, 0.0000167 from FP32 |
| 2,048 tokens | 2.00 s warm, 0.0000091 from FP32 | 20.3 s, 0.0000057 from FP32 |
| 8,192 tokens | 10.98 to 11.26 s warm | 107.2 s, one forward |

At 8,192 tokens the two providers differed by 0.004814 and chose the same
winner. The WebGPU warm forward is about 2.3 times the Large CUDA candidate's
4.7935 s. Large's WebGPU peak was 15,300 MiB above baseline in that four-case
session. With the reserve that is 17,348 MiB, inside the 24 GB target. On
CPU the 2,048-token tier fits easily at eight
threads, and the full tier is marginal at 107 s for a single forward.

The independent graph proof now covers the portable recipe
(`portable_graph.py`, commit `4f56352d`). Both real exports pass it: Compact
with 28 layers and 4,230 nodes, Large with 36 layers and 5,414 nodes. It
proves graph structure only. A diagnostic found that the half tensors of both
portable graphs carry the same names, shapes, offsets and lengths as the
default-recipe candidates whose learned bytes were verified, and the weight
files are identical. A maintained learned-byte check for the portable recipe
is still owed.

AIRK's static ONNX Runtime 1.30 CPU build loads and scores the portable
Compact graph through a test-only seam (`portable_cpu.rs`, commit
`02966c7f`), with no change to a digest-input file. The process was capped to
8 physical cores of a Threadripper PRO 5975WX. These are development
observations, taken while other jobs ran.

| Measure | Result |
| --- | --- |
| Largest difference from the FP32 references at 32, 128, 1,024, 1,588 and 8,192 tokens | 0.000282, every winner the same |
| Cold load with a warm file cache | 9.0 to 9.4 s |
| Peak resident memory, set during load | 11.77 GiB |
| Resident after load, and after drop | 6.35 GB, then under 50 MB |
| Fast tier, 2,048 tokens: median and p95 of 20 | 9.12 s and 9.21 s |
| Full tier, 8,192 tokens: median and p95 of 20 | 48.4 to 51.3 s and 52.0 to 53.3 s |

The projection weight casts fold at load. The embedding table stays in half
and only gathered rows are cast per forward. The static runtime was 10 to
40% slower than the published build on the same inputs, and the cause is not
isolated. Cold load plus a first Full forward takes about 61 to 65 s of the
120 s deadline. A catalog row needs a host reservation of about 13 GiB. Under
the default Balanced preset that is admitted only from about 52 GiB of RAM,
so a 32 GB machine on defaults would refuse Compact on both tiers; the Fast
tier gives no memory relief because the peak is the load transient.

Still owed for step 3: a quiet-machine p95, a 2,048-token reference
comparison, Large on the static runtime, the learned-byte check for the
portable recipe, and an 8,192-token reference for Large.
[Exporter contract](../../scripts/arbiter-fp16-export/CONTRACT.md) ·
[Large weight-cast edit](../../scripts/arbiter-fp16-execution/large_weight_only_candidate.py) ·
[Shared-weight verifier](../../scripts/arbiter-fp16-execution/verify_shared_onnx_weights.py) ·
[Reference contract](../../scripts/arbiter-fp16-reference/CONTRACT.md)

Step 4 detail, from a read-only review on 21 November. The Compact GPU panel
is complete: 100 rows and 700 scores on the calibrated image, which is now
sealed. Rows 000 to 047 have matching and publisher CPU references, and rows
048 to 099 have neither.

- **The fit is predicted to find no safe cutoff for Compact.** This is
  arithmetic on the retained panel, not the native owner. The rule admits a
  row when its top card is a generic Read card and its top-two margin reaches
  the cutoff, and it takes the cutoff with the most certified admissions and
  no uncertified one. The top card is Sufficient on 55 rows and Read on 45.
  Only two Read winners are certified, with margins 0.52 and 5.62, and four
  uncertified Read winners have larger margins, from 6.37 to 10.68. Both CPU
  references agree with every CUDA winner on the 48 checked rows, so this is
  model behaviour and not a CUDA defect. More references will not change it.
- **What that would mean.** If the native study confirms it, Compact has no
  calibrated policy under the generic Read rule, a confirmation capture has
  nothing to confirm, and Context accepts no action in either precision. The
  user decides what follows. The study runs first, before the remaining
  references and before any confirmation capture, because its result decides
  whether they are worth their cost.
- **Large.** No image contains the Large selector and no launcher can invoke
  it. The build launcher refuses to run: its deadline constant expired, and
  its D: floor encodes 120 GiB of growth where the approved budget is
  195,421,011,968 bytes. Successor scripts are being written. Large's panel
  shows whether Large has a safe cutoff where Compact has none.
- **Capacity under A6.** Frozen before any trial: the control is the ballast
  process's own shared usage in the same run, the margin is 16 MiB above it
  for peak shared and peak fresh non-local bytes, and the peak dedicated
  figure plus 2 GiB must fit the tier. Every clean retained trial exceeded
  the control by exactly 2 MiB. By the reviewing agent's arithmetic on
  retained receipts, the rule would admit Compact at 24 and 12 GiB and Large
  at 24 GiB. The maintained offline reclassification is being written, and no
  live trial has run under the rule.
- **The near tie.** On TRAIN_Q543 the candidate, the matching reference and
  the publisher reference keep the same order with margins 0.014313, 0.014381
  and 0.014355. The winner is Sufficient, which the generic Read rule cannot
  accept, so the decision is to abstain in both precisions.

Step 5 detail. `GpuPrecisionPreference` is serialized with `auto`, `fp32` and
`fp16` values, the provider type lists CUDA, DirectML and Core ML, and the
catalog still carries FP32 local-test Context CPU profiles. Each change to a
wire or persisted shape needs its recorded decision first. Status must say
that a portable-graph profile stores FP16 weights and computes in FP32. For
per-result disclosure, use the existing provenance and work-accounting fields
where they already carry the route, model, tier and budget. A new result
field needs its own recorded decision, because the recorded read-parity
decision keeps the Search and Answer schemas unchanged and places device
status in diagnostics. The DirectML and Core ML provider values stay in the
type until a recorded decision removes them. Build nothing new on them.
`allow_gpu` now defaults to off in `service_resources.rs` (commit
`0b5fe97c`). The stored configuration gained one optional field,
`gpu_opt_in`. An enabled value without it was written under the earlier
default: it reads as off with `gpu_opt_in: "default_reset"`, and nothing is
rewritten until the next accepted change. An accepted request that states
`allow_gpu: true` stores `gpu_opt_in: "user_choice"`, and that value survives
restarts. Either answer through the service-wide setting clears the notice.
The installer carries stored bytes unchanged. Tested with literal bytes in
Core, real files in Host and a restarted service process. The notice, a
"Keep GPU off" action, the VS Code row and the generated `GpuOptIn` type are
in place, and the Linux workflow has a successor that expects the new
default. Showing the notice issues no call beyond the page's existing
read-only resources read. Reading the Admin controls must still not
initialize a GPU or load a model.
[Service resource configuration](../../crates/airk-core/src/service_resources.rs)
[Device policy source](../../crates/airk-core/src/arbiter/device.rs) ·
[Native device and profile contract](../../crates/airk-core/src/arbiter/CONTRACT.md)

Steps 6, 11 and 12, the WebGPU probe. Build the pinned runtime with its WebGPU
option from pinned inputs. Load the Compact graph with every floating
operation on the GPU. If the portable graph does not dispatch the fused
attention kernel, use a small provider-specific graph on the same weights and
scoring contract. These pass lines are fixed before the probe runs:

- **Correctness.** Scores at 32 and 8,192 tokens and on the hard inputs are
  within the route's drift limit of the reference, with the same winner.
- **Attention.** At 8,192 tokens the optimized graph and the dispatch record
  show the fused attention kernel running and no sequence-squared attention
  tensor.
- **Memory.** At 8,192 tokens Compact fits a 16 GB card and Large a 24 GB
  card. Those are the user's targets, and the product states them as its
  requirement. Fit means the measured peak plus the 2 GiB reserve is within
  the card's memory. The test devices hold 32 GB, so this line is what speaks
  for smaller cards.
- **Latency.** A warm 8,192-token forward meets 120 seconds. The rented runs
  use the 20-sample protocol.
- **Speed, on the A6000 smoke test.** The warm 8,192-token Compact forward is
  within five times the CUDA candidate's on the same card and input. The last
  CUDA fixed20 p95 was 1.7721 s, which puts the line near 8.9 s. Measure the
  CUDA figure again in the same session. If this line fails, the probe stops
  and no host is rented.

If the correctness line passes and the attention, memory or latency line
fails at 8,192 tokens, repeat those lines at 2,048 tokens. On a pass there,
WebGPU serves the fast tier on that platform. Whether Auto then prefers the
GPU fast tier or the CPU full tier is decided when that outcome occurs.

The first measured iteration ran on the A6000 through Direct3D 12 on
21 November, with the prebuilt plugin and probe variants of the Compact
attention graph at 8,192 tokens. Memory is the peak above the adapter's
baseline.

| Configuration | Peak memory | Warm forward | Difference from FP32 |
| --- | --- | --- | --- |
| FP32 graph, default buffer cache | 20,997 MiB | 5.12 to 5.23 s | 0.004486 |
| FP32 graph, `storageBufferCacheMode=simple` | 10,797 MiB | 5.08 to 5.11 s | 0.004486 |
| Explicit attention bias removed | 10,537 MiB | 4.37 to 4.38 s | 0.004486 |
| Also half weights, expanded again at load | 9,958 MiB | 4.37 to 4.38 s | 0.004494 |
| Also half weights kept resident | 7,160 MiB | 4.37 to 4.40 s | 0.004494 |

The last row meets the memory target: 7,160 MiB plus the 2 GiB reserve is
9,208 MiB, inside a 16 GB card. Its warm forward is about 2.5 times the CUDA candidate's 1.7721 s. The
winner matched in every run. Keeping the weights resident needs two runtime
passes disabled, `ConstantFolding` and
`FuseFp16InitializerToFp32NodeTransformer`. The default cache mode and
`lazyRelease` were worse than `simple`, and `disabled` was worst. The same
attention graph on the CPU provider at eight threads differed from FP32 by
0.000191, so the 0.0045 is specific to the WebGPU arithmetic and that route
keeps the 0.007 limit. These are probe variants. Step 3 carries the three
changes into the exporter's portable graph. The prebuilt plugin package is
enough for the probe, and a build of the runtime with WebGPU is needed only
for the product.

The fourth measured iteration, on 21 November, scored the exported Compact
graph on the 100 frozen rows and then examined the buffer cache. The rows are
1,263 to 1,480 tokens with 77 distinct lengths.

| Provider | Largest raw difference from the calibrated identity | Median | 95th percentile | Winner changes |
| --- | --- | --- | --- | --- |
| CPU at eight threads | 0.000519 | 0.000130 | 0.000252 | 0 of 100 |
| WebGPU on the A6000 | 0.001633 | 0.000492 | 0.001038 | 0 of 100 |

No row's reference margin is below twice its difference; the smallest margin
on the panel is 0.054. The panel has no long rows, so it adds nothing at
8,192 tokens. Normalized-mass error was not computed.

The memory figures of the earlier iterations hold for one input length.
Under `simple`, adapter memory rose by 558 to 786 MiB for every token count
not yet seen in the session and never fell, and the 100-row session reached
44,739 MiB. Under `bucket`, memory stayed flat across those lengths but the
8,192-token peak more than doubled:

| Model | Cache mode | Peak above baseline at 8,192 tokens | With the 2 GiB reserve | Target |
| --- | --- | --- | --- | --- |
| Compact | `simple`, one length | 7,168 MiB | 9,216 MiB | 16 GB: fits |
| Compact | `bucket` | 16,035 MiB | 18,083 MiB | 16 GB: misses |
| Large | `simple`, one length | 15,300 MiB | 17,348 MiB | 24 GB: fits |
| Large | `bucket` | 28,864 MiB | 30,912 MiB | 24 GB: misses |

So neither mode meets the targets for varying lengths as measured.

The fifth measured iteration, on 21 November, padded each input on the right
to a short ladder of fixed lengths under `simple`, which bounds the number of
buffer sizes. In both exported graphs the attention mask feeds one `Shape`
node only, every attention node is causal with no bias and no past state, and
scores are gathered at the decision position, so pad positions cannot reach
the scores.

- On CPU at eight threads, padded and unpadded scores are bit-identical.
- On WebGPU, the 100 frozen rows padded to 2,048 tokens differ from the
  unpadded run by at most 0.000278 with no winner change, and memory stays
  flat at about 4.76 GB. Changing the pad token changes nothing, so the
  difference is not leakage from pad positions. It follows the unpadded
  length modulo 64, and its cause is not established.

| Model | Ladder under `simple` | Peak above baseline | With the 2 GiB reserve | Target | Warm at 8,192 tokens | Times CUDA |
| --- | --- | --- | --- | --- | --- | --- |
| Compact | 512, 1,024, 2,048, 4,096, 8,192 | 9,708 MiB | 11,756 MiB | 16 GB: fits | 4.04 to 4.06 s | 2.3 |
| Compact | 2,048, 8,192 | 8,017 MiB | 10,065 MiB | 16 GB: fits | 4.10 to 4.13 s | 2.3 |
| Large | 512, 1,024, 2,048, 4,096, 8,192 | 18,647 MiB | 20,695 MiB | 24 GB: fits | 10.37 to 10.38 s | 2.2 |
| Large | 2,048, 8,192 | 15,188 MiB | 17,236 MiB | 24 GB: fits | 10.19 to 10.31 s | 2.1 |

The order in which lengths arrive does not change the peak. `lazyRelease`
and `disabled` fill the 48 GB card on Large at 8,192 tokens and are not
candidates. A padded forward costs what an input of the padded length costs,
so the five-step ladder bounds the extra time near 2.2 times, while a
two-step ladder makes a 2,049-token input pay for 8,192. The working
configuration is therefore `simple`, half weights resident, and the
five-step ladder for both models. The product applies the padding in a feed
step of the WebGPU scorer and not in the input compiler, whose source is a
digest input. Still unmeasured: a real 16 GB or 24 GB card, Radeon, Vulkan
and Metal, Large on real rows, a long mixed-length soak and the product
path.

One consequence for the machine itself. The unpadded session that reached
44.7 GB pushed the desktop's own GPU memory out of dedicated memory. Until
the desktop repaints, CUDA's free-memory estimate and the adapter's differ by
about 410 MiB in the opposite direction from before, and the CUDA capacity
trials' ballast cannot establish a tier.

The A6000 run is a smoke test and decides only whether to rent. Adoption
under A3 needs passing runs on the Radeon card under Windows and Linux and on
the rented Mac. Stop after one build attempt and one corrective attempt per
stage. A platform where the probe fails stays on the CPU ladder. No native
Radeon route is pursued, and a native macOS route would need a new decision.
[Planning targets](final-plan.md#future-gpu-release-precision) ·
[macOS addendum](final-plan.md#addendum--datagram-macos-qualification-weekend) ·
[Radeon addendum](final-plan.md#addendum--experimental-radeon-fp16-qualification)

Step 8 detail. A pair with identical delivered evidence is concordant under
any judgment, so only discordant pairs and the FP32 success rate need
judgments. A person makes those judgments: the user, unless the user names
someone else. The packet for each discordant pair holds the request and the
frozen source evidence from both arms in random order, with no model output,
arm name or precision. An agent prepares the packet and never fills it in,
because agent judgments are not gold. As a planning approximation that ignores clustering, with baseline
success `p`, discordance `d` and `n` pairs, the lower bound is near
`0.02 × p − 1.645 × sqrt(d / n)`. With about 200 cases and a baseline near
0.8 it clears zero only when discordance is below about 2%. Estimate
discordance on the development slice before spending the held-out set. The
routes differ from FP32 by weight rounding and small kernel differences, so
discordance should be near zero. If the development slice shows more than
about 2%, find where it concentrates by stage, width and near tie before
requesting any judgment. Discordance that high points to a defect, not to
precision loss.
[Active source cohort](software-v2-data-audit.md#active-source-cohort-after-the-aws-exclusions)

Steps 2 and 10, Linux, from a read-only review on 21 November. Four things
stand between the current state and a Linux measurement:

- The storage gate. Resolved on 21 November: a successor volume policy
  applies the recorded C: allowance, D: budget, free floor and the retained
  guest and VHD caps to the GNU gates (commit `dee76ef0`). A boot admission
  wrapper started the campaign instance under it, and the read-only inspect
  gate passed, with no VHD growth. The guest has 8 physical cores, the pinned
  driver libraries match, and a read plus hash from the Windows drive runs at
  about 130 MB/s: 25.5 s for Compact's weights and 62.3 s for Large's. Each
  gate's reservation is also its ceiling for one run (128 MiB for Clippy and
  512 MiB for the others), which a first compile of current source will
  probably exceed.
- Linux cannot score a CUDA model with current source. The GNU selectors only
  load, the reviewed Linux CPU-bounds table is empty, and Large scoring is
  refused on GNU. The source that holds the GNU selectors has never been
  compiled on GNU.
- About 7.33 GB of VHD growth and 11.8 GB of guest space remain under the
  retained 24 GiB guest and VHD caps. By retained measurement the step 2 work
  fits once and in one order: build the service pair, then copy the Focus and
  Compact models, leaving about 2.2 GB of VHD. A rebuild after a fix, an
  optimized build, packaging, and Large in the guest would all be refused,
  and VHD growth cannot be reclaimed. Raising those caps is the user's
  decision, and no Linux build starts before it.
- No Linux end-to-end latency harness exists, the committed catalog has no
  8-core or portable-graph profile, and the resource setting caps inference
  threads at 4. Step 2 on Linux therefore depends on step 5's ladder
  profiles.

The campaign distribution is `Ubuntu` with user `tyler`. A bare `wsl`
command starts a different distribution as root and must never be used.

Step 11 detail. The Neo needs no large GPU for this. It proves the build, the
pinned inputs, WebGPU start-up on Metal, operator coverage and half-precision
support on small graphs, attention kernel dispatch at short sequence lengths,
numerics at small widths, Focus on CPU, and the batch scripts with their
receipt export. It cannot prove Compact or Large at a useful width, memory
fit, latency, or behavior at 8,192 tokens, and a kernel that dispatches at a
short length must be retested at full length on the rented Mac. The build may
be slow or memory-tight on 8 GiB. Verify the SSH account against the Neo's
logged-in user before each batch, keep evidence outside the target, and clean
the target after every batch.

Step 12 detail. Rehearse each batch on local hardware first, so rented time
goes to measurements. A batch runs the model-free memory control, the CPU
ladder tiers, the WebGPU probe at 32 and 8,192 tokens, the frozen rows and
confirmation set, and the four memory figures. Run Compact first and Large only if step 9 admits it. Copy
the receipts to the campaign evidence root before the host is released, and
scope every claim to the tested system, GPU, driver and memory.

## 5. Not doing

- Half arithmetic on any provider, and any further kernel parity work.
- A live calibration capture per route. The offline confirmation in
  acceptance rule 2 takes its place.
- Separate Core ML, DirectML and MIGraphX recipes. A platform where WebGPU
  fails runs on the CPU ladder.
- Large at full width on CPU, and Large on any route beyond Windows CUDA
  before step 9 admits it.
- A native Linux host beyond the Azure VM as a release gate.
- FP32 weight files and an FP32 GPU choice in Auto.
- Terminal classes and richer answer generation.
- Any change to MiniLM vectors or identity.
- A dedicated-only memory claim on Windows.
- New labelled multilingual or code-ranking sets for specialization.

## 6. Specialization and MiniLM

**Specialization (P7).** Start after the second milestone, when the pipeline
can qualify a weight file end to end. A specialized model is a new FP16
weight file. Graphs, runtimes and routes do not change. Four rules protect
existing abilities. The original weights stay selectable and qualified.
Training includes general replay, and the general-document regression panel
must not regress under the frozen rules. On unlabelled general, multilingual
and code pairs, the specialized weights must stay close to the original
model's own scores and ranking, which needs no new labels. A new weight file
receives its own calibration and judged comparison, and then reaches every
route through acceptance rule 2. The first candidate is the existing Focus
pilot checkpoint, which is trained and not yet compared. Adapt Compact or
Large only where the stage comparison shows a specific failure they can
learn to fix, and after the data acceptance of the parent plan's step 8.
[Frozen acceptance rules](final-plan.md#6-frozen-acceptance-rules) ·
[Weights rationale](weights-v2-plan.md)

**MiniLM (P8).** Keep the bundled INT8 encoder, its vectors and its identity.
After the second milestone, run one bounded examination with unchanged
weights: exact against approximate search on the same vectors, the chunk and
title recipe, and hybrid fusion. A change that alters vectors needs a new
embedding identity and a reindex, and so a separate decision.

## 7. Evidence this plan rests on

| Evidence | What it establishes |
| --- | --- |
| Windows width controls | Half-operand recipes exceeded 0.007: Compact 0.0248556137 at 32 tokens and Large 0.011738 at 32 tokens. The passing recipes compute in FP32 on half-stored weights. Large reached at most 0.00038146973 at 8,192 tokens. |
| Windows Compact calibration | 100 complete score vectors, 48 with both CPU references. Fixed20 warm p95 1.7721 s. |
| Windows Large | Four widths pass. Fixed20 warm p95 4.793535 s. Raw calibration needs an input selector, and no capacity trial has run. |
| Windows Compact memory pressure | Exact scores at 24, 16, 12 and 8 GiB with a 79,691,776-byte shared peak. A model-free CUDA context holding 2 GiB peaked at 77,594,624 shared bytes. The 6 GiB tier lost ballast residency. |
| CPU at 8,192 tokens | The FP32 Compact graph measured a 20-sample p95 of 86.049136 s under 16 GiB at the CPU4 class. The FP32 Large score did not return within 120 seconds. |
| CPU at 2,048 tokens | Earlier limits measured Compact at about 19.5 s and Large at about 23.1 s. Historical, on earlier artifacts. |
| Focus on CPU at 1,024 tokens | 14 of 14 winners matched. Matching error 0.000019073486328125. Publisher difference 0.2344675064086914, inside the 0.25 approximate-export limit. Per-pair 20-sample p95 4.280648 s and 2,936,418,304 bytes peak commit. An engineering run scored 14 pairs in 59.14 s. |
| Focus benefit | Unmeasured on delivered evidence. The one retained signal is exploratory: instruction selection moved macro-component AUROC from 0.480313 to 0.554258 on three development clusters, which does not establish significance. |
| This workstation's CPU | A Threadripper PRO 5975WX with AVX2 and no AVX-512, so no half arithmetic instructions. |
| FP32 baseline | Width fixtures, Resource20 and the publisher reference for 48 Compact calibration rows. No FP32 result for the held-out cohort was found in the ledger. |

[Receipts](../simplify/one-process-ledger.md) ·
[Shared-memory observations](../../scripts/cuda-component-execution/CAPACITY-SWEEP.md#windows-shared-memory-observations) ·
[Earlier limits](knownissues.md#4-limits-were-raised-and-no-evidence-exists-at-the-new-values) ·
[CPU placement checkpoint](../agent-current-work.md#windows-cpu-fp16-operator-placement-20-november-2026-at-548-pm-denver-time)

### Pinned ONNX Runtime 1.30.0

The static Windows runtime compiles ONNX Runtime 1.30.0 at upstream commit
`f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7`. These findings were read from
that pinned source tree on 20 November. Three files (`mlas.h`,
`cpu_execution_provider.cc` and `halfgemm.cpp`) matched the `v1.30.0` tag
byte for byte through a public mirror. The rest were read from the pinned
tree only. Nothing in this subsection was built or run. Paths are relative
to the `onnxruntime/` source directory.

1. **No half matrix arithmetic on x86-64.** `core/mlas/inc/mlas.h` lines
   107–127 and `core/providers/cpu/cpu_execution_provider.cc` lines 30–32,
   3759–3795 and 4090–4099 compile half `MatMul` and `Gemm` kernels only for
   ARM64 and RISC-V. `core/mlas/lib/halfgemm.cpp` lines 51–79 return false
   from both runtime checks elsewhere.
2. **The runtime already converts to FP32.**
   `core/optimizer/insert_cast_transformer.cc` wraps half nodes in FP32 casts
   and removes adjacent inserted casts.
   `core/optimizer/fuse_initializers_transformer.h` expands their half
   weights at load. The half attention kernel that x86-64 does register
   upcasts internally (`core/providers/cpu/llm/attention.cc` lines 120–191).
3. **Constant folding has a size limit.**
   `core/optimizer/constant_folding.cc` lines 147–149 refuse a folded output
   above 1 GiB by default.
4. **A WebGPU provider is present.** The build has an
   `onnxruntime_USE_WEBGPU` option. `cmake/deps.txt` pins Dawn with Direct3D
   12 enabled on Windows and Vulkan as an option, and the provider options
   select either backend. Metal on macOS is the expected route and was not
   confirmed in this reading. The runtime's documentation names Metal as a
   backend and describes a prebuilt plugin package for the provider. Step 0
   installed that package and ran it on Direct3D 12. The provider supports half tensors through the
   shader half feature and has kernels for `MatMul`, `Gemm`, `Softmax`,
   `Cast`, `Gather`, `Slice`, `Concat`, `Where`, `Range`, normalization and
   rotary embedding. `contrib_ops/webgpu/bert` holds `GroupQueryAttention`
   with a flash attention path that includes long-prompt prefill. That path
   writes the present key and value outputs, so the graph may have to declare
   them for the kernel to dispatch.
5. **Other GPU providers.** DirectML registers half `MatMul`, `Gemm`,
   `Softmax`, `Cast` and attention operators. The tree has no ROCm provider,
   and MIGraphX converts a compiled program to FP16 itself. The Core ML
   provider accepts half tensors only in ML Program format and has no
   attention operator builder.
6. **4-bit weight tooling is present.**
   `python/tools/quantization/matmul_nbits_quantizer.py`, the CPU
   `MatMulNBits` kernels and their AVX2 and AVX-512 routines.

Among x86-64 processors, half arithmetic instructions (AVX512-FP16) ship on
Intel Xeon from Sapphire Rapids, and the pinned runtime uses none of them.
[Intel AVX512-FP16 guide](https://builders.intel.com/docs/networkbuilders/intel-avx-512-fp16-instruction-set-for-intel-xeon-processor-based-products-technology-guide-1710148129.pdf)
