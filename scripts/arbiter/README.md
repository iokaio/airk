# Arbiter research and artifact reproduction

The [source rescue catalogue](../../docs/arbiter/source-rescue.md) lists every receiving README, including historical source families and the remaining coverage limits.

This tree keeps the implementation used to analyze, train, fine-tune, export and
validate AIRK's selected models in the repository. Versioned recipes retain
original source hashes and distinguish mechanical relocation from changed paths
or interfaces. Old artifact copies and their receipts remain immutable evidence.

- [Training](training/README.md): Focus LoRA and MiniLM finite/parent-bag recipes.
- [MiniLM export](export/minilm/README.md): versioned export, quantization, grading
  and preparation source with an index of each recipe.
- [Qwen export and references](export/qwen/README.md): Focus and Context reference,
  storage, conversion and export recipes.
- [Evaluation](evaluation/README.md): independent numerical reference preparation
  and assembly.
- [Source inventory analysis](analysis/source_inventory/README.md): read-only
  inspection of ignored source candidates and existing repository ownership.

## Inputs and reproduction boundaries

Training datasets, human labels, model weights, checkpoints, generated fixtures,
compiled libraries, private seals and execution receipts are not committed here.
The recipe READMEs identify the external inputs and required dependency versions
or authenticated environment. Source/provenance maps contain filenames and hashes,
not those payloads. Acquire inputs through their documented sources and respect
their licenses; supply exact inputs to fresh, separately owned output directories.

Do not write generated files beside these sources. Use each workflow's explicit
private input/output arguments. Historical campaign wrappers are retained where
needed to understand a prior artifact; expired deadlines and old source seals do
not authorize a new run. Their READMEs distinguish operational entry points from
historical preparation code and document remaining prerequisites.

Lightweight migration, syntax and decision tests establish source preservation
and tested harness behavior. They do not establish repeat training quality,
byte-identical model exports, native GPU execution, or release qualification.
