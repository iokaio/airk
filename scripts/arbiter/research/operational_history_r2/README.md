# AIRK operational source history, revision 2

This package preserves standalone operational implementations found by the broad artifact inventory after the first rescue checkpoint. It covers actual AIRK Python, PowerShell, shell, CMD and CJS/MJS drivers. Third-party VS Code Git askpass helpers and two Microsoft ONNX Runtime build scripts belong to separate upstream-provenance reviews and are not copied here. Packaged/generated JavaScript, embedded JSON code and native proposals have separate owners.

`source-map.json` records each original path, SHA-256 and length, its maintained owner and the only permitted transformation: a literal Apache SPDX prefix. No original newline, encoding, body, hardcoded pin or deadline is rewritten. Identical bodies share one source owner; existing committed prefix-only owners are reused when their inverse exactly matches. Filenames retain the original SHA-256 and a historical basename.

## What these scripts do

| Family | Behavior and inputs | Outputs and limits |
|---|---|---|
| P8/P9 frozen replay | Exercises the frozen E0 workload through the retained predecessor/current-runtime upgrade chain, using private fixtures and exact runtime manifests | Commands, timing and gate observations. The old upgrade path is historical; these files are not current qualification commands. |
| P6f guarded qualification-pair builder | Builds a fresh unsealed cfg(test) native-owner pair from an explicit frozen manifest, without running qualification | Candidate binaries and source/build receipts. Pinned dependencies, output custody and current build ownership must be reviewed before reuse. |
| Workspace and one-process drivers | Runs selected compiler/tests, counts real results, collects process outcomes, compares performance and reviews source revisions | Compiler artifacts, logs and receipts in private run roots. Never run old target owners concurrently or infer success from copied receipts. |
| Windows installer/package probes | Prepares or tests matched runtime layouts, installation, upgrades, package inventories, release assets and recovery | Candidate payloads and test observations. Some historical scripts contain publication commands; preservation grants no install, release, signing or publication authority. |
| Mac Neo and remote helpers | Transfers source/evidence, creates bounded command batches and verifies platform source/artifact agreement | Transfer manifests and remote observations. Existing connection configuration and same-human remote identity must be separately authenticated. No credentials are supplied here. |
| Preparation/adaptation helpers | Constructs historical source/input plans and replaces exact source anchors | Generated source copies or proposal files. These are not patches to apply blindly to the live tree. |
| Diagnostic/evidence helpers | Parses retained receipts and process/platform observations, checks hashes, extracts counts and compares gate outcomes | Derived analysis records, not numerical/model/platform qualification. Private input datasets and runtime states remain external. |

## Use

The maintained preservation adapter is read-only by default:

```powershell
python -B -S scripts/arbiter/research/operational_history_r2/preserve.py
python -B -S scripts/arbiter/research/operational_history_r2/test_preserve.py
```

`--write` is only for initial guarded materialization into fresh destinations. The verified human shell must check this folder and `sources/` with the repository execution guard before writes, then audit every created output. The adapter authenticates the exact inventory and source bytes and never imports or executes historical scripts. Preservation checks read bytes and verify complete inverse mappings; they are not historical test runs.

Historical source files remain unsupported for direct execution. Their `__file__` depth, sibling imports, absolute machine paths, old limits and authority may no longer apply. Moving a behavior into a live owner requires current contract review and authenticated inputs/outputs. Some executable shebangs are preceded by an explicit license prefix; original body recovery is exact, but direct executable semantics are not claimed. No credentials, model weights, training examples, indexed content or runtime fixtures are intentionally included. Source-review and pattern checks do not constitute a general secret detector.

The remote-overlay-exact.py orchestrator preserves an encoded two-file Rust proposal bundle inline: onnx_macos.rs (2825 bytes, SHA1a7e5e26...) and arbiter_assets/offline.rs (9449 bytes, SHA2f82fe22...). The map records strict Python-AST/base64/JSON/base64 decoding and exact body hashes. Neither body is applied or executed; no sole implementation remains in ignored state.

Twenty-four historical airk.cmd wrappers are excluded after exact replay of the Git-owned cliInstall.ts template; their pinned disposition is recorded in the map. Third-party provider skills, upstream rmcp test code, private indexed snippets, an empty placeholder and a synthetic worker with an existing generator are separately routed, not treated as authored operational implementations.

Final scoped roster: 2413 original paths, 2047 distinct new retained source copies, and seven origin aliases reusing existing maintained owners. The full receiving-folder test rejects unmapped extra files. Eighteen build CMD wrappers remain explicitly labelled historical generated copies with retained producer candidates; exact parameter reconstruction is a separate proof, not a prerequisite for preserving their already reviewed source. The map is a source-preservation result, not a broad all-artifacts completion claim.

Permanent `test_preserve.py` checks now reconstruct every original from maintained bytes and verify the recorded original SHA256/length, maintained SHA256/length, transformations and literal roster. They do not require artifact originals. Run the same script with `--audit-originals` for a separate strict custody audit: each actual original must match, or its absence must be justified by a SHA/length-pinned successful removal receipt and bound pre-removal proof. Missing files never become skips. An optional `--removal-roster <private-json> --removal-roster-sha256 <SHA256> --removal-roster-bytes <length>` supplies the explicit removal lookup defined by the source_cleanup contract. This transition preserves source identities and historical evidence; it does not delete anything or qualify recovered programs.
