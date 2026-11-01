# AIRK 1.8.4 local candidate

The owner requested a distinct 1.8.4 package on 2026-11-01 after a same-version replacement could not be qualified. Native and extension manifests, lockfiles and the offline template catalog are synchronized at 1.8.4. This is an unpublished Windows x64 candidate; no merge or publication is authorized by this local build task.

**Platform scope:** this candidate was built and tested on Windows x64. The owner's Mac was not accessed, and no 1.8.4 macOS or Linux build/test run was performed. Earlier platform results retained in the 1.8.3 record do not qualify this newly versioned artifact.

## Why the version changed

Several different local builds had retained version 1.8.3. The live retest could confirm a running 1.8.3 payload matched the installed extension's manifest, but could not establish that it contained the repaired checkout: the running native source digest was `c6616f37f97b79fdd3dcbf66bd55e69784471b461935c2ce90ecaab589fc37ec`, while the checkout used for passing focused regressions had digest `45190cf815b46824d25653c1701d0ef5bdb06e1ca19e1f5302fe5676aab988ec`. Both represented 747 inputs. The installed mixed-load status failure also persisted.

This evidence does not prove why the same-version installation differed. It does mean the version string and passing checkout tests were insufficient to accept those installed bytes. Version 1.8.4 creates a separate VS Code extension directory and requires a fresh compiled native pair, synchronized package identities and a new payload/VSIX digest. The resulting candidate can be identified and tested without overwriting the historical 1.8.3 receipts. Versioning itself is not proof that the behavioral fixes pass; validation below records that separately.

## Issues addressed

- **Mixed-load status supersession:** the installed 1.8.3 retest returned `status_observation_superseded` in 3 of 6 four-call batches, even though all 18 sibling evidence calls succeeded. The candidate distinguishes ordinary scalar invalidation from authenticated authority retirement. A completed same-authority query returns its original snapshot with an informational note; obsolete scope/worker authority and expired grants remain errors. Source tests cover original timestamps/publications, no replay, no stale cache/receipt acknowledgment and hard-fence rejection. Actual candidate-process checks must confirm the packaged behavior separately.
- **Concurrent cold evidence admission:** simultaneous reads can capture tickets before the first worker binding exists. Bounded queued reads may join exactly that first authenticated binding; they cannot be promoted across a later worker replacement, changed scope, expired grant or shutdown. Frontend and worker queues keep finite deadlines and refusal receipts, and canceled/unsubmitted work stays unsubmitted.
- **Windows developer-directory eligibility:** harmless read/execute sharing, creator-owner inheritance and partial SYSTEM/Administrators grants previously disqualified an otherwise owner-controlled native write boundary. These layouts can now qualify, while foreign write/delete/ACL/ownership grants and unsupported filesystems select strict verified reads. This changes optional read-path eligibility, not source write permissions, corruption checks or confidentiality guarantees.

The original S5 embedding-client struct lookup still returned empty, truncated navigation during the 1.8.3 retest. Conservative same-file declaration handling can refuse incomplete retained declarations; written targets remain syntax candidates. This limitation is retained rather than labeled as a repaired installed behavior. Concept ranking, universal reference recall, cold-start/performance claims and other-platform qualification are also outside the fixes established by that retest.

## Behavior and review scope

This candidate carries forward the unpublished [1.8.3 implementation](release-1.8.3.md): bounded admission waiting, scoped metadata filters, exact/suffix/contains discovery, reference paging/counts/file summaries, bounded definition/method pointers, and Workbench form persistence, exports and publication-fenced follow-ups. Written calls remain syntax candidates. The same-file Rust navigation limits and whole-query reference completeness limits remain explicit.

The status follow-up preserves a completed same-authority snapshot after scalar invalidation, with its original publication/time and an informational supersession note. It does not replay the query, repopulate the invalidated cache or acknowledge an obsolete receipt. Worker/scope replacement, source authorization expiry and shutdown remain guarded. Windows native write-boundary eligibility permits ordinary read sharing and creator-owner/partial privileged grants; foreign write/delete/ACL/ownership grants and unsupported layouts retain strict fallback.

The [1.8.3 installed retest](../mcp-improvements-test-results.md#replacement-installed-build-retest--2026-11-01) remains a failed qualification observation: three status errors across six mixed batches. Its installed native source digest differed from the checkout used by the passing focused tests. A passing source regression must not be presented as proof that those installed bytes were repaired.

Review covers the complete pending tracked/untracked public diff against the existing HEAD, including native contracts/adapters, paging/navigation budgets, process admission/status ownership, Workbench state/export guards, tests, CI selection, documentation and packaging provenance. This is a single-agent review, not an independent review. Existing unrelated work and historical evidence are preserved.

## Validation and build evidence

The local Windows x64 candidate completed validation on 2026-11-01. Logs, raw vector-batch responses, initial failures, review manifests and package receipts are retained under `artifacts/mcp-optimizations-1.8.4/`.

| Check | Observed result |
| --- | --- |
| Workspace formatting and all-target/all-feature Clippy | Passed |
| Full Windows workspace tests, all features, locked/offline | **1,821 passed, 0 failed, 31 explicitly ignored**; final run used the documented isolated owner-controlled census base |
| Separate workspace doctests and generated CLI reference | Passed; both CLI-reference checks passed |
| Extension compile and unit suite | **535 passed, 19 skipped**; native cases are qualified separately below |
| Python repository/CI/packaging contracts | **285 passed, 6 skipped** after process-local Git trust for this checkout |
| Repository, version synchronization and whitespace checks | Passed; AGENTS.md and CLAUDE.md remain byte-identical |
| License notices and dependency advisory policy | Passed, including existing dated advisory exceptions; not a claim of zero advisories |
| Compiled-browser Workbench forms and discovery/navigation | Both checks passed on the Windows host with fictional data; not installed-editor acceptance |
| Fresh release main/helper pair and real offline model | Passed; real inference, persistent DiskANN and default/vector/hybrid retrieval, input rejection, immutable runtime files, revision identity and source preservation |
| D10-style vector-backed MCP retest | **6 four-call batches / 24 single-shot calls passed** across source-bound and pinned connections; 3 status replies carried the informational supersession note; all 18 search citation hashes matched the original fictional source |
| Managed MCP smoke under four logical processors | **3/3 final rounds passed**, 7 tests each; the mixed case completed **36 batches / 144 measured reads**, with **zero unsubmitted refusals and no retries required** in these final rounds |
| Native clients and installed-process lifecycle under four processors | **3/3 rounds passed for each suite** on this same runtime; their successful suites were not repeated after test-fixture-only corrections |
| Native ZIP and Windows VSIX packaging | Passed inventories, executable versions, archive self-test, payload/hash checks and compiled-module/README/lock agreement |

The isolated source-bound and pinned vector retests retained the same service and worker identities across each batch series. Their final diagnostics reported `versions_match`, `builds_match` and `payloads_match` all true. This qualifies the observed candidate processes, not the user's normal installed editor or agent connection.

### Initial failures and test-fixture corrections

The initial workspace run stopped at three native-owner recovery tests because the shared temporary directory did not qualify as a native write boundary. An initial private-base setup also retained an unsuitable inherited ACL. A separately created directory with a canonical current-owner-only inheritable descriptor passed all three recovery cases, followed by the complete passing workspace run above. No existing directory ACL, production eligibility guard or source permission was weakened. A qualified isolated fixture is not evidence that every developer directory qualifies.

The first four-processor campaign was **8/9 suites passing**: the newly added mixed test treated every unsubmitted source-setup refusal as a product failure. Warming its evidence channel alone was insufficient; a smoke-only rerun was **2/3 passing**, with another `direct_query_setup`, `query_submitted=false` source-search refusal. Source-selector/setup admission can legitimately reject an observation before submission even on an established channel. Both runs remain retained in `artifacts/ci-local/20261101-134247-66e812cf/` and `artifacts/ci-local/20261101-135358-2fb0c011/`.

The final test uses the existing bounded typed-read helper and observes every refusal, rather than assuming unconditional admission. Its observer requires error receipts to be explicitly unsubmitted; unknown/permanent errors and **every submitted status failure remain fatal**. A new unit regression supplies `status_observation_superseded` with `query_submitted=true` and `retryable=true` and verifies exactly one request, with no replay. The final three-round process run is retained in `artifacts/ci-local/20261101-140022-f59a52b2/`; it required zero retries in its 144 measured reads. These corrections change fixture assumptions and observability, not the production refusal or status policy.

Git ownership restrictions, Chromium GPU subprocess restrictions and a read-only advisory-cache lock initially prevented individual checks in the sandbox. The Python suite passed with process-local trust for this checkout; browser and advisory checks passed on the host. No persistent Git configuration or contributor identity changed. Minor harness setup/compilation corrections and their outcomes are retained in `validation-notes.json` alongside the initial failure logs.

### Exact artifact identity

- VSIX: `artifacts/mcp-optimizations-1.8.4/vscode/publish/airk-1.8.4-win32-x64.vsix`
- Size: **70,767,733 bytes**; **763 ZIP entries**. The explicit inventory excludes `temp/`, private/index state and local validation artifacts.
- VSIX SHA-256: `83effe242993c3c37967559c019fc44972baeefb8bba4a169e2dcc1b6eadc98f`.
- Native source digest: `b356747fff3c1382071c50d61578506228cbd208b9030c42281e41df739b0694` (**747 inputs**), matching the live native checkout after compilation.
- Runtime payload/install-manifest SHA-256: `954972f90b9680fbb3a7ef48478524cb83182069c57a9344d896dc702db8b0c4`.
- `runtime/airk-build.json` retains both executable hashes, release profile and compiler identity; `native/publish/manifest.json` and `vscode/publish/manifest.json` bind the archives and package contents. The VSIX was reverified against the final compiled product modules, normalized Marketplace README and npm lock after test-only changes; no packaged binary or manifest was relabeled.

This is a dirty-checkout local candidate, not a publication input authenticated by main CI. **macOS and Linux 1.8.4 qualification, hosted native/container gates, actual installed-editor/manual Workbench acceptance, remote-share fixtures and the matched performance campaign were not run here.** Known conservative navigation/reference limits remain disclosed. No normal installation, commit, push, PR, merge or publication was performed.
