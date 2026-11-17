# Qwen reference and export recipes

This folder owns versioned Focus and Context mathematical references, graph export, storage and quantization source. `source-provenance.json` records all 80 original and maintained SHA-256 identities. Every original source and all private weights, receipts, seals and data remain under artifacts unchanged. SPDX headers were added where absent. Recipe names retain their experimental version; before-review and unreleased predecessor folders are historical.

## Maintained selected entry points

Run from the repository root, with the pinned provisioned runtime and a separately reviewed same-user process/resource owner. These are worker command templates, not permission to bypass the execution owner. Prepare a fresh seal including the maintained dependency closure and the exact private input/output paths. Historical seals authenticate historical runs and cannot qualify new source hashes. Outputs must be fresh private directories; never put generated data in this source tree.

```powershell
python -B scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/run_focus_export.py --phase reference --tokens 128 --threads 1 --device cuda --output artifacts/<fresh-focus-reference>
python -B scripts/arbiter/export/qwen/qwen3-reranker06-static-export-r1/run_static_export.py --source artifacts/<focus-reference> --cases artifacts/<reviewed-static512-cases.json> --output artifacts/<fresh-static512>
python -B scripts/arbiter/export/qwen/qwen3-reranker06-weight8-r1/run_weight8.py --source artifacts/<static512-export> --output artifacts/<fresh-focus-weight8>
python -B scripts/arbiter/export/qwen/qwen3-challengers-r1/full-stages-r4/full_reference_r4.py --release artifacts/<fresh-reference-release.json> --assets artifacts/advanced-split/models/qwen3-challenger-assets-r1 --output artifacts/<fresh-context-reference>
python -B scripts/arbiter/export/qwen/qwen3-challengers-r1/full-stages-r4/export_scores_r4.py --release artifacts/<fresh-export-release.json> --assets artifacts/advanced-split/models/qwen3-challenger-assets-r1 --output artifacts/<fresh-context-export>
```

Focus reference now locates retained publisher assets from the repository, independent of cwd. Static512 and weight8 require explicit private source paths, and static512 requires the independently reviewed cases file. Their imports resolve sibling maintained modules. Context reference/export already take explicit release, publisher assets and output paths and import the maintained sibling reference. Releases must name fresh output paths and new source pins. Weight8 retains CPU-only admission and all original numeric gates.

## Historical recipes and limits

The retained prepare/owned wrappers bind historical receipt hashes, source locations and owner profiles. They are source-preserved recipes, not relocated operational launchers. Do not run them unchanged. A fresh owned launch must bind the selected worker commands above and reviewed process/resource controls; generating such a new execution seal and running a model are separate campaign steps. The static128 predecessor, before-owner-review exporter, earlier quantization versions and tiny prototypes remain historical. Tests beside these recipes may require retained private fixtures or historical seals and are not all runnable after relocation.

No weights, datasets, receipts, reference-code vendor packages or Rust backups are included. Publisher inputs must already be provisioned and independently authenticated. Pure source/path and ranking tests validate this move; no model, native, quality or cross-platform qualification is claimed. Numerical tolerances and source-model revisions remain unchanged.
