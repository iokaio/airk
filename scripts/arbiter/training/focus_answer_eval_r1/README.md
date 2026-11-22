# Focus Answer-evaluation candidate r1

Training track of the AIRK Answer evaluation (plan `docs/arbiter/final-plan-fp16.md`,
sections 5.1, 5.4 and 5.5): build Focus examples from the pairs mined from both
repositories, train one LoRA candidate on the A6000, measure it before and after,
and export it into a static-1024 weight8 Focus asset folder that the private
evaluation fixture can attach catalog-only. Evaluation tooling only, never product.

## Inputs

- Mined pairs `{airk,bookbuilder}.jsonl` and `stats.json` from
  `../answer_eval_pairs/mine_pairs.py`. Every candidate trains on both repositories.
- Both checkouts, read only through the miner's git plumbing (`ls-tree`,
  `cat-file --batch`, `GIT_OPTIONAL_LOCKS=0`). The miner's credential, benchmark,
  binary and generated path filters apply before any blob is read, and `aikeys.json`
  is never read.
- The evaluation profile's Focus tokenizer, the pilot's replay files (400 judged code
  TRAIN rows, 45 QASPER and 45 TechQA positives for training; 85 DEV code rows for
  evaluation), the publisher checkpoint and the static-1024 weight8 asset folder.

## What the examples are

- Query: the pair's instruction, capped at 600 characters on a word boundary.
- Document: the hit Focus would see. A hit-like line window (about 1,200 bytes,
  grown around the region; a region over 4,096 bytes is cut on a line boundary), then
  the kernel's `software::focus_window` (ported in `focus_text.py`), then the pair
  template of `compiler/focus.rs` with the SoftwareEvidenceV1 instruction of
  `compiler/focus/experiment.rs`, `model_view` quoting, left padding to width 1024.
  Over-width pairs are refused and counted, never cropped. The ignored Rust test
  `crates/airk-retrieval/tests/focus_answer_eval_candidate.rs` checks a sample
  against the native `compile_software`.
- Positives: up to two evidence regions per pair (largest first) at the pair's commit.
- Hard negatives, same repository: another region of the first positive's file at the
  pair's commit, the best BM25 window of the repository snapshot outside the pair's
  evidence files, and a random snapshot window of the same class (code or Markdown).
- Splits: the miner's id-hash held-out split, plus a path-grouped split (a
  deterministic 10% of each repository's evidence paths). Training drops every pair
  that touches a held path and never draws a negative from one. Evaluation sets:
  `heldout_id`, `heldout_path` (positives only from held paths), `train_sample`
  (trained pairs, kind-matched to `heldout_id`) and `replay_dev`.
- Training sample: commit pairs, then hash-ordered doc and test pairs per repository,
  at most three pairs per first evidence path; replay rows interleaved evenly.

## Files

| File | Role |
| --- | --- |
| `focus_text.py` | Pure ports: instruction texts, `model_view`, template, `identifiers`, `focus_window` |
| `build_examples.py` | Pairs to `train.jsonl`, `eval.jsonl`, `stats.json`, `manifest.json` |
| `build_examples_cand2.py` | Candidate 2: `build_examples.py` unchanged, an optional `--per-path-cap`, and 15 QASPER + 15 TechQA replay rows moved to `replay_general_heldout` |
| `worker_math.py` | Pure schedule, padding, target, AUROC and ranking |
| `make_plan.py` | Seals an owned `probe` or `run` plan for the reused `../focus_lora_r8/launch_owned.py` |
| `answer_worker.py` | Owned worker: score base, LoRA train (pilot `inject_lora`/`merge_lora`), merge, score candidate, save |
| `score.py` | Before/after metrics and the train-minus-held-out memorization gap |
| `export_candidate.py` | Merged checkpoint to a weight8 static-1024 asset folder (graph reused) |
| `onnx_parity.py` | CPU ONNX Runtime logits of base and candidate assets against the recorded PyTorch logits |
| `parity_sample.py` | Rows for the native compiler parity check |
| `test_focus_answer_eval.py` | Synthetic tests |

The LoRA recipe is the pilot's (rank 16, alpha 16, q/k/v/o on all 28 layers, FP32,
AdamW 5e-5, no weight decay, seed 1908, effective batch 32, one epoch, full-vocabulary
cross-entropy at the decision position). Microbatch 8 replaces 1 for throughput; the
per-update loss is the same mean over 32 rows.

## Use

```powershell
$py = 'artifacts\advanced-split\models\training-env\venv\Scripts\python.exe'
$T = 'scripts\arbiter\training\focus_answer_eval_r1'
& $py -I -B $T\build_examples.py --pairs PAIRS --repo airk=AIRK --repo bookbuilder=BOOKBUILDER --tokenizer TOKENIZER --execution-root E --output OUT\examples --docs 700 --tests 700
& python -I -B $T\make_plan.py --mode probe --base-python BASE_PYTHON --examples OUT\examples --log-dir OUT\probe --output OUT\probe-plan.json --wall-seconds 900
$env:PYTHONPYCACHEPREFIX = '<launch_owned.FRESH_PYCACHE>'; $env:PYTHONDONTWRITEBYTECODE = '1'
& python -B -S scripts\arbiter\training\focus_lora_r8\launch_owned.py --plan OUT\probe-plan.json --plan-sha256 SHA
# then --mode run --wall-seconds N, the same launcher, then:
& $py -I -B $T\score.py --eval OUT\examples\eval.jsonl --result OUT\run\worker\result.json --output OUT\score.json
& $py -I -B $T\export_candidate.py --merged OUT\run\worker\merged-full-checkpoint\model.safetensors --publisher PUBLISHER --source-assets ASSETS1024 --output OUT\assets
& $py -I -B $T\onnx_parity.py --eval OUT\examples\eval.jsonl --result OUT\run\worker\result.json --base-assets ASSETS1024 --candidate-assets OUT\assets --output OUT\onnx-parity.json
& $py -I -B $T\parity_sample.py --examples OUT\examples\train.jsonl OUT\examples\eval.jsonl --output OUT\parity-rows.jsonl
# Native stage (Cargo owner): set the AIRK_FOCUS_CANDIDATE_* variables listed in the test, then
cargo test --offline -p airk-retrieval --features test-support --test focus_answer_eval_candidate -- --ignored --nocapture
& $py -I -B -m unittest discover -s $T -p "test_*.py"
```

`make_plan.py` takes the base interpreter from `--base-python` or `AIRK_FOCUS_BASE_PYTHON`
and has no machine default. Candidate 1's sealed probe and run plans used an earlier
version that named the interpreter in the code (SHA-256
`86da0c5bd1987f6eedf497f3c77993e1280937857be03f82db1e02bc0d814f0e`); their receipts pin
that historical version, and the change does not alter the plans it produces.

All outputs are private and belong under the campaign's `artifacts\` root. Tracked
files hold code and synthetic tests only; no repository text, pair or weight.
