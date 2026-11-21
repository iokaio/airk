# Windows scientific CAL readout

## R8 complete calibration inputs

Run `python -I -B -S r8_chronological_inputs.py --plan-sha256 <R8 plan SHA>
--lifetime-sha256 <lifetime 00 SHA> --lifetime-sha256 <lifetime 01 SHA>
--terminal <outer SHA> <fixture SHA> <mutex release SHA> --output <empty checked
campaign child>` after both R8 lifetimes close. Use the Windows execution guard
before admitting the output directory. The CLI verifies the complete original
capture once, then writes the independent `reference-preflight.json`, both
model compiler inputs, and all 13 linked phase reports. The final
`projection-receipt.json` binds those results to the exact R8 plan and source.
The projection executes no model forwards. For the compiler-only check,
use `run_compiler_batch.py` with the existing verified image, the two model
files, and `--launch-audit <path> <SHA>` from the projection receipt. The
existing single-child Job and selector verify both original compiler inputs.

## R7 chronological original inputs

Run `python -I -B -S r7_chronological_inputs.py --plan-sha256 <R7 plan SHA>
--lifetime-sha256 <lifetime 00 closure SHA> --output <empty checked campaign
child>` in the verified Windows shell after the first native lifetime has
closed and while the original R7 parent still owns the installation. The
adapter authenticates the optimized service audit, R6 retirement, R7 plan,
closed seven-phase Job and journal chain, and each released native W4 grant.
It keeps the two original model input censuses, ordered cards, captured
requests, refusal positions, and remaining allowances.

For all 100 observations, pass a second `--lifetime-sha256` for lifetime 01
and `--terminal <outer SHA> <whole fixture SHA> <mutex release SHA>`. Whole
capture verification requires natural closure of both lifetimes and all 13
phase Jobs, and the original shared clock and mutex release. Use a fresh
output directory for each projection.

The output consists of linked phase reports, `compact.json`, `large.json`
and `projection-receipt.json`. The model JSON files use the original native
chronological compiler input shape with R7's exact audit and plan digests.
They are ready for the two-model compiler selector on a verified image built
with the candidate-audit support. Native compiler execution, raw GPU/CPU
scores, human-reference certificates and fitting have their own receipts.

Use `run_r7_compiler_batch.py` with the exact projection path and SHA, the
native inference test executable and SHA, and the image build, Cargo log and
outer build Job SHA values. Preflight a fresh D: compiler output directory
and a sibling scratch directory with the same name plus `-temp`, then point
`TEMP`, `TMP` and `TMPDIR` to that scratch directory. The original two-model
selector runs in a 2 GiB Job with a 120-second limit and emits `native.log` and
`native.json`. Its report retains the R7 projection, exact executable and
compiler custody, both model counts and zero inference invocations.

## R5 chronological original inputs

After R5 lifetime 00 closes naturally, run `r5_chronological_inputs.py --lifetime-sha256 <first closure SHA> --output <empty checked campaign child> --compile` with the checked Windows Python in isolated, no-site, no-bytecode mode. The producer independently verifies all seven phase receipts, the original launch/plan, live supervisor and held installation mutex. It writes both original model batches, including all refusals, and runs the preserved CPU-only native compiler selector against 56 requests. `projection-receipt.json` pins both model files and the completed compiler Job. A directory without this final receipt is incomplete.

For 100 requests supply both `--lifetime-sha256` arguments in order and `--terminal <outer SHA> <whole fixture SHA> <mutex release SHA>`. The producer verifies the entire original fresh R5 chronology before the two-model compile. Readouts and gain certification have separate owners. The R4 producer below retains its original input identities.

## Chronological original inputs

Run `python -I -B -S chronological_inputs.py --model compact --phase <absolute-phase00-receipt> <independently-verified-SHA256> --ordinal 0`. Supply each subsequent closed R4 lifetime with another `--phase <path> <SHA256>`, in original phase order. Select `large` for the independent original Large compiler census. Without `--ordinal` the command reports each status count and the first eligible ordinal. With an ordinal it retains the original request's capture pins, complete native compiled tensors, ordered action cards and remaining token and time budgets. Direct stdout only to a checked private file. Inputs from a future grouped capture require independent authorization of its actual closed group before they can use this path.

To recompile a selected eligible row, pin its exact stdout file in `AIRK_CUDA_CAL_CHRONOLOGICAL_INPUT` and `AIRK_CUDA_CAL_CHRONOLOGICAL_INPUT_SHA256`, and set the original reviewed `AIRK_CUDA_CAL_LAUNCH_AUDIT` and `AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256`. Select the ignored native test `chronological_original_context_compiler_row` from the matched scientific inference image. It rebuilds the original compiler from the audited model's actual tokenizer and compiler assets and compares the entire input, roster, deadline and token budget against the original observed capture. This is a zero-forward Windows compiler check.

To run a closed prefix in one native process, prepare `chronological_inputs.py --model compact --batch` and the corresponding `--model large --batch` with identical ordered `--phase` receipts. Pin both output files via `AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT`, `AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE` and their `_SHA256` companions. The ignored `chronological_original_context_compiler_batch` checks the same phase roster, both original compilers, the full prefix in original request order, all eligible inputs and the retained refusal counts. The complete fit still requires the certified original population. Run the source-sealed image once via `python -I -B -S run_compiler_batch.py --image <absolute verified executable> --image-sha256 <digest> --compact <Compact batch path> <digest> --large <Large batch path> <digest> --log <fresh private log>`. Preflight the log and stdout receipt in the verified campaign child under the Windows execution guard and audit both after return. The suspended native child must match the reviewed executable, user SID and default owner. The wrapper reports one natural Job settlement, observed peak host charge and each model's actual compiler count under a 2 GiB and 120-second diagnostic budget.

Source selection does not register a precision variant. New native scoring requires its exact graph, external weights, optimizer, measured placement, independent matching arithmetic covering the selected sequence length, and verified original reference for the same model, input and card order. The historical Compact QKV integration below applies to its registered scientific control. Its FP16 scores and fit cannot be reused for another candidate.

## Historical Compact QKV control

The private CPU reference worker reads the ordered, independently verified native admissions of one closed Focus1024 CAL lifetime. Run its read-only preflight with `python -I -B -S phase_reference.py --check-only --phase-input <absolute phase receipt> --phase-sha256 <digest>`. For a reserved CPU reference window replace `--check-only` with `--ordinal <zero-based corpus ordinal> --output <fresh absolute campaign directory>`. Each 32 GiB owned Windows Job scores one eligible original input with both the authenticated publisher model and the matching QKV recipe. Each Job settles before another ordinal is scheduled. The original T128 controls and all card scores stay in custody. Per-phase references have no fit or release authority.

After all 100 original requests close, run the ignored inference selector `compact_cal100_original_compiled_input_census` with `AIRK_CUDA_CAL_LAUNCH_AUDIT`, `AIRK_CUDA_CAL_PREFLIGHT` and their `_SHA256` values. Use the qualification owner's fresh `AIRK_CUDA_QUALIFICATION_OUTPUT`. It reconstructs the original CPU compiler from its verified assets, rebuilds each complete delivered prompt and retains every original refusal. The independent archive owner takes an original compiled file pin, thirteen phase pins in order, the reviewed reference worker SHA and the completed eligible reference Job pins in ordinal order. Run `assemble.py --descriptor <absolute file> --sha256 <digest>` through the checked Python runner and retain stdout privately. It verifies the complete original native tensors and offered cards across both sources.

The Windows ignored selector `compact_qkv_scientific_cal_original_readout` uses the registered QKV unprofiled scientific scorer and actual bounded native lease for one ordinal. It consumes the ordinary reviewed QKV probe inputs plus `AIRK_CUDA_CAL_ORDINAL`, `AIRK_CUDA_CAL_PREFLIGHT`, `AIRK_CUDA_CAL_ORIGINAL_COMPILED`, `AIRK_CUDA_CAL_REFERENCES` and a matching `_SHA256` for each file. The native owner checks the entire 100-row input and independent reference coverage before dispatch, recompiles the selected original prompt, enforces the original deadline, synchronizes the forward and releases the native charge. A different candidate requires a separately reviewed native profile, graph, matching reference math and fit-consumer registration. Scientific output carries zero release qualifications.

Launch the native row through the maintained Windows owner in `scripts/arbiter-fp16-execution/probe_compact.py`, mode `qkv-scientific-cal`. Supply the identical reviewed projection-stage and graph-receipt options, plus `--cal-ordinal`, `--cal-preflight`, `--cal-original-compiled`, `--cal-references` and their SHA options. The owner verifies graph and complete measured placement, all actual GQA FlashAttention records, ordered Load/Numeric journal events, WDDM identity, original finite output, and terminal Job custody.

Use `close_scores.py --descriptor <private score job census> --sha256 <digest> --output <fresh guarded campaign directory>` to materialize the raw, owned Job and scoring closure files. Preflight the directory, all `owned-000.json` through `owned-099.json`, `raw.private.json` and `scoring-closure.private.json` through the checked Windows guard, then audit the created tree. The descriptor holds original_compiled, references, producer_binary and ordered score_jobs pins. The emitted raw and closure pins feed the scientific fit owner.
Once R5 closes, `run_original_cpu.py --descriptor <path> --sha256 <sha> --output <fresh owned campaign child>` launches the original Compact FP32 raw scorer. The descriptor binds the full R5 chronological projection (`full`), signed human reference preflight (`certificate`), and original Kernel Cargo/staging pins (`build`, `cargo_log`, `staged_images`). The launcher reopens the original 100 inputs, independently issued gain certificates and no-forward compiler receipt, preserves request/refusal order, and acquires a fresh private native service lease. Its Windows Job has the original 1,200-second wall limit and 32 GiB host cap. The selected CPU profile reserves 16 GiB and uses four physical cores. Schedule it only in an assigned CPU slot. The resulting native raw closure supports scientific fitting; the package still needs its own qualified catalog entry.

The original FP32 comparator reuses the kernel selector `selected_context_cal_complete_readout_raw_scores`. Run `close_original.py --descriptor <descriptor> --sha256 <digest>` with original_compiled, raw, owned_job and producer_binary pins after natural Job settlement. It verifies the suspended native image, original static-1024 capture, all 100 original ordinals and every eligible full-vector result. Its stdout is the private scientific CPU scoring closure. Use `python -I -B -S test_scientific.py` for zero-model corruption controls.
`original_reference.py` can extract only the publisher FP32 vector from a previously closed matching/publisher reference Job for the exact target phase and ordinal. It compares the complete source and target request, captured scope, ordered cards and compiled inputs, then preserves five original publisher file pins, producer/runtime and the checked Job. The output can supply the original vector for another candidate's independent reference producer. The different candidate needs its own matching vector and actual native score before it can enter an artifact-specific fit.
The first historical Compact QKV CPU result used the original `replay_compact_t1024_suffix.py` bytes identified by SHA-256 `2ccd33badbfddb795f34f0e52ffce0091f9290956336fc135b0657c029441ff0`. Reproduce it with the original authenticated source epoch and pinned dependencies. The launcher checks these bytes before running. A later graph or changed replay source requires its own reference producer and result.

For the protected Windows Compact graph, `probe_compact_cutlass.py --mode cal`
uses the normal authenticated native image, Cargo source snapshot and same-image
`--graph-receipt` and `--graph-sha256` options. Select `--cal-epoch r4-prefix64
--cal-phase 0` solely for the historical engineering phase. Select
`r5-first56` or `r5-full100` with the exact native `--cal-compiler-sha256`
and `--cal-projection-receipt-sha256` for fresh R5 rows. The shared launcher
retains the original 120-second bound and 32 GiB campaign host ceiling. The R5 scorer uses a stricter 16 GiB process and Job cap. Both limits are recorded. Every original
eligible forward binds the global source row to its 1-based phase-local
journal operation, an authentic owned PID, unchanged remaining allowance and
28 native efficient-attention kernel records. Rows with no request or a
compiler refusal remain in their original positions. The receipt contains raw
scientific activity and scores only. The R4 and R5 rows belong to separate
epochs, and no score from the engineering phase enters the R5 fit.

### Private Windows Compact CUTLASS Read study

After the final R5 mutex release, prepare a private JSON descriptor with
`first56`, `full` (original chronological projection receipts), `certificate`
(the independent R5 reference preflight), ordered `provider` (all 13 phase
execution receipts) and ordered `protected_matching` and `publisher_fp32`
CPU batch receipt lists covering every original ordinal. Each reference has
an absolute path and SHA-256. In the checked human shell run
`python -I -B -S scripts/arbiter/evaluation/selected_goal/cal_raw/cutlass_study_inputs.py --descriptor <absolute-file> --sha256 <descriptor-sha> --output <empty-checked-campaign-directory>`.
It authenticates both reference families, native placement and synchronized
score journals, reproduces original signed-source gains and retains the
original refusal rows and numeric comparison results.

Use `run_cutlass_study.py` with the same three arguments to execute the study.
Its descriptor wraps the original evidence descriptor as `evidence` and
supplies `build`, `cargo_log` and `staged_images` pins (original
receipt_runner Cargo JSON image build, Cargo log and matched staging list).
The launcher regenerates the input, validates the originating Kernel source and
image, runs the study in a bounded 120-second, 2 GiB Job, and emits
`execution.json` with its source pins, ordinal outcomes and Job closure.
The result reports the greatest native f32 cutoff at maximum certified coverage
with zero uncertified admissions. A null cutoff records that the evidence
supported no such admissions. Numeric qualification, semantic utility and
product activation each require their own evidence.

### R8 Compact Read study

The Kernel study accepts the completed R5 and R8 plans. One accepted plan
must name both the input and its preflight. The rule is the same for both.

`r8_cutlass_study_inputs.py` builds the study input for R8 from the retained
Compact panel. Its descriptor has three fields:

- `panel`: absolute path and SHA-256 of the R8 Compact raw panel.
- `certificate`: absolute path and SHA-256 of the R8 reference preflight.
- `provider_sha256`: the six R8 phase receipt digests, phases 07–12 in order.

The builder reproduces the panel from all 13 native phase receipts through
`compact_matching/r8_compact_panel.py`. It requires the certificate pinned by
the R8 projection receipt, and checks every row's compiled input and roster
through `compact_matching/r8_reference_inputs.py`. It reads no CPU reference
Job, so the input carries no numeric comparison.

Build and stage the Kernel test image before the study. Run scoped Clippy,
the module's unit tests with Nextest, and `cargo test -p airk-kernel --lib
--all-features --locked --offline --target x86_64-pc-windows-msvc --no-run
--timings --message-format=json`, each through `scripts/dev/receipt_runner.py
--groups native --out <checked-campaign-directory>`. Create an empty checked
`images` directory beside the receipts. Then run `python -I -B -S
stage_cutlass_study_image.py --build <image receipt> --build-sha256 <sha>
--unit <unit receipt> --unit-sha256 <sha>`. It copies the one linked Kernel
test image and writes `images/images.json`. Set the `unit_sha256` pin in
`run_cutlass_study.py` to the new unit receipt digest; the launchers accept
only that image.

`run_r8_cutlass_study.py --descriptor <absolute-file> --sha256 <descriptor-sha>
--output <empty-checked-campaign-directory>` runs the study. Its descriptor
wraps the three fields above as `evidence` and adds the `build`, `cargo_log`
and `staged_images` pins. It reuses the R5 launcher for the image check, the
120-second, 2 GiB Job and `execution.json`, and requires the R8 plan in the
native report.

`r8_cutlass_study_report.py --execution <execution.json> --sha256 <sha>`
describes a completed study by ordinal, count and margin: top-card counts,
the rank of each certified card, Read winner margins, and the best certified
coverage at 0, 1, 2, 3 and 5 tolerated uncertified admissions. It refuses to
print when its zero-tolerance result differs from the native cutoff or
admissions. The tolerated rows are descriptive and propose no rule. Redirect
stdout only to a checked private path.

Run `python -I -B -S test_r8_cutlass_study_inputs.py` for the zero-model
controls of the builder, launcher, stager and description.

Run collect_first56_gpu.py with isolated Python to reopen the seven original fresh R5 CUDA phases against their frozen 56-row projection. Its private JSON output preserves the phase pins for the full-cohort study.

After both original R5 lifetimes and all 13 GPU Jobs close, collect_full100_gpu.py --projection-sha256 <verified full100 projection hash> reopens the prior seven original phase receipts and the six terminal receipts. It checks complete first56 compiled row equality against the terminal projection, per-phase admission and operation evidence, and ordered source/card IDs. Its provider list is the input to the independent scientific study; the native raw-score collection does not fit a rule.
