# AIRK 1.8.8 manual-test working build

P9 adds [permanent source exclusions](../source-policy.md)
through reviewed native management. Native policy fences precede removal;
logical removal and physical reclamation have separate resumable observations.
Windows workspace/runtime and focused Mac ARM checks passed with the limits in
the [ledger](../research/implementation-v190.md). P10–P14 acceptance remains pending;
this working build is unpublished.

P8 adds [Recommendations and copied agent scope instructions](../recommendations.md)
through the canonical native `query_scope` read, CLI/MCP aliases and Workbench
plans/forms. Indexing renders available/selected/omitted scope, explicit temporary
selection and separately reviewed defaults. Missing observations remain unknown;
split actions remain pending P10; permanent exclusions use reviewed P9 management. [Validation and limits](../research/implementation-v190.md#p8--recommendations-and-harness-guidance)
are recorded separately from release qualification. The working version remains 1.8.8.

P7 implements [combined Project reads](../combined-reads.md) across native, CLI, MCP and Workbench, including plans, sessions, verification and exports. Windows release and Apple Silicon debug native checks passed, along with all 84 unchanged legacy latency gates and 120 Windows fan-out trials. [Evidence and platform limits](../research/implementation-v190.md#p7--combined-retrieval-and-consumers). Working version **1.8.8** remains unpublished.

P6e makes [oversized stdio input](../mcp.md) an explicit bounded failure and uses [qualified Workbench publication comparisons](../workbench/implementation.md) instead of treating unequal tokens as older. It adds no status polling and preserves retained exports. Local qualification and release limits remain in the implementation ledger.

P6d's [source admission changes](../service-freshness.md) give eligible source work priority over pending Vector jobs and yield running Vector turns at safe boundaries. Saved semantic work resumes afterward. Actual Windows and Mac ARM backlog scenarios passed; the implementation ledger records final qualification and remaining platform limits.

P6c's [search execution contract](../search-execution.md) discloses policy-qualified lexical results when the embedding helper is unavailable, preserving strict explicit semantic requests and independent source-verification receipts. Local Windows/Mac ARM gates and all 84 unchanged-bound latency comparisons passed; the ledger records exact coverage and remaining release limits.

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
Recommendations; trim qualification and split implementation remain pending P9/P10. See the [storage boundary](../storage-formats.md#development-catalog-boundary).

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
