# AIRK 1.6.3 status and index-directory migration candidate

Historical local candidate record. This candidate was not published; its changes are consolidated in the [1.6.4 public release](release-1.6.4.md). Validation below retains its original version and scope.

Native and extension versions, Cargo.lock and the npm lockfile are synchronized at **1.6.3**. This is a local candidate record; publication and installed cross-platform qualification are separate.

This release also includes the previously unpublished [1.6.1 installer, MCP and diagnostics fixes](release-1.6.1.md) and [1.6.2 project-removal and registration fixes](release-1.6.2.md). Their dated validation records remain historical candidate evidence.

Bundled extension and native MCP help describe the 27-tool catalog, lightweight status versus detailed diagnostics, source-bound agent connections and reconnect requirements, automatic project cleanup, request receipt handling and index-directory migration.

Explorer distinguishes a source watcher that has not reported since startup from a confirmed backend failure. Published index information remains visible while live-update status is checked. Confirmed failures remain warnings.

Unacknowledged requests remain in the internal journal with their original keys and are logged locally. Read-only receipt checks continue in the background. Indexing and Recovery no longer display request-confirmation banners, request IDs or retry/forget controls. Failed receipt checks do not prevent loading current project observations, and unresolved records do not globally block new actions. Unknown requests are never replayed automatically. Actual native job failures and lifecycle problems retain their separate reporting.

The Service tab omits fact rows whose entire value is `Not observed`, including case variants. Zero, No and none remain visible. Diagnostic data remains complete.

Installation renames a registered historical default index-data root from `data/service-1.4` to `data/index-v1`, with the corresponding Linux and preview paths. CLI coordinates migration after native process drain; store journals the exact ownership edits, renames the directory and updates path-derived project bindings and all matching service references in one transaction. Collection ownership retains its original identity input, preserving published revision tokens, index bytes, settings and cleanup guards. The namespace-bound completion receipt selects the renamed root on subsequent discovery.

Interrupted migration resumes before service startup. Live owners, redirected paths and occupied destinations are refused without merging or deleting either root. Custom data roots remain in place. The extension also updates a saved storage preference that exactly names the missing historical default, so later project registration follows the renamed directory; other custom locations and existing roots are preserved. Historical runtime-install/state directories and internal compatibility filenames are outside this index-data rename. See [storage formats](../storage-formats.md).

## Local validation — October 29, 2026

- Native/extension/lockfile version agreement at 1.6.3, formatting, workspace Clippy with all targets/features, repository checks and whitespace checks passed.
- The original workspace test command completed 1,040 passing tests with 16 ignored before finding the new migration regression in the store suite. After correcting shared-lock inventory and durable-current-grant admission, the complete store suite passed: 411 tests with one ignored. Workspace doctests and 82 native installation tests also passed. The final correction was checked with the owning suites rather than repeating all completed workspace suites.
- The native-enabled extension suite passed 413 tests with two platform-specific cases skipped (POSIX startup and helper replacement). After the final storage-preference caller refinement, all 37 startup/workspace/onboarding/native-status tests passed again.
- Python tooling tests passed: 186 tests, four skipped. Notices were generated for 365 dependencies; the tool retained release-review notices for three dependency license declarations without separate license text. No dependencies were added by these fixes.
- The release runtime pair and native ZIP passed version/inventory/self-test checks. The service/client check covered two isolated projects, 16 files per project, 27 MCP tools and real offline model work. Transformer qualification passed real helper inference, persistent DiskANN/default/vector/hybrid retrieval, opt-out/re-enable, invalid-input rejection, immutable model files, revision identity and source preservation.
- The Windows x64 1.6.3 VSIX was packaged and verified against its native candidate, version, payload hashes and archive allowlist. Artifacts remain local.
- An explicit primary-agent review covered process drain/resume, native ownership and quota contracts, request receipts and deadlines, saved editor/storage bindings, failure logging and validation evidence.

Actual upgrade of the user's installed default directories and VS Code GUI installation were not performed. macOS/Linux installed qualification, hosted CI and publication were not run.
