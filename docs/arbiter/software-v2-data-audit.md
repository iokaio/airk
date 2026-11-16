# Software-v2 data and evaluation audit

Observed 15 November 2026 (America/Denver), during execution of the
[final plan](final-plan.md), Steps 7–8. This is an input audit, not a model
quality or qualification result. No new model inference, gradient step,
CAL/FINAL scoring or publication was performed by this audit.

The additive [software-v2 protocol](../../tools/retrieval-train/software-v2-freeze.json)
preregisters the approved 8192-token/120-second shared policy, measured p95
limit, unchanged quality floors, split/label acceptance, terminal refusal rules
and M1/F1/F2/C1 drafts. It explicitly remains non-executable until actual source,
rights, native-visible examples, checkpoint identities, sample/power rationale
and measured compute caps are bound. The original A0, encoder, baseline-selection
and resource protocols remain byte-identical under their historical identities.

## Available evidence

These are metadata counts from retained local inputs. They do not grant a new
use, establish current native alignment or transfer earlier model results.
The private input inventory retains complete paths and SHA-256 identities at
`artifacts/advanced-split/models/weights-v2/software-v2-inventory-r1/inventory.json`.
It contains no questions, answers, training examples or source text.

| Input | Observed scope | Acceptance limitation |
| --- | --- | --- |
| Retained human corpus | QASPER: 107 TRAIN, 14 DEV, 29 CAL, 44 TEST; Hotpot: 512 TEST; 65 QASPER source-rights decisions | Research-paper evidence and evaluation-only Hotpot do not supply the planned broad software corpus. Preserve all human alternatives and unresolved alignment. |
| Requests source/docstring pairs | 218 TRAIN-only tasks; one repository; exact Apache-2.0 source revision and retained LICENSE/NOTICE | Containing-function/docstring association is mechanical supervision, not independent human relevance, whole-card or sufficiency judgment. No independent software DEV/CAL/FINAL population. |
| Historical encoder TRAIN audit | 315 positive rows, 287 questions: 69 QASPER questions from 36 papers and 218 Requests tasks | 58 cropped positive rows, 147 cropped distractors and 82 distractors whose mined lexical terms disappeared. The old corpus does not satisfy the new software target. Paper families are not repository families. |
| Selected Context CAL reference certifier | Original-reference complement for actual canonical Read choices; no-gain observations remain UNKNOWN | Its producer explicitly sets human semantic gold count to zero and excludes terminal, gap and source-role gold. Generic Read evidence does not qualify Sufficient, Clarify or NoJustifiedRead. |
| Retained fresh Hotpot metadata audit | Before rights/alignment: 746 DEV, 731 sealed CAL, 2396 sealed FINAL; 3532 already exposed | Counts alone establish no admitted cohort. Hotpot remains evaluation-only; reserved text is not a training source. |
| Retained Context structural census | Request/card/source counts and byte sizes with earlier compiler refusals | JSON byte size is not native prompt-token count. Earlier remaining budgets and profile widths do not qualify 8192-token execution. |

The inspected sources establish no accepted 2000-query-group corpus spanning
20 unrelated software repository families, no stratified human review for that
corpus, and no class-specific terminal human adjudication. This is a scoped
finding about the retained corpus, audit and selected-driver metadata, not a
claim that no suitable public benchmark exists.

## Missing acceptance packet

Before any M1/F2/C1 gradient probe, the data owner must provide:

1. A licensed software-source inventory with exact repository/fork revisions,
   underlying file rights and retained notices; mutually assigned 40% code/tests,
   35% docs/contracts/configuration, 15% cross-file diagnostics and 10% general
   replay, with the planned language coverage and a concentration audit.
2. TRAIN/DEV/CAL/policy-confirmation/FINAL repository, issue/symbol-family and
   chronology closures, deduplication evidence and an untouched FINAL reservation.
   Prior failed, exposed or partly cancelled FINAL remains consumed.
3. Exact source spans, original question provenance and alternative coherent
   support sets, mapped to Rust-emitted visible examples. Refusals stay in the
   original denominator; invisible positives and unjudged negatives are refused.
4. Suitable existing human judgments for stratified TRAIN review, critical
   final evidence, teacher conflicts and each proposed terminal class. Record
   annotation authority and uncertainty. Agent review and native projection
   cannot fill a missing human label.
5. A before-score repository-cluster sample/power rationale and frozen rubric,
   followed by executable cards binding data/checkpoint/policy hashes, seed,
   optimizer, controls, maximum steps and measured compute/settlement caps.

Terminals additionally need independent whole-card class evidence and the exact
native support/calibration proofs. Sufficient requires zero observed false
accepted decisions and a one-sided 95% false-acceptance upper bound below 1%,
with a defensible repository-cluster analysis. The roughly 299 independent
zero-error accepted-case guide does not impose 299 repositories or establish
independence by itself. Unsupported winning classes must abstain without
substituting a lower-scoring Read.

## Work that can proceed independently

Current native input/padding/token/menu census, acquisition/lifecycle checks,
resource probes on already authorized inputs, generic-Read qualification and
ordinary product/platform checks can proceed with their own admitted inputs.
They do not require pretending that the software training data has passed.

The native tokenizer census seam is
`arbiter::compiler::qwen::tests::context_prompt_census_old_versus_new_against_profile_cap_and_remaining_budget`
in `airk-retrieval`. Its ignored test consumes `AIRK_QWEN_TOKENIZER` and
`AIRK_CONTEXT_CENSUS_REQUESTS`, optionally writing a fresh
`AIRK_CONTEXT_CENSUS_OUTPUT`. It measures old/new prompt counts against 2048,
8192 and the original request's actual remaining shared budget. Use complete
retained native request bytes and pin their hashes; do not rebuild requests,
prompts or tokens in Python. A new census is required on current source before
claiming fit. The sole Cargo owner selects and records the actual command.

The first execution of that historical census stopped at row 14 because its
shrinkage assertion failed: the native verbose view counted 602 tokens and the
compact view 649; both fit 2048. The original failed receipt and assertion are
preserved. The separate ignored diagnostic
`context_census::complete_context_prompt_census_records_growth_and_refusals`
retains every original native request and records growth, shrinkage and refusal
without assuming that compact always means fewer tokens. It uses the native
Qwen compiler-profile emitter and public compiler, changing only a private
diagnostic metadata width to 8192. Original request budgets remain unchanged;
no graph executes and no profile/resource fit is established. Counts exist only
for successful compiles; structural, prompt or compiler refusals keep null
counts and full raw detail, explicitly reducing comparable measurement coverage
rather than guessing over-cap tokens or cropping input. The run requires exact
`AIRK_CONTEXT_CENSUS_REQUESTS_SHA256` and `AIRK_QWEN_TOKENIZER_SHA256` pins in
addition to the same input/tokenizer/output paths. The complete report retains
input identities, diagnostic/compiler policies, every original row and refusal,
and population/comparison counts. This diagnostic source does not alter the
hash-bound production compiler recipe or weaken the historical test.

F1 is gradient-free. Existing QASPER DEV human evidence can support a separately
identified general-document diagnostic after native admission and lifecycle
checks. It cannot select a software instruction or qualify software gains.
Requests TRAIN must not be relabeled DEV, and the 24 agent-authored AIRK pilot
questions remain unadjudicated diagnostics with their existing use restrictions.
F1 changes native prompt/policy identity and needs its owning contract/fixtures;
source review or a copied string is not an executed prompt experiment.

Pending accepted adaptations, retain exact original MiniLM U8/S8 and selected
Qwen publisher weights. This audit reports no new scientific success or failed
experiment. Original optional models still require their own current-artifact
CAL, resources, held-out and composed evidence. The all-three qualified catalog
and native publication capture remain mandatory for final-series delivery.

## Validation

The metadata-only [audit tool](../../tools/retrieval-train/software_v2_audit.py)
verifies historical protocol hashes and refuses executable research cards with
pending data/native/probe bindings. Its optional research source metadata checks
repository/issue/clone and source-content separation, explicit FINAL reservation,
rights/notice use, exposure and existing-human-review attribution before any
training consumer reads text. This metadata format is not a native contract;
passing it never authorizes training or proves annotation adequacy. The
[independent synthetic controls](../../tools/retrieval-train/test_software_v2_audit.py)
exercise these refusals: 11 tests passed. Exact commands, source hashes and
timings are retained in the private inventory directory as `validation.json`.

The additive JSON parses; its original-protocol SHA-256 bindings match live
files. The audit's relative links resolve and new files have no trailing
whitespace. The private inventory's input hashes match the observed bytes.
These checks establish documentation and input identity only. No model quality,
training, runtime, transport, platform or release pass follows from them.
