# Materialization and rubrics source history

Versioned independent corpus materializers, opaque FINAL/DEV preparation, metadata projections and synthetic refusal regressions.

These preserve historical source bodies exactly; files missing the required first-line license add only a recorded SPDX prefix. Filenames append the first 12 SHA-256 characters to distinguish versions; the [source map](../source-provenance.json) records every original path and complete original hash. Exact duplicate original bodies share one maintained owner. Distinct implementations are retained.

Do not launch these archived files directly: original sibling imports, __file__ roots, machine paths, runtime versions, process identities and seal pins have deliberately not been rewritten. Reuse requires an explicit reviewed adapter with private inputs/outputs, the matching sibling closure reconstructed from the map, and fresh source/runtime/input seals. Never overwrite old receipts or use historical account IDs as execution authority.

Inputs include separately provisioned native images, private source manifests/corpora/labels, publisher model assets or metadata, and applicable execution seals. Outputs are private captures, generated preparation, diagnostics or receipts under an audited ignored run root. None are included here. PyArrow, Torch/ORT/Transformers, Win32 Job APIs or PowerShell are needed only where the individual recipe imports them; original versions must come from its retained runtime seal. Source inspection requires only Python standard library.

Run from the repository root: `python -B -S scripts/arbiter/evaluation/research_history/test_source_identity.py`. This checks preservation without importing workers or touching private inputs. No model, native, networking, provisioning, build or historical worker tests were run for this migration.

## Versioned source roles

Descriptions below are extracted from the retained module descriptions or named implementation entry points; original paths distinguish experiment versions. The source map also records every role and its defined entry points.

- [adapter--06e7b3c60cfc.py](adapter--06e7b3c60cfc.py) â€” Opaque FINAL-only neutral materialization; explicit reviewed execution required. Original versions: `a1-fresh-final-materializer-r2/adapter.py`.
- [adapter--2936a7a3c2f9.py](adapter--2936a7a3c2f9.py) â€” Opaque FINAL-only neutral materialization; explicit reviewed execution required. Original versions: `a1-fresh-final-materializer-r3/adapter.py`.
- [adapter--2f351de8edc7.py](adapter--2f351de8edc7.py) â€” Opaque FINAL-only neutral materialization; explicit reviewed execution required. Original versions: `a1-fresh-final-materializer-r5/adapter.py`.
- [adapter--503e956476d7.py](adapter--503e956476d7.py) â€” Opaque FINAL-only neutral materialization; explicit reviewed execution required. Original versions: `a1-fresh-final-materializer-r1/adapter.py`.
- [adapter--fee5090efe5e.py](adapter--fee5090efe5e.py) â€” Opaque FINAL-only neutral materialization; explicit reviewed execution required. Original versions: `a1-fresh-final-materializer-r4/adapter.py`.
- [audit--0a11c9d27f21.py](audit--0a11c9d27f21.py) â€” Private, sealed ID/title projection. Never imports corpus/model producers. Original versions: `a1-next-experiment-blind-metadata-r1/audit.py`.
- [audit--1feb120e7825.py](audit--1feb120e7825.py) â€” Private, sealed ID/title projection. Never imports corpus/model producers. Original versions: `a1-next-experiment-blind-metadata-r4/audit.py`.
- [audit--aa044386c8af.py](audit--aa044386c8af.py) â€” Private, sealed ID/title projection. Never imports corpus/model producers. Original versions: `a1-next-experiment-blind-metadata-r2/audit.py`.
- [audit--de01bb699237.py](audit--de01bb699237.py) â€” Private, sealed ID/title projection. Never imports corpus/model producers. Original versions: `a1-next-experiment-blind-metadata-r3/audit.py`.
- [diagnose_refusals--e51e4ce6c4b5.py](diagnose_refusals--e51e4ce6c4b5.py) â€” Bounded existing DEV-only conversion diagnosis; metadata output, no scoring. Original versions: `a1-component-development-experiment-r1/diagnose_refusals.py`.
- [fixture--df4313047080.py](fixture--df4313047080.py) â€” Private FINAL mechanical source/range projection. No native wire DTO owner. Original versions: `a1-fresh-final-evaluation-r1/fixture.py`.
- [materialize--641189ab4eed.py](materialize--641189ab4eed.py) â€” Opaque selected-Context data alignment. Separate Root content release required. Original versions: `context-action-cohort-admission-r1/materialize.py`.
- [materialize_dev--b8adb549b151.py](materialize_dev--b8adb549b151.py) â€” Private sealed DEV adapter; no execution before reviewed input-seal release. Original versions: `a1-component-development-experiment-r1/materialize_dev.py`.
- [metadata-only-control--f451e26e10ac.py](metadata-only-control--f451e26e10ac.py) â€” Metadata hashes only; never opens raw questions, Qasper or gold payloads. Original versions: `a1-fresh-final-materializer-r4/metadata-only-control.py`.
- [observer--e77cbea37f95.py](observer--e77cbea37f95.py) â€” Explicit opaque FINAL fixture/export and grading; never launches inference. Original versions: `a1-fresh-final-evaluation-r1/observer.py`.
- [prepare-anchor--0027be1b3290.ps1](prepare-anchor--0027be1b3290.ps1) â€” DACL-only producer. Never request SetOwner/privileged security sections. Original versions: `a1-fresh-final-materializer-r2/prepare-anchor.ps1`.
- [prepare-anchor--6d7b40da280a.ps1](prepare-anchor--6d7b40da280a.ps1) â€” DACL-only producer. Never request SetOwner/privileged security sections. Original versions: `a1-fresh-final-materializer-r1/prepare-anchor.ps1`.
- [prepare-anchor--8a57d6476d46.ps1](prepare-anchor--8a57d6476d46.ps1) â€” DACL-only producer. Never request SetOwner/privileged security sections. Original versions: `a1-fresh-final-materializer-r4/prepare-anchor.ps1`.
- [prepare-anchor--b86aaddbe530.ps1](prepare-anchor--b86aaddbe530.ps1) â€” DACL-only producer. Never request SetOwner/privileged security sections. Original versions: `a1-fresh-final-materializer-r5/prepare-anchor.ps1`.
- [prepare-anchor--bfe54bd0b33b.ps1](prepare-anchor--bfe54bd0b33b.ps1) â€” DACL-only producer. Never request SetOwner/privileged security sections. Original versions: `a1-fresh-final-materializer-r3/prepare-anchor.ps1`.
- [requests_pairs--7dfa7c6dd0d1.py](requests_pairs--7dfa7c6dd0d1.py) â€” requests pairs: top-level preparation, observation or controlled experiment script Original versions: `requests_pairs.py`.
- [run-materialize-reviewed--3dbdf5bf17be.py](run-materialize-reviewed--3dbdf5bf17be.py) â€” Root-owned opaque alignment launch; observer never opens emitted task corpus. Original versions: `context-action-cohort-admission-r1/run-materialize-reviewed.py`.
- [run-prepare-reviewed--e01a979f079b.py](run-prepare-reviewed--e01a979f079b.py) â€” Primary-only owned60s opaque fixture preparation. Never parses FINAL content. Original versions: `a1-fresh-final-evaluation-r1/run-prepare-reviewed.py`.
- [run-reviewed--c45c58faa331.py](run-reviewed--c45c58faa331.py) â€” Primary-only reviewed bounded launch. Never reads emitted corpus or real gold. Original versions: `a1-fresh-final-materializer-r1/run-reviewed.py`, `a1-fresh-final-materializer-r2/run-reviewed.py`, `a1-fresh-final-materializer-r3/run-reviewed.py`, `a1-fresh-final-materializer-r4/run-reviewed.py`, `a1-fresh-final-materializer-r5/run-reviewed.py`.
- [run-synthetic-tests--b04301966198.py](run-synthetic-tests--b04301966198.py) â€” Synthetic-only suite under actual existing 256MiB policy; no publisher data. Original versions: `a1-fresh-final-materializer-r5/run-synthetic-tests.py`.
- [runner--6175c945ef72.py](runner--6175c945ef72.py) â€” Private mechanical projection of original human answers/support into exact ranges. Original versions: `human-goal-rubric-r1/runner.py`.
- [runner--85995cf22d91.py](runner--85995cf22d91.py) â€” Private mechanical projection of original human answers/support into exact ranges. Original versions: `human-goal-rubric-r3/runner.py`.
- [runner--ace295120ef1.py](runner--ace295120ef1.py) â€” Create private exact-text native indexing fixtures and freeze goal task IDs. Original versions: `native-human-fixtures-r1/runner.py`.
- [runner--e3e57899186f.py](runner--e3e57899186f.py) â€” Private mechanical projection of original human answers/support into exact ranges. Original versions: `human-goal-rubric-r2/runner.py`.
- [synthetic-arrow-memory-control--308fc9936b43.py](synthetic-arrow-memory-control--308fc9936b43.py) â€” Pinned Arrow synthetic-only import/projection under the unchanged 256MiB cap. Original versions: `a1-fresh-final-materializer-r4/synthetic-arrow-memory-control.py`.
- [synthetic-arrow-memory-control-singlethread--37584bdd738c.py](synthetic-arrow-memory-control-singlethread--37584bdd738c.py) â€” Pinned Arrow synthetic-only import/projection under the unchanged 256MiB cap. Original versions: `a1-fresh-final-materializer-r4/synthetic-arrow-memory-control-singlethread.py`.
- [test-anchor--78c3d2274555.ps1](test-anchor--78c3d2274555.ps1) â€” test anchor: top-level preparation, observation or controlled experiment script Original versions: `a1-fresh-final-materializer-r1/test-anchor.ps1`, `a1-fresh-final-materializer-r2/test-anchor.ps1`, `a1-fresh-final-materializer-r3/test-anchor.ps1`, `a1-fresh-final-materializer-r4/test-anchor.ps1`, `a1-fresh-final-materializer-r5/test-anchor.ps1`.
- [test_adapter--0e089b4d9d5c.py](test_adapter--0e089b4d9d5c.py) â€” Synthetic guards only: never open real FINAL gold or raw dataset content. Original versions: `a1-fresh-final-materializer-r5/test_adapter.py`.
- [test_adapter--67e11f0eddca.py](test_adapter--67e11f0eddca.py) â€” Synthetic guards only: never open real FINAL gold or raw dataset content. Original versions: `a1-fresh-final-materializer-r4/test_adapter.py`.
- [test_adapter--83ac7324ceff.py](test_adapter--83ac7324ceff.py) â€” Synthetic guards only: never open real FINAL gold or raw dataset content. Original versions: `a1-fresh-final-materializer-r2/test_adapter.py`.
- [test_adapter--895239f0b51f.py](test_adapter--895239f0b51f.py) â€” Synthetic guards only: never open real FINAL gold or raw dataset content. Original versions: `a1-fresh-final-materializer-r3/test_adapter.py`.
- [test_adapter--b2538779b72b.py](test_adapter--b2538779b72b.py) â€” Synthetic guards only: never open real FINAL gold or raw dataset content. Original versions: `a1-fresh-final-materializer-r1/test_adapter.py`.
- [test_materialize--f634d6733562.py](test_materialize--f634d6733562.py) â€” test materialize: defines SelectionControls Original versions: `context-action-cohort-admission-r1/test_materialize.py`.
- [test_materialize_dev--7c13f28e13c7.py](test_materialize_dev--7c13f28e13c7.py) â€” Synthetic-only isolation and exact historical neutral-conversion controls. Original versions: `a1-component-development-experiment-r1/test_materialize_dev.py`.
- [test_observer--16b8359522e7.py](test_observer--16b8359522e7.py) â€” Synthetic FINAL projection guards only; never opens real FINAL payloads. Original versions: `a1-fresh-final-evaluation-r1/test_observer.py`.
- [test_wrapper--bf47f946b093.py](test_wrapper--bf47f946b093.py) â€” Synthetic wrapper boundary guards. Never opens a FINAL input or starts a job. Original versions: `a1-fresh-final-evaluation-r1/test_wrapper.py`.
