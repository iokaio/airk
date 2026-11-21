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
| CPU | Windows, Linux and macOS on any hardware | The portable graph: the model's FP32 graph with its learned weights cast from the FP16 file. | The FP32 form of this graph is retained and measured. The FP16-file form is not yet exported. |
| CUDA | NVIDIA on Windows and Linux | The existing Compact and Large candidates. | Both pass the four development widths on Windows. Linux links and has not scored a model. |
| WebGPU | Radeon on Windows and Linux, Apple GPUs on macOS, and other adapters | The portable graph, with FP32 arithmetic on the GPU. | Present in the pinned runtime source. Not built or run here. Entered through a staged probe (steps 6, 11 and 12). |

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
token budget produced its evidence. No downgrade is silent. Service-wide GPU
opt-out, a missing or unsupported GPU, insufficient device memory and a
settled GPU failure all enter the ladder.

A tier is admitted on the parent protocol for the model forward: one cold
load, one warmup and 20 warm samples with nearest-rank p95, on a named CPU
class. The target class is also measured end to end (step 2). Every latency
figure states which of the two it is.

1. **Full.** The selected model at the 8,192-token budget, where its measured
   p95 meets 120 seconds. The FP32 Compact graph measured a model-forward p95
   of 86.049136 s over 20 samples.
2. **Fast.** The selected model at a 2,048-token budget. At the earlier
   2,048-token limits Compact measured a p95 of about 19.5 s and Large about
   23.1 s. Those are historical figures on earlier artifacts. Measure again.
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
   inputs. The guard band protects only while drift stays within the limit,
   and that limit is an observation on these rows. A route that breaks it on
   the confirmation set is not admitted.
3. **Judge task quality once per model.** Run the 98% paired comparison on the
   calibrated identity. Other routes inherit it through rule 2.
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
| A1 | Adopt the weight rule: nothing larger than FP16 ships, and passing integer artifacts stay. | The FP16-only wording of 20 November. | P3 names the purpose. An integer artifact serves it better than FP16. The records name the selected Focus artifact FocusW8, an 8-bit export. Confirm the 1,024-token export's format from its receipt before step 2. |
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
and run in parallel within the one-Cargo-owner rule.

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

### Steps

| Step | Work | Done when | Needs | Confidence |
| --- | --- | --- | --- | --- |
| 1 | Freeze the judged cohort. Run the first two stage arms on Windows CPU at the same evidence budget: retrieval alone and retrieval with Focus. | Focus's contribution to delivered evidence is measured, with missing evidence, duplicates and latency. | Nothing | High. |
| 2 | Deliver the CPU milestone. Install the product on Windows and on Linux under WSL2 and measure the ladder tiers end to end at 8 cores, the target class. Start with retrieval and Focus, and add the Compact tiers when step 3 delivers them. Record the four memory figures. macOS joins at step 11. | Each tier has an end-to-end figure at 8 cores on both systems, and the tiers that meet 120 seconds are admitted for the target class. | Nothing | Medium. The runtime build on each system is the risk. |
| 3 | Export the portable graph for Compact. Record the recipe in the exporter contract first. Verify shared weight bytes. Load on the static runtime with zero forwards. Score the four widths and 2,048 tokens against the reference. Time each width and record the four memory figures. | Compact has measured CPU numbers for the full and fast tiers. | Nothing | High. It reuses the retained FP32 graph and the Large weight-cast edit. |
| 4 | Finish Windows CUDA. Complete Compact matching references, fit calibration, add the Large selector and score its panel. Run capacity under A6. | Both candidates hold a calibrated panel and a memory figure. | Nothing | High for Compact, medium for Large. |
| 5 | Record the Core contract decision, then implement: Auto order, the ladder tiers and the CPU target class, typed reasons, truthful status, per-result disclosure of route, model, tier and budget, the weight rule and a WebGPU provider value. Test with injected capabilities. | Reviewed Core changes with qualification flags false. | Nothing | High. |
| 6 | Run the WebGPU smoke test on the A6000 through Direct3D 12 as soon as step 3 has a graph. It stays early by the user's decision and includes the speed line. It takes the GPU only when no CUDA run holds it. | A go or no-go for the rented runs. | Step 3 | Unknown until it runs. |
| 7 | Score the portable graph on the 100 frozen Compact rows and the confirmation set on CPU, and apply acceptance rule 2. | The CPU route is admitted or its first failing row is named. | Steps 3 and 4 | High. |
| 8 | Complete the stage comparison and the 98% comparison: the FP32 arm, the calibrated identity, and the Compact and Large arms. Compare delivered evidence first and judge only the discordant pairs. | The 98% bound and each Context model's added evidence are measured. | Nothing | Medium. Label coverage decides it. |
| 9 | Decide Large's expansion from step 8. Extend Large to the CPU ladder, Linux and WebGPU only if it delivers evidence Compact misses. Until then its CPU fallback is the Compact downgrade. | A recorded disposition for Large. | Step 8 | Not applicable. |
| 10 | Port the CUDA candidates to Linux on the existing WSL2 instance. Score widths and the frozen rows. | Linux CUDA is admitted by rule 2. | Step 4 | Medium. |
| 11 | After the Windows and Linux CUDA work is complete, rehearse macOS on the Mac Neo. Build the arm64 runtime with the CPU and WebGPU providers from pinned inputs. Confirm that WebGPU starts on Metal. Run synthetic graphs and reduced fixtures, Focus on CPU, and a dry run of the rented-Mac batch. | The build is reproducible, Metal support is confirmed or refuted, and the batch runs end to end on small inputs. | Steps 4 and 10 | Medium. Memory and disk on the Neo are the risk. |
| 12 | Run one rehearsed batch on each rented host: the Azure VM under Windows, the Azure VM under Linux, and the rented Mac. | WebGPU is adopted under A3, or the failing platform stays on the CPU ladder. Native Linux and macOS hold CPU evidence for the Context tiers. | Steps 6 and 7. Step 11 for the Mac batch. | Unknown for WebGPU, medium for CPU. |
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
pinned constant folding refuses a folded output above 1 GiB.
[Exporter contract](../../scripts/arbiter-fp16-export/CONTRACT.md) ·
[Large weight-cast edit](../../scripts/arbiter-fp16-execution/large_weight_only_candidate.py) ·
[Shared-weight verifier](../../scripts/arbiter-fp16-execution/verify_shared_onnx_weights.py) ·
[Reference contract](../../scripts/arbiter-fp16-reference/CONTRACT.md)

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
- **Memory.** The goal at 8,192 tokens is that Compact fits an 8 GB card and
  Large a 16 GB card. If the goal cannot be met, the line is a 16 GB card for
  Compact and a 24 GB card for Large, and the product states that
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
judgments. As a planning approximation that ignores clustering, with baseline
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
| CPU at 8,192 tokens | The FP32 Compact graph measured a 20-sample p95 of 86.049136 s under 16 GiB. The FP32 Large score did not return within 120 seconds. |
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
   confirmed in this reading. The provider supports half tensors through the
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
