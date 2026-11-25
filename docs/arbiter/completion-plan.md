# Answer loop and FP16 completion plan

**Review draft, revised 25 November 2026.** This plan sequences the remaining work
on `feat/arbiter-final`. It records the latest Windows-first staging decision.
Use the [current assignment](../agent-current-work.md) for execution
authority and the [Answer plan](final-plan-fp16.md#128-work-order) for the
underlying contracts and acceptance criteria. Where older instructions
describe Large, one-turn Answer, or a different platform order, use the
current model selection and the sequence below. Review of this document
does not start execution.

The Windows research question is whether an agent can make a correct change,
write a grounded requirement or finish a review after giving Answer the task
once. Answer should do the searching, following of leads and reading that
would otherwise consume the agent's next several turns. First research indexing itself. Check source fidelity, test alternative
representations through individual and combined retrieval routes, and choose
the indexes that best support an agent using the defaults. Close consequential
upstream defects before baselining Answer. The earlier Answer series and eight-step Search study supply leads.
[Windows experiment and acceptance](#3-stage-l7-with-a-windows-first-review) · [Search findings](search-improvement-plan.md#what-we-achieved-and-where-it-falls-short)

| Milestone | Exit evidence or decision |
| --- | --- |
| Index and Search quality | Complete source-fidelity checks and matched indexing experiments across individual and combined retrieval routes. Select and freeze the configuration for both repositories. [Research and selection](#how-to-test-indexing-for-agent-tasks) |
| Windows CUDA | Account for shakedown and L6 evidence on the admitted stack. Judge a current-stack baseline and three cumulative retrieval changes, then stop for your review. |
| Windows review | You choose another Windows round or approve Linux. Each additional Windows round returns to the same review. |
| Linux CUDA | LX, Linux L6, and the agreed L7 comparisons against the Windows-reviewed candidate. Bring both hosts' results to review. |
| E8 and E9 | Run sealed confirmation once on your word. Record the evaluation report and route decisions. |
| Local platform preparation | Build and unit-test all three WebGPU targets. Capture the Windows A6000 integration, Linux A6000 integration or the WSL2 limitation, and Mac Neo Focus-only smoke or its measured admission limit. |
| Rental and delivery | Request booking after platform preparation. Qualify the selected routes on each rented target, prove CPU fallback, and verify the final deliverables. |

## First milestone — Research and test indexing for agent retrieval

Start here on resumption, before Windows Answer changes, model tuning, or
a judged Answer baseline. Use the identity, disk and source checks in
section 1 for diagnostic work. This milestone asks two separate questions.
Did indexing lose or distort evidence? Could an accurate index be organized
better for agents using ordinary retrieval tools? A clean fidelity audit
answers only the first. Test the representations and retrieval routes below
even when the integrity checks pass.

### Index and Search quality before Answer

**Current finding.** Bookbuilder's loss of two first-reply essentials is real
in the paired Search experiment. It first appeared when ranked keyword
candidates were introduced on the *same* two publications, with identical
requests, hit and byte limits, and zero new index jobs. That identifies a
query-time tradeoff for that comparison. It leaves open whether either
publication lost evidence at ingest, provided weak lexical or semantic inputs,
or faithfully preserved a representation that was already weak. The historical
Windows L7 baseline and change set 1 already used CUDA MiniLM. A different
publication token or an older report date cannot by itself identify the old
indexing algorithm. Build speed, Ready status, vector count and a three-file
smoke test answer different questions from retrieval quality. For the two
lost Bookbuilder essentials, T1/N1 and W1/N3, the index still contains one
and four qualifying units respectively. In the Step 7 wide shortlist the
first was absent and the second ranked 28. Check the lexical, semantic and
fusion stages for these cases before changing either index.
[Step 1 comparison](search-improvement-plan.md#step-1-result--ranked-lexical-candidates) · [Index adoption and smoke scope](../simplify/one-process-ledger.md#24-november-2026--plain-store-adopted-and-semantic-smoke-passed) · [Windows baseline inputs](answer-evaluation/05-loop-baseline.md#what-was-tested)

**What we verified in this review.** The two *post-refactor* index pairs
behind historical Windows CS1 and the Search study have byte-identical
complete vector files per repository. Across all 8,116 Bookbuilder paths
and all 1,023 Munarium paths, the stored source hashes and extraction
digests match between the two builds. Both directions of the comparison
also matched chunk identities and citations, lookup selectors, committed
unit membership and embedding keys mapped to physical vector rows. Fresh
installations have different artifact handles and publication identities.
The checks do not compare every lexical posting or establish how the
pre-refactor producer behaved. Six Step 8 UTF-8 excerpts, three per repo,
also matched independently reconstructed pinned source, including
Bookbuilder's CRLF checkout representation. These results justify reusing
the settled indexes while investigating retrieval.
[Comparison scope and receipts](../simplify/one-process-ledger.md#25-november-2026--answer-planning-and-index-quality-review)

Run the remaining diagnostic before investing in the Answer changes. Start with
retained receipts and read-only probes on the two settled publications.
Use an independently enumerated source manifest, a scoped literal scan and
reads of the pinned Git exports as the reference. Keep reference answers,
probe design and diagnostic-only source access out of product inputs and
judged agent sessions. Report Bookbuilder and Munarium separately.

| Question | Discriminating check | What the finding changes |
| --- | --- | --- |
| Did indexing admit the intended evidence? | Reconcile eligible paths, file hashes and declared skips against the selected, pinned export. Check former hits, current misses and a stratified sample of independent files, including code, tests, documentation, long files, duplicate names and excluded controls. Track intentional exclusions separately from unexplained omissions. | A missing eligible file, an incorrect revision or out-of-scope inclusion belongs to selection, ingest or publication. Fix that path before judging downstream changes. |
| Did it preserve useful content and locations? | Open known paths through the native reader. Compare exact lines and boundaries to independent source bytes. For extracted formats use independent page, sheet or section anchors and the original location mapping. Check clipped sections, duplicate content at different locations, update/delete and unchanged-file reuse in a small disposable fixture. | A reproducible missing or mislocated passage in a known file is an index or materialization defect. Isolate the writer from the reader, correct it, and rerun the affected probe. |
| Does lexical and structural lookup find what was stored? | Compare known unique literals, punctuation-bearing identifiers and scoped whole-name matches with the direct scan. Check postings or candidate membership before ranking, then source range after ranking. For symbols, facts and edges record provider support and ambiguity. | Distinguish an absent posting or misbound symbol from tokenization policy, a bounded query or a later ranking loss. Missing optional provider data becomes an explicit coverage gap. |
| Are the right vectors attached to the right passages, and do relevant passages score well? | Reconcile committed rows to eligible occurrences and member spans. On a stratified sample reproduce the exact model inputs, tokenizer, truncation, pooling, normalization and route. Compare stored embeddings and independent exact scores within the admitted numeric tolerance. Inspect scoped ranks for literal, paraphrase and short focused queries, including near ties and hard negatives. | A wrong input, row binding or scoring mismatch warrants an indexing/reader fix. Correct vectors with consistently poor passage ranks justify a representation experiment. Raw vector counts cannot distinguish these outcomes. |
| At what step does usable evidence disappear? | Hold the publication, request and model state fixed. Compare available lexical and vector candidates, fusion order, Focus input, selected excerpt and final bytes. Trace lost as well as gained notes, including the original Bookbuilder losses. | If a correct passage is available and drops only at fusion, selection or byte packing, repair Search there. If it is accessible through a native structural or range read, record a viable route for Answer. |

The existing vector audit established that 111/113 development notes have
one qualifying committed unit and the other two need a union of passages.
Only five requests retained pre-fusion traces. Of 22 misses in those traces,
six qualifying passages entered the capped lists. This narrows the suspected
failure without establishing global rank, vector quality, lexical coverage,
or end-to-end index fidelity. Reuse that audit where it already answers the
question. [Vector coverage and limits](search-improvement-plan.md#step-6-result--committed-vector-coverage)

**Distinguish regression from a longstanding weakness.** Build a lineage
table for any usable older and current receipts. Pin source/export, scope,
indexer and extractor, publication, unit recipe, embedding weights and
tokenizer, reader/Search revision and request. Where compatible authenticated
artifacts survive, hold the reader and request fixed and vary the index,
then hold the index fixed and vary the reader. Record incompatible cells as
unavailable. If only the new index fails under a matched reader and source,
classify a producer regression after reproducing the first difference. If
both fail in the same way, record a shared limitation. If no matched older
cell is available, report a current defect or limitation with origin unknown.
A small matched old/new build on identical representative
files can test a specific suspected writer change after the existing evidence
is exhausted. Historical partial indexes and full-corpus rebuilds are
unnecessary for this diagnostic.

The retained pre-refactor Bookbuilder depth probe used the same source pin
but different query strings, limits and embedding pipeline. It can supply
a candidate historical comparison, provided the inputs and budgets can be
matched. A fixed-reader comparison across incompatible store formats needs
a common observable boundary such as independently verified postings, cited
ranges or exact query results from each compatible reader.

**Exit decision.** Produce a per-repository report of recovered and lost
paths, verified ranges, lexical matches and semantic ranks. Show denominators,
coverage of the sample, first failing stage, reproducer, confidence and
unexplained cases. Hold Answer changes for any demonstrated upstream defect
that prevents a required fact from being recovered, or for an unexplained
material source-fidelity mismatch. Repair and recheck the affected layer on
the small fixture first. If a correctly represented passage is lost by
ordinary Search, repair a repeatable Search regression where it affects the
control and remeasure, or demonstrate an alternative canonical read that
reaches it from the task or a previously observed clue within Answer's actual
scope and budget. Opening the grader's known target only establishes storage.
A bounded limitation can proceed with its affected tasks
explicit in the report. Reconcile the frozen publications or take a documented
index proposal to the user before a full build. Carry this fidelity disposition
into the indexing experiments below. It qualifies the tested paths and records
what remains unknown about historical causation.

### How to test indexing for agent tasks

1. **Freeze the default behavior as the control.** On both pinned exports
   use code, documentation, architecture and test tasks plus a small independent
   development sample for debugging, review and negative or ambiguous lookups.
   Include natural-language tasks that omit the exact identifier. Sample known
   successes and misses. Establish expected facts and source locations
   independently. Keep labels and known target paths out of queries, indexed
   metadata and training. Keep the sealed set closed.
2. **Exercise the individual readers and their combinations.** Compare lexical,
   semantic and hybrid Search, symbol/definition and reference lookup, code
   and document facts, relationships/imports, and outline plus range reads.
   Record which share underlying representations and where provider coverage
   is absent. Then test realistic sequences such as hybrid discovery → symbol
   → callers → source, heading or fact → governing section → exception, and
   diagnostic string → references → test assertion. Start with the task alone.
   Follow only anchors available in the preceding replies. Measure out-of-box
   behavior and an equal-work comparison with fixed calls, candidate counts
   and bytes. Pair Focus off/on wherever supported. Hold the Answer planner
   and Arbiter out of this experiment.
3. **Build and test small matched representations.** Choose candidates from
   the table below based on observed misses and direct-tool advantages. State
   what each change should recover. Index identical representative sources
   with the control and candidate writers. Hold query policy, model and
   budgets fixed wherever compatible. Run each new representation alone and
   in a useful combination, then remove one contribution at a time from a
   winning combination. If a change needs a new reader, report a writer/reader
   result until their contributions can be separated.
4. **Measure usefulness to an agent.** Report eligible-path recall, decisive
   spans at common depths and byte limits, wrong citations, and which next
   read each returned anchor enabled. Give a fresh agent the default result
   or the same bounded sequence on each candidate. Compare correct work
   briefs, remaining direct-file reads, turns, bytes and elapsed time.
   Show gains and displaced facts separately for each repo and task alongside
   storage, full-build and incremental-update costs.
5. **Choose the indexes before answering the first scored task.** Record the
   tested candidates, limitations and selection in the research report and
   ledger. An index with no demonstrated improvement can be rejected on that
   evidence. If a candidate warrants a full rebuild, present its measured
   gain, cost, migration and recovery plan to the user first. Qualify and
   freeze the selected publications, or reuse the existing ones when they
   win the comparison. Only then establish the current-stack Answer baseline.
   A later L7 trace can reopen a specific indexing question.

### When to change an index

These candidates target ways an agent can lose time even when every source
was indexed correctly. Choose from measured failure classes and exercise
the resulting evidence through the native readers.

| Repeated failure on known source truth | Candidate to test on a small matched sample |
| --- | --- |
| Scoped scans recover exact strings or identifier fragments that lexical lookup misses, or scans are too costly | Positional literal/trigram postings with final retained-byte verification. For path, title, symbol or heading losses, try fielded lexical scoring separately. |
| Impact work stalls on aliases, implementations, config consumers or test discovery | A language/provider-backed definitions and references index, with typed edges and coverage. Test explicit document links, heading IDs and supersession metadata for governance questions. Keep inferred associations distinguishable from verified edges. |
| Correct file ranks but useful context crosses a unit boundary | Try query-time parent expansion first, then file/section-to-passage links or definition-aware chunks. Keep the corpus-wide passage route for isolated facts. |
| A qualifying passage exists but consistently scores poorly under independent focused queries | Compare body-only embeddings with heading, path or enclosing symbol context using the same MiniLM. If the loss persists, compare a code-aware model or late interaction on the same sample. Verify citations against original source spans. |

Pin identical source revisions, selection and query policy in control and
treatment. Change one representation at a time and compare its own searchable
publication with the control. Test previously missed and previously successful
requests across both repositories, including independent examples outside the
development labels. Measure gains and losses both in candidate rank and in
evidence delivered through the fixed reader sequence. Reserve a matched
Answer replay for L7 after selection. Repeat the ordinary Search comparison
with Focus off/on. Measure full and incremental build time, query latency,
index bytes, memory, peak temporary disk and migration or rollback needs.
Propose a full build only with repeatable recoveries on distinct tasks, a
measured cost and a plan for affected existing publications. Discuss the
specific representation and evidence with the user first. Keep accepted
originals available and record contracts for any persisted format or digest
change. [Index alternatives and prior audit](search-improvement-plan.md#alternatives-to-test)

## 1. Reconcile the starting point

1. **Start with the completed build.** The ledger records a signed Windows
   1.8.209 installer and a VSIX built from `16aa576d` with r7 static ONNX
   Runtime and bundled MiniLM. It lists both package hashes. The review on
   `7a1bc1fe` passed workspace Clippy and 183 selected native spot checks.
   The newer build receipt establishes packaging; live install, upgrade,
   Linux and model qualification still need evidence. Match each receipt to
   its exact source, binaries, features, runtime and selectors before
   reusing it. Use a sealed, matched service and CLI pair for evaluation.
   [Build and review receipt](../simplify/one-process-ledger.md#24-november-2026-at-1820-utc--review-of-the-codex-ui-merge-a-dead-link-fix-and-the-18209-installer-and-vsix-candidate)
   · [1.8.209 release notes](../releases/release-1.8.209.md)
   · [Build order](../ci-and-builds.md)
2. **Carry forward LG and LH.** LG compared three arms and recorded zero
   correct finishes in 60 sessions. Large is retired. LH exercised neither
   model, Focus only, Arbiter only and both on one live Windows CUDA
   service. The Windows evaluation controls are the static 1,024-token W8
   Focus export and the FP16 Arbiter export. Verify their exact profile,
   compiler, task policy and runtime identities before reuse. Keep prior
   one-turn results as historical evidence.
   [LG report and receipts](answer-evaluation/04-gate-lg.md)
   · [Loop changes and profile lineage](answer-loop-changes.md)
   · [Windows LH acceptance](../simplify/one-process-ledger.md#23-november-2026-at-1955-utc--lh-accepted-on-windows-cuda-the-four-switch-combinations-on-one-live-service-lh-merged-into-featarbiter-final)
3. **Admit resources before creating fresh fixtures.** Confirm the logged-in
   human and default owner on each host. Check the resolved parents and
   audit the new outputs. Assign one Cargo owner per host/target and one
   evaluation owner for the shared A6000. Reuse warm targets. Record actual
   free space, reservations, peak growth and cleanup ownership. Include
   Windows cache and temp paths, and the Ubuntu VHD and swap host volumes
   before starting WSL. Use the existing `Ubuntu` distribution and
   `tyler` account through the checked boot path. Reobserve the campaign
   floor before admitting writes; the build's free-space figure is a dated
   observation. Keep work under authorized roots.
   [Checked Windows execution](../../CONTRIBUTING.md#checked-windows-execution)
   · [Campaign paths and private state](../agent-current-work.md#build-targets-and-private-state)
   · [Linux build and WSL runbook](../../scripts/arbiter/evaluation/linux_cuda/README.md)
4. **Freeze clean evaluation inputs.** Use munarium `9948345d` and
   bookbuilder-v2 `238b22a7`. Use their verified settled plain-store and
   semantic publications as the indexing control. For Answer, use the exact
   publications selected in the first milestone. The checked restart
   preserved both publication tokens and scheduled zero index or vector jobs.
   Authenticate source manifests, selection, indexing and embedding recipe,
   publication tokens, readiness and leak exclusions on the admitted pair.
   Give each comparison fresh request scopes. Keep both repositories read-only.
   Retain the ten frozen development scenarios from each repository and the
   private sealed-set custody proof. Keep sealed evidence outside the index,
   retrieval prompts and training corpus. Size the
   native dispatch census from the plan and verify teardown. Use the
   current fixture helpers and regenerate the plan from the frozen inputs.
   Record the private run root, source export and scenario hashes, and
   publication IDs in the series receipt. Keep these inputs and any
   training examples outside Git and release payloads.
   [Settled restart proof](../simplify/one-process-ledger.md#25-november-2026--reuse-settled-development-indexes) · [Scenario inventory](answer-evaluation/scenarios.md)
   · [Harness and freeze procedure](../../scripts/arbiter-answer-eval/README.md)
   · [CUDA fixture runbook](../../scripts/arbiter-answer-eval/GPU_FIXTURE.md)
   · [L7 template](../../scripts/arbiter-answer-eval/l7_plan_template.json)

## 2. Prove Windows CUDA and complete Windows L6

1. **Windows shakedown.** Match the existing live receipts to the candidate
   and the reused publications. Exercise the four switch combinations on
   changed paths and carry forward checks whose inputs are identical. Check that
   search records are present, Focus calls occur where enabled, and
   Arbiter decisions have model provenance. Include disable and re-enable
   across the configuration publication transition that previously refused
   a status read. Exercise a deadline case with disclosed gathered
   evidence and verify clean fixture settlement.
   [Four-switch runbook](../../scripts/arbiter-answer-eval/README.md#the-four-switch-combinations-on-one-lifetime-lh-acceptance)
   · [Ledger and switch-sequence history](../simplify/one-process-ledger.md)
2. **Record cold, warm and reload behavior.** Capture Windows placement,
   stage and end-to-end latency, peak device memory, and reload behavior.
   Verify a reload-time Answer makes progress and accounts for any
   degradation. A failed shakedown gets focused diagnosis, a regression
   check and an affected-path rerun before an expensive pass.
3. **Complete Windows L6.** Follow the workspace Clippy and test
   matrix with exact feature and target receipts, including Cargo timings.
   Use the current candidate and reuse unchanged checks. Record selected
   counts and known failures with owners. Resolve any failure that
   compromises the evaluation and retain the applicable service-client and
   transport parity evidence. Workbench parity on the newer UI is already
   recorded and can be reused within its tested scope. Begin Windows L7
   once the live shakedown passes and the applicable L6 evidence is
   accounted for. Record the exact candidate in the ledger.
   [Commands](../../CONTRIBUTING.md#agent-command-reference)
   · [Validation matrix](../../CONTRIBUTING.md#agent-validation-matrix)
   · [Canonical read parity](../read-interface-parity.md)

## 3. Stage L7 with a Windows-first review

### Stage A — Run the Windows CUDA baseline and three change sets

1. **Measure the current stack before changing Answer.** The earlier Windows
   baseline and Change Set 1 were judged. In that first change set, Search
   gathered 55/113 essentials across agents' sessions, Answer gathered
   37–44/113, and all five paths had zero correct finishes. The historical
   receipts describe their own index and Search configurations. The later Search
   study measured only the *first* reply.
   Across its own paired runs Bookbuilder fell from 5/61 essentials to 3/61,
   while Munarium rose from 9/52 to 17/52. Ten of the 20 final requests
   returned no essential. Neither study measures today's relative session
   quality. In particular, the 55 consisted of 14 essentials in the first
   replies, 19 first recovered by later Search calls and 22 first recovered by
   targeted file reads. Report today's corresponding three counts, their union
   and the old-versus-current per-note differences. Additionally replay the
   historical Search arm's recorded arguments and targeted ranges verbatim on
   today's stack, under equivalent scope, budgets and source revision, naming
   any incompatible call. That diagnostic isolates what the same follow-up
   probes now return. The fresh blind sessions measure whether an agent can
   still *discover* those probes from today's earlier replies. If the total
   falls, identify the first lost lead, unavailable source, changed excerpt or
   agent choice that accounts for it. Preserve the historical reports and
   the unjudged model experiments under their original identities. Give the new retrieval series a distinct
   run/report identity.
   [Historical baseline](answer-evaluation/05-loop-baseline.md) · [Judged first change set](answer-evaluation/06-loop-change-set-1.md#first-sample-results) · [Search results by repository](search-improvement-plan.md#what-we-achieved-and-where-it-falls-short)

   Run fresh, matched judged sessions for Search, Parse and the three Answer
   arms (Focus, Arbiter, both) on the current pair and publications. Keep both
   optional stages off for the judged Search and Parse controls. Record the
   actual scope, query, explicit or omitted hit limit, byte limits, served
   Search mode, embedding route, model dispatches and placement. Ordinary
   Search with Focus off/on is a separate diagnostic. Answer's explicit
   internal Search limits can select different results from ordinary Search
   with an omitted limit. Freeze the same 20 scenarios, prompts, notes,
   harness, job policy, Haiku agents and two independent Sonnet judges at
   medium effort. Record baseline headroom against the stronger control per
   scenario. Use both-models-off Answer in matched replays and label any
   separately judged no-model control. Keep this baseline throughout the
   three changes and show each candidate against it and its predecessor.
   [L7 work order](final-plan-fp16.md#128-work-order) · [Rubrics](answer-evaluation/rubrics.md) · [Report layout](answer-evaluation/README.md)
2. **Change set 1 — let Answer gather the evidence.** Compare the current
   Search-led loop with evidence needs routed to native readers, then add
   targeted range expansion and the bounded retained-text scan. Use both
   models off for attribution. Replay fixed and need-based read allocations
   on the same routes. Determine where each essential first becomes available
   and where it is lost. Carry the best supported combination into the judged
   arms. Pilot saved working-tree reads separately on controlled changes and
   an explicit source grant. The pinned comparison always sees the same bytes.
   [Retrieval rounds](#retrieval-rounds-inside-one-answer) · [Evidence budgets](#read-and-delivery-budgets)
3. **Change set 2 — let the models choose consequential work.** Hold gathering
   and allocation fixed. Compare deterministic choices with Arbiter choices
   over the same eligible actions, with Focus held fixed. Separately compare
   Focus off/on for the same candidates and Arbiter setting. Then test the
   interaction on the real loop where a changed choice affects later reads.
   Capture offered candidates, scored windows/cards, decision source,
   abstentions, next read and newly delivered evidence. Train or recalibrate
   only on repeated errors for which the right choice was available. Reuse
   eligible pairs from both repositories, protect scenario exclusions, retain
   original weights and general replay controls, and verify native parity of
   any new export before judging.
   [Model roles](#retrieval-rounds-inside-one-answer) · [Training exclusions](final-plan-fp16.md#54-training-data-for-model-weights) · [Focus tools](../../scripts/arbiter/training/focus_answer_eval_r1/README.md) · [Arbiter tools](../../scripts/arbiter/training/context_answer_eval_r1/README.md)
4. **Change set 3 — close the largest remaining task failures.** Select from
   the preceding traces. A failure to reach a passage, an incomplete function
   in the reply, and a confidently wrong claim about a caller need different
   fixes. Test the smallest change that closes the observed gap. Where the
   gap survives direct indexed reads and retrieval tuning, run the matching
   index pilot below. Show gains, displaced essentials and external reads
   still necessary for each repository and task. Discuss any proposed full
   rebuild before starting one. Keep a viable query-time improvement
   available for the judged series while an index decision is outstanding.
   [Index experiments](#when-to-change-an-index)

5. **Close each series before starting the next.** Replay the complete
   bundle, run leave-one-out replays for expensive changes and regressions,
   then judge the affected arms on Windows CUDA. Reuse Parse and Search
   receipts when their inputs and behavior match. Land one signed-off
   commit per series after judging, containing its code or baseline
   harness, report, results and ledger entry. Keep private artifacts out
   of Git. Follow the current branch authorization. Run the baseline and
   three change sets back to back on Windows, then stop for manual review.
   Apply the approved substantial-win rule: at least 12 of 20 scenarios,
   at most two worse, at least five wins in each repository, and no task
   category worse on average. A substantial win saves at least two turns
   against the stronger of Parse and Search, or reaches a correct finish
   when both comparators fail. Require session coverage of at least the
   essentials Search covers, and a timely reply or a disclosed partial. Rubric B breaks ties
   and guides presentation. Low baseline headroom is a finding for the
   review. Keep the sealed set closed.
   [Substantial-win rules](final-plan-fp16.md#43-what-substantially-improves-means)
   · [Acceptance](final-plan-fp16.md#127-acceptance)
   · [Commit and report rules](final-plan-fp16.md#56-a-report-for-every-series-of-changes-and-tests)

#### Retrieval rounds inside one Answer

**Give each need an observable finish condition.** In the existing Split,
Plan, Retrieve, Coverage, Follow-up, Compose and Union loop, identify the
decision the agent must make and the sources that could establish it.
A definition can identify the owner. Proving a behavior change can also
require its entry point, governing contract, consumers, exceptions and test.
Seed the plan from the request and observed sources. Only the evaluator
sees the reference notes.

| Anchor or open need | First native read | Evidence that earns the next round |
| --- | --- | --- |
| Known path, symbol, setting or diagnostic | File Info, Symbols, Code Facts, or exact lexical lookup within scope | Read the owning definition. Follow a concrete caller, import, configuration key, or test name found in that source. |
| Behavior described in unfamiliar language | Hybrid Search with focused lexical and vector queries, followed by a file or section read | Use the discovered project vocabulary to narrow another query or navigate a distinct implementation, contract or test. |
| Requirements, docs or architecture | Document Facts, heading/path search, Outline and governing documents | Read the rule through its exception. Follow a referenced decision or inspect implementation when stated and observed behavior disagree. |
| Impact, references and test coverage | References, Imports, available Relationships and source/fixture reads | Resolve a candidate occurrence to a call or assertion. Record unresolved dispatch, missing provider coverage, and excluded platforms. |
| A specific anchor remains missing | Bounded literal or whole-identifier scan over authorized retained text, followed by a range read | Start with a known file, then the narrowest justified subtree. Record files, bytes and matches examined, cutoff and continuation. |

Build on the existing kernel gathering and canonical readers. Current Answer
starts each need with Search and has a narrower typed `LoopRead` roster.
Prove proposed Code Facts, Document Facts and Relationships routes through
the existing readers on missed cases, then record the Core contract decision,
source bindings and transport fixtures before adding them to that roster.
Distinguish identifier occurrences, provider-resolved edges and source-verified
call sites. Keep collection, root and publication identity on every hop,
including when two indexes contain the same path.
[Current recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Core loop types](../../crates/airk-core/src/arbiter/answer_loop.rs) · [Existing AIRK readers](search-improvement-plan.md#how-airks-existing-tools-can-improve-search)

**Choose the next read from what the last read established.** Schedule
independent needs within the shared budget. Focus ranks candidate passages
against the particular open need. Give Context, exposed as Arbiter, a short
roster of grounded choices such as expanding the current range, following a
verified symbol, trying a newly found term, scanning an eligible scope, or
stopping with a specified gap. Context scores those choices. The service
validates scope, arguments, cost and evidence support, executes the read,
and uses the result in the next round. Keep deterministic selection for
disabled, abstaining or unavailable models. Cache by source identity and
range. Skip a repeated action that adds no evidence. Stop on sufficient
cited coverage, exhausted admissible actions, cancellation, budget or
deadline. If the current three-round ceiling regularly stops a productive
chain, replay those cases with more funded rounds and compare external work
saved. Increase a ceiling only on that evidence and through its contract.
[Loop model roles](final-plan-fp16.md#122-how-the-models-take-part) · [Native limits](../../crates/airk-core/src/arbiter/answer_loop.rs)

**Use current files when the question requires current files.** For a task
about a saved change, use a caller-supplied or otherwise explicitly authorized
base/head or changed-path selection. Verify its identity before following
the diff into surrounding definitions, callers and tests. A read-only,
in-process live matcher is a proposed capability beyond today's saved-source
verification. Bind every opened file to the granted selection, exclusions
and resolved file identity. Begin with known files. Permit bounded subtree
discovery only where that authority actually covers new paths. Distinguish
indexed and live citations by digest, observation and coordinates. Detect
rename, deletion or change during the read and invalidate stale excerpts.
Treat an unavailable diff or unsaved buffer as a specific missing input.
Exercise this on separate disposable fixtures with known changes, denied
paths, links and stale handles. Record the source-authority and evidence
contract decisions before exposing the capability.
[Saved-source verification](../source-verification.md) · [Source selection](../decisions/002-projects-indexes-and-query-scope.md)

#### Read and delivery budgets

Separate bytes scanned and retained, model input tokens, and serialized bytes
delivered. Ordinary Search defaults to **8,192 text bytes**. Some Answer
References calls use that size, the broad Answer Search can use 24,576 bytes,
and some targeted reads get 3,072. Answer defaults to 256 KiB of retention,
has a 128 KiB reply ceiling subject to the request budget, and Context admits
up to **8,192 input tokens per call**. Record which limit actually cut each
source or stopped an action.
[Read recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Evidence budget analysis](search-improvement-plan.md#context-inference-and-evidence-budgets)

Replace fixed per-read allocations inside Answer with small discovery reads
and larger reads for decisive functions, tests and document sections.
Redistribute unused budget across open needs. Score compact, cited choices
while retaining the longer source spans. Reserve delivery space for the
decision-bearing excerpt from each essential need and any conflict or gap.
Merge overlapping text while keeping its distinct provenance and locations.
Show the agent exact paths and line or original document ranges it can open
directly. Mark a clipped exception, branch or assertion as incomplete. If
useful evidence remains, a handle-only continuation should return new
evidence under rechecked authority without requiring the agent to replay its
request or previous results. Count it as an external round trip.
[Delivery and client checks](#4-close-the-parallel-section-12-work) · [Job authority and lifetime](../../crates/airk-service/src/service_runtime/answer_jobs/CONTRACT.md)

#### What an agent must be able to do with the reply

| Task | Practical review of the delivered evidence |
| --- | --- |
| Implement a behavior change | Can the agent open the exact edit site, identify the invariant and all relevant adapters in scope, find a comparable implementation, and choose a regression test? Exercise a missing-option versus explicit-value case across CLI and service to expose a wrapper that silently supplies a default. |
| Write requirements or documentation | Can it cite the current authority, intended reader, scope, exception and example? Include a superseded decision that still matches the query. If code and approved wording disagree, deliver both ranges and flag the unresolved decision. |
| Review a patch | Can it connect a particular changed hunk to a plausible consequence, identify an affected caller and check counterevidence such as a guard elsewhere? Distinguish a confirmed finding, an unresolved candidate and the portion of the diff actually reviewed. |
| Debug a failure | Give the agent a symptom and the authorized sources. Can it find the failing path, identify a discriminating observation, locate a reproducer or command, rule out a plausible alternative, and select a regression assertion? Report a diagnosis supported by source separately from an observed reproduction. |
| Design or select a test | Can it trace the real entry point through fixture setup to an assertion with an independent expected outcome, locate the command, selector and platform flags, and tell whether that test ran on this revision? Only a matching execution receipt establishes a pass. |
| Make an architecture or impact decision | Can it trace a complete path across an interface, including a boundary, variant or failure path that could change the decision? For “all callers” or “no remaining uses,” identify traversed roots, exclusions and completeness. A bounded or text-only search leaves that claim open. |

Preselect a small development sample spanning the task kinds above and both
repositories. Give a fresh agent only the task and the initial Answer delivery,
including a partial if that is what arrived. Block follow-up reads in this
diagnostic and independently grade the brief or proposed action against the
frozen facts. This tests whether the first reply stands on its own. In a
separate fresh session allow external tools and record which gaps a scoped
file inventory, literal scan or source read had to close. Give neither agent
the notes or target paths.

For downstream checks, apply a proposed patch and run its selected regression
in a disposable checkout, trace written requirements to the governing source,
or verify a review finding against the diff. Record the observed result and
the proposed actions that remain unexecuted. Keep these usability diagnostics
alongside the frozen L7 work-brief rubric. Use synthetic or development cases
for additional tasks such as debugging. Keep private source and sealed cases
out of public reports.

For each judged scenario, record correct finish and turns to correct finish.
A shorter unfinished session earns no turn-saving claim. Also report usable
first deliveries, essentials by task and repository, external searches and
file reads, continuations, total request and response bytes, repeated context
and time to usable evidence. Count repeat bytes by overlap with source
already delivered to this agent at the same scope, revision, path and original
range. Account separately for repeated request text, status polling, and model
input tokens where observable. Preserve the frozen rubric's turn definition. Pair this with internal rounds, reads,
scan bytes, model calls, queue/wall time, partials and reasons for stopping.
Compare a full task session against the stronger of current Search and Parse.
More internal work earns its cost when the agent finishes correctly with
fewer round trips and less repeated context.

### Stage B — Stop for manual review of Windows results

Present the Windows baseline and series comparisons, what each model added,
where the acceptance criteria were missed, and the exact candidate and
receipts. Include both index-quality dispositions, unresolved retrieval
failures, and examples of external work displaced or still required.
Wait for the user's decision before starting Linux LX or L7.

- **Repeat on Windows.** Agree on the next changes and number of iterations
  at the review. Preserve the original baseline, rerun the affected
  measurements and comparators, and bring the new results back for manual
  review. Repeat this decision point until the user approves moving to Linux.
- **Proceed to Linux.** Freeze the Windows-reviewed candidate, model and
  calibration identities, scenarios, rubrics, harness configuration, and
  comparison set. Carry forward the agreed evaluation scope for Linux.

### Stage C — Prove and evaluate Linux CUDA after approval

1. **LX on Linux.** Reuse authenticated GNU and CUDA build inputs where
   compatible. Build and seal the current service, CLI and fixture against
   the matching Linux runtime and execution identities. Add and validate a
   supported Linux fixture launch path. The current launcher explicitly
   requires Windows, and the binary's default math initialization is
   Windows-only. Supply `ORT_CUDA_GEMM_OPTIONS=2` in the Linux launch
   environment before ORT initializes; preserve refusal of conflicting
   settings. Prove the same four-switch shakedown on the A6000. The old
   successful GNU link leaves the current model profiles and live loop to
   prove. Keep separate Windows and Linux receipts.
   [Fixture launcher](../../scripts/arbiter-answer-eval/fixture.py)
   · [Math startup contract](../../crates/airk-cli/src/startup_math/CONTRACT.md)
   · [GNU CUDA contract](../../scripts/arbiter/evaluation/linux_cuda/CONTRACT.md)
   · [Linux FP16 runbook](../../scripts/arbiter/evaluation/linux_cuda/FP16_QUALIFICATION.md)
2. **Complete Linux L6.** Apply the same workspace matrix used for Windows
   to the matching GNU candidate, including applicable platform tests.
   Capture placement, cold and warm timing, reload, peak memory, a
   deadline-disclosed partial, and clean fixture settlement.
3. **Run Linux L7.** Measure the agreed arms with fresh Linux receipts and
   compare them with the corresponding Windows results on matched inputs.
   Report correct finishes, turns, essential coverage, stage decisions,
   deadline outcomes and resource costs by host and repository. If an
   underlying candidate change is needed, diagnose it with a focused
   replay and rerun the affected Windows and Linux evidence before
   claiming cross-platform qualification. Close with the Linux report,
   results and ledger. Identify the accepted candidate and any remaining
   device-specific gaps at the cross-platform review before E8 and the
   later WebGPU and CPU work.

## 4. Close the parallel Section 12 work

- **L2c–L2e, job delivery and clients.** Reuse the merged L2c work and its
  receipts. Finish evidence-first replies and handle-only continuations
  that deliver new evidence once. Prove one worker execution across
  disconnect and reattach, separate executions for `fresh`, and correct
  selection authority, retention, and per-delivery records. Count a
  continuation as another agent turn. Implement and verify distinct
  “Stop waiting” and “Cancel job” behavior and full recovered-result parity
  across the applicable clients. Apply the planned independent and
  security reviews. Run this work alongside L7 where resources allow.
  Keep the policy fixed within each series. If a later series takes a
  changed job policy, record it as an evaluated change. Report open
  items at inspection with their next executable action.
  [Job contract](../../crates/airk-service/src/service_runtime/answer_jobs/CONTRACT.md)
  · [Answer contract and continuation proposal](../../crates/airk-core/src/arbiter/CONTRACT.md)
  · [Combined Answer contract](../../crates/airk-frontend/src/combined_frontend/answer/CONTRACT.md)
  · [Read parity selections](../read-interface-parity.md)
- **L5b, measured capacity.** Complete same-session CUDA concurrency and
  drive admission from measured capacity and observed costs. Prove one
  resident copy per enabled stage, reserve compliance, the required
  1.3× gain before increasing concurrency, and shedding under pressure
  without interrupting active runs. Exercise status, unload/reload, recovery, and
  throughput with two and four concurrent Answers. Record the exact
  settings and distinguish old unqueued LG results from current queued
  behavior. Run these checks outside the frozen L7 measurements.
  [Capacity design](final-plan-fp16.md#1251-using-local-resources-at-runtime-with-one-copy-of-each-model)
  · [Known capacity gap](answer-loop-changes.md#26-l5b-runtime-capacity-the-rule-is-built-concurrent-runs-are-not)
  · [Inference contract](../../crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md)

## 5. After accepted Windows and Linux CUDA results, close FP16

1. **Freeze the CUDA-reviewed candidate and complete E8/E9.** Record the
   selected Focus and Arbiter weights, graphs, compiler and task policies,
   calibration, provider options, and Windows/Linux CUDA receipts. On the
   user's word, freeze the criteria and run E8 sealed confirmation once.
   Keep sealed judgments out of fitting. Complete the E9 report and obtain
   the route decisions before opening WebGPU qualification, as required by
   A3. Preserve the original controls and delivery precision rules. The
   current plan retires A4 calibration transfer, so WebGPU and CPU results
   need evidence on their actual routes. Keep the deferred FP32 replay
   diagnostic and run it only on the user's direction.
   [E8 and E9](final-plan-fp16.md#6-work-order)
   · [Decisions in force](final-plan-fp16.md#11-decisions-in-force)
   · [FP16 delivery approvals](final-plan.md#fp16-delivery-plan-approvals-20-november-2026-americadenver)
   · [Model catalog contract](../../crates/airk-retrieval/src/model_catalog/CONTRACT.md)
2. **WebGPU qualification on three targets.** Use one frozen model and
   policy selection, with a separate receipt and disposition for each
   operating system and adapter. Start with a same-card A6000 smoke against
   the matching CUDA candidate. Apply the existing five-times latency
   criterion to that smoke. Use the retained probes to choose graph and
   padding options for the current artifacts. Freeze the applicable
   per-target fit, latency and quality criteria before qualification.
   Use the qualified CPU artifacts from step 3 for final fallback
   checks. CPU preparation can proceed alongside local WebGPU work.

   **Pre-rental build and unit-test gate.** Build and unit-test macOS
   locally on the user's Mac Neo over the existing SSH connection after
   CUDA closes. Verify the SSH UID against the logged-in human before
   each batch, retain the receipts, and clean the Mac Neo target after
   each batch as required by the assignment. Use the existing local
   Windows and Linux environments for their respective targets.
   Add the native WebGPU provider and route selection for the service
   and its test harness. The existing `--provider webgpu` switch belongs
   to the standalone probe. Compile the WebGPU-enabled service, CLI,
   provider dependencies and integration test binaries for each intended
   OS and architecture. Run applicable unit tests and scoped Clippy on
   each local target. Fix build or unit-test failures before requesting
   a booking. Stage the matched binaries, provider packages, pinned model
   exports, fixtures and expected outputs. Dry-run the harness commands
   and verify the staged hashes, with exact launch, capture and cleanup
   commands ready for the rental. Record the build and unit-test receipts,
   selected counts and any untested hardware assumptions per platform.

   **Local A6000 integration before Radeon rental.**
   On Windows, rerun the standalone WebGPU smoke on the A6000 and verify
   the expected adapter and provider. Then select WebGPU in the native
   service with GPU opt-in enabled. Verify actual placement, Focus and
   Arbiter work independently and together, the model-free control,
   Answer evidence and CPU fallback on a controlled provider refusal.
   On Linux, run the standalone WebGPU probe in the existing Ubuntu WSL2
   instance first. Require evidence that WebGPU actually binds the A6000
   and runs the model there. If it passes, run the same native service
   checks on Linux. If WSL2 cannot expose the required adapter or
   provider, capture the refusal, retain the passing Linux build and
   unit-test receipts, and list the deferred native checks explicitly in
   the Linux Radeon runbook. Resolve product or harness failures locally
   before requesting a booking. Use separate Windows and Linux receipts
   and classify the A6000 passes as local integration evidence.

   **Local Focus-only WebGPU smoke on Mac Neo.** After the macOS build
   and unit tests pass, use the native service with GPU opt-in, WebGPU
   selected, Focus enabled and Arbiter disabled. Measure model-free
   headroom first. If the selected Focus export and required reserve fit,
   load one copy and run a short Answer with the smallest supported
   Focus workload. Verify that WebGPU binds the Apple GPU, Focus actually
   scores on it, and Answer returns evidence. Capture the adapter,
   provider, model identity, peak memory, result and clean shutdown.
   Stop if admission or placement fails or memory pressure breaches the
   reserve. Fix reproducible build, provider configuration and harness
   failures locally. If the Neo's hardware cannot admit Focus, retain
   the measured limit and prepare the same case for the rented Mac.

   **Rented-target scope.** Run full macOS integration and qualification
   on the rented Mac, including Arbiter at the full budget and both
   models together. Run Radeon-specific integration and qualification on
   the rented Windows and Linux targets. Compile the test binaries and
   prepare the fixtures and commands for all three targets before booking.

   [Build order](../ci-and-builds.md)
   · [Validation commands](../../CONTRIBUTING.md#agent-command-reference)

   | Target | Hardware and order |
   | --- | --- |
   | macOS | Build, unit-test and run a Focus-only WebGPU smoke on Mac Neo, then qualify on the planned rented Mac with a 32 GB GPU and 128 GB RAM. |
   | Windows Radeon | Qualify the Windows WebGPU route on the planned Azure Radeon V620 VM with 32 GB VRAM. |
   | Linux Radeon | Qualify the Linux WebGPU route on the planned Azure Radeon V620 VM with 32 GB VRAM. Capture its own driver, adapter, placement and fallback evidence. |

   **Rented hardware integration.** Put the staged hashes, completed
   local checks, deferred hardware checks, and launch and teardown
   commands in the booking handoff. Request booking only after that
   target's local preparation is complete. Booking remains a manual
   user action. On the rented hardware, run the native AIRK service
   with GPU opt-in and check the actual
   adapter, provider and operator placement for Focus and Arbiter. Exercise
   Focus only, Arbiter only, both, and the model-free control. Check numeric
   and decision drift against matching references and verify Answer quality
   on that route. Record evidence coverage, deadline behavior, cold load,
   warm latency, reload and peak memory against the model-free control.
   Verify that provider refusal or loss leads to the separately qualified
   CPU route and is disclosed. Record pass, CPU fallback, or blocked for
   each target. Keep WebGPU as the GPU qualification path for these targets.

   [Named hardware and booking rules](../agent-current-work.md)
   · [WebGPU probes and receipts](../../scripts/arbiter-webgpu-probe/README.md)
   · [WebGPU platform scope](final-plan-fp16.md#11-decisions-in-force)
3. **Explicit CPU and automatic CPU fallback.** Qualify the exact CPU
   artifacts and composed Answer on the approved CPU class, measuring
   at eight cores. Use the retained Focus CPU export as a control.
   Verify the full-budget and 2,048-token Arbiter rungs, then retrieval
   only when no model rung fits. Exercise the independent stage switches,
   GPU opt-out, absent provider, insufficient headroom and load failure.
   Verify the actual device, downgrade reason, deadlines, partial
   results, latency and memory. Establish the CPU evidence before
   claiming a WebGPU fallback disposition on any target. Keep these
   measurements outside the Section 12 CUDA baseline.
   [Device and fallback requirements](final-plan.md#11-automatic-gpu-execution-and-cpu-fallback-d08)
   · [Section 12 platform scope](final-plan-fp16.md#120-scope-cuda-only)
4. **Delivery qualification and final findings.** Verify the applicable
   installed runtime and client journeys, catalog acquisition and
   recovery, and exact-artifact packages. Reconcile current documentation
   and release notes. Report per-route support, measured limits, failures
   and unavailable checks, with separate dispositions for the local
   A6000, Mac Neo and rented hardware. Use existing authority for
   local builds and the branch. PR, merge, publication and deployment
   require separate authorization.
   [Final work order](final-plan-fp16.md#6-work-order)
   · [Installed qualification](final-plan.md#step-16--qualify-the-reviewed-integrated-candidate)
   · [Release verification](../release-verification.md)

Record evidence in the [campaign ledger](../simplify/one-process-ledger.md)
and decisions in the [loop change log](answer-loop-changes.md) and owning
contracts. Each substantive receipt identifies exact commands, inputs,
source and artifact hashes, selected tests, elapsed time and output
ownership. For Cargo include target, features and incremental setting.
Retain small receipts, settle owned writers and reclaim disposable fixtures
after their last use.
