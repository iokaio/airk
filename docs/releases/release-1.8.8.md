# AIRK 1.8.8 manual-test working build

On 3 November 2026, the maintainer selected **1.8.8** as the working version for
completing and manually testing the [1.9.0 implementation plan](../research/plan-v190.md).
Native crates, workspace/local lock entries, extension and npm root lock entries
are synchronized. Generated extension catalogs and fresh native pairs must carry
this identity too. This working build is unpublished.

The completed P1–P4 implementation includes compact plan repairs, independent
Lean/Rich replies, evidence guidance and saved-source verification with **Report**
as its default. The [implementation ledger](../research/implementation-v190.md)
retains the original source/runtime identities, dated qualification and limits.
Earlier 1.9.0 development measurements belong to those exact earlier binaries;
they do not qualify a newly built 1.8.8 pair.

P5's foundation passed local isolated qualification: normalized catalog metadata,
backup/migration/import/recovery, candidate-process handoff and a gated installer
coordinator. Normal
catalog activation remains disabled until P6's Indexing/grant consumers are ready.
Project/index management, recommendations, combined queries, trim and split remain
subject to their implementation and parity gates. See the [storage boundary](../storage-formats.md#development-catalog-boundary).

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

Final native workspace tests passed **1,939, 37 skipped**; Clippy passed with
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
user's installed runtime. P6 implementation, CI dispatch and publication have not
started. The maintainer authorized the signed P5 commit and feature-branch push,
including both dated issue-analysis reports, followed by a stop for review.
