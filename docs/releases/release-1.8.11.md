# AIRK 1.8.11 local working build

**7 November 2026 — unpublished manual-test candidate.** The maintainer requested
**1.8.11** for the simplified local-user Indexing workflow, clear index deletion
and the VSIX update-detection fix. Native packages, extension and lockfiles must
agree before packaging. This authorizes a local VSIX build; publication and later
1.9.0 promotion remain separate. The [1.8.10 record](release-1.8.10.md) retains its
original source, package hashes, test results and failures.

## Simplified Indexing

| Tab | Task |
| --- | --- |
| **Search scope** | Check local indexes and index sets, then choose **Apply selection**. **Refresh choices** reloads current choices. |
| **Index library** | Create and manage indexes, inspect sources and settings, split when useful, and **Delete index**. |
| **Index sets** | Create and manage named reusable search groups using existing indexes or selected source scopes. |

The local developer no longer manages grants or saved window associations in the
normal UI. Native scope and access records remain internal to the existing host
and catalog owners. Checkbox edits are drafts until explicitly applied. Evidence
MCP remains read-only, and a window's choice does not silently rewrite an external
coding agent's configured scope. Native `Project`/grant API names remain for
compatibility; the UI calls these reusable groupings **index sets**.

**Delete index** opens **Delete this index?**. The explicit **Delete index and stop
maintenance** confirmation uses the existing native retirement request, stops
maintenance and removes current set/default references. Owned derived data is
cleaned up separately. The result confirms logical removal and explicitly says
physical cleanup completion has not been observed in that view. Source files, shared models and independent indexes remain
untouched. A differently shaped new index can cover some of the same source files
without splitting: use a distinct storage leaf, collection and backing identity.
The exact retired backing cannot be reactivated or used to clear its permanent
exclusion policy. See [setup](../service-project-setup.md) and
[deletion details](../service-project-retirement.md).

## VSIX updates

The installed extension observes its flat host/UI JavaScript and CSS payload,
including same-version UI-only rebuilds. An already open Explorer receives the
pending-update state and offers **Reload to finish update**. Choosing it loads
the updated host and UI together. Local installation scripts verify every
shipped flat JavaScript/CSS file against the VSIX. Metadata detection still uses
filenames, size and modification time; equal-size replacements retaining the
same timestamp can evade same-version detection. Installer verification checks
SHA-256 independently.

## Validation and package evidence

Earlier 1.8.10 package and test results do not qualify this candidate. The
current qualification record follows. Native and extension source checks are
complete; the optimized runtime and package steps remain in progress until their
results are recorded. Installed-editor acceptance,
macOS/Linux qualification and publication remain separate unless explicitly
recorded below.


## Development record

The maintainer rejected the former **This window / Projects** presentation as
confusing for a developer's own local indexes, selected **Search scope / Index
sets**, requested direct deletion and a fresh 1.8.11 package, then authorized
committing and pushing the journey without creating a PR. The source commit also
retains the earlier uncommitted MCP QA and 1.8.10 Indexing work. Their dated
release/review records preserve the original observations instead of treating
older packages as qualification of this source.

The host preserves the native ownership model behind the simpler controls.
Whole-index selections use exact-owned immutable supporting sets, with a
fingerprint cache for repeated choices; A → B → A reuses its supporting set.
An independent window's different choice cannot rewrite those memberships.
Named-set edits preserve existing partial-folder scopes. Exact saved selections
revalidate current memberships and source policy on reconnect. Stale choices
stay visible but inactive. Failed or uncertain mutations do not replay or silently reactivate a stale
selection; any restored grant must pass current native metadata validation. A restoration epoch
prevents late responses from replacing a newer selection. Deselecting everything
persists an explicit empty scope. Distinct choices and native grant history
remain subject to existing catalog capacity; this change adds no new garbage
collector.

Separate source reviews covered the scope/UI changes and the retained native,
MCP, storage and packaging journey. The primary reviewed the combined changes
and validation evidence. Reviews found and corrected pagination retention,
partial-membership preservation, cross-window scope interference, stale restore
races and failed-save status before packaging.

## Selected current validation

- `npm test --prefix packages/vscode`: host, webview and Assistance compilation,
  bundle/catalog drift check and **878 passed / 0 failed / 21 optional cases
  skipped**. The separately exercised native scope case is one of those optional
  default-suite skips.
- Actual matched-debug native scope preflight: **1 passed / 0 failed** in
  **41.360 seconds**, including four actual CLI scope reads for direct indexes,
  named sets, mixed selections and a renamed set. It also checks full-source
  memberships, native setup/publication and explicit empty scope without catalog
  writes. This is an actual native transport test, separate from renderer mocks.
- Actual native delete/recreate preflight:
  `cargo test -p airk-cli --no-default-features --test combined_reads --locked
  --timings explicit_retirement_allows_fresh_subset_index_without_split --
  --nocapture` passed. Test execution took **30.13 seconds**, wrapper including
  compilation **71.390 seconds**. It proves worker removal, physical cleanup,
  independent new subtree-index setup, unchanged original sources and unchanged
  healthy-index publication.
- Actual browser component review passed for all three screens, checkbox Apply,
  visible library deletion and removal of grants from the normal flow. Screenshots
  use fictional metadata and do not claim installed-editor acceptance.
- `python -m unittest discover -s scripts/tests -v`: **484 tests, 0 failures,
  9 expected/environment skips**, **31.779 seconds**. This includes installer
  hash verification against Explorer/Workbench assets.
- Repository checks, version synchronization at **1.8.11** and whitespace checks
  passed.
- Offline dependency gate passed in **19.714 seconds** with existing pinned tools:
  source policy, cached Rust advisories, Cargo deny and frozen Cargo vet. The
  advisory snapshot is dated 6 November 2026. Fresh Rust/yanked-package checks and
  npm advisory queries were not run; this is not a fresh release advisory audit.

Every Cargo step reused `artifacts/v190/p10-target`; Windows development steps
used `CARGO_INCREMENTAL=1` and retained Cargo `--timings` HTML:

| Command/scope | Elapsed | Result |
| --- | ---: | --- |
| `cargo fmt --all --check` | 5.728 s | Passed; new external regression formatted again before compilation. |
| `python scripts/dev-check.py --workspace --all-features --target-dir artifacts/v190/p10-target --skip-fmt` | 66.951 s | Strict workspace Clippy passed. |
| `cargo check -p airk-cli --no-default-features --all-targets --locked --timings` | 57.638 s | Lightweight main passed. |
| `python scripts/build-service-runtime.py --profile debug --destination artifacts/local/1.8.11-indexing-20261107/runtime-debug` | 134.908 s | Fresh matched debug pair and release notices passed. |

The debug native compiled-source seal is
`17a2a8b144653cb536561baa8f54d4f75b90810d141f058f3a49e982bb2e16be`
across **978 inputs**. Final catalog regeneration preserved that identity.
Timing reports, original outputs, native fixture receipts and browser screenshots
are retained under ignored `artifacts/local/1.8.11-indexing-20261107/`; Cargo
HTML remains in the persistent target's `cargo-timings/` directory.

Initial local failures remain recorded: the first Python run encountered an
in-progress UI encoding write and a newly allowlisted module not yet compiled;
the encoding was recovered against the exact original source hash before edits
were reapplied. Initial extension runs exposed a missing required `next` input
and outdated display-label/test assumptions. Three scope preflight failures
omitted required `query_scope.view` and/or inherited an unsupported
`response_mode` from the fixture helper. Those test inputs were corrected; the
native orchestration then passed. The successful final runs above are distinct
from those original failures.


## Complete Windows native workspace run

`cargo nextest run --workspace --all-features --locked --offline --timings
--profile ci --ignore-default-filter --no-fail-fast --retries 0 --test-threads 4`
passed **2,721 / 2,721 tests**, with **51 skipped** and **zero retries**. Execution
took **1,653.644 seconds**; the command including compilation took
**1,749.956 seconds**. JUnit, the exact command, source seals and Cargo timing
report were retained. The source seal above was unchanged before and after the
run. All four routine excluded campaigns were explicitly restored and passed:

| Campaign | Execution |
| --- | ---: |
| Large-catalog compact reads | 256.110 s |
| 200-edit retention plateau | 155.017 s |
| Sparse catalog edits | 31.333 s |
| Manual backlog | 118.775 s |

The run used a new child of the existing verified sandbox-owned fixture base,
with its inherited owner-only permissions, plus a process-local
`XDG_CONFIG_HOME` under this run directory. Existing identities, ACLs and global
configuration were unchanged. This satisfies the documented private-boundary
fixture requirement; the earlier 1.8.10 failures remain in their original record.
A post-run wrapper failed to print a Unicode summary through its CP1252 console
after persisting the native exit code **0**. The complete native log, JUnit and
receipt independently show the pass; the wrapper note is retained, and no tests
were rerun to replace that observation. The final formatting check passed again
in **5.766 seconds** before final lint/build checks.


Final post-regression checks passed using the same warm target, development
incremental setting and retained Cargo timings: scoped CLI strict Clippy with all
features/targets in **40.065 seconds**, and the required default workspace build
in **41.792 seconds**. JUnit independently reports **2,721 executed cases across
68 suites, zero failures and zero errors**; Nextest's 51 skips are recorded in
its command summary rather than emitted as JUnit cases.

Serial workspace doctests passed in **35.607 seconds** across **8 crate targets**.
There were **zero eligible examples**, not eight executed examples.
