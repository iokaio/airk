# AIRK 1.9.0 development candidate

Started 2 November 2026 on `feat/finetuning`, from the 1.8.7 source baseline.
Native workspace/local lock entries, extension manifest/npm lock entries and
versioned MCP catalogs are synchronized at **1.9.0**. This is an unpublished
candidate under sequential implementation; publication and installed-artifact
acceptance require their separate authorized gates.

## Accepted work and current behavior

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

Existing read behavior inherits [1.8.7](release-1.8.7.md). P0a changes contributor
measurement tooling; P0b changes candidate identity and forward activation policy.
They add no registry migration, new Project/index hierarchy, split/trim operation,
recommendation screen or source-verification guarantee. Those capabilities remain
planned until their implementation, parity and performance phases are accepted.

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
