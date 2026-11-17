# Arbiter training recipes

Maintained source for the retained Focus LoRA and MiniLM parent-bag experiments.
Run commands from the repository root. `focus_lora_r8` retains the coherent F2 r8
worker/owner/selection/source-epoch package. `minilm/encoder_recovery_r1` retains
the finite trainers and their resource, release and tokenizer-admission prerequisites.
`minilm/parent_bag_r1` owns the original bag loss and TRAIN-only preparation.

These are versioned research recipes, not an unattended training service. The
Windows ownership wrappers require the interactive human identity, matching default
owner, guarded output parents and pinned interpreter/runtime. Read each recipe's
README and CONTRACT before constructing fresh execution seals. No GPU run, training
or artifact qualification was performed as part of this source migration.

Cheap maintained tests (no Torch, tokenizer, data or model):

```powershell
python -B -S scripts/arbiter/training/test_migration.py
python -B -S -m unittest discover -s scripts/arbiter/training/focus_lora_r8 -p test_workflow.py
python -B -S -m unittest discover -s scripts/arbiter/training/minilm/encoder_recovery_r1 -p test_train_r3.py
```

Other test files include owned-process tests, synthetic Torch operations and tokenizer
fixtures. Do not run broad discovery as an implicit GPU/process campaign. Bootstrap
trap tests require `AIRK_RESEARCH_TEST_OUTPUT` to name a separately guarded private
temporary output directory. The copied old process-owner test additionally uses the
maintained `tools/retrieval-train/training_execution.py` and is historical coverage,
not evidence that the current owner has been qualified.
