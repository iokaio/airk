# Model diagnostics source history

Historical local model/compiler/operator/numerical diagnostic implementations and analysis; no weights or third-party implementation snapshots.

These preserve historical source bodies exactly; files missing the required first-line license add only a recorded SPDX prefix. Filenames append the first 12 SHA-256 characters to distinguish versions; the [source map](../source-provenance.json) records every original path and complete original hash. Exact duplicate original bodies share one maintained owner. Distinct implementations are retained.

Do not launch these archived files directly: original sibling imports, __file__ roots, machine paths, runtime versions, process identities and seal pins have deliberately not been rewritten. Reuse requires an explicit reviewed adapter with private inputs/outputs, the matching sibling closure reconstructed from the map, and fresh source/runtime/input seals. Never overwrite old receipts or use historical account IDs as execution authority.

Inputs include separately provisioned native images, private source manifests/corpora/labels, publisher model assets or metadata, and applicable execution seals. Outputs are private captures, generated preparation, diagnostics or receipts under an audited ignored run root. None are included here. PyArrow, Torch/ORT/Transformers, Win32 Job APIs or PowerShell are needed only where the individual recipe imports them; original versions must come from its retained runtime seal. Source inspection requires only Python standard library.

Run from the repository root: `python -B -S scripts/arbiter/evaluation/research_history/test_source_identity.py`. This checks preservation without importing workers or touching private inputs. No model, native, networking, provisioning, build or historical worker tests were run for this migration.

## Versioned source roles

Descriptions below are extracted from the retained module descriptions or named implementation entry points; original paths distinguish experiment versions. The source map also records every role and its defined entry points.

- [backend_details--a0944b59b985.py](backend_details--a0944b59b985.py) â€” Pin the specific backend implementation sources needed for feasibility review. Original versions: `backend_details.py`.
- [backend_probe--3fe5800c7004.py](backend_probe--3fe5800c7004.py) â€” Inspect bounded official backend/export package source metadata, no execution. Original versions: `backend_probe.py`.
- [emit_compiler_assets--8ef7836a33c4.py](emit_compiler_assets--8ef7836a33c4.py) â€” emit compiler assets: top-level preparation, observation or controlled experiment script Original versions: `emit_compiler_assets.py`.
- [integer_kernel_diagnostic--acd4f3368f42.py](integer_kernel_diagnostic--acd4f3368f42.py) â€” integer kernel diagnostic: top-level preparation, observation or controlled experiment script Original versions: `integer_kernel_diagnostic.py`.
- [ort_operator_versions--3aaf861cf818.py](ort_operator_versions--3aaf861cf818.py) â€” Inspect official released operator catalogs, without downloading a runtime. Original versions: `ort_operator_versions.py`.
- [probe-source--3cdd633ee499.py](probe-source--3cdd633ee499.py) â€” Bounded ORT1.30 built-in CPU operator mechanics; no real model qualification. Original versions: `ort130-operators-r1/probe-source.py`.
- [run-reviewed-references--5830096306fb.ps1](run-reviewed-references--5830096306fb.ps1) â€” run reviewed references: top-level preparation, observation or controlled experiment script Original versions: `weights-v2/software-v2-independent-numeric-r1/runs-native-r1/run-reviewed-references.ps1`.
- [run-reviewed-references-r2--80b001cb831f.ps1](run-reviewed-references-r2--80b001cb831f.ps1) â€” run reviewed references r2: top-level preparation, observation or controlled experiment script Original versions: `weights-v2/software-v2-independent-numeric-r1/runs-native-r1/revision-r2/run-reviewed-references-r2.ps1`.
- [runner--531938e88aac.py](runner--531938e88aac.py) â€” Measure one complete model's bounded CPU prefill; no native-resource claim. Original versions: `intern4-weight8-resource-cpu1-r1/runner.py`.
- [runner--6ccb2569c45b.py](runner--6ccb2569c45b.py) â€” Produce an explicit development INT8-weight optional profile; no fallback. Original versions: `intern08-weight8-r1/runner.py`.
- [runner--719e32ac2ab8.py](runner--719e32ac2ab8.py) â€” Produce an explicit development INT8-weight optional profile; no fallback. Original versions: `intern08-int8-r3/runner.py`.
- [runner--7e21c52a23e3.py](runner--7e21c52a23e3.py) â€” Measure one complete model's bounded CPU prefill; no native-resource claim. Original versions: `intern4-resource-cpu1-r1/runner.py`.
- [runner--92e2d94ec296.py](runner--92e2d94ec296.py) â€” Produce an explicit development INT8-weight optional profile; no fallback. Original versions: `intern08-int8-u8-r1/runner.py`.
- [runner--a90d92676494.py](runner--a90d92676494.py) â€” Produce an explicit development INT8-weight optional profile; no fallback. Original versions: `intern08-int8-r2/runner.py`.
- [runner--b2945c235d04.py](runner--b2945c235d04.py) â€” Reviewed text-only single-question compiler; never execute model repo Python. Original versions: `intern08-compiler-r2/runner.py`, `intern4-compiler-r2/runner.py`, `kev4-compiler-r2/runner.py`, `strands2-compiler-r2/runner.py`.
- [runner--b2f446b836c9.py](runner--b2f446b836c9.py) â€” Measure one complete model's bounded CPU prefill; no native-resource claim. Original versions: `intern08-resource-cpu1-r1/runner.py`.
- [runner--edb86afb380c.py](runner--edb86afb380c.py) â€” Measure one complete model's bounded CPU prefill; no native-resource claim. Original versions: `intern4-weight8-resource-cpu8-r1/runner.py`.
- [runner--f02c3467f2de.py](runner--f02c3467f2de.py) â€” Produce an explicit development INT8-weight optional profile; no fallback. Original versions: `intern08-int8-u8-ffn-r1/runner.py`.
- [runner--f894b5baa3a0.py](runner--f894b5baa3a0.py) â€” Reviewed text-only single-question compiler; never execute model repo Python. Original versions: `intern08-compiler-r1/runner.py`, `intern4-compiler-r1/runner.py`, `kev4-compiler-r1/runner.py`, `strands2-compiler-r1/runner.py`.
- [signed_dynamic_diagnostic--151d051017c3.py](signed_dynamic_diagnostic--151d051017c3.py) â€” signed dynamic diagnostic: top-level preparation, observation or controlled experiment script Original versions: `signed_dynamic_diagnostic.py`.
- [summarize-reference-runs--5aeafad9535d.py](summarize-reference-runs--5aeafad9535d.py) â€” Read actual independent executions/assemblies; no scoring or qualification. Original versions: `weights-v2/software-v2-independent-numeric-r1/runs-native-r1/revision-r2/summarize-reference-runs.py`.
