# AIRK 1.8.8 manual-test working build

## Post-P14 removed-source recovery (6 November 2026; qualification in progress)

The working source adds explicit retirement after source or storage loss,
ownership-checked absent-storage cleanup and removal of current Project/default
references while preserving frozen grants and healthy indexes. A fresh 1.8.7
upgrade can separately review an unavailable old index, save its exact native
authorization and explicitly resume the same candidate. Logical Applied,
physical cleanup, activation readiness and evidence authority remain separate.
See [guarded retirement](../service-project-retirement.md) and
[upgrade recovery](../service-installation.md#unavailable-legacy-index-during-upgrade).

The first post-P14 focused run failed; corrections and current-source validation
remain in progress. Earlier P14 evidence below retains its original source,
package identities and platform limits. It does not qualify this added behavior
or the final current-source local test installer. Version 1.8.8 is unpublished.

**6 November correction checkpoint:** Native source
`6de490d57d7053cbd63862366e840cc3657df4ef10e8a57c4ca018e6d19a2633`
/ 968 inputs adds logical cleanup for valid registered-but-never-started indexes
without creating ownership or storage. The preceding full Windows run on
`4292d19f...` finished **2,651 passed / 7 failed**: six metadata-only retirement
cases and one stale generated CLI reference. The corrected focused selection
passed **47 tests** in **438.126 s** of owner elapsed time. Current-source
42-case Neo, fresh matched DEBUG/process and full 2,661-test qualification remain
pending at this checkpoint. Earlier source results retain their original
identities; these focused results do not qualify the final local test installer.

### 6 November 2026 - selected current-source recovery validation

On native source
`6de490d57d7053cbd63862366e840cc3657df4ef10e8a57c4ca018e6d19a2633`
/ 968 inputs, the corrected Windows recovery selection passed **47 tests**.
The fresh matched DEBUG pair passed all **four** actual public 1.8.7 predecessor
process scenarios: source-only loss, source and storage loss, returned storage
and foreign returned storage. The storage-loss scenario also passed the three
compiled-host read-only review/authorization/applied receipt comparisons.
Complete CLI/MCP bodies, healthy-index evidence, source preservation and native
teardown proofs remain part of the scenario evidence. These are development
runtime results, not optimized installer or installed-editor qualification.

The same source passed **42 selected Mac ARM tests** across 20 owning Cargo
commands plus formatting, using the saved Neo SSH setup and offline assets.
Logs and Cargo timings were retained before mandatory `cargo clean`, which
exited zero; the exact owned target was confirmed absent. This selected portable
coverage does not establish full Mac workspace, signed distribution, Intel or
Linux qualification. Earlier source results keep their original identities.

A fresh dependency-policy check passed in **7.970 seconds** with live advisory
data and locked vet imports. It reported three findings covered by existing
maintainer-reviewed exceptions: unmaintained `paste` 1.0.15
(`RUSTSEC-2024-0436`), unmaintained `ttf-parser` 0.25.1
(`RUSTSEC-2026-0192`) and the moderate `minimatch` 10.2.6 packaging-tool advisory
(`GHSA-g3hv-75wg-m249`). This is a policy pass with disclosed exceptions, not an
absence-of-findings claim or an independent audit of every dependency.

The completed full Windows run on this source executed **2,661 tests**:
**2,660 passed / 1 failed**, with **51 selection skips**. All four restored P14
long campaigns passed. Nextest ran for **1,652.597 seconds**; its owner elapsed
time was **1,741.196 seconds**. The sole failure was the public-command test-owner
inventory: canonical reference regeneration exposed five new retirement commands
missing their external process-test ownership entries. This completed run remains
failed evidence; selected recovery passes do not make it a passing workspace gate.

After complete collection, a reviewed test-only correction registered those
actual process-harness owners and made command discovery independent of stale
generated documentation. The corrected test-source identity is
`edde8163ee9b796a11723f5c94db2a6417c4fddc5bf204dec32573fa3d35ad46`
/ 968 inputs. Its scoped validation and production-input identity review are
recorded separately from the original full run and its retained JUnit.

Final optimized runtime, package, local installer and installed-host results
require their own source/artifact evidence. Version **1.8.8** remains unpublished;
manual acceptance, promotion and publication remain separate.

### 6 November 2026 - final local test-package checkpoint

The corrected owning `cli_commands` suite passed **all 3 tests** in
**30.6086 s** on test source `edde8163...` / 968 inputs. Independent review
verified that only this external test file changed from `6de490d5...`; production
inputs were unchanged. The completed **2,660 passed / 1 failed** full run above
and the corrected complete owning-suite pass were accepted as combined local
test evidence. The original full run remains failed; this is not a claim of one
all-green full invocation.

Current strict workspace Clippy passed in **14.327 s**. The lightweight main,
workspace build and serial doctests passed in **15.1054 s**, **20.689 s** and
**15.5437 s** respectively; doctests covered **8 targets / 0 eligible examples**.
The fresh optimized Windows pair built in **853.0671 s** with incremental
compilation disabled. Its complete **675-file** runtime seal is
`f3d00b0dbbf0526db9da42c18d4120285437a52e21deefcce8518d82610895b1`.
Canonical extension compilation passed in **11.8519 s**.

The first full extension run remains failed evidence: **812 passed / 2 failed /
3 skipped**. Its private console required separately recorded owned-process
cleanup; that intervention is not a successful native Stop receipt. Test-only
fixture corrections preserved typed scope refusals and grant authority, retained
primary failures during cleanup, and exercised root metadata separately from
catalog-grant evidence. The final full run over **87 Node test files** passed:
**820 total / 817 passed / 3 skipped / 0 failed / 0 cancelled**, in
**213.642421 s** of Node time (**214.310321 s** owner elapsed), against the same
final optimized runtime and packaged production bytes. The native model
client check passed with **2 projects / 16 files per project**; isolated
installation and the complete assistance scenario passed without increasing
the original assistance deadline.

The local Windows **1.8.8** VSIX is **77,549,374 bytes / 775 entries**, SHA-256
`1e790218aa7acbbf057e55e51d246197e99f951e16fcc2549d066d07be0da519`.
Its native archive is SHA-256
`76492fe9f7743b56357bb0bbd15fa40a2c7cf90bc3c127d01bc0950815ca8dbc`.
Packaging verified all **675 native files**, **77 current production outputs**
and the exact frozen Marketplace README; private test artifacts are absent.
This is a local manual-test package, not a published release or a fresh installed
VS Code host qualification. Original Mac/debug source identities, historical
corpus deadline failure, unselected 100k/Linux coverage and other P14 limits
remain as recorded. Manual acceptance and release promotion remain separate.

### 6 November 2026 - setup and uninstall follow-up

This extension-only follow-up addresses the setup and uninstall issues reported
by omar16100 in [PR #60](https://github.com/iokaio/airk/pull/60), adapted to the
current explicit Index/Project setup and scope-selection contracts.

- Successful setup and healthy reconnection clear obsolete failed-startup state;
  current failures remain visible without hiding reload, Stop or native-execution controls.
- Untrusted-workspace and empty Workbench messages explain the next user action
  while diagnostic authorization codes remain available.
- Unix install-directory guidance distinguishes foreign ownership from group/other
  permissions. Native ownership checks remain authoritative; setup never changes
  a directory's permissions automatically.
- Shell-profile cleanup merges creation receipts before editing each file, removes
  only empty files AIRK created, and preserves user content and unsafe/unknown files.
- VS Code controls uninstall-hook scheduling. Another restart is conditional on
  pending removal; a missing cleanup receipt never establishes success.

The existing maintainer-requested working version remains **1.8.8**. These changes
introduce no native protocol, Rust dependency or permission relaxation.

Selected follow-up validation passed: TypeScript/webview compilation; the full
87-file Windows Node suite (**828 passed, 4 skipped**, 832 selected); and the
script suite (**480 selected, 9 skipped**). The four owning suites passed on
Windows (**158 passed, 1 skipped**) and the Mac Neo over saved SSH (**156 passed,
3 skipped**), including the actual POSIX directory-permission case on Mac.
Windows used Node **24.21.0** and Neo used **26.3.1**; these runs do not establish
qualification on the documented pinned Node version. Neo `cargo clean` exited
zero after testing and its owned target was absent.

All **968 native inputs** and **675 retained runtime files** matched their prior
identities, so the optimized native pair was reused with its original compiled
identity. No fresh Rust build, Clippy or native workspace campaign was run for
this extension-only change. Version, repository and whitespace checks passed.
The corrected local Windows VSIX was packaged and its **77 compiled files** and
**675 runtime files** verified. Actual installed-editor uninstall scheduling,
Linux and release publication remain separate qualification.

## Retained local phase checkpoints

The unpublished **1.8.8 manual-test working build** includes accepted local
P9 permanent exclusions, P10 reviewed index splitting, P11a per-file profile
reporting and P11b explicit Lean/Rich retained profiles. Existing indexing levels
and reply defaults keep their meanings. These development checkpoints do not
constitute a published release or final installed-editor acceptance.

P12 adds accepted local **reviewed reference refresh and separate requery**.
Its focused Windows native selection passed **39 tests**, including actual
native/CLI/MCP shared-helper, Lean/Rich, pause, lost-reply and restart paths.
The 12-stage Windows release qualification and 19 scoped Neo tests passed;
Neo testing ended with `cargo clean` and its target removed. Primary review
accepted this unpublished local phase. The first wave refreshes one existing index
from eligible references and requires full-index metadata authority; partial
Project/path grants are unsupported. Evidence reads and source verification do
not start indexing.

P13 retained current ranking and diversity; no candidate was adopted. P14 selected local qualification results below use source `237f8969...` / 960 native inputs.
The current and historical observations below retain their original identities.

**Prior source `5d8912b1...` evidence (6 November 2026):** Windows full Nextest passed **2,626 tests** with **50 selection exclusions**; its fresh producer took **764.1665 s**, and all **15 final R4 stages / 84 unchanged gates** passed, including **459 Python tests / 9 expected skips**. All **10 selected broader stages** passed by exact evidence adoption; the original failed queue stays failed. The real-corpus campaign remains a deadline failure with partial coverage. Its native/extension/VSIX package stages, `same`, `different`, `multi` and `untrusted` editor scenarios passed on their original native/package identities. Failed usage Stop and initial legacy attempts are retained. These observations do not become current-source qualification by relabeling.

Current native source `237f8969...` / **960 inputs** adds bounded native installation-lock admission before accepting lifecycle work. Only typed installation-lock contention can wait within the **60-second admission budget** before Stop/Restart mutation acceptance. After acquiring the lock, native code rechecks the same instance/runtime and retains the lock through authority checks and completion. Submitted actions are never replayed; generic text and other failures do not become retryable. Scoped CLI Clippy passed in **32.292 s**, the unaccepted-state timeout regression passed in **36.053 s**, a fresh matched debug pair built in **90.262 s**, and the actual held-lock Stop integration passed in **29.522 s**. The current optimized producer passed in **734.0468 s**; native packaging passed in **15.255 s** and VSIX packaging in **17.183 s**. The fresh current-source legacy fixture passed in **68.399 s** and its whole installed-editor scenario in **33.561 s**, including native Stop and canonical cleanup. The subsequent whole usage scenario passed in **97.960 s**, including its native Stop and cleanup. A preceding **259.85 s** quick-pick failure remains unresolved and intermittent; the successful rerun used observational diagnostics, with no product picker correction or cause claim. All six selected editor scenarios are covered using four retained old-source scenarios and fresh current-source usage/legacy. Current-source Windows qualification passed formatting (**6.244 s**), strict workspace Clippy (**24.3543 s**) and full Nextest: **2,627 passed / 0 failed / 51 skipped**, with no retries. All four restored long campaigns passed: compact reads (**246.170 s**), retention plateau (**146.632 s**), sparse catalog edits (**29.658 s**) and manual backlog (**108.021 s**). Nextest ran for **1,542.644 s**; its wrapper took **1,617.6504 s**. Serial doctests passed across **8 crate targets** in **18.6745 s**; there were **0 eligible examples**, not eight executed examples. Immutable JUnit and Cargo timing reports retain this current-source run. These are selected local qualification results; manual acceptance and release promotion remain separate. Earlier D4e
passes (2,624 tests, 15 final stages and 15 repeated native smoke suites) retain
their source and limits. Deleted-source recovery and the local test installer
remain unfinished. Version **1.8.8** is unpublished.

**Dated, superseded P14 evidence:** source `d4e48a53...` included the cold-default model correction. Its 15 final Windows stages, 2,624-case workspace suite and two scoped Neo cases passed on that earlier source. These records retain their original identity and do not qualify the newer producer. The P12 record below likewise retains its original source and limits. P14 had not yet been accepted at that dated checkpoint.

The [refresh guide](../source-verification.md#explicit-reference-refresh)
keeps native mutation review separate from read-only verification and MCP evidence.
The [implementation ledger](../research/implementation-v190.md) records accepted
P9–P12 local checkpoints on source `4121692a…` / 960 native inputs. The
matched Windows release pair built in **815.503 seconds**; all **12** final
qualification stages and **19** scoped Neo tests passed. Neo `cargo clean`
exited zero and removed the target. These results do not qualify a published
release, installed editor, default ONNX provider or the remaining P14 matrix.

On 3 November 2026, the maintainer selected **1.8.8** as the working version for
completing and manually testing the [1.9.0 implementation plan](../research/plan-v190.md).
Native crates, workspace/local lock entries, extension and npm root lock entries
are synchronized. Generated extension catalogs and fresh native pairs must carry
this identity too. This working build is unpublished.

P6b adds durable helper cooldown recovery, measured healthy reset, native exit
codes and isolated worker-grant repair. Local Windows/Mac ARM gates and all 84
unchanged-bound latency comparisons passed;
see the [recovery contract](../service-embedding.md#helper-recovery) and ledger.

The completed P1–P4 implementation includes compact plan repairs, independent
Lean/Rich replies, evidence guidance and saved-source verification with **Report**
as its default. The [implementation ledger](../research/implementation-v190.md)
retains the original source/runtime identities, dated qualification and limits.
Earlier 1.9.0 development measurements belong to those exact earlier binaries;
they do not qualify a newly built 1.8.8 pair.

P5's foundation passed local isolated qualification: normalized catalog metadata,
backup/migration/import/recovery, candidate-process handoff and a gated installer
coordinator. P6f's working candidate activates only qualified authority-1 catalogs
with reviewed Indexing/setup/grant consumers. P7 adds combined reads and P8 adds
Recommendations. P9 exclusions, P10 splitting and P11a/P11b profile work are
accepted for unpublished local development, including P12 reviewed refresh. Earlier dated receipts retain their original scope. See the [storage boundary](../storage-formats.md#development-catalog-boundary).

Complete implementation and local gates, then manually test the identified 1.8.8
native pair and VSIX before the CI/release handoff. Use explicit isolated testing
locations and retain logs and payload identities. Existing version/schema downgrade
guards remain in force. No installed runtime or extension is changed by the source
version adjustment.

After manual acceptance, promote the source to **1.9.0**, regenerate catalogs and
rebuild artifacts under that version; retain the 1.8.8 evidence as history. Full
CI and the authorized Marketplace/release workflows must qualify the promoted
source and artifacts. Do not relabel existing binaries or publish this working
build as an implementation shortcut. No CI run or publication is authorized by
this working-version adjustment.

At the P5 checkpoint on 3 November 2026, native workspace tests passed **1,939, 37 skipped**; Clippy passed with
warnings denied and all eight doctest targets passed (zero executable examples).
The actual staged release-pair coordinator, all five catalog transport tests and
both four-core managed smoke suites passed. Extension tests passed **547, 19
skipped**, and Python passed **309 cases, six skipped**. All **60** task-latency
comparisons and **16** dedicated verification comparisons passed unchanged limits;
all **42** non-latency relevance comparisons matched P4. The ledger records exact
source/payload identities, commands, failure history and qualification limits.

The dependency advisory audit could not complete because the advisory cache was
not writable and the cached fallback yielded an unusable npm report. No clean
advisory result, installed-editor acceptance or macOS/Linux qualification is
claimed. Isolated fixture installations are test evidence, not changes to the
user's installed runtime. The P5 commit and feature-branch push included both dated
issue-analysis reports, followed by the requested review stop.

After review, the maintainer authorized continued phased implementation, with a
signed commit and feature-branch push for each phase and no PR until all phases
are complete. P6a's standalone Mac launcher layout, installer
publication coordination, shared signed release producer and offline bootstrap
passed local source gates. Current results, failures and remaining Mac
qualification are recorded in the [ledger](../research/implementation-v190.md).
The exact final R7 Mac pair passed fresh Developer ID signing, independently
verified standalone copied-launcher execution and its isolated installer lifecycle.
Windows passed 1,945 native tests
(37 skipped), and Python passed 360 (nine skipped). These changes add no
retrieval-time model or indexing work. All 60 unchanged latency comparisons and
the four-core Windows managed/client/installer smokes passed. Notarization,
installed-editor and remaining platform qualification remain release gates.
No CI dispatch, release publication or change to the user's installed
runtime has been performed.

## Dependency supply-chain checks

At the maintainer's request, this work also adds `cargo-deny` and `cargo-vet` to
the contributor and dependency-review workflow. The reason is to make dependency
policy and review decisions reproducible and visible in the repository:

- `cargo-deny` checks known advisories, allowed licenses, dependency sources and
  dependency bans against a committed policy.
- `cargo-vet` records the versions covered by explicitly trusted imported audits
  or documented review decisions, while keeping unreviewed baseline dependencies
  visible as exemptions. An exemption is not an audit or a safety certification.
- New or changed Rust dependencies require a reviewed policy/audit decision; pinned
  Git revisions must remain visible even when a package version does not change.

These are development/release checks and add no AIRK runtime work. They complement
the existing Rust/npm advisory checks, license notices and pinned native/model
asset inventories. Rust-only checks do not establish the safety of npm packages,
downloaded native libraries or model files. Locked checks provide reproducibility;
reviewed database/import refreshes are still necessary to obtain newer findings.
Tool versions, actual results and remaining review gaps belong in the
[implementation ledger](../research/implementation-v190.md); configuration alone
does not establish that the dependency graph has been audited.

The final fresh policy run passed on 3 November 2026 with the three existing
dated advisory dispositions. The exact locked vet graph retains 46 fully covered
imported paths, one partial path and 319 baseline exemptions. Offline checks
remain explicitly limited and do not check npm advisories; see the
[setup and policy limits](../dependency-advisories.md).
