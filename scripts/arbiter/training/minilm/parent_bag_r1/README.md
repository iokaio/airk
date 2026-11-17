# MiniLM parent bags r1

Preserves `bag_loss.py`, `prepare.py`, `train.py`, `test_bag_loss.py`, and the
transitive `prepare_sources.py` from native-parent-bag-proposal-r1. That helper
loads the maintained `tools/retrieval-train/training_recipe.py` and audit owner
under their retained exact hashes. No executable dependency is copied from a
vendor environment.

The recipe uses weak parent bags, temperature 0.05, first-maximum pooling and
checkpointed unit encoding in microbatches of 8. Original training seed is 1908.
Use the versions and prerequisites in `../encoder_recovery_r1/README.md`; the
finite trainers there are the later retained execution variants.

`AIRK_RESEARCH_MODELS` selects the private models-layout tree (explicit path beneath repository artifacts required). `prepare_sources.py --help` describes its source
fixture arguments. `prepare.py` reads pinned TRAIN corpus, Requests rights inputs,
native units/tensors and writes a fresh `native-parent-bag-candidate-r1/master.private.json`.
`train.py --help` lists the original epoch trainer command. Run them only under
newly reviewed ownership/releases; preserved historical source pins intentionally
refuse modified dependencies. `test_bag_loss.py` requires Torch and performs only
synthetic loss/gradient comparisons, but was not run as part of this migration.
No corpus, examples, tensor fixtures, base weights, recipe or training receipts are
included. Licenses/rights for external training inputs must be supplied with those
inputs; these scripts do not grant redistribution rights to them.

## Source identity and retained evidence

`source-map.json` records each original relative source path and SHA-256 alongside
its maintained SHA-256 and transformations. Original artifact copies are retained.
SPDX headers and path changes give these files new identities. Historical receipts
qualify their original bytes only. Existing digest, release, deadline, ownership,
CUDA identity and finite-value checks remain enforced. No source hash is silently
updated to make a historical seal pass. Fresh execution needs a reviewed source
transition and fresh releases/seals; this migration does not authorize execution.

Only implementation and synthetic tests are included. Data, labels, checkpoints,
weights, receipts, runtime environments and machine-specific releases are excluded.
The Apache-2.0 repository license applies to this authored code; installing model
and runtime dependencies does not transfer their licenses into this package.
