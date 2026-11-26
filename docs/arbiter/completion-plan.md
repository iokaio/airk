# Answer loop and FP16 completion plan

**Paused at the Windows results review, 26 November 2026.** Sol finished 2 of 20 tasks correctly after receiving all 113 required source spans. The next decision is how Gather Context should help an agent use the evidence it retrieves. The [retrieval strategy](retrieval-strategy.md) traces all 20 tasks with ordinary source tools, maps the steps to AIRK and lays out the design for review. The baseline and change sets await that review.

**Windows and CUDA execution approved on 25 November 2026.** This plan
sequences the remaining work on feat/arbiter-final. The authorized scope
extends through the Windows results review. Apply the earlier review trigger
if Sol misses the complete-evidence threshold. Linux, macOS and WebGPU start
after the user reviews the Windows disposition and authorizes the next scope.
Use the current assignment for execution authority and the Answer plan for the
underlying contracts and acceptance criteria.
[Current assignment](../agent-current-work.md) · [Standing rules](../../AGENTS.md)
· [Answer work order](final-plan-fp16.md#128-work-order)


Complete section 1, the index research and the [autonomous repair milestone](#autonomous-index-repair)
in that order, then continue with sections 2 to 5. The
[agent-facing tool surface](#agent-facing-tool-surface) stage
starts after the Windows review and is frozen before the sealed run.

## Goals and how progress is measured

The Windows research question is whether an agent can make a correct change,
write a grounded requirement or finish a review after giving Answer the task
once. Answer should do the searching, following of leads and reading that
would otherwise consume the agent's next several turns. The index goal and
the acceptance rule below serve that question.
[Windows experiment and acceptance](#3-stage-l7-with-a-windows-first-review) · [Search findings](search-improvement-plan.md#what-we-achieved-and-where-it-falls-short)

### Where the evidence stands

| Measure on the 20 development scenarios | Result |
| --- | --- |
| Essential notes covered by an agent's tool results by the end of a session | Search 55 of 113 in 8.65 turns on average. The three Answer arms 37 to 44 of 113 in 9.70 to 10.15 turns. |
| Essential notes in the first reply | Search 14 of 113 originally and 20 of 113 with the selected Step 8 policy. Answer 19 of 113 at the loop baseline and 24 to 28 of 113 in the change set 1 replays. |
| Scenarios whose first Search reply covered every essential | 0 of 20 |
| Correct finishes | Two in the six judged reports, both by Search on airk in the one-turn series. The loop gate, the loop baseline and loop change set 1 had none. |

In the last judged comparison Answer covered fewer notes than Search and
took more turns. These are historical results on their own inputs. The fresh
baseline in Stage A measures today's stack.
[One-turn baseline](answer-evaluation/01-baseline.md) · [Loop gate](answer-evaluation/04-gate-lg.md) · [Loop baseline](answer-evaluation/05-loop-baseline.md) · [Change set 1](answer-evaluation/06-loop-change-set-1.md#first-sample-results)

### What the gate needs and how to steer toward it

The approved substantial-win rule is unchanged and remains the gate. A
correct finish needs every essential note of the scenario in the agent's
tool results, decided by script, and every one judged correct in the brief.
The 20 scenarios carry 113 essentials, between five and six each on average.
As a rough guide only, if notes were covered independently, full coverage on
12 of 20 scenarios would need about nine notes in ten covered. The best
session arm covers about half. The gate can therefore read zero through a
large real improvement, and it cannot express fewer turns while no arm
finishes.
[Rubric and rule](answer-evaluation/rubrics.md)

**Steering measures.** The coverage half of a correct finish is decided by
script from the turn log, so native replays can compute it with no agent and
no judge. Report these for every replay and every judged arm:

- scenarios whose first delivery covers every essential, and scenarios one
  essential short;
- the turn at which each essential is first covered, and the external
  searches and reads still made after the first delivery;
- essentials covered per repository and task kind.

They steer the change sets. They do not replace the gate.

**Complete-evidence experiment (decided 25 November 2026).** Before the
Windows baseline, give GPT-6.2 Sol the cited source spans for every
essential in the first Answer reply. Verify that the delivered bytes cover
all essentials. Use the ordinary brief, follow-up tools, turn cap and
rubric. Run one fresh session on each of the 20 development scenarios.
Two independent Astra sessions score each brief, and one separate Sol judge
provides a diagnostic verdict. Budget up to 40 scoring and 20 diagnostic
judge sessions. Only the Astra verdicts enter adjudication.

Report Sol's correct finishes overall and separately for the ten tasks in
each repository. Proceed to the normal baseline if Sol finishes at least
12 of 20 correctly and at least five in each repository. If either count
falls short, bring the failed briefs, the delivered passages and the judge
disagreements to the user for review before proceeding. The matched Search
and Parse comparisons in the normal baseline establish turn savings.

Astra and Terra can serve as agents in an optional later extension on the
same frozen inputs. That extension requires a separate decision. Retain
the Sol results under their original freeze and keep these cases out of
tuning.

### Agent and judge models

The user chose OpenAI for all model-assisted implementation and testing and
named GPT-6.2 Sol to implement the plan after approval. Use Codex for that
work, including delegated reviews, probe preparation, smoke tests and
judging. The proposed evaluation roles below preserve the five-path gate,
the stronger-agent comparison and the two independent judges. AIRK runs
its local MiniLM, Focus and Arbiter models inside the service under test.

| Role | OpenAI model ID | Coverage |
| --- | --- | --- |
| Gate agent | `gpt-6.2-sol` | All 20 scenarios on Search, Parse, Focus-only Answer, Arbiter-only Answer and Answer with both. Apply the substantial-win rule within this model at the baseline and each of the three change sets. |
| Capable-agent panel | `gpt-6.2-astra` | All 20 scenarios on Search, Parse and Answer with both local models at the baseline and the last change set. Compare Answer with Astra's own controls. |
| Complete-evidence agent | `gpt-6.2-sol` | All 20 development scenarios before the baseline. Optional later comparison agents are `gpt-6.2-astra` and `gpt-6.1-terra`. |
| Scoring judges | `gpt-6.2-astra` | Two fresh, tool-free sessions per judged brief. Give each the same blinded packet and apply the existing adjudication rule. |
| Diagnostic judge, ceiling only | `gpt-6.2-sol` | One extra tool-free verdict per ceiling brief. Report agreement with each Astra judge by agent model. Only the two Astra verdicts enter adjudication. |

Use medium reasoning effort throughout the evaluation. Give each agent a
fresh session, including when the implementer and gate agent share a model.
Keep model and path labels out of the judge packet. Record the limits of
blinding where the reply format reveals a path. Historical Anthropic
sessions retain their original identities. Establish a fresh OpenAI baseline.

**Headless runner.** Port the existing agent and judge launchers, pass
specifications and probes to OpenAI Codex before the first model-assisted
test. The implementation and tests must run with Claude Code absent and
without Anthropic credentials. Use a synthetic task to check the selected
model and effort, each distinct agent tool catalog, the 12-call cap and a
tool-free judge's schema-valid verdict. An agent sees only the tools for its
path. Exclude built-in shell, direct file and web access, other MCP servers
and inherited repository instructions. Reconcile the offered tools and
attempted calls, including refusals, with the wrapper log. Preserve failure,
timeout and discard outcomes. If the runner cannot establish isolation or
serve a selected model, record the blocker before collecting benchmark data.
[Agent launcher](../../scripts/arbiter-answer-eval/run_session.py) · [Judge launcher](../../scripts/arbiter-answer-eval/judge.py) · [Codex non-interactive mode](https://developers.openai.com/codex/noninteractive/)

**Freeze and reporting.** Pin the runner version and executable hash,
provider, model IDs, effort, prompts, tools and verdict schema for every role
before its first measured session. Give each agent model a separate pass
identity. Check that both scoring verdicts belong to the frozen judge model
and to distinct sessions. Reuse a control only when its model, effort,
freeze, publication and other comparison inputs match. Report usage by
role, with measured or estimated cost identified and missing cost shown as
unavailable. Update the harness defaults, its README and the current rubric
directions with the baseline harness, retaining the historical freezes.

The current grader groups by scenario, configuration and device. Two models
on the same path can overwrite each other's first sample. Before the
baseline, have the grader reject a model or effort outside the pass freeze
before grouping rows, including resumed sessions, reused controls and
close-call samples. Test one synthetic scenario where Sol wins and Astra
loses against their respective controls. Each pass must report its own
outcome in either processing order. A mixed pass must fail explicitly.
[Grader](../../scripts/arbiter-answer-eval/grade.py) · [Pass and reuse checks](../../scripts/arbiter-answer-eval/run_pass.py) · [Prompt freeze](../../scripts/arbiter-answer-eval/prompts_freeze.py) · [Judge and adjudication](answer-evaluation/rubrics.md#the-judge)

### Task kinds the gate judges

The judged gate uses the 20 frozen coding, writing, testing and architecture
scenarios. Judges assess the agent's work brief. On 25 November 2026 the
user chose separate, unjudged diagnostics for debugging and patch review:
give Sol a known failure and a specific patch and record the evidence Answer
surfaced or missed. Report those examples outside the gate totals.
[Scenario inventory](answer-evaluation/scenarios.md) · [Agent task reviews](#what-an-agent-must-be-able-to-do-with-the-reply)

### The index goal and its cost envelope

Show that the settled indexes do not cap Answer. Change an index only for a
measured class of misses that query-time work on the settled publications
cannot recover. For this round, keep the existing bundled FP16 MiniLM, its
CPU route and platform support. Compare each pilot with a fresh control
using the same representative inputs, host, indexing scope, query workload
and settings for each repository. Final index bytes, full build time,
incremental build time and 95th-percentile query latency must **each** be
at most 1.10 times that repository's control value. Use the same changed
files for the incremental build. The historical lexical-only measurements
provide context. The matched runs determine whether a pilot fits.
[Plain-store results](../indexing/performance-plan.md#10-status-report-and-handoff-24-november-2026) · [Delivery priorities](../agent-current-work.md)

### Decisions for the user at this review

Dated entries record the user's choices. The remaining proposals are open.

1. **Development quality and cost (decided 25 November 2026).** For
   task-derived queries, require a net gain of at least six of the 113
   essential facts within the top 40. Every fact the control covers within
   the top ten must still be covered there in its own repository. Apply the
   [10% cost limits](#the-index-goal-and-its-cost-envelope) separately to
   all four measures in each repository.
2. **Held-out confirmation (decided 25 November 2026).** Freeze 30 probes
   per repository before piloting. Compare the control and one finalist
   once. Require at least as many successful probes in each repository,
   more in at least one, and no new false match on any no-match probe.
3. **Index pilots and rebuild authority (decided 25 November 2026).** Test
   at most two small changes with the existing MiniLM. A passing finalist
   earns a rebuild proposal for the user's decision. Otherwise reuse the
   settled indexes.
4. **Implementation and evaluation models.** The user requires OpenAI
   throughout and named GPT-6.2 Sol for implementation after approval.
   The proposed evaluation roles and model IDs are in the table above.
5. **Complete-evidence scope (decided 25 November 2026).** Run Sol first
   on all 20 scenarios with up to 60 judge sessions. Apply the overall and
   per-repository review trigger above. Additional agent models are optional.
6. **Windows retrieval scope (decided 25 November 2026).** Test Code Facts,
   Document Facts, Relationships and a scoped literal scan of retained
   indexed text under today's limits first. Then compare one versus two
   follow-up reads per open need per later round and test need-based excerpt
   sizes. Hold total calls, bytes, three rounds and deadline fixed. Defer
   handle-only delivery until after the Windows review.
7. **Index model scope (decided 25 November 2026).** This round uses the
   existing MiniLM. A different embedding model falls outside these pilots.
8. **A third outcome at the Windows review:** ship the retrieval gains
   without the turn-saving claim.
9. **Debugging and patch review (decided 25 November 2026).** Use a known
   failure and a pinned patch as unjudged Sol diagnostics. The judged gate
   keeps its original 20 scenarios, four task kinds and scoring rule.
10. **Answer's public call shape (decided 25 November 2026).** Advertise
    `task` as the only required argument. The service supplies the schema
    and default budget and honors an explicit valid budget. Accept the
    existing full `question` request throughout the 1.9 release line.
11. **Default MCP tool profile and leaner default schemas:** decided on the
    free-choice comparison, after the Windows review. The CLI stays full
    featured either way.
12. **Public tool names:** the proposed renames, tested in the free-choice
    arm, and whether an old name stays callable for a release or the rename
    is clean at 1.9.
13. **Autonomous index repair (decided 25 November 2026).** Add post-index
    and idle checks plus retrieval-triggered tickets. A proven, bounded
    repair can improve the triggering reply within twice the
    ordinary time allowance and all applicable hard deadlines.
    [Repair rules and acceptance](#autonomous-index-repair)

## Milestones

| Milestone | Exit evidence or decision |
| --- | --- |
| Index and Search quality | Complete the rank census, the label and corpus audit, the source-fidelity checks and the two query-time checks. Run at most two matched representation pilots chosen from the census. Select and freeze the configuration for both repositories against the agreed bar. [Research and selection](#how-to-test-indexing-for-agent-tasks) |
| Autonomous index repair | Prove post-publication and idle discovery, same-request recovery within budget and autonomous background completion on isolated fixtures. [Repair rules and acceptance](#autonomous-index-repair) |
| Windows CUDA | Account for shakedown and L6 evidence on the admitted stack. Run the Sol complete-evidence experiment and apply its review trigger. Judge a current-stack baseline and three cumulative retrieval changes, then stop for your review. |
| Windows review | You choose another Windows round, Linux, or shipping the retrieval gains without the turn-saving claim. Each additional Windows round returns to the same review. |
| Linux CUDA | LX, Linux L6, and the agreed L7 comparisons against the Windows-reviewed candidate. Bring both hosts' results to review. |
| Agent-facing tool surface | Starts when the Windows review accepts the retrieval and Answer behavior, and can run alongside Linux CUDA. Record the candidate's catalog, run the free-choice arm, test the description rewrite, the proposed names and the narrowed default profile against it, and freeze the surface. [Stage and rules](#agent-facing-tool-surface) |
| E8 and E9 | Run sealed confirmation once on your word, on the frozen tool surface. Record the evaluation report and route decisions. |
| Local platform preparation | Build and unit-test all three WebGPU targets. Capture the Windows A6000 integration, Linux A6000 integration or the WSL2 limitation, and Mac Neo Focus-only smoke or its measured admission limit. |
| Rental and delivery | Request booking after platform preparation. Qualify the selected routes on each rented target, prove CPU fallback, and verify the final deliverables. |

## First milestone — Research and test indexing for agent retrieval

Start here on resumption, after the identity, disk and input-freeze steps of
section 1 and before Windows Answer changes, model tuning, or a judged
Answer baseline. This milestone asks two separate questions. Did indexing
lose or distort evidence? Could an accurate index be organized better for
agents using ordinary retrieval tools? A clean fidelity audit answers only
the first. The milestone is bounded. It ends with a selection against the
agreed bar, and the default is to reuse the settled publications.

Every measurement here is deterministic and runs on native replays,
read-only probes and scripts. Agent sessions are used once, at the end, on
the control and the selected configuration.

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

**What the index stores today.** Three properties of the current
representation bear on the candidates below. They are read from source.
Their effect on ranking is not yet measured.

- Each chunk's lexical row is the deduplicated set of terms from its path
  and text, in one column. BM25 therefore sees every term once per chunk. It
  has no term frequency and cannot weight a path, heading or symbol match
  apart from body text. This is a candidate cause of the Bookbuilder loss
  when ranked keywords were introduced.
- Terms are whole identifiers plus their underscore and camel-case parts,
  lowercased, with no stemming or plural folding. A run of capitals such as
  `APIKey` stays one term. Prose task wording can miss identifiers for that
  reason. The base lexical branch requires every query term to match.
- A vector unit targets 256 tokens, already carries up to 32 tokens of
  written heading or definition context, and has no overlap.

[Lexical terms](../../crates/airk-core/src/lexical.rs) · [Chunk rows](../../crates/airk-store/src/current/lexical.rs) · [Index schema](../../crates/airk-store/src/index/schema.rs) · [Vector units](../../crates/airk-retrieval/src/vector_units.rs)

**Step 1. Rank census.** For all 113 development notes on the two settled
publications, record the exact rank of the best qualifying unit where one
exists. For the two notes that require multiple passages, record the first
rank at which the retrieved prefix covers the note under the existing
same-path overlap rule. Count each fact once at each cutoff. Measure ranks

- in the full vector scan, in the ranked lexical list, and after fusion,
- for three query forms: the instruction as sent, a focused query derived
  from the task text alone with the Step 2 subquery derivation, and a query
  built from the note's own fact sentence.

Hold publication, model state and budgets fixed. The vector scan is exact,
so the full rank is available without a wider public reply. Only five of the
20 requests retained pre-fusion traces, and 16 of the 22 misses traced there
have a qualifying unit of unknown rank. The census replaces that sample and
sorts every miss into one of three classes.

| Class | Evidence | Owner |
| --- | --- | --- |
| Representation | The qualifying unit ranks poorly even for the query built from its own fact | An index pilot from the table below |
| Query wording | It ranks well for a focused or fact-built query and poorly for the instruction | Answer's planning and follow-up queries in Stage A |
| Search policy | It is inside a branch list and is lost at fusion, selection or byte packing | Ordinary Search, repaired where the control regresses |

The fact-built query uses the labels. It is a diagnostic, and nothing is
tuned on it. Trace the two lost Bookbuilder essentials through the same
stages.
[Vector coverage and limits](search-improvement-plan.md#step-6-result--committed-vector-coverage) · [Subquery derivation](search-improvement-plan.md#step-2-result--focused-semantic-queries)

**Step 2. Label and corpus audit.** Bookbuilder holds 48 of the 66 original
misses with no hit from an accepted path. Before those count against the
index, have an independent grader check a sample of the returned hits for
missed notes. Does a hit establish the note's fact from a location the note
does not list? Report accepted alternatives beside the frozen counts.
Changing a frozen note stays under the freeze rule and is the user's
decision. Then classify both corpora by path class (source, tests,
generated, vendored, documentation and bundled payloads) and report each
class's share of files, units and top-40 candidates. If a class crowds the
shortlist, source selection or a path-class weight is the first index
candidate.
[First-Search miss attribution](../simplify/one-process-ledger.md#25-november-2026--first-search-miss-attribution) · [Freeze rule](answer-evaluation/scenarios.md#how-the-instructions-are-used)

**Step 3. Source-fidelity checks.** Start with retained receipts and
read-only probes on the two settled publications. Use an independently
enumerated source manifest, a scoped literal scan and reads of the pinned
Git exports as the reference. Keep reference answers, probe design and
diagnostic-only source access out of product inputs and judged agent
sessions. Report Bookbuilder and Munarium separately.

| Question | Discriminating check | What the finding changes |
| --- | --- | --- |
| Did indexing admit the intended evidence? | Reconcile eligible paths, file hashes and declared skips against the selected, pinned export. Check former hits, current misses and a stratified sample of independent files, including code, tests, documentation, long files, duplicate names and excluded controls. Track intentional exclusions separately from unexplained omissions. | A missing eligible file, an incorrect revision or out-of-scope inclusion belongs to selection, ingest or publication. Fix that path before judging downstream changes. |
| Did it preserve useful content and locations? | Open known paths through the native reader. Compare exact lines and boundaries to independent source bytes. For extracted formats use independent page, sheet or section anchors and the original location mapping. Check clipped sections, duplicate content at different locations, update/delete and unchanged-file reuse in a small disposable fixture. | A reproducible missing or mislocated passage in a known file is an index or materialization defect. Isolate the writer from the reader, correct it, and rerun the affected probe. |
| Does lexical and structural lookup find what was stored? | Compare known unique literals, punctuation-bearing identifiers and scoped whole-name matches with the direct scan. Check postings or candidate membership before ranking, then source range after ranking. For symbols, facts and edges record provider support and ambiguity. | Distinguish an absent posting or misbound symbol from tokenization policy, a bounded query or a later ranking loss. Missing optional provider data becomes an explicit coverage gap. |
| Are the right vectors attached to the right passages? | Reconcile committed rows to eligible occurrences and member spans. On a stratified sample reproduce the exact model inputs, tokenizer, truncation, pooling, normalization and route. Compare stored embeddings and independent exact scores within the admitted numeric tolerance. Include one cross-route case: a query embedded on the processor route against vectors built on the graphics card, compared by rank agreement. | A wrong input, row binding or scoring mismatch warrants an indexing or reader fix. A rank disagreement between routes is a product finding, because the processor is the default route. Whether correct vectors rank relevant passages well is the census's question. |

The existing vector audit established that 111/113 development notes have
one qualifying committed unit and the other two need a union of passages.
This narrows the suspected failure without establishing global rank, vector
quality, lexical coverage, or end-to-end index fidelity. Reuse that audit
where it already answers the question.
[Cross-route decision D5](../indexing/performance-plan.md#4-decisions)

**Step 4. Two query-time checks on the settled publications.**

- *Follow-up queries.* Steps 1 to 8 of the Search study were selected on the
  first query of each scenario, a 40 to 68 word instruction. Of the 41 notes
  the historical Search sessions recovered later, further searches first
  found 19 and targeted reads first found 22. Replay those recorded searches
  and ranges verbatim under the original policy and the Step 8 policy, with
  equivalent scope, budgets and source revision, naming any incompatible
  call. Freeze Step 8 as the control only if it does not lose on these
  queries.
- *A wider Focus set.* The 40-candidate shortlist held 28 essentials and the
  first ten held 18. Focus scores four hits and has not added an essential
  to a reply in any step. Score 16 and then 40 shortlist candidates once and
  report essentials in the first ten, latency and device memory. This is the
  direct test of whether a reranker converts the shortlist into the reply.

[Step 7 shortlist](search-improvement-plan.md#step-7-result--wider-shortlist-and-cutoff-selection) · [Later Search calls](../simplify/one-process-ledger.md#25-november-2026--first-search-miss-attribution)

**Historical cause.** Old-format indexes are abandoned at 1.9 and the
earlier producer is retired. Whether a weakness predates the plain store
does not change the next action, so no lineage study runs. Report a current
defect or limitation with origin unknown. If the census points at a specific
writer change, a small matched old and new build on identical representative
files can test it. The retained pre-refactor Bookbuilder depth probe used
different queries, limits and embedding pipeline. It is not a matched
comparison.

**Exit decision.** Produce one report per repository with the census table,
the audit results, the fidelity findings and the two query-time checks. Show
denominators, the first failing stage, a reproducer, confidence and
unexplained cases. The primary measure is essentials whose qualifying span
is in the top ten and in the top 40 for task-derived queries, including
facts covered jointly by multiple passages. Hold Answer
changes for any demonstrated upstream defect that prevents a required fact
from being recovered, or for an unexplained material source-fidelity
mismatch. Repair and recheck the affected layer on a small fixture first.
If a correctly represented passage is lost by ordinary Search, repair a
repeatable Search regression where it affects the control and remeasure, or
demonstrate an alternative canonical read that reaches it from the task or a
previously observed clue within Answer's actual scope and budget. Opening
the grader's known target only establishes storage. A bounded limitation
can proceed with its affected tasks explicit in the report. Reconcile the
frozen publications or take a documented index proposal to the user before
a full build. Carry this disposition into the experiments below.

### How to test indexing for agent tasks

1. **Freeze the control and the held-out probes.** The control is the two
   settled publications with the Search policy that passed the follow-up
   check. On both pinned exports keep the 20 development scenarios and add
   the held-out retrieval probes agreed at this review. Include
   natural-language probes that omit the exact identifier, negative and
   ambiguous lookups, and known successes as well as misses. Establish
   expected facts, accepted locations or a no-match outcome independently,
   using a session with no access to development notes or tuning results.
   Freeze each probe, its scoring rule and its hash before the first pilot.
   Keep labels and known
   target paths out of queries, indexed metadata and training. Keep the
   sealed set closed.
2. **Measure each reader alone.** Extend the census to the structural
   readers. For each note, can Symbols, References, Document Facts or
   Outline reach its location from an anchor in the task text, and from an
   anchor in the first Search reply? Record which readers share an
   underlying representation and where provider coverage is absent. Reader
   sequences are measured once, in change set 1 with both models off, as
   fixed recipes. Driving them here would need a script written with
   knowledge of the labels, or an agent, and an agent run is the Search arm
   of the baseline.
3. **Build and test small matched representations.** Choose at most two
   candidates from the table below, each from a census class or an audit
   finding, and state what each should recover. Test a query-time candidate
   on the settled publications first. For an index-time candidate, index
   identical representative sources with the control and candidate writers.
   Hold query policy, model and budgets fixed. If a change needs a new
   reader, report a writer and reader result until their contributions can
   be separated.
4. **Select on development data, then confirm once.** With Focus off,
   compare eligible pilots on the development facts and matched costs.
   Report gains, displaced notes, wrong citations and each cost ratio per
   repository. Select one finalist that meets decision 1. If none qualifies,
   keep the control and leave the held-out set closed.

   Freeze the finalist and evaluate it and the control once on the 30
   untouched probes per repository using the prespecified success criteria.
   Apply decision 2 and report the paired counts and any new false matches.
   If it misses, keep the control. Do not tune or try the other pilot on
   those probes. Use development cases for the separate Focus-on check.
5. **Confirm with agents once, then choose.** Give a fresh agent the control
   and the selected configuration on the same bounded task and compare
   correct work briefs, remaining direct-file reads, turns and bytes. Skip
   this when the control is selected. Record the tested candidates,
   limitations and selection in the research report and ledger. Reject an
   index with no demonstrated improvement on that evidence. If a candidate
   warrants a full rebuild, present its measured gain, cost, migration and
   recovery plan to the user first. Qualify and freeze the selected
   publications, or reuse the existing ones when they win the comparison.
   Only then establish the current-stack Answer baseline. A later L7 trace
   can reopen a specific indexing question.

### When to change an index

The candidates are ordered by what the measurements so far point to. Each
names its trigger from the census or the audit, and whether it can be tested
on the settled publications or needs a pilot build.

| Trigger | Candidate | Tested on |
| --- | --- | --- |
| A path class crowds the top 40, or accepted paths never enter it | Source selection, or a path-class weight | Settled publications for the weight. A pilot build for selection. |
| Prose queries miss chunks whose identifiers hold the word in another form | Query-side singular, plural and split-capital variants. If that recovers notes, fold the same normalization into indexed terms. | Settled publications first |
| Ranked lexical order is flat, or favors the wrong chunks of the right file | Lexical rows that keep term frequency, with path, heading, declared symbol and body as separately weighted columns | Pilot build |
| Scoped scans recover exact strings or identifier fragments that lexical lookup misses | The retained-text scan of change set 1 is the control. Add positional literal or trigram postings, with final retained-byte verification, only if the scan is too slow at corpus size. | Settled publications for the scan. A pilot build for postings. |
| The correct file ranks but useful context crosses a unit boundary | Query-time parent expansion first. Then unit overlap, or file and section links to passages. Keep the corpus-wide passage route for isolated facts. | Settled publications first |
| A qualifying unit ranks poorly even for the query built from its own fact | Add path or enclosing-symbol text to the context a unit already carries, with the same MiniLM. Verify citations against original source spans. | Pilot build |
| Impact work stalls on aliases, implementations, config consumers or test discovery | A provider-backed definitions and references index with typed edges and coverage, and explicit document links, heading IDs and supersession metadata for governance questions. Keep inferred associations distinguishable from verified edges. Neither publication has provider data today, and none of the 83 symbol-annotated essentials names its symbol in the query, so this pays only on follow-up hops. Last in order. | Pilot build and a provider |

Pin identical source revisions, selection and query policy in control and
treatment. Change one representation at a time and compare its own searchable
publication with the control. Measure rank gains and losses on the
development notes and the four matched costs for each pilot. Record memory,
peak temporary disk and migration or rollback needs. Only the frozen
finalist and control enter held-out confirmation. Reserve a matched Answer replay for L7 after
selection. Propose a full build only with repeatable recoveries on distinct
tasks, a measured cost inside the envelope above and a plan for affected
existing publications. Discuss the specific representation and evidence with
the user first. Keep accepted originals available and record contracts for
any persisted format or digest change.
[Index alternatives and prior audit](search-improvement-plan.md#alternatives-to-test) · [Step 4 structural result](search-improvement-plan.md#step-4-result--exact-structural-evidence)

### Autonomous index repair

Build and qualify these two mechanisms on isolated fixtures after the index
research and before the Windows baseline. Use the service's existing scheduler,
maintenance guards and canonical retrieval path. Keep measured publications
and budgets frozen during the pilot, judged and sealed comparisons. Exercise
repairs in their own tests. The loop runs locally without a human judge,
ticket triage or per-repair approval.

1. **Check each published index and revisit it during idle periods.** After a
   successful indexing run publishes changes, coalesce changed artifacts by
   collection, root and publication. Check that expected identifiers resolve to
   retained passages, citations reproduce the indexed bytes, and sampled
   semantic units bind to the expected inputs once vectors are ready. Rotate
   through older artifacts on idle sweeps, including when no files change.
   Reuse physical Verify where applicable. Yield to foreground work and bound
   the queue, disk, CPU and I/O per sweep. Repair-generated events cannot
   schedule another sweep of the same work.
2. **Let retrievals nominate defects and verify them automatically.** A
   broken citation or a missing posting for a known indexed term can trigger
   a check on the first occurrence. Repeated empty or truncated results can
   trigger sampled diagnosis. Use pinned, independently checkable evidence
   to distinguish storage or indexing faults from ranking, legitimate
   no-match, readiness and capacity outcomes. Merge reports of the same
   defect into one local ticket. Respect usage-recording settings. Retain
   only a bounded diagnostic recipe with opaque scoped identifiers, defect
   class and outcome. The original query stays in memory for its call.

**Repair on the triggering retrieval.** Keep the ordinary result available.
For example, retained bytes and the active tokenizer can establish that an
eligible identifier is missing from a lexical posting. If a bounded repair
is available, replay the original request once through ordinary retrieval
after repairing that posting. Return the corrected result **on this call**
when the replay recovers the expected cited span and passes the original
filters, scope, revision and response limits. Only include verified evidence
in a fallback reply and report any resulting coverage loss.

Let `B` be the ordinary time allotted to this retrieval operation at
admission. Allow at most **2 × B total from request arrival**, capped by
the earliest caller, parent-operation, service or transport hard deadline.
For example, a 100 ms allowance gives the entire call 200 ms, or 150 ms
if the caller's deadline is 150 ms from arrival. The first lookup, diagnosis,
lease acquisition, repair, verification, replay and response all share that
clock. Admit one attempt only when a measured bound for that repair class
fits the time remaining, with time reserved to deliver the fallback. If
either allowance or repair bound is unknown, time is short or capacity is busy, return
promptly and leave the ticket to the background worker. Honor cancellation
immediately. Nested reads share the outer deadline and work budgets.

**Repair authority and ticket closure.** Both paths use the same bounded
set of repairs that regenerate derived data from retained, authorized inputs.
Test the proposed repair against its independent probe and nearby known
success and no-match controls before publication. If a check fails, retain
the previous state. The maintenance owner rechecks the captured incarnation,
publication and settings under its lease and publishes atomically. Concurrent
reports share one repair. A request with an explicit publication pin keeps it. A request
that permits the repaired publication may replay wholly against it and cite
its revision. On a race, discard stale work and recheck in the background.
Close a ticket as repaired only after its probe succeeds through normal
retrieval on the committed state. Verification replays cannot open tickets.
A nonreproducing signal expires. A failed or ineligible repair records the
reason and goes dormant after bounded retries until relevant inputs change.
Source edits, full rebuilds, destructive fallback and ranking or model changes
fall outside the loop's authority.
[Maintenance guards](../service-maintenance.md#preview-and-approval) · [Index verification](../../crates/airk-store/src/index/CONTRACT.md) · [Local usage](../usage-reporting.md)

**Acceptance.** Seed an omitted posting with a known cited answer and show
that the very first external request returns that answer within the cap.
Use an injected clock to exercise a tighter caller deadline, a slow repair
and cancellation. For the slow repair, show a timely fallback and a later
ordinary retrieval that succeeds after background verification. Exercise
a post-publication defect with no user traffic, an older defect found by
an idle sweep, a legitimate empty result, a pinned reader during a
publication race, and concurrent readers producing one repair. Compare
against a healthy control per repository. Record the fraction of confirmed
defects fixed on the triggering call, background recoveries, incorrect
repairs and p95 latency for ordinary and repair-triggering calls separately.
Require zero incorrect repairs, scope violations and hard-deadline overruns
in these cases. Apply the existing 1.10× p95 cost limit to the matched
ordinary workload, including its observed mix of repair attempts.

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

**Before the run starts.** The user approved the Windows retrieval
experiments in decision 6. Record the new read types, the experimental
follow-up allowance and the shared budget limits in the loop change log and
owning contract before implementing them. Measure the baseline on the
existing retrieval behavior and delivery protocol. Verify the approved
[public Answer call](#agent-facing-tool-surface) through the actual tool and
benchmark before freezing the baseline. Run the
[Sol complete-evidence experiment](#what-the-gate-needs-and-how-to-steer-toward-it)
first and apply its review trigger before starting the matched baseline.
Report the two experiments separately.
[Decisions for this review](#decisions-for-the-user-at-this-review) · [Core loop limits](../../crates/airk-core/src/arbiter/answer_loop.rs)

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
   and the old-versus-current per-note differences. The first milestone
   already replays the historical Search arm's recorded follow-up searches
   and targeted ranges on today's stack. Carry that result here. It isolates
   what the same follow-up probes now return. The fresh blind sessions
   measure whether an agent can still *discover* those probes from today's
   earlier replies. The historical sessions used Haiku agents, so compare
   them with the fresh sessions as history and not as a matched pair. If the total
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
   harness, job policy, and the approved OpenAI
   [agent and judge models](#agent-and-judge-models) at medium effort.
   Record baseline headroom against the stronger control per
   scenario. Use both-models-off Answer in matched replays and label any
   separately judged no-model control. Keep this baseline throughout the
   three changes and show each candidate against it and its predecessor.
   For every arm report the
   [steering measures](#what-the-gate-needs-and-how-to-steer-toward-it):
   scenarios whose first delivery covers every essential, scenarios one
   essential short, and the turn at which each note is first covered.
   [L7 work order](final-plan-fp16.md#128-work-order) · [Rubrics](answer-evaluation/rubrics.md) · [Report layout](answer-evaluation/README.md)
2. **Change set 1 — let Answer gather the evidence.** Replay the same 20
   development tasks on the frozen publications with Focus and Arbiter off.
   Use one fixed recipe per task kind. Start from the task and follow only
   anchors found in earlier reads.

   First compare the existing loop with one that can use Code Facts,
   Document Facts, Relationships and a scoped literal scan of retained
   indexed text. Keep the present follow-up allowance and per-read byte
   allocations for this comparison. Then hold the reader roster fixed.
   Compare one versus two follow-up reads per open need in each later round.
   At the same follow-up allowance, compare fixed excerpt sizes with
   need-based sizes that use short discovery reads and longer decisive
   passages. Keep the same total call and byte limits, three-round ceiling
   and deadline in every replay.

   Report the first read that recovered each essential and any essential
   lost when another need consumed its share. Show gains and losses per task
   and repository, together with external reads still needed. Carry the
   best-supported combination into the judged arms. Use the existing
   delivery protocol throughout the Windows series.
   [Retrieval rounds](#retrieval-rounds-inside-one-answer) · [Evidence budgets](#read-and-delivery-budgets)
3. **Change set 2 — let the models choose consequential work.** Hold gathering
   and allocation fixed. Compare deterministic choices with Arbiter choices
   over the same eligible actions, with Focus held fixed. Separately compare
   Focus off/on for the same candidates and Arbiter setting, at the
   candidate count the first milestone's wider-Focus check selected. Then
   test the
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
   gap survives direct indexed reads and retrieval tuning, use an available
   slot within the two-pilot limit or bring the finding to the Windows review.
   Once the held-out probes have been opened, they cannot confirm another
   finalist. Show gains, displaced essentials and external reads still
   necessary for each repository and task. Discuss any proposed full
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
Prove the approved routes on missed cases. Bind Code Facts, Document Facts
and Relationships to their canonical readers and the literal scan to the
authorized retained text. Record the Core contract and exercise its source
bindings and transport fixtures when extending the roster.
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
deadline. The Windows comparison keeps three rounds. When today's single
follow-up per open need cuts a productive chain short, trace whether a
second follow-up finds the missing evidence and which other needs still
receive their share of the same budget.
[Loop model roles](final-plan-fp16.md#122-how-the-models-take-part) · [Native limits](../../crates/airk-core/src/arbiter/answer_loop.rs)

**Use current files when the question requires current files.** This
capability is deferred until after the Windows review. It needs a new
source-authority contract and its security review, the pinned comparison
cannot use it, and no measured class of misses yet depends on current
bytes. If Stage A traces reveal such a class, bring the evidence to the
Windows review. The matched Windows comparisons use the pinned indexed
sources. For a task about a saved change, use a caller-supplied or otherwise explicitly authorized
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

In the need-based allocation comparison, use small discovery reads and
larger reads for decisive functions, tests and document sections.
Redistribute unused bytes across open needs within the same total budget. Score compact, cited choices
while retaining the longer source spans. Reserve delivery space for the
decision-bearing excerpt from each essential need and any conflict or gap.
Merge overlapping text while keeping its distinct provenance and locations.
Show the agent exact paths and line or original document ranges it can open
directly. Mark a clipped exception, branch or assertion as incomplete. In
Windows, record the external round trips the existing delivery requires.
Use those observations in the later handle-only delivery design.
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

For the debugging diagnostic, give Sol the symptom of a known failure on a
pinned fixture. Check which returned passages identify the cause and which
facts Sol still needs. Record a reproduction only if the fixture actually
exhibits the failure. For patch review, give Sol a specific pinned diff
and check its finding against the changed hunk, affected caller and any
counterevidence. Report what Answer surfaced or missed in each example.
These two diagnostics contribute no scores, wins or turns to the judged
gate and support observations only about the cases exercised.

Preselect a small development sample spanning the task kinds above and both
repositories. Give a fresh agent only the task and the initial Answer delivery,
including a partial if that is what arrived. Block follow-up reads in this
diagnostic and independently check the brief or proposed action against the
reference facts for that case. This tests whether the first reply stands on its own. In a
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

Present the Windows baseline and series comparisons, Sol's complete-evidence
correct finishes by repository, the steering measures and the Astra panel
alongside the Sol gate. Report what Focus and Arbiter each added, which acceptance criteria
were missed, and which candidate and receipts support the comparison. Include both index-quality dispositions, unresolved retrieval
failures, and examples of external work displaced or still required.
Wait for the user's decision before starting Linux LX or L7.

- **Repeat on Windows.** Agree on the next changes and number of iterations
  at the review. Preserve the original baseline, rerun the affected
  measurements and comparators, and bring the new results back for manual
  review. Repeat this decision point until the user approves moving to Linux.
- **Proceed to Linux.** Freeze the Windows-reviewed candidate, model and
  calibration identities, scenarios, rubrics, harness configuration, and
  comparison set. Carry forward the agreed evaluation scope for Linux.
- **Ship the retrieval gains without the turn-saving claim.** If the rule is
  missed again and the evidence-ceiling arm or the steering measures show
  the remaining gap is outside Answer's reach, the user can stop the Answer
  series. The Search changes, the selected indexes, and the Focus and
  Arbiter routes then go forward on their own evidence. Decide at this
  review which later milestones still apply: Linux CUDA for the model
  routes, the sealed run, WebGPU preparation and the rentals.

Once the retrieval and Answer behavior is accepted at this review, the
[agent-facing tool surface](#agent-facing-tool-surface) stage starts. It
applies under each of the three outcomes above.

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

## Agent-facing tool surface

Start this stage when the Windows review accepts the retrieval and Answer
behavior. It can run alongside Stage C, because it changes what an agent is
shown and not how a route computes. Freeze its result before the sealed
run, so that E8 confirms the surface that ships. The approved `task` call
must already be in place for the Stage A baseline.

**What the evaluation tests and what the product advertises.** The harness
wrapper offers an agent six tools on the Answer and Search paths and one on
Parse. It prescribes the first call, and it supplies Answer's `schema` and
`budget` itself, so the judged agent calls Answer with a question alone.
The product advertises more and asks more:

- The installed service read for this review listed 30 tools to its MCP
  client. The branch builds its public catalog from the native tool router
  plus the Explorer drafting, template and help tools.
- The Core Answer request requires `schema`, `question` and a `budget` that
  carries every loop limit. The advertised `schema` is a string with no
  description.
- The server guidance routes identifiers to Symbols, References and reads,
  and concepts to Search. It does not mention Answer. Answer's description
  says what it does and not when to prefer it.
- Four tool definitions read on the installed service ran to roughly 3 to
  5 KB each. Most of that is ten shared scope, response and verification
  parameters that the evidence tools repeat.

A passing gate therefore shows what Answer does when it is called the
wrapper's way. It does not show that an agent holding the product's tools
chooses Answer, or can call it. The installed service may be older than
the candidate, so the first step below records the candidate's own catalog.
[Wrapper tool lists](../../scripts/arbiter-answer-eval/README.md) · [Server guidance](../../crates/airk-mcp/src/unified.rs) · [Public catalog](../../crates/airk-mcp/src/combined.rs) · [Answer request](../../crates/airk-core/src/arbiter/answer_loop.rs)

**Approved public Answer call for the Stage A baseline.** The user selected
option A on 25 November 2026. Advertise `task` as the only required argument.
The service maps its text to the canonical Answer question and supplies the
request schema. An omitted budget uses the service's complete default. An
explicit valid budget passes through with the caller's values. Existing
clients can send the full `schema`, `question` and `budget` request throughout
the 1.9 release line.

During implementation, record this public input contract in the owning
contract and remove the wrapper's injected schema and budget. Send the same
`task` through the real public tool and the benchmark. Compare the canonical
question, schema and every effective budget field at the service. Exercise
an omitted budget and an explicit non-default valid budget, then check the
existing full request against its equivalent `task` call.
[Answer contract](../../crates/airk-core/src/arbiter/CONTRACT.md)

**Step 1. Record the candidate's catalog.** From the accepted build, save
the public tool list as an MCP client receives it. Report the tool count,
the bytes of definitions and guidance, the parameters the tools share, and
each statement in a description that the accepted behavior made untrue.

**Step 2. Free-choice arm.** Give a fresh agent the task and that catalog
unchanged: every advertised tool, its description and schema, the server
guidance, no prescribed first call and no supplied arguments. Use the gate
agent on the 20 development scenarios with the same turn cap, audit and
judges. Record:

- the first tool called, and the order of tools through the session;
- whether the first Answer call is accepted, and each refusal with its
  reason;
- calls to tools that could not serve the task, and repeated or abandoned
  calls;
- the steering measures, turns and bytes, beside the prescribed-first-call
  results for the same candidate.

This arm is the control for each change below.

**Step 3. Test changes against that control.**

| Change | What it is | Needs |
| --- | --- | --- |
| Guidance and descriptions | Rewrite the server guidance and each description to say when to use the tool, what it returns and what to call next. State when Answer is the first call and when a direct lookup is. Remove statements the accepted behavior made untrue. | Regenerated catalog fixtures and help text |
| Tool names | Rename the public tool names that mislead or that collide with an agent's own tools, as proposed below. | The user's recorded decision, a ruling on old names, and an update to everything that names a public operation |
| Default tool profile | Advertise a small evidence set by default. Proposed start: Answer, Search, Symbols, References, Read File, Context, Outline, List Files, File Info and Status. The drafting, template, proposal, handoff and diagnostic tools move to a full profile that a user or client opts into. | The user's recorded decision and a contract for the profile setting |
| Leaner default schemas | Leave shared verification, response and scope parameters out of the default schemas where their defaults serve an agent. The handler and its defaults do not change. | The user's recorded decision |

Test the description rewrite first, because it needs no decision. Test the
names next, and compare the full catalog with the narrowed profile last, so
the three effects stay separate.

**Proposed names.** A name can decide the choice before a description is
read. One MCP client observed in this review listed the AIRK tools by name
alone until their definitions were loaded, and several current names are
generic enough to lose to an agent's own file reader and search. This is a
hypothesis until the free-choice arm measures it. Rename only where the
current name misleads or collides.

| Current public name | Proposed | Reason |
| --- | --- | --- |
| `answer` | `gather_context` | The tool returns cited evidence for a whole task and does not answer. |
| `search` | `search_passages` | Says what comes back, and separates one ranked query from the multi-round tool. |
| `symbols` | `find_definition` | The tool defaults to exact written definitions. |
| `references` | `find_references` | Joins the verb family. Its description must open with "textual occurrences", because the name suggests resolved references. |
| `read_file` | `read_indexed_range` | Collides with an agent's own reader. Says the read is bounded and comes from the indexed revision that citations point to. |
| `context` | `read_around` | Vague today, and it would collide with `gather_context`. |
| `outline` | `outline_file` | Says it takes one file. |
| `list_files` | `list_indexed_files` | Collides with directory listing. |
| `file_info` | unchanged | Clear enough. |
| `status` | `index_status` | "Status" alone could mean Git or the service. |

- The baseline already uses the approved `task` argument. Hold it fixed
  while comparing proposed tool names. The 1.9 compatibility decision for
  the existing full request applies independently of any tool rename.
- The catalog already separates public names from native ones, `answer`
  from `airk_answer`. The rename changes the public layer only. Native names
  and CLI commands stay.
- The feature keeps the name Answer in documents, Admin and Workbench. Only
  the public tool name changes.
- Prepared Parse plans, the extension, Workbench and the harness catalog
  name public operations. Update each with the rename.
- Whether an old public name stays callable for a release, or the rename is
  clean at 1.9, is the user's decision.

**Rules for narrowing.**

- Narrow what MCP advertises. Remove no capability. The CLI keeps every
  operation, and Admin and Workbench keep theirs.
- Each operation keeps one canonical handler. Whether MCP advertises it is
  a per-operation applicability entry, as it already is for management
  operations. Add no MCP-only implementation, and no alias that makes
  surfaces look equivalent. The old public name of a renamed tool is a
  separate ruling, listed with the proposed names.
- The full profile restores the present catalog. A parameter left out of a
  default schema keeps its default and stays in the full profile.
- Prepared Parse plans, the extension, Workbench and the harness catalog
  name public operations. Check each before a tool leaves the default list.
- An advertised tool keeps full read parity for the inputs and fields it
  advertises.
- Decide each cut on the free-choice comparison. A tool leaves the default
  profile when agents finish at least as well without it.

**Acceptance.** Adopt a surface change when, in the free-choice arm, it
raises the scenarios whose first delivery covers every essential or lowers
the turns to coverage, lowers neither repository, and every first Answer
call an agent makes is accepted. Report the Astra panel beside the Sol gate
agent on the final surface. Then freeze the catalog, names, guidance and
profile with the candidate, regenerate the harness catalog from that build, and run
the sealed confirmation on it.
[Read parity](../read-interface-parity.md) · [Steering measures](#what-the-gate-needs-and-how-to-steer-toward-it) · [Agent and judge models](#agent-and-judge-models)

## 4. Close the parallel Section 12 work

- **L2c–L2e, job delivery and clients.** Reuse the merged L2c work and its
  receipts. Finish evidence-first replies. After the Windows review, settle
  how an authorized client recovers a lost reply and how a second authorized
  client retrieves earlier evidence. Record those semantics before adding
  handle-only continuations. Prove one worker execution across
  disconnect and reattach, separate executions for `fresh`, and correct
  selection authority, retention, and per-delivery records. Count a
  continuation as another agent turn. Implement and verify distinct
  “Stop waiting” and “Cancel job” behavior and full recovered-result parity
  across the applicable clients. Apply the planned independent and
  security reviews. Independent job and client checks may run alongside L7
  with the evaluated Windows delivery protocol held fixed. Keep the policy
  fixed within each series. If a later series takes a
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
   calibration, provider options, Windows/Linux CUDA receipts, and the
   frozen [agent-facing tool surface](#agent-facing-tool-surface). On the
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
