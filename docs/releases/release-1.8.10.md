# AIRK 1.8.10 local working build

**7 November 2026 — unpublished Windows manual-test installer.** The maintainer
requested version **1.8.10** after reworking the confusing Indexing screen. Native
workspace packages, the extension and their lockfiles use that version. This is
local installer preparation; publication and later 1.9.0 promotion are separate.
The [1.8.9 record](release-1.8.9.md) and its QA evaluation retain their original
versions, source identities and validation scope.

## Indexing workflow

The three Indexing tabs now put their own task first:

| Tab | Task |
| --- | --- |
| **This window** | Saved window associations, reviewed access grants and explicit grant selection. Recommendations are a secondary disclosure. |
| **Index library** | Automatically load a bounded metadata page, open an index, or start its split review. Selected-index controls separate sources, splitting, Project memberships and settings. |
| **Projects** | Automatically load a separate Project page; open a Project to manage memberships and default search scope. |

A tab's initial metadata read is attempted once for the current view/authority;
a failure stays visible and requires an explicit **Refresh indexes**,
**Refresh Projects**, or **Read saved association** retry. Reads respect workspace
trust, native-execution settings, runtime upgrades and existing busy admission.
They never create an index, start indexing, restore access or issue a grant.
Catalog pages remain bounded; filters apply only to the displayed page and
continuations require **Next page**.

The library exposes **Open index** and, for an eligible active index, **Split
index**. Opening a row brings its details into view. Project details do not leak
into the library and index details do not appear on the Project tab. Setup and
secondary recommendations use disclosures. Operation status/recovery and complete
catalog JSON/Markdown copies and exports remain available in disclosures; native
response fields remain retained.

The split form first loads the complete native source baseline, then offers a
source-folder dropdown and relative exact-file/subtree selections, followed by
destination, Project memberships and explicit capacity limits. Review and Apply
remain separate. See the [split walkthrough](../index-splitting.md).

## Ownership and inherited behavior

The extension's split-opening selection accepts an active index on the current
native library page or an index from the existing reviewed recommendation scope.
It still reads the exact current native detail and preserves catalog identity,
revision and capability fences. The underlying native split controls and their
reviewed mutation/recovery contracts are unchanged.

Automatic authenticated capability discovery, runtime-upgrade progress, reviewed
index/grant setup and the existing structured MCP QA follow-up remain inherited.
Source files stay read-only. Existing permanent exclusions, independent collection
identities and grant authority remain authoritative.

## Validation and package evidence

The synchronized 1.8.10 version check passed. Current local observations:

- Host, webview and Assistance TypeScript compilation and the normal bundle/catalog
  check passed. The final extension suite passed **858 tests**, with **20 optional
  native/environment cases skipped**. New regressions cover tab separation,
  empty-page identity, readable split sources, retained form values, complete
  exports, late context capability, connection readiness and upgrade completion.
- All **482 Python tests** completed successfully, with **9 expected skips**.
  Repository and whitespace checks passed. A fictional-data browser preview of
  the actual Indexing component confirmed the library layout; this is separate
  from installed-editor acceptance.
- Rust formatting passed. Strict workspace Clippy passed with all features and
  all targets using `python scripts/dev-check.py --workspace --all-features
  --target-dir artifacts/v190/p10-target --skip-fmt` after the formatting check.
  Windows development incremental compilation was on. The wrapper took
  **80.383 seconds**; Cargo's timing report is retained in that persistent
  target's `cargo-timings` directory.
- A separate agent reviewed the Indexing implementation and regression coverage;
  the primary agent reviewed the combined changes. No unresolved Indexing review
  finding remains.
- The dependency-policy attempt is **unavailable**, not passing: source policy,
  locked vet and immutable-input checks passed, but Cargo advisory caches could
  not be fetched/used and npm did not return a complete advisory report. The
  complete diagnostic is retained with the local build evidence.

Fresh native source **`90eb4c4e72ad7c28be0d25de8216b4d50680dd0e8998f70ef3bfb3848f5d79a1`**
contains **978 inputs**. The matched release pair passed construction, embedded-model
verification, version checks and full payload sealing. The build command was
`python scripts/build-service-runtime.py --profile release --destination artifacts/local/1.8.10-indexing-20261107/runtime`.
It reused `artifacts/v190/p10-target`, with release incremental compilation off,
and took **829.532 seconds**. Both Cargo invocations retained `--timings` reports;
the helper took 307 seconds and the feature-minimal main 511 seconds. These are
fresh builds, separate from historical qualification.

The actual bundled-transformer gate passed in **19.509 seconds**. The two-Project
native CLI/MCP client gate passed in **28.752 seconds**, including **19 native read
tools / 29 MCP tools**, six MCP connections, three simultaneous clients,
reconnect, model readiness, native failure diagnostics and preserved source bytes.
The lightweight-main all-target check passed in **58.020 seconds**, with Cargo
timings and Windows development incremental compilation on.

The original full-workspace invocation stopped in the CLI library with **508
passed / 6 failed / 23 ignored**. All six failures retained the same
`ignore_control_metadata` cause: this sandbox cannot read the inherited global
Git-ignore fallback during restricted split scanning. The crash-child exit
mismatch was secondary to that same refusal. Filesystem metadata checks confirmed
the denied fallback; no policy or assertion was weakened. The existing compiled
paid-build regression passed in **4.778 seconds** with only a child-process
`XDG_CONFIG_HOME` pointing to a new isolated test configuration directory. No
runtime, native source, identity, existing ACL or global configuration changed.
The original failure remains recorded. In the full `--no-fail-fast` workspace
rerun with that same process-only test isolation, the CLI library passed **514
tests / 23 ignored**, including all six original failures.

That complete workspace run took **2,653.032 seconds**, ending with **2,715 passed /
5 failed / 51 ignored**. The remaining five failures were native private-boundary
capability refusals: three `native_owner_recovery` cases and two Store default
producer-layout cases. The ordinary temporary directory has foreign write grants.
All other executed cases passed, including the four routine workload campaigns.
Eight doctest targets completed successfully with **zero eligible examples**.
The raw run stays recorded as failed; it is not relabeled as a clean full pass.

A new child under the documented retained sandbox-owned fixture base was verified
to inherit exactly one owner FullControl ACE with container/object inheritance.
No existing ACL, identity or source changed. Using process-local
`AIRK_PRIVATE_CENSUS_BASE`, the **same compiled test binaries** then passed all
five cases: **3 passed / 1 ignored** in the complete native-owner target
(**3.701 seconds**) and **2 passed / 772 filtered out** in the exact Store
selection (**0.417 seconds**). The ignored case is the subprocess entry exercised
by its parent tests. These are qualified fixture reruns, not rebuilt runtimes or
another complete workspace invocation. Binary hashes, exact commands, fixture
permissions and the original failures are retained in the evidence directory.
The native compiled-source seal still matches all **978 current inputs**.


Native archive verification passed in **17.520 seconds**; VSIX packaging and
payload verification passed in **25.629 seconds**. The package's actual extension,
native runtime/build/install metadata and new Indexing labels were verified:

- Artifact: `artifacts/local/1.8.10-indexing-20261107/vscode/publish/airk-1.8.10-win32-x64.vsix`
- Target: **win32-x64**, version **1.8.10**, **77,661,351 bytes**.
- SHA-256: `28019ac5ddda0b57c8f62314502ac2e8ed53b92632fd93b798c8f81f0bcb786b`.

Evidence is retained under `artifacts/local/1.8.10-indexing-20261107/`. Installed-editor
acceptance, other native platforms and release qualification remain separate.

## VSIX update detection follow-up (7 November 2026)

The previous update identity checked only `out/serviceExtension.js`. A rebuilt
VSIX could change Explorer JavaScript or CSS while that file stayed unchanged,
leaving an open panel on the earlier interface without an update action.
The working source now observes the flat installed JavaScript/CSS payload and
watches replacement events inside the extension directory. The existing Explorer
receives the pending-update state and offers **Reload to finish update** without
requiring the panel to be closed first. The user chooses when to reload the window;
updated host modules and UI then load together. Incomplete payload observations
wait for the replacement files. Local installers verify every shipped flat
JavaScript/CSS file, including Explorer and Workbench code, against the VSIX.

This is a source follow-up to the earlier package evidence, not a rebuilt or
installed candidate. Installed-editor acceptance remains pending.

Follow-up validation: host/webview/Assistance compilation and the complete
extension suite passed: **863 passed, 0 failed, 20 optional cases skipped**.
Regressions keep Explorer open during UI asset replacement, verify one pending
update notification, preserve explicit reload and cover incomplete replacement.
Both local-installer regression tests passed (matching assets and three UI-only
mismatch subcases). Repository, synchronized-version and whitespace checks passed.
Native Rust qualification and real VSIX installation were not run for this
extension/install-script change. Metadata identity still uses filenames, size and
modification time; a replacement preserving both size and timestamp can evade
same-version detection. Installer verification independently compares SHA-256.
