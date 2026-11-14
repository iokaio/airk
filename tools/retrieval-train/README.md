# Developer model preparation and evaluation

These tools prepare the reviewed licensed human corpus, train MiniLM, export
the four actual optional trained readouts, and support native and downstream
goal evaluation. Raw sources, weights, fixtures and results stay private under
`artifacts/advanced-split/models/`. They never automatically install or promote
a production model. Native service, human held-out quality, resource, recovery,
platform and package qualification remain separate gates.

The original diagnostic pilot below measures the existing verified MiniLM INT8
profiles on public AIRK source bytes. Its labels do not qualify as human gold.

`pilot.json` contains 24 original agent-authored questions and manually inspected
evidence anchors over eight files at commit
`f43511506b0bed6436f97aaf35482da8a331da1e`. Source hashes, Apache-2.0 provenance,
authorship and pending human adjudication are explicit. Agent-authored labels
are model/teacher labels; source review by another agent does not turn them into
human gold. They are unadjudicated diagnostics, ineligible for training,
evaluation or final model promotion. The three task groups
are developmental train/development/calibration labels within one repository.
They provide no repository-disjoint final holdout and must not be promoted as
the plan's final A0 corpus. Two questions require complementary evidence roles.
Expected roles/anchors were authored before seeing retrieval scores.

The pinned Windows x64/Python 3.13 environment reuses existing approved package
versions and installs only into ignored `artifacts/`. Wheel hashes and retained
rights artifacts are written to its environment receipt; installs use an offline
wheel directory, generated exact hash lock and `pip check`. Tokenizers 0.21.4's
wheel omits its license file; `license-supplements.json` binds the retained
Apache license from the exact upstream release commit. No product package
includes this environment. Other hosts need their own wheel-closure qualification.

```powershell
python tools/retrieval-train/provision.py --destination artifacts/advanced-split/models/baseline-env --download
# Later runs install/verify from retained wheels without network:
python tools/retrieval-train/provision.py --destination artifacts/advanced-split/models/baseline-env
python -m unittest discover -s tools/retrieval-train -p 'test_*.py'
artifacts/advanced-split/models/baseline-env/venv/Scripts/python.exe tools/retrieval-train/baseline.py --model-dir target/local-vector-model/minilm-l6-v2-u8-v1 --output artifacts/advanced-split/models/pilot-u8-r1.json
```

The baseline reads source only through the pinned Git revision and refuses hash
drift or missing evidence anchors. Its fixed line chunks differ from native
extraction. Every inference is batch-one, CPU-only, one thread, 256 tokens and
384 normalized dimensions; per-chunk truncation is disclosed. Stable source
location/hash and complete selected top-ten hit identities/scores are retained.
Tokenizer and inference sessions load retained, verified bytes, and reports
seal the manifest and every model asset through completion. Provisioning seals
its source, requirements and license supplements, and rejects extra or changed
packages in a reused environment. Reports preserve selected task count, exact model metadata and package versions.
Output paths are private fresh artifacts; existing reports are never overwritten.

The original pilot remains a diagnostic with unadjudicated expected labels; it
is not the human evaluation corpus. The subsequent authorized campaign uses
the separately reviewed licensed human annotations and frozen acceptance rules
in `a0-freeze.json`. Private inputs, weights and reports remain under artifacts.

`train_encoder.py` keeps the original `paragraph-prefix-v1` recipe by default.
The explicit `--training-recipe visible-prefix-mining-v1` development ablation
mines unjudged lexical distractors from the exact query/passage prefixes visible
to its configured256-token tokenizer. It preserves positive annotations, the
licensed TRAIN-only pool, tokenizer state, loss and optimizer. This mode filters
TRAIN task/document scope before deserializing text; it never admits Hotpot,
development or held-out text for gradients or mining. Its version2 private
receipt names the changed recipe. Existing checkpoints and receipts stay intact.
TRAIN token/crop audits and synthetic regression passes do not establish model
quality. A candidate needs a sealed recipe, actual training/export, matched native
DEV measurements and the unchanged independently reserved final gates.

An isolated development runtime can exercise Qwen3.5's built-in CPU operator
path without changing AIRK's production runtime or downloading candidate weights:

```powershell
python tools/retrieval-train/provision.py --profile ort130 --destination artifacts/advanced-split/models/ort130-env --download
artifacts/advanced-split/models/ort130-env/venv/Scripts/python.exe tools/retrieval-train/ort130_probe.py --output artifacts/advanced-split/models/ort130-operators-fresh
artifacts/advanced-split/models/ort130-env/venv/Scripts/python.exe tools/retrieval-train/ort130_shapes.py --output artifacts/advanced-split/models/ort130-candidate-shapes-fresh
```

The first probe checks eight small gate, convolution, recurrent carry and toy
pointer cases against independent NumPy expectations. The second checks FP32
recurrence at the four retained publisher configuration shapes, including the
16-key/32-value-head grouping. It requires the pinned public metadata snapshots
under private `artifacts/advanced-split/models/publisher/`. Both retain their
inputs/results and refuse source drift. These are operator checks, not complete
hybrid architecture, real trained head, BF16, model export or service qualification.

`development-adoptions.json` records exact reviewed paper hashes, attribution,
license/version and permitted scope. `qasper_development.py` materializes the one
approved QASPER development view from the privately retained verified archive and
arXiv metadata. It preserves all original human annotation alternatives and the
publisher validation split, excludes quote-bearing passages and figures/tables,
and reports unresolved alignment and disagreement. It does not invent consensus,
training permission or final evaluation gold. Its output and original annotations
remain private under `artifacts/`.

The subsequent developer tools implement the actual training/export path:

- `human_corpus.py` retains original QASPER/Hotpot human alternatives and exact
  rights decisions. Hotpot is evaluation-only and never feeds weight training.
- `materialize_sources.py` preserves whole-source paragraph order, original byte
  mappings and pre-score goal IDs. `goal_rubric.py` projects original answers and
  evidence to exact source spans; its utility labels are mechanical projections,
  not human judgments of AIRK actions, authority, gaps or sufficiency.
- `train_encoder.py` trains the MiniLM encoder using only approved train rows;
  `encoder_development.py` selects the checkpoint on development rows.
- `export_encoder.py` creates explicit mixed U8/S8 weight profiles with six
  protected FP32 FFN output projections and FP32 embeddings. `safe_int8.py`
  preserves U8 quantization exactly in S8 storage and lowers saturating AVX2
  products to standard ONNX low/high integer products. This is mixed precision.
- `verify_encoder_inputs.py` compares the trained FP32 and INT8 vectors using the
  live native B1 padding buckets. `qwen_export.py`, `verify_export.py` and
  `consolidate_onnx.py` export and independently compare all four actual optional
  trained readouts, including full pointer heads and adapters.
- `compile_reference.py` retains original hostile caller inputs alongside exact
  expected compiler vectors. `resource_probe.py` records developer CPU prefill
  observations; `eval_agent_smoke.py` checks the fixed local answering model.
- `prepare_encoder_baselines.py` preserves exact shipped v1 assets for controlled
  ORT1.30 evaluation. `export_encoder.py --baseline-original` applies the identical
  new mixed recipe to exact original untrained weights. These separate training
  effects from precision/runtime changes; neither enables production v1 execution.
- `native_goal_client.py` retains complete native requests/results through the
  original-instance persistent W4/I2 session and checks actual helper/child exit.
  `goal_sources.py` authenticates original source/citation bytes and publication;
  `goal_trial.py` drives equal-budget local-agent followups and same-alternative
  human grading. `calibration_labels.py` labels actual native counterfactual reads
  without leaking another choice's observation or inventing human action labels.
- `split_model_assets.py` creates private ordered parts no larger than1GiB and
  verifies their complete logical digest. It emits developer evidence, not a
  handwritten native download schema, and never hosts or uploads payloads.
- `aggregate_native_retrieval.py` requires all six complete native family
  receipts, the same frozen 205 questions, model identities, source closure and
  query budget before human grading. It preserves original raw fields and
  artifact identities. Legacy retrospective source bindings remain explicitly
  retrospective; they never manufacture a before-run source seal.
- `audit_sources.py` checks the complete licensed source roster and hashes twice
  with actual UTC observations. Its requested phase label does not establish
  timing relative to a different process. `grade_native_retrieval.py` grades
  actual returned byte ranges against original alternatives, never whole-paper
  membership or unions of different human alternatives.
- `goal_summary.py` accounts for all fixed-population trials and reports both
  paired controls. The primary turns gate uses the baseline selected only by
  baseline completion, then its own completed-task turns, then the frozen
  deterministic tie break; an empty common-success set cannot pass.
- `run_goal_condition.py` runs one fixed development/final condition with a
  persistent local answering model. It consumes actual native fixture Ready,
  native-emitted index bindings and a Rust-emitted Core request instance; it
  never invents setup, index IDs or model readiness. It retains full raw calls,
  all model turns, actual shown/cited byte ranges and complete source/input
  seals, stops infrastructure failures and requires a fresh private output.
  `--profile-manifest` is the native manifest digest, not a profile display ID.
  Configure the actual resident fixture through native management before each
  condition. A passing driver test or smoke does not qualify the full comparison.

`context_action_facts.py --protocol <private-protocol.json> --protocol-sha256 <sha256> --output <fresh-private-output>`
prepares data-only publisher facts from pinned inputs. Its protocol supplies input
paths and hashes; the repository contains no human answers or private protocol.
The producer inspects structural split metadata before decoding text: only approved
QASPER TRAIN and the exact CAL question roster enter the output. It rejects paper
family overlap, ambiguous selectors and changed source/rights hashes. All Hotpot,
TEST, developer evaluation and other closed populations remain excluded. Original
human alternatives stay separate, and original answers are archived separately.

Reference completion and a useful support addition are mechanical projections of
original annotations, not new human action, gap, role or sufficiency judgments.
Full-paragraph controls do not certify cropped native evidence windows. The native
owner must validate actual returned spans and every whole-card hypothesis. Requests
is one TRAIN-only source family: pinned archive/LICENSE/NOTICE bytes and read-only
AST/doctest parsing support structural predicates; source and examples never run.
There is no independent structural CAL population here, so those classes remain
unqualified. Clarification, counterevidence and general sufficiency are unsupported.
No model, native request/roster, prompt, tensor or qualification is produced. Run
`python -B tools/retrieval-train/test_context_action_facts.py` for seven synthetic
split, annotation, rights/source and structural-predicate controls.

The baseline, goal, source-presentation and resource freeze JSON files preserve
the approved before-score rules. The additional CPU8 proposal is a distinct
hardware profile; it does not waive failed CPU1 measurements or transfer
qualification. `row_quant.py` and `signed_dynamic.py` remain explicitly
experimental alternatives, with no admitted-profile or production claim.

These tools produce development evidence. They do not establish native service,
human held-out quality, resource, recovery, platform, package or release
qualification. Candidate catalog changes and actual native qualification have
their existing Rust owners. Preserve failed reports and never replace a failed
candidate silently. Run the focused tool tests in the pinned training environment
so ONNX-dependent integer extrema checks execute rather than skip.
