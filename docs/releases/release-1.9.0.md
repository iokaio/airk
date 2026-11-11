# Planned AIRK 1.9.0 release and development checkpoints

Post-P14 removed-source/storage retirement and fresh 1.8.7 legacy upgrade
recovery are implemented in the working source and under local qualification.
The [1.8.8 working-build record](release-1.8.8.md#post-p14-removed-source-recovery-6-november-2026-qualification-in-progress)
separates this follow-up from the retained P14 checkpoints below. The local test
installer and manual acceptance remain separate; no promotion is claimed.

## Retained phase-plan record

This is the planned promotion record, not a published release. The current
working version remains **1.8.8**. P9 exclusions, P10 splitting and P11a/P11b
profile work are accepted for unpublished local development. P12's explicit
[reference refresh/requery](../source-verification.md#explicit-reference-refresh)
passed 39 focused Windows native tests, all 12 final release qualification
stages and 19 scoped Neo tests with successful cleanup. Primary review accepted
P12 for unpublished local development; installed-editor and final plan acceptance
remain separate. P13 review retained current ranking and diversity; no candidate
was adopted. P14 now evaluates source `237f8969...` / 960 native inputs, adding bounded lifecycle
installation-lock admission after the cold
bundled-default and typed quota-read admission corrections. Prior source `5d8912b1...` passed 2,626 Windows tests with 50 selection exclusions,
focused checks, five Neo tests, fresh ARM64 release and Developer ID/copy checks,
isolated installed lifecycle and cleanup. Ten selected broader stages passed; the corpus deadline failure and partial editor results retain their scoped results. Selected local observations retain their documented scope. Dated D4e core passes do not qualify
the newer source. Deleted-source recovery and the local
test installer remain unfinished; no promotion or publication is implied. Exact accepted and historical evidence is in the
[working-build record](release-1.8.8.md) and [ledger](../research/implementation-v190.md).

**Prior source `5d8912b1...` evidence (6 November 2026):** Windows full Nextest passed **2,626 tests** with **50 selection exclusions**; its fresh producer took **764.1665 s**, and all **15 final R4 stages / 84 unchanged gates** passed, including **459 Python tests / 9 expected skips**. All **10 selected broader stages** passed by exact evidence adoption; the original failed queue stays failed. The real-corpus campaign remains a deadline failure with partial coverage. Its native/extension/VSIX package stages, `same`, `different`, `multi` and `untrusted` editor scenarios passed on their original native/package identities. Failed usage Stop and initial legacy attempts are retained. These observations do not become current-source qualification by relabeling.

Current native source `237f8969...` / **960 inputs** adds bounded native installation-lock admission before accepting lifecycle work. Only typed installation-lock contention can wait within the **60-second admission budget** before Stop/Restart mutation acceptance. After acquiring the lock, native code rechecks the same instance/runtime and retains the lock through authority checks and completion. Submitted actions are never replayed; generic text and other failures do not become retryable. Scoped CLI Clippy passed in **32.292 s**, the unaccepted-state timeout regression passed in **36.053 s**, a fresh matched debug pair built in **90.262 s**, and the actual held-lock Stop integration passed in **29.522 s**. The current optimized producer passed in **734.0468 s**; native packaging passed in **15.255 s** and VSIX packaging in **17.183 s**. The fresh current-source legacy fixture passed in **68.399 s** and its whole installed-editor scenario in **33.561 s**, including native Stop and canonical cleanup. The subsequent whole usage scenario passed in **97.960 s**, including its native Stop and cleanup. A preceding **259.85 s** quick-pick failure remains unresolved and intermittent; the successful rerun used observational diagnostics, with no product picker correction or cause claim. All six selected editor scenarios are covered using four retained old-source scenarios and fresh current-source usage/legacy. Current-source Windows qualification passed formatting (**6.244 s**), strict workspace Clippy (**24.3543 s**) and full Nextest: **2,627 passed / 0 failed / 51 skipped**, with no retries. All four restored long campaigns passed: compact reads (**246.170 s**), retention plateau (**146.632 s**), sparse catalog edits (**29.658 s**) and manual backlog (**108.021 s**). Nextest ran for **1,542.644 s**; its wrapper took **1,617.6504 s**. Serial doctests passed across **8 crate targets** in **18.6745 s**; there were **0 eligible examples**, not eight executed examples. Immutable JUnit and Cargo timing reports retain this current-source run. These are selected local qualification results; manual acceptance and release promotion remain separate.

P7 implements [combined Project reads](../combined-reads.md) with complete
native, CLI, MCP, Workbench, plan/session, verification and export support.
Windows release and Apple Silicon debug native suites passed, as did all 84
unchanged legacy latency gates and 120 Windows fan-out trials. Exact evidence
and unrun platform/release checks are in the
[implementation ledger](../research/implementation-v190.md#p7--combined-retrieval-and-consumers).
The unpublished working version remains **1.8.8**.

P6c's [search execution contract](../search-execution.md) discloses policy-qualified lexical results when the embedding helper is unavailable, preserving strict explicit semantic requests and independent source-verification receipts. Local Windows/Mac ARM gates and all 84 unchanged-bound latency comparisons passed; the ledger records exact coverage and remaining release limits.

Started 2 November 2026 on `feat/finetuning`, from the 1.8.7 source baseline.
Native workspace/local lock entries, extension manifest/npm lock entries and
versioned MCP catalogs initially used **1.9.0**. On 3 November the maintainer
selected **1.8.8** as the working version for completion and manual testing before
later promotion to **1.9.0**. See the [current working-build record](release-1.8.8.md).
The checkpoint identities and measurements below retain their original tested
versions. Publication and installed-artifact acceptance require separate gates.

P6b adds durable helper cooldown recovery, measured healthy reset, native exit
codes and isolated worker-grant repair. Local Windows/Mac ARM gates and all 84
unchanged-bound latency comparisons passed;
see the [recovery contract](../service-embedding.md#helper-recovery) and ledger.

## Accepted work and current behavior

P5's foundation passed local isolated qualification at working version 1.8.8.
P6f enables guarded authority-1 activation in the 1.8.8 working candidate.
Its catalog upgrade fixtures exercise candidate-owned migration, retained predecessor
refusal, bounded backup/import/recovery and the no-restore barrier after writes.
The [storage boundary](../storage-formats.md#development-catalog-boundary)
and [ledger](../research/implementation-v190.md) record qualified coordinator and
performance results, including completed local UI/grant qualification and the
completed full Neo checkpoint before P7. Installing
the working candidate requires qualified native Genesis/upgrade readiness;
full release acceptance remains separate.

P6a implements the copied Mac launcher correction, shared signed release
producer and offline contributor bootstrap. Local tests, actual Developer ID
signed installation and all 60 latency comparisons passed. The current [working-build record](release-1.8.8.md)
and [ledger](../research/implementation-v190.md) retain its tests and pending
qualification separately from the completed P5 checkpoint.

The maintainer also requested committed dependency policy and review checks using
`cargo-deny` and `cargo-vet`. The [working-build rationale](release-1.8.8.md#dependency-supply-chain-checks)
explains their distinct roles, visible unreviewed baseline and non-Rust limits.
Their validated implementation will carry forward into 1.9.0.

- **P0a — baseline:** signed commit `60f5c9d` freezes original relevance fixtures,
  source/runtime identities and uncontended native MCP measurements. It reproduces
  the compact chained-plan failure and retains negative cases and all failures.
- **P0b — candidate identity:** version synchronization and extension activation
  policy include the supported 1.9 line. Downgrades, unknown lines and malformed
  versions remain refused. Validation passed at this checkpoint in the
  [implementation ledger](../research/implementation-v190.md).
- **P1 — compact plans:** compact shared file/provenance
  bindings now produce self-contained cited evidence and dependent reads without
  extra implicit queries. Invalid tables, scope/revision mismatches, ambiguity,
  truncation and output limits retain their guards. Nested parse inputs are
  advertised as optional objects. Symbol-ID follow-ups use an explicit rich
  lookup when compact rows omit IDs. Ordinary direct read defaults are unchanged.
- **P2a — independent replies:** native reads accept
  `reply_detail: lean|rich`; CLI aliases expose `--reply-detail`, and Workbench
  forms/slash commands retain choices in sessions and exports. Explicit choices
  are independent of retained indexing detail; omission preserves legacy defaults.
  Conflicting old/new flags reject. No storage/profile migration or implicit
  enrichment occurs. Rich search keeps focused snippets; `full_chunks` remains
  an independent explicit choice. See [the reply guide](../reply-detail.md).

- **P2b — evidence guidance:** optional bounded reasons
  distinguish returned capability, retention, work, selection and truncation
  observations. Empty results remain unknown. Workbench displays inert next-read
  advice and retains it in exports. No profile/storage or hidden scan changes.
  See [evidence guidance](../evidence-guidance.md).

- **P3 - design safeguards:** [ADRs, schemas and original fixtures](../research/contracts-v190/README.md) fix explicit many-to-many scope, persistent denial ownership, migration/recovery boundaries and dated verification contracts. This phase changes documentation/design validation only; it activates no schema, source verification, trim, split or new profile.

Existing read behavior inherits [1.8.7](release-1.8.7.md). P0a changes contributor
measurement tooling; P0b changes candidate identity and forward activation policy.
They add no registry migration, new Project/index hierarchy, split/trim operation,
recommendation screen or source-verification guarantee. Those capabilities remain
planned until their implementation, parity and performance phases are accepted.

## P4 locally qualified: dated saved-source verification

Evidence reads now implement Report by default with a bounded native preset of
8 sources, 16 MiB aggregate raw bytes and a cooperative 2-second deadline. Off
explicitly skips checks; Require preserves full attempted evidence and a
nonretryable failure when checks do not satisfy the requirement. Plans deduplicate
final references, and Workbench/Inspector/Checker preserve original dates and
uncertainty. No source refresh or schema upgrade occurs in P4. See the
[guide](../source-verification.md) and [phase ledger](../research/implementation-v190.md).
P4 passed local native/transport checks and frozen/preceding latency and relevance
gates. Report adds intentional checking cost: small-file median 135.76 ms versus
66.67 ms with Off in the final warm-cache campaign. This does not qualify installed
editors, concurrent editor saves, physical disk I/O or other platforms. Earlier
failures and their corrections remain in the ledger. At that P4 checkpoint,
Project/index management, upgrade activation and combined scope remained planned;
P6f and P7 now implement those paths, and P8 adds Recommendations. Trim and split
remain planned for P9/P10.

## Performance and validation evidence

The [frozen 1.8.7 baseline](../research/baseline-v190.json) records 270 measured
MCP task trials and 2,430 calls with no errors/retries and complete required
coverage/cleanup. The original graded corpus separately reports lexical/vector/
hybrid nDCG@10 and Recall@10. These are baseline observations, not a measured
1.9.0 speedup, general relevance result or installed-editor benchmark.

The initial unfiltered Windows workspace test command reported five temporary
private-boundary failures; each failed identity subsequently passed using a fresh
owner-only fixture base. Original failures, declared ignores, corrected commands
and limitations remain in the ledger. P0b passed 1,836 native tests under the
unchanged CI profile (35 skipped), workspace doctests, formatting/Clippy, 307 Python
tests (six skipped) and 541 extension tests (19 skipped). Its staged 1.9.0 runtime
retains all frozen relevance scores and passes the legacy latency sentinel:
90 trials/810 calls, zero errors/retries. These isolated local checks do not
qualify installed-editor upgrades, other platforms or physical I/O.

P1 passed 1,843 workspace tests under the unchanged CI profile (35 skipped),
doctests, formatting/Clippy, all 11 actual CLI/MCP transport tests, 307 Python tests
(six skipped) and 542 extension tests (19 skipped). One staged four-CPU native
smoke/client repetition passed. Its 75-query relevance scores exactly match both
preceding checkpoints. All median/p95 latency limits pass against both 1.8.7 and
P0b across 90 trials/810 calls, with zero errors/retries. The captured compact and
rich dependent workflows now both complete with equivalent citations. See the
ledger for exact identities, original failures, commands and qualification limits.

P2a passed 1,847 workspace CI tests (35 skipped), workspace doctests,
formatting/Clippy, 307 Python tests (six skipped) and 543 extension tests
(19 skipped). Post-review checks passed all 84 MCP cases, 204 Core/Kernel CI
library cases (two skipped), all 11 CLI/MCP process cases and a final four-CPU
staged smoke/client repetition. Frozen relevance scores remain identical to 1.8.7
and P1; 90 trials/810 calls pass both latency reference gates with zero errors or
retries. Default response byte counts match P1; the input catalog intentionally
grows by 5,699 bytes. Superseded attempts and qualification limits remain recorded
in the ledger.

P2b passed 1,851 workspace CI tests (35 skipped), doctests, formatting/Clippy,
307 Python tests (six skipped) and 544 extension tests (19 skipped). Final affected
Core/MCP and all 14 CLI/MCP/tools cases passed after catalog/message review; the
qualified runtime passed one four-CPU smoke/client repetition. Its matched frozen
75-query campaign preserves all quality outcomes. The 90-trial/810-call sentinel
passes both latency references with zero errors/retries. Guidance intentionally
adds bounded metadata: measured minimal symbol/outline replies grow 360 bytes,
affected search replies up to 427 bytes, and discovery grows 168 bytes. The
initial affinity-uncertain chained campaign is retained as unmatched and excluded
from acceptance. See the ledger for final hashes, conditions and limitations.

## Delivery and compatibility gates

Follow the [approved plan](../research/plan-v190.md) and ledger. Native registry,
collection-policy and receipt formats retain independent compatibility checks.
The numeric candidate increment does not admit a live schema migration. The
Project/index upgrade must complete its U0–U6 safety path before P6 activation;
combined retrieval must include complete consumers before P7 advertisement.

Each phase preserves the frozen performance/quality guards and current behavior
when new features are off. Keep changed local test builds isolated and identified
by commit/payload hashes. Do not distribute different installable artifacts under
the same numeric version or relabel tested binaries. Push, PR, merge and release
publication remain separate authorized actions.


## Conditional P13 decision: current ranker retained

After the accepted local P12 checkpoint, P13 follows the plan's retain-current
path. No ranking or diversity candidate is enabled. The prospective research
lacks qualified held-out quality/resource results and native ranker/window
disclosure; this is an unqualified candidate, not a measured quality failure or
gain. Current lexical eligibility/order and vector/hybrid behavior remain intact.
The [dated decision](../research/implementation-v190.md#p13--conditional-decision-retain-current-ranking-6-november-2026)
records frozen research identities, label-independence limits and remaining gates.
At the P13 checkpoint, P14, missing-source cleanup and final manual installer
acceptance were pending. Selected local P14 results are recorded above; missing-source
cleanup and final manual installer acceptance remain required.
