# Private indexed fixture inputs

This owner accounts for 11,007 selected source paths in three historical bookbuilder benchmark copies. It preserves private payloads in place and emits only provenance metadata. The original benchmark producer, configurations, completed isolation summaries and native inventory/manifest are pinned in catalog.json; they are evidence, never executed. See CONTRACT.md for admission and limits.

Run pure tests from a verified human shell: `python -B -S scripts/arbiter/analysis/private_fixture_inputs/tests.py`.

Projection command: `python -B -S scripts/arbiter/analysis/private_fixture_inputs/project.py --destination artifacts/<fresh-private-destination>`. This emits JSON-line bundles on stdout and creates no files. Guard any redirection output parent, audit the retained bundle, and use `../upstream_ort/materialize.ps1` with its exact SHA and the same destination to create fresh metadata maps. Do not direct receipts into this maintained source folder.

Current-original matches, drift, missing files and access failures are reported separately. Exact native historical witnesses remain distinct from current observations. Eighteen generated AssemblyInfo paths have no exact historical witness; their source role is established by the retained copy chain and their explicit identity gap propagates into both later copies. This does not assert their bytes equal the original or qualify any historical benchmark. No copies, imports, models, builds or network operations are performed.
