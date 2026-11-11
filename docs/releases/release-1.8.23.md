# 1.8.23 candidate preparation — empty installation and explicit indexing

**Stopped source checkpoint (10 November 2026).** Manual installation of the
1.8.23 candidate reported an HTTP cleanup refusal. The native eligibility fix
is committed but untested. No installer was rebuilt for that fix. Installer
repair, builds and qualification are stopped at the user's direction; retained
artifact receipts describe their original inputs.

**10 November 2026; Windows debug manual-test candidate built and package-verified.**
The Windows debug runtime, native ZIP and VSIX have been built and package-verified. Installed-editor acceptance and full release qualification remain pending.
Native/extension source manifests and lockfiles are synchronized at 1.8.23;
Generated CLI reference, compilation, packaging and artifact checks passed for this Windows debug candidate. This record does not authorize
publication or relabel an existing binary.

The previous [1.8.22 artifact](release-1.8.22.md) failed manual acceptance:
Reconnect created an index before rejecting a valid singleton combined search
scope. Its version, hashes, failed receipts and observed process identities stay
unchanged. The [manual-test task ledger](../simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
retains the investigation, reproductions and exact source-validation receipts.
Scoped checks and package verification do not establish installed-editor or full release qualification.

## Prepared behavior

- **Installation starts empty.** Through 1.9.0 inclusive, installation must
  finish uninstalling the owned predecessor before successor setup. Installing
  the same build also requires full uninstall. Saved indexes, indexing jobs and
  watching are not recreated. New indexing requires explicit Index library
  setup. The `airk-state/1` format baseline remains 1.8.16.
- **Reopening reconnects.** A normal editor reopen does not repeat installation.
  Native operation and host-generation identities fence concurrent windows and
  interrupted recovery; retry resumes the same accepted operation.
- **Cleanup requires proof.** Verify owned process exit before retiring
  executable names and deleting proven installation, state and index-storage
  targets. Unproven shutdown or incomplete owned cleanup blocks successor setup.
  Source files and unrelated/unowned data stay untouched. Unknown historical
  residue stays reported; this is not a universal purge of all historical layouts.
- **Interrupted directory moves are not runnable rollback.** A failed move can
  restore directories after executable names have already been permanently
  retired. Resume the recorded installation operation; restored directories do
  not establish a runnable predecessor or completed cleanup.
- **Reconnect cannot create an index.** It waits for explicit setup when no
  index exists. It recognizes reviewed single and combined native selections,
  matches actual index metadata and enrolls only the matching index. A combined
  selection's first display folder is not authorization for another member.
  Ambiguous/incomplete inventory and uncertain outcomes remain explicit.
- **Historical client references remain visible.** Replacement retains proven
  launcher references even when their executable files are already absent, so
  affected entries can be captured for reviewed reconnect. Configuration writes
  retain ownership, exact-preimage and unsaved-edit checks.

## Pending evidence

Windows manual-test VSIX (debug runtime):
`artifacts/one-process/bounded-extension-1823-final/publish/airk-1.8.23-win32-x64.vsix`.
SHA-256: `9913a08e1fc6323b7dff01b6a26e8857e87f0cddd59e6f8bbeaa234690329b89`.
Package verification passed. The mocked extension suite passed 1,404 tests with
21 gated skips; the final packaged-runtime native UI selection passed all four
tests without failures or skips. Actual CLI predecessor-exit and empty-successor
checks passed. This debug candidate does not establish optimized release,
installed-editor or cross-platform qualification. A pre-existing structural
boundary check failure remains recorded in the task ledger.

The [task ledger](../simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
is the source-validation record; each receipt keeps its own source and artifact
identity. No passing earlier build qualifies 1.8.23.

Version synchronization and matching native/VSIX package identities passed. Remaining gates include installed-editor empty-install and
explicit-index/reconnect acceptance, same-build reinstall and normal-reopen
acceptance, actual client transport checks, required repository qualification
and each claimed platform's lifecycle/signing evidence. Record observed results
and unavailable checks before claiming completion. Windows results do not
qualify macOS or Linux, and publication requires separate authorization and
successful release workflows.
