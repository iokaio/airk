# AIRK 1.6.4 — runtime upgrades, project cleanup and diagnostics

Version **1.6.4** is the public release version for all work on this branch since 1.6.0. Native crates, the extension and both lockfiles agree. The unpublished [1.6.1](release-1.6.1.md), [1.6.2](release-1.6.2.md) and [1.6.3](release-1.6.3.md) records retain their dated candidate evidence. Publication requires successful exact-commit main CI and verified release workflows; this source record does not assert that publication has already completed.

## Runtime installation, upgrade and uninstall

Supported installed 1.4, 1.5 and 1.6 runtimes upgrade before connection checks, including recovery from an interrupted preceding activation. Downgrades and unknown minor lines remain outside automatic activation. On Windows and Linux, upgrade/uninstall stops verified owned main, service, index, source, helper and launcher processes. Unknown images and other installations remain untouched. macOS retains authenticated graceful drain; forced predecessor termination is not qualified there.

VS Code completes uninstall after restart through a detached, digest-checked cleanup worker outside the extension directory. Recorded AIRK entries in supported project agent configurations and owned executable entry points are removed. Uninstall preserves source files and index/service data. Unsafe or changed configurations require review and report explicit failure. Organization-managed payloads retain their external ownership policy.

## Registration and complete project removal

Trusted open folders register and begin indexing after upgrades unless explicitly opted out. Saved pause, manual-update, semantic opt-out, custom model and service Stop choices remain authoritative. Missing registry entries after local state deletion can be restored; unavailable registry metadata never authorizes replacement registration. Missing workspace folders do not prevent other folders from being prepared.

Removing a project stops admission, cancels its work, drains owned workers and deletes its ownership-matched AIRK index, usage ledger and saved project settings. Cleanup resumes after interruption without another approval. A durable removal barrier prevents registration during deletion. Only opaque project/source identifiers and removal time remain in native history. Sources, other collections and shared models are preserved. Unknown files, links, active leases and ownership failures defer cleanup with explicit reporting. Registry format 10 brings previously removed registrations into this cleanup path; older runtimes cannot open that registry schema.

A missing source folder is confirmed by two metadata probes at least ten seconds apart while its immediate canonical parent is accessible. Missing drives/ancestors, redirected paths and permission errors remain unavailable rather than authorizing removal. This minimum interval is not a cleanup deadline. Explorer keeps active evidence choices independent of Management's removed-project filter.

## MCP status, connections and Explorer

The unified connection exposes **27 tools**. `status` reads bounded scalar publication/readiness metadata without process hashing, detailed work probes or editor startup. Its two-second deadline reports unknown readiness on timeout. Successful snapshots may be cached for less than five seconds, with observation/cache age disclosed. `diagnostics` supplies detailed runtime identities, jobs, queue, worker progress, watcher state and optional Explorer attachment. Both are read-only. Neither cached status nor diagnostics proves live source bytes match an indexed publication.

Managed agent entries use the stable launcher and `--root SOURCE mcp`. Requests resolve the active registration for that source while preserving configured root isolation and exact publication validators. Missing, removed and ambiguous registrations remain distinct. `--index-collection` explicitly disambiguates multiple indexes. Explorer's internal session stays pinned to its reviewed binding. Reconnect existing agents after upgrading to load the new runtime and catalog.

Overview and Management share a 15-second status-poll deadline and reject late observations. Local log actions remain responsive independently. A watcher awaiting its first observation is unconfirmed; confirmed backend failures remain warnings. Published index counts remain visible. The Service tab omits rows whose entire value is Not observed; zero, No and none remain visible.

Unacknowledged management requests retain their original journal identity and undergo read-only receipt checks in the background. Explorer no longer presents request-confirmation banners or retry/forget controls. Unknown requests are never replayed automatically and do not globally block new actions. Native failures and lifecycle errors retain separate reporting.

Local raw diagnostics include bounded setup, connection, polling/action and job errors. Each logical stream uses FIFO segments: an append exceeding 200,000,000 bytes removes oldest complete segments until the resulting stream is at most 100,000,000 bytes. Open/Save handles empty and large captures without waiting behind status polling. Logs may contain local paths; nothing is uploaded automatically.

## Index-directory migration and compatibility

Installation renames a registered historical default `data/service-1.4` index root to `data/index-v1`, including corresponding Linux and preview paths. CLI coordinates native process drain; store journals ownership edits, the directory rename and path-derived registration updates. Collection ownership preserves its original identity input, so publication tokens, index bytes, settings and cleanup guards survive. A namespace-bound completion receipt selects the renamed root on later discovery.

Interrupted migration resumes before service startup. Live owners, redirected paths and occupied destinations are refused without merging or deleting either root. Custom roots remain in place. An exact saved storage preference for the missing historical default follows the renamed root. Historical runtime/state directories and internal compatibility filenames remain separate. Do not rename or merge index directories manually.

Index-v1/service-v1 identities remain independent of application versions. The bundled CPU-selected INT8 models and DiskANN engine from 1.6.0 remain unchanged; this patch alone does not require rebuilding an otherwise compatible index.

## Validation and publication evidence

Local Windows validation on October 29, 2026:

- Formatting, all-target/all-feature workspace Clippy with warnings denied, full all-feature workspace tests, default-feature doctests, version agreement, repository and whitespace checks passed. Existing ignored workload tests remain ignored.
- The native-enabled extension suite passed: 413 tests, two platform-specific skips. Python contracts completed 186 tests with four environment-dependent skips; after the final fixture correction, all 20 tests in the owning service-client suite passed, including the new removal-receipt regression.
- The optimized 1.6.4 runtime pair, native ZIP and Windows x64 VSIX passed version, source/payload identity, embedded-model, archive-inventory and package verification. Real helper inference, DiskANN/default/vector/hybrid retrieval, opt-out/re-enable, invalid-input rejection, immutable model inputs, revision identity and source preservation passed.
- Native client qualification passed for two isolated projects with 16 files each, 27 public MCP tools, 17 native aliases and real offline inference. Frontend/CLI acceptance verified pre-publication diagnostics, source-bound connections, runtime/build identity, exact request recovery and retained evidence with an unavailable source ancestor. Code indexing, document-table and usage-input checks passed.
- The first storage campaign timed out because its old expectation required quota pressure to end in a failed job. The corrected campaign observed the native durable quota wait, restored capacity and verified completion of the same accepted job without another index request. It also verified complete removal, minimal removal proof after detailed receipts were pruned, fresh settings on re-registration and source preservation. Native behavior and assertions were not weakened to accommodate the fixture.
- Installer acceptance scripts now follow the 27-tool, source-bound and diagnostics contracts. Outer scripts and the generated editor driver passed syntax checks; the GUI installer/upgrade scenario itself was not run.
- Notices were generated for 365 dependencies. No dependencies were added. The three existing publisher-declaration-only entries retain their original unresolved standalone-text review status in [the pinned license provenance](../third_party/licenses/sources.json).

The complete branch diff and validation evidence received an explicit primary-agent review covering process ownership, installation/payload roots, migration/removal recovery, worker admission, read/receipt contracts, callers, CI selection and release provenance. This was not an independent review. Historical candidate evidence remains separate; local Windows checks do not qualify installed macOS/Linux behavior or manual GUI acceptance.

The public release promotes only the complete candidate set from successful CI on the exact merged main commit. GitHub publication verifies signed Windows/macOS downloads, all native/VSIX pairs, checksums and provenance; Marketplace publication verifies all four platform packages separately. Publication results, workflow links and remaining manual installed-editor/platform limits are reported with the release. No independent review, manual GUI acceptance or full opt-in platform qualification is implied by routine CI.
