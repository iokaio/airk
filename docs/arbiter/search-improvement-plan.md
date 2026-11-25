# AIRK Search improvement plan

**Status, 25 November 2026.** The eight-step Search campaign is measured
through the selected Step 8 policy. Windows Answer work now starts with an
indexing research milestone. It tests source fidelity and alternative index
representations through ordinary single and combined reads, then freezes
the selected indexes before a fresh matched Answer baseline.
[Completion plan and upstream decision](completion-plan.md#index-and-search-quality-before-answer)

Improve the evidence returned by ordinary AIRK Search on Bookbuilder and Munarium. Test every proposal with Focus off and on. Keep Arbiter disabled in every run. The first five proposals reuse the settled indexes. The sixth needs a small experimental index only if the coverage audit justifies it. Steps 7 and 8 reuse the publications selected at the end of Step 6.

## What we know

- The original Search-study baseline is the first Search call from each of 20 development scenarios, ten per repository. Each call sent the full 40–68-word instruction and returned ten hits under an 8,192-byte response budget. Search returned qualifying citations for 14 of 113 essential notes.
- Of the 99 misses, 66 had no hit from an accepted source path, 30 had a hit from the right path with zero line overlap, and three had too little overlap to count. Bookbuilder accounted for 48 of the path misses. Munarium accounted for 23 of the wrong-span misses.
- The retained Search sessions found 41 more notes later. Additional searches first found 19 and targeted file reads first found 22. An older wider Search probe found the same 14 in its first ten and 16 more deeper in the list. The older probe used different query strings, publication tokens and a wider budget. It gives us a reason to measure depth on the current inputs.
- The settled publications contain 156,255 Bookbuilder vectors and 40,536 Munarium vectors. Ordinary hybrid Search embeds the full query once, scores eligible committed vectors by exact dot product, and admits up to 100 resolved semantic units before fusion. In the matched Focus comparison, Focus scored four of the ten returned hits per query. Essential coverage stayed at 14, and the top-three count rose from six to eight.

## Recommended implementation and test order

### 1. Add ranked keyword and identifier retrieval for prose queries

**Original proposal 1.** Test the keyword and identifier branches from the existing private `search_v2` prototype individually, then together. Keep the current conjunctive lexical branch, vector query, fusion and passage selection fixed. The current conjunctive branch requires every query term to match, which is restrictive for full task instructions.

Start here because missing files were the largest measured gap. Check how many accepted paths enter either lexical candidate pool and how many of their qualifying spans reach the final ten, especially on Bookbuilder. Track any essentials displaced by the extra candidates.

**Focus comparison.** Check whether passages introduced by the new lexical branches reach Focus’s scored set and whether Focus promotes or demotes them.

### 2. Search the vector index with focused subqueries

**Original proposal 4.** Compare one embedding of the full task with the original query plus two or three focused subqueries, each covering a distinct evidence need. Derive subqueries from the request alone. Embed with the existing MiniLM model, merge by rank, deduplicate by passage identity, and keep the total vector candidate budget fixed.

Measure vector-only essential passage recall before fusion, then final hybrid coverage. This tests whether evidence for narrower requirements ranks better when they get their own query. Record the time spent on embedding and vector ranking so the cost of extra queries is visible.

**Focus comparison.** Track which subquery retrieved each new passage and whether Focus scores it. Keep the original task as the Focus input so changes in rank can be attributed to the new candidates.

### 3. Improve hybrid fusion and file diversity before the cutoff

**Original proposal 2.** Freeze the candidate lists while comparing current reciprocal-rank fusion with alternative lexical/vector weights and a rule that defers redundant passages from the same file. Test weights and diversity separately before combining them. Allow distinct useful passages from the same file to survive.

Run this after the candidate-generation tests so we can see which misses are still recoverable by ranking alone. Compare top-three and top-ten essential coverage, previously covered notes that drop out, and the ranks of passages recovered from below ten.

**Focus comparison.** Check whether the new fusion or diversity rule changes which passages reach Focus or the order they enter in. Record which of those changes Focus preserves, amplifies or reverses.

### 4. Let verified structural evidence compete for a result slot

**Original proposal 5.** Reuse native definition lookup for queries that name a symbol and test admitting verified definition spans to the ranked pool. On architecture queries, separately test a bounded one-hop traversal along a verified reference from a strong match to its definition or implementation. Keep scope checks, the pinned publication and source citations intact.

Start with coding and architecture scenarios and then check the full cohort for displacement. Measure whether structural retrieval surfaces relevant evidence whose wording differs from the query and whether it chooses the right definition when names are ambiguous.

**Focus comparison.** Record whether verified definition and reference spans reach the scored set and whether their ranks rise or fall, including cases with ambiguous names.

### 5. Choose more useful passages within the ranked files

**Original proposal 3.** Hold the ranked files fixed and compare the existing snippets with windows centered on the strongest matching terms or identifiers. Test how the fixed byte budget is shared across hits. Recompute the original source line ranges for the text actually returned.

Prioritize Munarium, where 23 misses already had a hit from the right path with no overlap. Look for more qualifying lines with the same result and byte limits. If the right lines fall outside the retrieved unit, record that separately for the next step.

**Focus comparison.** Keep candidate file identities fixed and record the snippet text and citation span supplied to Focus. Check whether changes in excerpt text affect eligibility, scores or rank.

### 6. Improve the vector representation of passages where needed

**Original proposal 6.** Audit whether the missing essential lines are present in a committed vector unit and what rank that unit receives. AIRK already uses written headings and definition boundaries. If the audit finds evidence split across units or short passages missing essential context, test bounded overlap or additional heading and symbol context on a small, isolated index.

Use the same MiniLM weights and the same source sample for both builds. Compare passage coverage, exact vector ranks, index size and build cost. If the isolated test demonstrates a gain that the query-time changes leave unresolved, review the proposed representation changes and measured tradeoffs with the user before starting a full rebuild.

**Focus comparison.** Within each isolated index, test the same queries with Focus off and on. Record whether changed vector units improve the candidates supplied to Focus and whether the reranker preserves or reduces the gain.

### 7. Rank a wider shortlist before selecting ten results

Collect a wider internal shortlist from the retained lexical, vector and
structural branches. Deduplicate identical or substantially overlapping
passages and reduce repeated files before scoring. Measure a deterministic
selection and a selection that uses the same deterministic shortlist with
Focus scoring up to four candidates around the cutoff. This gives Focus a
chance to move a candidate from outside the top ten into the response
within its existing scoring budget. Select the final ten after the second
stage, under the same 8,192-byte budget. Keep each candidate bound to the same
publication, source span and read scope throughout the request.

Test against Step 6 and the original with Focus off and on. Compare
shortlist recall, final top-three and top-ten coverage, newly admitted
essentials, displaced hits and latency. For each gain, record whether
deterministic selection or Focus moved a passage across the response
cutoff. Record how many candidates were scored and what happened when
Focus was unavailable. Keep a deterministic result within the original
request budget on that path.

### 8. Size automatic responses to the evidence

On top of Step 7, test an automatic result count for requests that omit an
explicit limit. Keep the response byte budget and the tool's validated
ceilings fixed. Use measurable signals such as distinct source paths,
passage overlap, agreement between retrieval branches and score separation
to test when a smaller or larger response helps. Keep an explicit caller
limit as a hard cap. Report the selected count and any truncation using
the existing Search response contract. The caller or agent decides which
returned evidence to use.

Compare adaptive sizing with Step 7's fixed ten, Focus off and on, and
retain the original comparison. Report coverage in the first three and
first ten as well as across the entire response, along with result count,
bytes, unique paths, displaced notes and latency. Include short precise
queries, broad tasks and repeated evidence. Verify that explicit limits
and response byte budgets hold in all cases. This isolates the effect
of result count from Step 7's reranking change.

## Comparison method

1. Pin both existing publications and freeze the original 20 queries and 113 note locations. Use the same ten-hit target and 8,192-byte limit through Step 7. Step 8 varies the automatic hit count and also reports coverage at the original ten-hit cutoff. Keep the note labels out of query construction and ranking.
2. Keep one original baseline pair. Add each proposal to the preceding implementation and collect one Focus-off and one Focus-on result. Retain that pair as the baseline for the next step. Compare each step with both the preceding pair and the original. Keep the Focus profile, device and scoring limits fixed. For the two isolated indexes in step 6, pin each publication and use identical source revisions.
3. Measure each proposal’s effect with Focus off, its effect with Focus on, the on/off difference within each version, and whether Focus changes the size or direction of the proposal’s benefit. For each run, report top-three and top-ten essential coverage, gains and losses by repository and scenario, elapsed median and p95, and response bytes. Record differences in publications or served modes. Alternate measurement order when assessing latency.
4. Trace each note from branch candidates through fused rank, the candidates presented to Focus, and the returned source lines. Use the existing overlap rule consistently. Retain Focus receipts with the active profile and runtime, original and final order, scored candidates and scores when available, and any reason Focus did not apply. Explain each material on/off difference using the trace, and label explanations that remain hypotheses. Keep each measured result and its receipt with the corresponding implementation milestone.
5. Advance a change when it improves final essential coverage or materially improves early rank with a measured latency cost and understood losses. If the first measured attempt yields no useful gain, try up to two revisions supported by the trace. Stop sooner if the evidence rules out the mechanism. Keep the preceding baseline when a step yields no useful gain and record why. Keep source scope and citation integrity as acceptance requirements.

## Milestone results

### Step 1 result — Ranked lexical candidates

Each variant used the same 20 requests, the same two publications, and the
same CUDA Focus profile. All 20 Focus calls applied in each enabled arm.
Scope, ten-hit limit and response byte limit matched the original baseline.

| Search policy | Essentials off / on | Top 3 off / on | Median CLI ms off / on | p95 CLI ms off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 | 1,718 / 2,477 |
| Ranked keywords, including named identifiers | 14 / 14 | 7 / 7 | 1,129 / 1,701 | 2,254 / 3,152 |
| Separate ranked identifier branch alone | 9 / 9 | 3 / 4 | 997 / 1,474 | 1,785 / 2,484 |
| Both ranked branches | 12 / 12 | 6 / 7 | 1,191 / 1,754 | 2,443 / 3,249 |

Carry the ranked keyword stream into step 2. It ranks named terms as part
of the same stream. The separate identifier stream and the two-stream
variant remain available for private evaluation. The selected policy
gained four essentials and lost four. Bookbuilder went from five to three
and Munarium from nine to eleven. This is a coverage tradeoff. With Focus
off, seven essentials reached the first three hits, one more than the
original. With Focus on, the same seven reached that window, one fewer
than the original Focus-on arm.

Focus saw the same ten candidates as the off arm and scored four per
query. For the selected policy it moved two essential notes higher and
one lower. The remaining eleven stayed at their rank. With both added
branches, five of the six lost essentials disappeared with their source
paths from the returned pool. Focus could only reorder the passages
supplied to it. Full pre-fusion ranks were not captured in this step.

The selected matched comparison SHA-256 is
`d6241b0f3a49f57e787dfbabcc70ba4f2281ee7c1800d6d0d0329085023f1b0f`.
Its comparison to the original and prior pair is
`32c533734ee1635b9a5f726df776c23e2e38fd1c77e60a202780d06eadc8863b`.
All three fixtures closed cleanly with zero new Index or Vectors jobs.
Latency uses off then on and includes elapsed CLI time.

### Step 2 result — Focused semantic queries

The five requests with a confident split each used the original query
and two focused queries. The original received 50 passage slots. Each
focused query received 25. The auxiliary branches surfaced four to 16
additional distinct paths per split request, measured against the
original query's 50-slot list in the same request.

| Reference or candidate | Essentials off / on | Top 3 off / on | Median CLI ms off / on | p95 CLI ms off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 | 1,718 / 2,477 |
| Step 1 | 14 / 14 | 7 / 7 | 1,129 / 1,701 | 2,254 / 3,152 |
| Step 2 | 15 / 15 | 6 / 6 | 1,197.5 / 1,733 | 2,311 / 3,179 |

Step 2 gained one Munarium essential and kept every Step 1 essential in
both settings. Relative to the original, it gained five and lost four.
Two essential ranks slipped in Munarium A2, one out of the top three.
The new note was in Munarium C2. Its accepted path appeared at rank 11
in the second auxiliary list and was absent from the original query's
allotted list. The matching passage reached final rank eight and was
outside Focus's four scored candidates. Focus promoted two other
essentials, demoted one and left twelve unchanged. Neither coverage
nor the count in the top three changed when Focus was enabled.

The median paired latency rose by 42 ms with Focus off and 26 ms with
Focus on versus Step 1. The measurements use off then on and include
elapsed CLI time. Carry Step 2 forward. The paired receipt SHA-256 is
`067221bd9efe69ed655bdf75d7fe8f6bc3380b6d22b48b5e5dab0f7bb9c22123`. The comparison to Step 1 and the original is
`c6669edc4fe4abc4474aa27480fa4847bebe94a82679d6f9b10cba0da3603016`. Both publications were reused and shutdown recorded zero
new indexing or vector jobs.

### Step 3 result — Hybrid fusion and file diversity

All three trials kept the Step 2 candidate sources and budgets. The
selected file-diversity run retained the same ten native semantic
branch traces as Step 2, including passage identities and ranks.

| Policy | Essentials off / on | Top 3 off / on | Median CLI ms off / on | p95 CLI ms off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 | 1,718 / 2,477 |
| Step 2 | 15 / 15 | 6 / 6 | 1,197.5 / 1,733 | 2,311 / 3,179 |
| Keyword weight 0.9 | 15 / 15 | 6 / 6 | 1,117.5 / 1,730 | 2,245 / 3,054 |
| Defer a file's hits after the first two | 16 / 16 | 6 / 6 | 1,139.5 / 1,727.5 | 2,316 / 3,261 |
| Both changes | 16 / 16 | 6 / 6 | 1,367.5 / 1,958 | 2,840 / 3,879 |

Carry forward file diversity with equal branch weights. Relative to
Step 2, it gained one essential in both settings and lost zero.
Relative to the original, it gained five and lost three in each.
In Munarium C1, deferral reduced the most-represented file from three
hits to two, opened space for a seventh distinct file, and recovered
one essential at rank ten. Three other Munarium notes rose within
the unscored tail. Focus ran on all 20 enabled requests and scored
four hits per request. It moved two essentials higher, one lower
and left thirteen at the same rank. The recovered passage stayed
tenth with Focus on. Weighting alone changed some ranks and kept
both coverage counts flat.

The selected pair SHA-256 is `745e4a7a37327bfcc1ce28be12cef31e34eebd313289ca5633331e6cf4e12a8c`.
Its incremental and cumulative report SHA-256 is `a201464d3d1a0a3bafbaec68f30f0c0660cdad566db587f8974b179cc2a0f2d8`.
The fixtures shut down idle on the same publications with zero new
indexing jobs. Timings represent this fixed-order Windows CUDA run.

### Step 4 result — Exact structural evidence

| Reference or candidate | Essentials off / on | Top 3 off / on | Median CLI ms off / on | p95 CLI ms off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 | 1,718 / 2,477 |
| Step 3 | 16 / 16 | 6 / 6 | 1,139.5 / 1,727.5 | 2,316 / 3,261 |
| Exact definitions | 16 / 16 | 6 / 6 | 1,165 / 1,717.5 | 2,260 / 3,097 |

Relative to Step 3, exact definitions added and lost zero essentials
and moved none of their ranks with Focus off or on. Relative to the
original, it gained five and lost three in both settings. Focus
applied on all 20 requests. Its within-pair movement was identical
to Step 3: two essentials rose, one fell and thirteen stayed put.

Three distinct requests admitted one verified definition excerpt
each. None overlapped a qualifying essential span. Of the 83
essentials annotated with a symbol, none had that symbol explicitly
named in its scenario query. Neither pinned publication exposed an
imported analysis resource, so the provider-resolved reference hop
was unavailable. This explains the lack of benefit from the tested
exact mechanism on this cohort. Keep it as a private evaluation
profile and use Step 3 as the reference for Step 5.

The measured pair SHA-256 is `e0a4ee29cebf18698961077204fce83c1ea2a2bba52119ed8087d9ccc2cdfc69`.
Its comparison to Step 3 and the original is `54821801e401a7db9d4bd5b59438fb471ec10604fef6eae25fc1ca342bdf4429`.
The reused fixture closed cleanly with zero new indexing jobs.

### Step 5 result — Contextual passages

| Reference or candidate | Essentials off / on | Top 3 off / on | Median CLI ms off / on | p95 CLI ms off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 | 1,718 / 2,477 |
| Step 3, retained after Step 4 | 16 / 16 | 6 / 6 | 1,139.5 / 1,727.5 | 2,316 / 3,261 |
| Step 5 | 18 / 18 | 9 / 10 | 1,155.5 / 1,662 | 2,197 / 3,491 |

The Focus-off results retained the same file order for all 20 queries.
The new selection changed 101 of 200 excerpts. Both Focus settings
gained two essential spans and lost none against the selected prior.
Against the original, both gained seven and lost three. The new
Munarium W1 and T2 evidence appeared at ranks two and three with
Focus off and rose to one and two with Focus on. Focus applied on
all 20 requests, promoted six essentials, demoted none and left
twelve unchanged. It scores the excerpt Search delivers, so these
changes come from the new windows entering its scored prefix. The
rank trace does not identify which terms drove the scores.

The paired median latency change versus Step 3 was +97.5 ms off
and +11.5 ms on. The p95 with Focus on was 230 ms higher.
Carry this policy into Step 6. The paired receipt SHA-256 is
`43a777e7a4b4278a60e82125cab843d0f944240e94ab1e850e8708d8b90ea991`. The comparison to the prior and original is
`74d6c3557cf3ead527f6f331b4d37271ebec49d941605ee6f3f9b75b495668f8`. Both publications reused the original job set
and the fixture shut down with zero new Index or Vectors jobs.

### Step 6 result — Committed vector coverage

The native audit mapped the committed units for 108 accepted paths to
source lines. All paths had vectors on the pinned Bookbuilder and
Munarium publications. Applying the existing Rule 2 threshold, 111 of
113 notes had a qualifying single unit. For the remaining two, only
the union of units on an accepted path reached the threshold.

| Repository | Misses in each Focus arm | Qualifying single unit | Coverage split across units |
| --- | ---: | ---: | ---: |
| Bookbuilder | 58 | 57 | 1 |
| Munarium | 37 | 36 | 1 |
| Total | 95 | 93 | 2 |

Five of the 20 requests had retained pre-fusion semantic traces. The
two Focus arms produced identical candidate lists for those requests.
Of the 22 notes both arms missed, six had a qualifying passage in a
recorded list and 16 had qualifying units outside all recorded lists.
The earliest qualifying positions for those six notes ranged from 14
to 46 within the relevant query's list. These are bounded occurrence
ranks. The other 15 requests have no retained semantic trace.

Retain Step 5 as the baseline for Step 7. Its coverage is 18/113 with
Focus off and 18/113 with Focus on, and its top-three coverage is 9/113
and 10/113 respectively. Step 6 changed neither ranking nor indexing.
There was no new Focus comparison or rebuild. Unit overlap could
directly address the two split notes on this cohort. The audit did
not measure whether adding embedding context would change ranking.
Prioritize retrieving and selecting the already indexed passages
before testing a new representation.

Private report SHA-256
`c5f294a58b5635c45c43ce21bdefe0d5ecaf349f42bcdf7cb675f9517658dded`.

### Step 7 result — Wider shortlist and cutoff selection

| Selection | Essentials off / on | Top three off / on | Median off / on | p95 off / on |
| --- | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 ms | 1,718 / 2,477 ms |
| Step 5, retained | 18 / 18 | 9 / 10 | 1,155.5 / 1,662 ms | 2,197 / 3,491 ms |
| Wider shortlist, ordinary Focus | 17 / 17 | 9 / 10 | 1,267 / 1,740.5 ms | 2,473 / 3,291 ms |
| Complementary choices, Focus at cutoff | 17 / 17 | 9 / 9 | 1,209.5 / 1,701 ms | 2,351 / 3,289 ms |

The 40-candidate shortlist held 28 of 113 essentials (27 in semantic
passages), compared with 18 in the first ten. The first ten citations
exactly matched Step 5 on all 20 requests. None of the ten additional
notes first appears at rank 11 or 12. The first 20 candidates held 24
essentials before packing into the response byte budget.

The first attempt held six results and favored deeper semantic files.
It displaced Munarium T1/N1 from rank eight and improved C1/N6 from
ten to nine. No new essential entered the results. Ordinary CUDA Focus
moved six essentials higher without changing membership.

The revised attempt held eight and chose four candidates using query
terms underrepresented among the held passages. It restored T1/N1 and
displaced C1/N6. Cutoff CUDA Focus scored all four candidates on each
of 20 requests and replaced 18 hits in the final two positions across
the cohort. None of those swaps changed an essential's coverage or
rank. Moving Focus work from the first four to the cutoff also lost
the six early promotions that ordinary Focus produced on Step 5.

Keep Step 5 as the production policy and Step 8's comparison baseline.
Both tested ten-slot policies lost a note, and the fixed cutoff had no
new essential to admit at ranks 11 or 12. Use the deeper candidates to
test adaptive result counts as a separate change. Both trials reused
the original publications with zero indexing jobs. The true cutoff
receipt stayed private because it includes candidates omitted from
the public response.

### Step 8 result — Adaptive result count

| Policy | Full coverage off / on | Top ten off / on | Top three off / on | Median off / on | p95 off / on |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original | 14 / 14 | 14 / 14 | 6 / 8 | 1,066.5 / 1,556 ms | 1,718 / 2,477 ms |
| Step 5, retained through Step 7 | 18 / 18 | 18 / 18 | 9 / 10 | 1,155.5 / 1,662 ms | 2,197 / 3,491 ms |
| Adaptive, shorter early excerpts | 20 / 20 | 16 / 16 | 9 / 10 | 1,151 / 1,757 ms | 2,293 / 3,083 ms |
| Adaptive, preserve early excerpts | 20 / 20 | 18 / 18 | 9 / 10 | 1,094 / 1,654 ms | 2,230 / 3,117 ms |

All 20 frozen wire requests omitted the limit. The first adaptive trial
gained four notes and lost two because shorter excerpts removed
qualifying lines at ranks five and seven. The selected revision packs
the original first ten, then uses the remaining bytes for extra hits.
For every request in both arms, the first ten hit identities, text and
order exactly match Step 5. Two new Munarium essentials at ranks 17 and
19 raise full coverage by two over Step 5 and six over the original.
Neither profile changes the published indexes.

The selected run returned 16 to 20 hits per request, median 19, and
12 to 19 distinct paths, median 15.5. Returned text was 7,969 to 8,192
bytes, median 8,192. The original and Step 5 receipts did not retain
individual hit text byte counts. The byte budget stayed at 8,192.
Ordinary CUDA Focus applied on all 20 requests and moved six essentials
up while leaving membership unchanged. It did not score the extra
tail. Observed median request time was 61.5 ms lower with Focus off
and 8 ms lower with it on than Step 5. Run order and this small cohort
limit the latency comparison.

The explicit 1, 10 and 100 controls each returned their requested
count on the same native fixture. Scoped tests exercise the byte ceiling,
duplicate paths and clear score separation for precise queries. Broad
requests supplied the live quality cohort. Select the prefix-preserving
policy for omitted hybrid snippet limits. Keep explicit limits and
fixed combined member allocations. The agent can choose which returned
passages to use.

## Evidence and implementation references

- [Matched Focus comparison and first-Search miss attribution](../simplify/one-process-ledger.md#25-november-2026--matched-ordinary-search-with-and-without-cuda-focus)
- [Change set 1 evaluation report and retained Search session coverage](answer-evaluation/06-loop-change-set-1.md)
- [Plain-store retrieval and private Search v2 contract](../../crates/airk-kernel/src/current/search/CONTRACT.md)
- [Current Search, fusion, snippets and native definition lookup](../../crates/airk-kernel/src/current/search.rs)
- [Vector passage construction](../../crates/airk-retrieval/src/vector_units.rs)

## Search quality from an AI agent’s perspective

Search is a useful starting point for finding where to work. On this
cohort, a single broad request rarely gives an agent enough evidence to
make a sound change, write grounded requirements, or finish a review.
The next quality question is how quickly an agent can assemble the
evidence required for its task and recognize what is still missing.

### What we achieved and where it falls short

On the 20 frozen development requests, full first-response coverage
rose from 14 to 20 of 113 essential notes (12% to 18%). The retained
changes add ranked keyword and identifier matching, focused semantic
subqueries, file diversity, better source windows, and an adaptive
count for omitted limits. The selected adaptive policy preserved the
first ten hit identities and order on every request in both Focus
arms. It returned 16–20 hits under the same 8,192-byte text budget.
Ordinary CUDA Focus moved six essentials higher and lifted top-three
coverage from 9 to 10, with the same 20 essentials across the full
response. The cutoff-only Focus trial scored real candidates and
changed 18 slots without improving essential coverage or rank.

The pooled improvement was uneven. Coverage counts below are identical
with Focus off and on. The task slices are small, and review was not
measured as a separate task class.

| Cohort slice | Original first response | Selected first response |
| --- | ---: | ---: |
| Bookbuilder | 5 / 61 | 3 / 61 |
| Munarium | 9 / 52 | 17 / 52 |
| Coding | 4 / 32 | 6 / 32 |
| Writing | 2 / 22 | 6 / 22 |
| Testing | 5 / 23 | 5 / 23 |
| Architecture | 3 / 36 | 3 / 36 |

The selected first response covered no essential note on 10 of the
20 requests and covered the full essential set on none. These counts
measure whether a response contained the labeled source lines. Task
success requires a separate assessment of what the agent did with them. The earlier retained sessions found 41 additional notes
after their first call. Further searches first found 19 and targeted
file reads first found 22. Those sessions provide evidence for
follow-up retrieval. They are separate from the Step 8 paired replay.
[First-Search miss attribution and follow-up evidence](../simplify/one-process-ledger.md#25-november-2026--first-search-miss-attribution)

The main gaps are finding the right artifact, bringing the right
passage into the candidate list, and returning enough of that passage
to establish the relevant fact. Bookbuilder accounted for 48 of the
original 66 missed source paths. Munarium accounted for 23 of the 30
original right-file, wrong-span misses. At Step 6, 93 of 95 notes missed
in each arm already had an adequate committed vector unit. Of 22 misses
in the five traced requests, just six qualifying units reached a capped
semantic candidate list. Step 7's 40-hit shortlist held 28 essentials,
of which 18 appeared in its first ten. These observations point to
candidate admission and passage selection as places to work first.
The audit establishes passage presence. It does not establish that the
current embeddings represent each passage well enough for the query.
[Step 6 audit](#step-6-result--committed-vector-coverage) · [Step 7 shortlist results](#step-7-result--wider-shortlist-and-cutoff-selection)

### What the Search results say about the index

Three comparisons answer different questions.

| Comparison | Observation | Interpretation |
| --- | --- | --- |
| Original Search to Step 8 on identical publications | First-reply essentials changed from 5/61 to 3/61 on Bookbuilder and from 9/52 to 17/52 on Munarium, with the same full-response counts in both Focus arms. Bookbuilder's decline first appeared in Step 1. | Retrieval policy can lose useful evidence on a fixed index. The pooled gain hides that regression. It cannot diagnose an earlier producer change. |
| Historical judged CS1 to the original Search replay | The CS1 agents found the same 14 essentials on their first Search calls, then 41 more through later searches and file reads. The Windows CS1 run already used the plain store and CUDA MiniLM. Its publication tokens differed from the later replay. | The 55/113 session total and the Step 8 first-reply 20/113 do not measure a fall in Search. Replay the historical follow-up calls, then run fresh blind sessions to measure present performance and the agent's ability to discover those calls. |
| Independent committed-unit audit on the later publications | All 108 accepted paths had committed vector occurrences. 111/113 notes had an individual unit that met the line-overlap rule. In five traced requests, only six of 22 misses reached the capped semantic lists. | Passage existence makes targeted reads promising. It leaves open lexical posting or scope errors, poor indexed inputs, vector-to-passage binding or scoring errors, and a representation that ranks poorly for the task. |

Two examples help separate mechanisms. In the original-to-Step-5 comparison,
Bookbuilder W1/N3 fell from rank 7 to absent despite four qualifying committed
units. In Munarium C2/N1, a focused subquery found a qualifying passage at
position 11 in its own bounded list, and it reached rank 8 in the final reply.
Both comparisons used the same publications. The first case needs earlier
candidate-stage evidence to locate the loss. The second shows a useful passage
that a different query could already recover from the index.
[Step 1](#step-1-result--ranked-lexical-candidates) · [Step 2](#step-2-result--focused-semantic-queries) · [Committed-unit audit and receipt](../simplify/one-process-ledger.md#25-november-2026--search-step-6-committed-vector-coverage) · [Historical first and later Search calls](../simplify/one-process-ledger.md#25-november-2026--first-search-miss-attribution)

**Did the refactor change what was indexed?** The historical CS1 and later
Search runs already used the refactored store. A direct comparison of their
settled databases found matching source and extraction digests, chunk citations,
lookup selectors, vector-unit members and physical embedding-row mappings on
both repositories. Each complete vector matrix was byte-identical across the
two runs. This rules out a change in those stored values as an explanation
for the difference between the two reports. The surviving older-format
Bookbuilder depth receipt uses different queries, limits and embedding recipe.
It leaves the actual before/after quality effect unresolved. Next compare
whole agent sessions to the earlier 55 and trace missing follow-ups separately
from any independently reproduced ingest or Search defect.
[Comparison and reproducibility record](../simplify/one-process-ledger.md#25-november-2026--answer-planning-and-index-quality-review) · [Matched-session design](completion-plan.md#stage-a--run-the-windows-cuda-baseline-and-three-change-sets)

**Test for improvements even if the existing indexes are faithful.** The
first completion milestone compares small control and candidate indexes on
identical source. Agents use ordinary lexical, semantic and structural
readers individually and in sequences assembled from the evidence returned.
This tests whether the default indexes give an agent better starting points
and more complete answers without any new Answer logic. Selection or a
documented negative result comes before Answer model work.
[First-milestone experiments and decision](completion-plan.md#how-to-test-indexing-for-agent-tasks)

**Evaluate historical regression separately.** The original indexing
performance baseline stopped before completing its vector and Search
measurements. Keep open three possibilities: the older representation
was already weak, the indexing change introduced a defect or quality
regression, or query-time selection lost otherwise accessible evidence.
Compare the current publication with source truth first. Isolate indexer,
reader and ranking changes using compatible retained evidence or a matched
small fixture. Report the earliest failure and its owner, including cases
whose historical cause cannot be recovered. The completion plan specifies
the checks, repair decision and rebuild discussion.
[Indexing plan and benchmarks](../indexing/performance-plan.md) · [Index and Search quality before Answer](completion-plan.md#index-and-search-quality-before-answer)

### What an agent needs to retrieve

| Task | Context that supports the work | Gap a ranked snippet list can leave |
| --- | --- | --- |
| Code generation and modification | Owning contract, definition, callers, comparable implementations, configuration gates, error handling and the tests that pin behavior | A plausible example may omit the actual entry point, a platform variant, or a caller whose assumptions constrain the change. |
| Requirements and documentation | Current approved decisions, constraints, exceptions, examples, observed implementation behavior and dated history | A similar paragraph may be obsolete, describe a different version, or omit an exception elsewhere in the same section. |
| Code review | The actual diff and surrounding functions, the behavior contract, affected callers and sibling paths, and tests or execution evidence for each finding | Top hits cannot establish the set of affected code paths or the coverage of a proposed finding. Search can lead to evidence of a defect. Confirm it against the changed code and relevant callers. |
| Testing | Existing test entry points and assertions, fixture setup, build flags and platforms, boundary conditions, and an independent expected outcome | A test name only locates a test. Verify what it asserts, whether it exercises the real entry point, and whether the selected test ran. |

An agent can use Search for unfamiliar terminology and related features,
then switch to exact queries as soon as it learns a path, symbol, command,
error string or configuration key. It can open the complete relevant
function, heading, test, or neighboring source range, read the owning
contract, and trace siblings and callers. For a proposed requirement,
it should compare the decision record with implementation and tests,
and surface disagreement with citations. For a review or regression,
it should pin the diff and validate consequences against the working
tree. Indexed saved revisions support reproducible discovery. Verify
the live bytes before editing or making claims about unindexed changes.

### How I would use direct tools as an agent

1. **Discover the vocabulary and paths.** I would start with the task's
   distinct concepts, a path inventory such as `rg --files`, and one or
   two targeted hybrid searches. For a checkout I can access, I would
   read the owning README or contract and check the relevant Git diff.
2. **Collect exact evidence.** Once I know the names, I would use a
   scoped literal search such as `rg -n -F -e SearchProfile -e automatic_limit -- crates docs`.
   For a known file I might use PowerShell
   `Select-String -LiteralPath $file -Pattern $needle -SimpleMatch -Context 3,5`
   and open a bounded range with `Get-Content -LiteralPath $file`.
   I would use globs, regex and Git path or revision filters when the
   question is about a family of names or a specific change. A direct
   scan can enumerate literal matches within its configured scope.
   Ignore patterns, binary and format support, encoding, and selected
   roots matter when interpreting an empty result.
   [ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) · [PowerShell Select-String](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/select-string) · [PowerShell Get-Content](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/get-content)
3. **Follow relationships and verify.** Text matches are leads for
   overloaded names, aliases, generated code, and indirect dispatch.
   I would prefer resolved definition and reference tools where their
   coverage is known, then check relevant source, configuration, tests
   and actual run receipts. For PDFs, slides and spreadsheets, I would
   use indexed extraction where available and open the relevant page,
   sheet or section. The extraction coverage should disclose missing
   text, including scanned pages without a text layer.
   [Document format support](../document-indexing.md)

The equivalent indexed workflow is discovery, exact or structural
lookup, and a targeted source read. Preserve scope, publication and
source locations at each step. Return a continuation or a clear limit
when a result is bounded. For an agent deciding whether a reference or
requirement exists anywhere in scope, that distinction is as important
as ranking the first result.

### Alternatives to test

1. **Use the existing indexes in an iterative retrieval workflow.**
   Let the agent split the task into evidence needs, search each need
   separately, and follow the new paths and names with targeted reads.
   Compare this with refining a single broad request and with an
   exact-first route for known identifiers. Offer bounded operations
   such as locating a definition, reading its section and finding its
   uses. The agent can choose and stop when it has enough evidence or
   can name its remaining gaps. RepoCoder is a precedent for iterative
   retrieval in repository-level completion. Its results are on a
   different task and dataset.
   [RepoCoder paper](https://aclanthology.org/2023.emnlp-main.151/)
2. **Index for exact text and fielded lookup.** Compare a positional
   literal or trigram lane for substrings, punctuation-bearing
   identifiers and regex candidate filtering. Verify the final match
   against retained source bytes. Add separately weighted path, file
   name, heading, declared symbol and body fields in a small lexical
   experiment. This targets questions a scoped `rg` scan answers well,
   including identifiers with punctuation and exact diagnostic strings.
   Compare indexed hits and completeness with a direct scan of the
   same publication and measure storage, update cost and fallback for
   patterns that cannot use the index. Zoekt offers an implementation
   reference for positional trigram search.
   [Zoekt design](https://github.com/sourcegraph/zoekt/blob/main/doc/design.md)
3. **Index verified relationships.** Pilot a language-aware index of
   definitions, references, implementations and imports for one
   important language. Test one-hop expansion from a verified hit to
   its contract, implementation or caller. For documentation, pilot
   explicit heading IDs, internal links and version metadata. Keep
   relation provenance and coverage visible, especially for exclusions,
   ambiguous symbols, generated code and unresolved dynamic calls.
   Step 4 lacked imported analysis on the measured requests, so it
   did not evaluate this populated-index alternative.
   [SCIP schema reference](https://github.com/scip-code/scip/blob/main/docs/scip.md)
4. **Separate discovery granularity from reading granularity.** Pilot
   file or section discovery using names, titles and heading ancestry.
   Score child passages within those candidates while keeping a
   corpus-wide passage lane for isolated facts. Once an agent selects a
   hit, fetch a bounded parent section, complete definition, or
   neighboring lines using that same source revision. This lets short
   units compete for rank while preserving a useful reading window.
   Test query-time parent expansion with the current publications
   first. An isolated follow-up can compare the same MiniLM model and
   source revisions on body-only versus heading or symbol-context
   embeddings. Any added retrieval context stays separate from cited
   source. Contextual retrieval is an external precedent for enriching
   both sparse and dense representations.
   [Anthropic contextual retrieval](https://www.anthropic.com/research/contextual-retrieval)
5. **Compare denser retrieval alternatives only where ranked evidence
   stays weak.** On a small matched sample, test a code-aware embedding
   model or token-level late interaction against the current MiniLM
   exact scan and fielded lexical retrieval. Measure gains on exact
   names and paraphrases separately, with disk footprint, incremental
   update cost and query latency. Approximate nearest-neighbor search
   belongs in a separate performance comparison against exact search
   because faster candidate access alone does not improve the scoring
   signal. Keep the existing indexes for the workflow experiments.
   Discuss the proposed representation, measured gain and rebuild
   costs with the user before a full rebuild.
   [ColBERTv2 paper](https://aclanthology.org/2022.naacl-main.272/)

### What would establish that Search is good enough

Compare four workflows on the same pinned revisions and development tasks:
current single-call Search, scoped direct tools, iterative AIRK Search and
reads, and a mixed direct and AIRK workflow. Give iterative arms comparable
time, calls and total evidence budgets, and use the same agent and grading
rules across them. Run matched Focus off/on on AIRK arms. Record which
evidence each tool first finds, elapsed time and bytes to the first usable
evidence and to adequate context, true misses, stale or ambiguous citations,
and how many calls were needed. Report results by repository and task type.
Include exact names, a broad task spanning multiple files, a changed file,
and an absence or exhaustiveness question within a defined scope.

Judge the downstream work alongside retrieval. Use build and behavior
checks for code, coverage and fault-detection checks for tests, verified
findings for review, and traceability to current decisions for writing.
Define the required facts per task before each run. Score whether the
agent found that set and whether it correctly identified gaps. Treat
similar-sounding passages, duplicate evidence, and dated decisions as
quality costs as well as byte costs. Use the results to choose which
query-time improvement or isolated indexing pilot earns the next test.

### How AIRK’s existing tools can improve Search

AIRK already has indexed counterparts for much of the direct-tool workflow.
Use the existing Kernel and Store readers for these operations. The MCP
handlers supply typed inputs and response shaping. Combining them at the
agent or explicitly prepared plan layer gives us a testable alternative to
adding more hits to every Search response.
[Native read entry points](../../crates/airk-mcp/src/lib.rs) · [Read interface parity](../read-interface-parity.md)

| Existing tool | How it works today | How to combine it with Search |
| --- | --- | --- |
| `airk_list_files` and `airk_file_info` | File listing walks the indexed inventory by literal prefix and pages in path order. File info reports an exact artifact's hash, indexed language or profile, extraction method, line basis and available structure. Saved-source verification is a separate observation. | Enumerate a candidate subtree, then check the document or source profile before narrowing Search by path, language, or evidence kind. This is the indexed counterpart to `rg --files` followed by inspecting selected files. File listing handles known prefixes. Ranked filename and heading discovery remains a separate improvement. |
| `airk_symbols`, `airk_code_facts`, and `airk_imports` | These use the common fact reader. It selects candidate paths through name, kind, language or inventory lookups and checks retained facts. Symbols defaults to exact written definitions. Code facts also expose signatures, scopes, written calls and structural cues when retained. Imports returns written declarations. Compact navigation can omit symbol IDs. | Route a known identifier to Symbols first. Narrow ambiguous candidates by file or language and request rich facts only for the selected candidates. Use the declaration and nearby names as anchors for Search or a targeted read. Imports can suggest related modules to inspect. Written imports and calls require confirmation before making a dependency claim. |
| `airk_references` and `airk_relationships` | References starts with scoped lexical candidates and verifies exact-case, whole-identifier matches on retained lines or fragments. It can page and report bounded matching-line counts. Comments, strings and declarations can match. Relationships opens bounded incoming or outgoing traversals from indexed edge selectors, with native syntax candidates and source-bound producer assertions where available. | Use References for grep-style impact discovery and Relationships to prioritize candidate callers or implementations. Read the cited lines before acting. Finish relevant pages and check candidate coverage. Token boundaries and extraction can still omit occurrences, and missing edges leave impact unresolved. Use a direct scan or a language-aware tool when the task requires stronger completeness. |
| `airk_document_facts` and `airk_outline` | Document facts query retained extraction records for headings, requirements, decisions, configuration and other supported categories. Prose extraction captures written clues such as modality, quantities and heading context. Outline pages the fact records for one exact indexed path in source order. | Run a targeted requirements or decision lookup alongside prose Search. Use Outline on a selected file to find the governing heading and neighboring exceptions, then open that section. Facts are candidates grounded in written text. Establish approval, version and authority from the cited source and relevant history. |
| `airk_context` and `airk_read_file` | Both use the same bounded line reader. Context expands around an anchor. Read File uses an inclusive range. They accept revision and hash-related controls through conditional reads. Structured document ranges must stay within a supported original location. | Keep Search excerpts small during discovery. Expand just the selected hits into a complete function, test assertion, or document section. Carry the publication, path, original coordinates and content hash into the follow-up. Use reported or required saved-source verification for evidence that will support an edit, and check the live working tree before changing it. |

[Discovery and cursors](../discovery.md) · [Code facts and relationships](../code-facts-and-relationships.md) · [References implementation](../../crates/airk-kernel/src/references.rs) · [Conditional read implementation](../../crates/airk-kernel/src/current/conditional.rs) · [Saved-source verification](../source-verification.md)

A scoped probe on the installed AIRK workspace index illustrates the
handoff. Symbols returned both the `SearchQuery` struct and its
implementation block as written definitions. The response reported
that the indexed file differed from its saved live bytes. A three-line
References page under the Kernel's current module reported incomplete
coverage and returned a cursor. Live `rg` confirmed the declarations
and showed uses in the current runtime. This was a tool-behavior check
on the installed workspace index, separate from the Bookbuilder and
Munarium quality runs. It illustrates why an agent should distinguish
multiple records from multiple files, a partial page from a complete
inventory, and a saved hit from a verified current file.

#### Where to compose the tools

- **Start with a visible workflow using current tools.** For code,
  extract exact names from the task or the first Search response.
  Use Symbols or Code Facts to disambiguate, read the declaration
  and its contract, then trace References or Relationships to the
  usage and tests. For requirements and documentation, combine
  Document Facts by kind with prose Search, then use Outline and
  bounded reads to collect the rule, its exceptions and examples.
  For review, start from the actual diff and use indexed lookups to
  find callers, analogous paths and tests outside the diff.
- **Reuse Search’s existing navigation hints.** Search already looks up
  candidate identifiers with native definition postings on the same
  scoped reader and returns a bounded `definitions` supplement. The
  hints carry locations and completeness. Imported analysis is outside
  this helper. Step 4 measured admitting a few exact excerpts as ranked
  hits. It did not measure whether an agent follows these hints to useful
  evidence. Test that follow-up first. Use the existing read guidance
  patterns to point to `symbols`, a rich lookup, a continuation, or an
  exact range when the evidence warrants it. Suggestions should leave
  decisions and tool calls visible to the agent.
  [Search definition hints](../../crates/airk-kernel/src/current/search.rs) · [Search contract](../../crates/airk-kernel/src/current/search/CONTRACT.md)
- **Compare agent-directed calls with an explicit prepared plan.** Parse
  already runs bounded read-only plans, with definition, documents,
  impact and tests recipes. Its bindings include a unique definition path,
  cited hit locations and selected symbol IDs. It refuses ambiguous
  unique bindings and requires rich results where it needs native IDs.
  Use these existing controls to test a repeatable composition without
  introducing new execution machinery. The Answer retrieval code is
  another implementation reference for issuing follow-up excerpts,
  outlines and governing-document checks from inspected hits. The
  Search comparison can reuse those patterns while keeping Arbiter off.
  [Prepared plan bindings](../read-interface-parity.md#regression-evidence) · [Answer retrieval implementation](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs)

If the composed workflow consistently recovers evidence the current
Search misses, test a bounded version inside Search. Try separate
candidate lanes for scoped exact-symbol hits on identifier requests
and for document-fact anchors on requirements or design requests.
Use the same current reader for lookup and excerpt materialization.
Compare those excerpts as explicit follow-up pointers and as candidates
in the existing fusion pass. Count the extra work and bytes, keep the
existing response and Focus budgets, and measure displaced essentials
alongside gains. Step 4 makes an unconditional definition lane a weak
starting point. Relationship expansion needs a selected anchor and
known edge coverage. Start by testing it as a follow-up.

#### What this changes about the indexing proposals

Use References as the first indexed exact-lookup comparator and a scoped
direct scan as the coverage control. If punctuation, substrings, or
verified token-boundary misses still matter, the positional lexical
proposal has a specific gap to fill. Use Document Facts and Outline to
test heading and requirement discovery before adding new fielded
postings. Use Context and Read File to test larger reading windows
before changing vector unit boundaries. Pilot additional relationship
indexing only where current syntax candidates lack the resolution the
task needs. Inspect producer and publication coverage first. AIRK's
documentation describes imported analysis support and marks previous
CLI import commands as historical. Treat comparable provider data and
an available import path as prerequisites for that pilot.
[Native facts and optional provider history](../code-facts-and-relationships.md)

For the next comparison, give single-call Search, iterative Search and
reads, and Search plus the existing specialized tools the same evidence
and time budget. Retain the direct-tools arm. Measure which tool first
provides each required fact, whether a follow-up recovers the correct
span, and whether the agent notices incomplete or stale evidence.
Report incremental gains by repository and task type with Focus off
and on. Keep the current publications for these experiments. The result
should identify whether an unmet need calls for a better route through
existing tools, a new indexed signal, or a wider reading window.

## Convergence with the Answer completion plan

Answer should take on the multi-turn context-building work an agent
would otherwise do with repeated Search, grep, navigation and file
reads. One Answer request should start a retrieval plan, execute it
in bounded internal rounds, and revise the next reads as evidence
arrives. Return the cited source ranges, the relationships or
constraints they establish, and the gaps the agent still needs to
resolve. The agent keeps responsibility for the actual implementation,
writing, review or test. The existing lexical, vector, fact,
relationship and retained-source readers supply the evidence.
[Completion plan, Windows Stage A](completion-plan.md#3-stage-l7-with-a-windows-first-review) · [Answer loop stages](final-plan-fp16.md#121-the-stages)

### What Answer already does

The Section 12 loop already splits compound tasks, plans evidence needs,
gathers and checks evidence per need, follows up, and composes a
deduplicated reply. The implementation starts each need with Search.
For a named definition it also calls Symbols. For callers or tests it
can call References and expand selected occurrences. It can follow hits
with Context, Read File, Outline or Imports, and checks File Info before
reading a candidate governing document. This is existing behavior to
measure and improve. The current typed loop-read roster has no dedicated
Code Facts, Document Facts, or Relationships entries. Using those
operations in Answer would require an explicit, reviewed change to the
relevant dispatch and evidence contracts. It does not require a second
implementation of the underlying reads.
[Answer retrieval recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Governing-document checks](../../crates/airk-kernel/src/arbiter/answer.rs) · [Canonical loop-read roster](../../crates/airk-core/src/arbiter/answer_loop.rs)

The current recipes are Search-led, and the judged results show that
their follow-ups have yet to take enough work off the agent. In the
earlier Windows Change Set 1 comparison, Answer's first-call yield
was higher than Search's. By the end of the sessions, ordinary Search
had surfaced 55 of 113 essentials, compared with 37–44 for the three
Answer arms. Answer averaged 9.70–10.15 agent turns versus 8.65 for
Search, and every arm had zero correct finishes. Those are historical
judged results. The later Step 8 Search replay measured the first
response only. Its 20 of 113 essentials and the 41 notes recovered
after the opening call in earlier Search sessions illustrate how much
work can sit beyond the first response. Answer's own Search recipes
use explicit limits, so its internal calls do not inherit Step 8's
omitted-limit behavior.
[Judged Change Set 1 results](answer-evaluation/06-loop-change-set-1.md#first-sample-results) · [Search results and follow-up evidence](#search-quality-from-an-ai-agents-perspective) · [Answer Search argument construction](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs)

### How an Answer retrieval plan should run

1. **Plan around questions the evidence must settle.** For a code
   change, identify the owning implementation, governing constraint,
   affected callers or configuration, and test path. For writing,
   identify the current decision, applicability and exceptions. Give
   each need a starting read and an observable condition for adequate
   evidence. Start exact-name and known-path needs at the corresponding
   indexed lookup. Use Search to discover vocabulary or locate
   conceptual evidence where the target is uncertain.
2. **Run initial reads and use the results to choose the next round.**
   Independent reads can run within the existing scope and budget.
   Expand the best anchors into bounded source ranges, and use the
   discovered names, paths or headings to launch targeted follow-ups.
   A symbol match establishes a place to read. A textual reference
   gives a candidate use. A heading locates a rule whose scope and
   exceptions must be read. Check what each result actually establishes
   before deciding which gap to investigate next.
3. **Finish with evidence the agent can use directly.** Return the
   decisive excerpts grouped by need, with their exact citations,
   scope, and what remains unresolved. For a code change, that might
   be the implementation and constraint, one representative caller,
   and the relevant test setup and assertion. Where the budget expires
   or a read leaves uncertainty, carry the evidence already gathered
   and the next specific read that would close the gap. Keep internal
   retrieval rounds distinct from any continuation the agent must call.

For example, for a request to change how an omitted option behaves,
Answer could look up the named argument type, read its parser and
owning contract, and follow the type to the CLI, MCP and Workbench
adapters. A later round can target the regression that exercises the
missing argument and the explicit-value path. Return the relevant
branches and assertions with citations, and flag any caller or test
scope still unchecked. This is the sequence of evidence an agent
would otherwise have to discover and assemble across several calls.

### Recommended retrieval responsibilities

| Evidence need | Start with | Close the gap with |
| --- | --- | --- |
| Locate the owning code or a compatible pattern | Symbols or Code Facts when an identifier is known. Search for vocabulary, behavior and examples. | Read the exact declaration and relevant body, then check the owning contract, usage and configuration. |
| Establish requirements and documentation context | Document Facts by kind and prose Search within the relevant document scope. | Outline and read the governing section, including its version, exceptions and examples. Resolve conflicting text against the decision record and implementation. |
| Trace impact and review changes | The actual diff, indexed References for occurrences, and Relationships for supported edge candidates. | Open the relevant callers and sibling implementations. Confirm ambiguous targets and report incomplete relationship coverage before asserting impact. |
| Find and evaluate tests | References to the subject, scoped Search for tests and fixtures, and retained test or framework cues from Code Facts where supported. | Read the test body, assertions, setup and entry point. A discovered test is a candidate until the agent verifies what it exercises and observes any claimed run. |

Keep this selection conditional on the task, what earlier reads found, and
the remaining call, byte and deadline allowance. Answer should share its
read cache and preserve source identity, publication and location across
tools. Deduplicate excerpts without dropping distinct locations needed
for impact work. Preserve contradictory evidence, truncation, stale
sources, unresolved relationships and unrun follow-ups in the reply.
Keep the deterministic path useful with both models disabled. Focus
ranks eligible passages for a need. Arbiter helps choose which gaps to
check or follow up. Evaluate each against those responsibilities.
[Answer stages and model roles](final-plan-fp16.md#122-how-the-models-take-part) · [Answer contract](../../crates/airk-core/src/arbiter/CONTRACT.md)

### Path back into the completion plan

1. **Qualify the index and establish a current baseline.** Complete the
   [indexing experiments](completion-plan.md#how-to-test-indexing-for-agent-tasks),
   close material defects and freeze the selected publications. Confirm
   the settled Bookbuilder and Munarium publications, exact service and
   CLI pair, model switches and request policy. Collect a fresh current-stack
   session baseline for the five primary arms. Retain older reports under
   their original identities. The next three change sets refer to this new
   retrieval series. Keep the Search first-call replay as a diagnostic
   alongside the judged Search and Parse arms.
   [Starting point and input freeze](completion-plan.md#1-reconcile-the-starting-point) · [Windows L7 sequence](completion-plan.md#3-stage-l7-with-a-windows-first-review)
2. **Use Change Set 1 to prove better gathering.** Replay the current
   deterministic Answer path on the development tasks. For every
   required fact, record the first tool that could supply it and the
   first point where the loop lost it: no candidate, rank or
   retention, omitted follow-up, premature coverage, clipped
   excerpt, or deadline. Compare the current recipes with a targeted
   new combination one missing evidence type at a time. Start with
   Document Facts plus a section read for requirements misses, or
   Code Facts or Relationships plus a source read for a demonstrated
   code or impact miss. Pick from the traces. Use explicit Parse
   recipes or direct calls to establish the incremental value before
   adding any operation to Answer's typed roster. Retain the current
   publications and test the no-model path first. If an audit shows
   the underlying signal is missing, discuss a small indexing pilot
   and the measured tradeoffs before considering a full rebuild.
3. **Use Change Sets 2 and 3 to isolate model value and fix remaining
   losses.** Hold the useful retrieval policy fixed while comparing
   the model switches on matched inputs. Separate whether a tool
   found the needed evidence, Focus ranked it usefully, and Arbiter
   chose the right follow-up or stopping point. Test prompt, policy,
   calibration or weight changes where those traces identify repeated
   failures. In the next bundle address remaining measured gaps and
   check whether extra reads, bytes or latency save agent turns.
   Keep the development and sealed sets separate.
4. **Judge the result under the existing Windows gate.** Measure correct
   finishes and turns against the stronger of Parse and Search.
   Report per-task and per-repository evidence coverage, time to a
   usable reply, rereads, bytes and deadline outcomes. Apply the
   existing substantial-win rule and retain one signed-off commit per
   judged series. Preserve the Windows manual review and sealed-set
   decision points. Alongside the quality comparison, retain the
   completion plan's Section 4 checks for useful partial replies,
   non-repeating continuations and client parity. Treat an omitted
   requirement, uncertain impact or unverified test as a visible gap.
   [Windows review and acceptance](completion-plan.md#3-stage-l7-with-a-windows-first-review) · [Job delivery and clients](completion-plan.md#4-close-the-parallel-section-12-work)

After the upstream quality check and current baseline, compare the Search-led Answer loop
with a need-driven retrieval plan on the same development inputs and
existing publications. Use both models off first so the comparison
isolates changes to gathering. For each arm, record what evidence is
usable in the first Answer delivery and how many external Search,
navigation, and file or context reads the agent still makes to finish.
Count any agent-requested continuation as an external turn. Record
Answer's internal reads, elapsed time and bytes separately so we can
see the cost of shifting that work inside the tool. Compare correct
completion and total agent turns with the stronger of Parse and
Search under the existing gate. Track the direct-tools workflow as a
diagnostic for the work the agent still has to do. Treat added hits
as a retrieval signal. Success means the agent finishes correctly
with fewer context-building turns.

### Is the evidence available to Answer enough?

**For many of the measured misses, the underlying evidence is already
available. The current Answer loop has yet to show that it can gather
enough of it to consistently spare the agent a second search or source
read.** At Step 6 every audited path had committed vectors, and 93 of
the 95 misses in each arm already had a qualifying individual unit.
Retained reads can also reach outside a returned snippet. For native
UTF-8 extractions the plain store can reconstruct and hash-check the
original indexed bytes. Other supported formats generally supply
retained extracted content with original locations. Those sources
give Answer a much larger evidence pool than its top-ranked hits.
They do not tell it which path to open, whether it has the decisive
lines, or what changed after publication.
[Step 6 audit](#step-6-result--committed-vector-coverage) · [Retained source reader](../../crates/airk-store/src/current/plain.rs) · [Bounded line reader](../../crates/airk-kernel/src/current/runtime.rs)

**Enough means the agent can make its next decision from the reply.**
Answer should hold the retrieval plan, discovered anchors, evidence
and gap status for the duration of the service-owned attempt. Each
internal round should consume that state directly. When more work
remains after the first delivery, use the existing handle-only
continuation design to return newly gathered evidence and updated
gaps. Count that continuation as a round trip. The agent should have
no need to resend the task or paste earlier evidence into it. This
addresses repeated service requests and the growing context an agent
has to carry while assembling evidence.
For a code change, that may require the owning implementation and
constraint, the relevant caller or configuration path, and a test
entry point with its decisive assertion. For requirements work, it
means the governing text, scope, exceptions, and any conflicting
decision or observed behavior. For review, it means evidence for the
changed path and affected callers, with a clear coverage boundary.
Give the agent the relevant source ranges, their provenance, and a
specific next read for any consequential gap. Measure whether it can
finish correctly with fewer external searches and reads. The
historical judged comparison is evidence that current Answer behavior
has not met that standard.
[Judged Windows Change Set 1](answer-evaluation/06-loop-change-set-1.md#first-sample-results)

#### Proposed fallback inside the AIRK service

1. **Use indexed discovery and exact reads first.** Plan the evidence
   needs, choose the appropriate native Search, fact, reference,
   relationship or outline read, then open the required source range.
   Read through a relevant heading or definition when a match alone
   cannot establish the rule. Continue only for gaps that could change
   the agent's decision.
2. **Scan retained text when candidate selection misses.** If Answer
   has an exact identifier, diagnostic string, configuration key, or
   plausible path and indexed lookup is incomplete or inadequate,
   run a bounded literal or whole-identifier scan over the authorized
   retained text. Use the narrowest justified file or subtree first.
   This is the indexed-revision equivalent of a scoped grep followed
   by a range read. It can recover exact matches that lexical candidate
   selection failed to admit. Use verified retained UTF-8 bytes where
   available and the existing verified extraction and location mapper
   for other formats. Treat a scan stopped by a limit as partial.
3. **Add an explicitly authorized live-source fallback for gaps the
   publication cannot close.** Pilot a read-only, in-process matcher
   with typed inputs for a literal or identifier and an allowed path
   or prefix. A confirmed stale file, a known missing path, or a
   request explicitly about saved working-tree changes can justify a
   live read. Begin with authorized known files, then evaluate a
   bounded subtree scan if the first pilot shows a need for discovery.
   The service should reuse the existing source authorization and
   native file-opening checks where applicable. Resolve within the
   granted roots, honor source selections and denies, and verify
   resolved file identity so traversal, aliases and links cannot
   widen access. A new or unindexed path requires authority that
   actually includes that path. An index selection alone does not
   authorize searching every file under its on-disk root.
4. **Return evidence with its actual basis and work accounted for.**
   Charge the scan to the same attempt's file, byte, match and deadline
   budget, including bytes read and excerpts returned. Reuse opened
   evidence across needs where its identity is still valid. Preserve
   the file path, exact observed digest, line range and observation
   time for live evidence. Show whether it matches the indexed
   revision or differs. A changed file has its own coordinates and
   source identity. A partial traversal, unsupported format, or file
   that changes during the scan leaves a disclosed gap. Use native
   extraction or a targeted document read when literal matching of
   plain text cannot supply reliable document locations.

Keep the fallback inside the existing Answer gathering and service
read path. Start with an explicit, budgeted retrieval action whose
inputs and output can be reviewed. The model may nominate an anchor
or prioritize a gap. Native code validates the requested operation and
scope. Saved-source verification today checks returned indexed evidence
against live saved bytes. The proposed fallback would deliver newly
observed evidence and needs a reviewed source-authorization and
response contract for that capability.
[Saved-source verification contract](../source-verification.md) · [Source scope and policy](../decisions/002-projects-indexes-and-query-scope.md) · [Answer loop and receipts](final-plan-fp16.md#121-the-stages)

#### Tie-in to the completion plan

Make this the **first gathering experiment in the new Windows L7 retrieval
series**, after the index-quality decision and current-stack baseline. Freeze the
existing publications, matched service and CLI, scenarios and grading
rules. Record a scoped direct-tools reference run on the same source
bytes to learn which facts an agent can recover through exact search
and targeted file reads. Replay the current deterministic Answer,
targeted indexed reads, and targeted reads plus the retained-text
scan. Use the traces to choose cases for the live-source pilot. Test
that pilot separately on controlled stale or missing-file cases with
known ground truth and explicit source scope. Keep its changed-source
results distinguishable from the pinned L7 comparator.
[Windows L7 baseline and Change Set 1](completion-plan.md#3-stage-l7-with-a-windows-first-review)

Advance the smallest combination that measurably reduces the agent's
subsequent Search, navigation and file reads while preserving correct
outcomes and source coverage. Report which route first recovered each
necessary fact, time to usable evidence, internal scan cost and any
gaps left open. Count external service round trips through task
completion, including continuations, and report total request bytes
and growth across successive requests, repeated evidence delivered,
and total reply bytes. If the agent harness exposes model input
tokens, report them separately from service traffic. Use these
measures to see whether added work inside Answer pays for itself in
fewer agent calls and less repeated context. Judge it against the
stronger of Parse and Search using the existing substantial-win rule. Carry
the useful gathering policy into Change Sets 2 and 3 so Focus and
Arbiter can be evaluated on ranking, gap choice and stopping. Keep
the Windows manual review and the completion plan's partial-delivery
checks. If the retained scan covers the relevant misses, keep live
scanning for the cases that require current bytes. If the remaining
misses involve concepts a literal scan cannot find, use those traces
to decide whether a small indexing experiment is justified.
[Windows review gate](completion-plan.md#stage-b--stop-for-manual-review-of-windows-results) · [Partial delivery and continuations](completion-plan.md#4-close-the-parallel-section-12-work)

### Context inference and evidence budgets

Use the Context inference model, exposed as Arbiter in Answer, to help
decide **what to read next and whether the evidence settles the need**.
The existing loop already makes bounded plan, coverage and follow-up
decisions. Extend the choices to cover indexed retrieval, a larger
source range, a retained-text scan and an authorized live-file scan.
For each open need, show Context the source anchors, what the returned
excerpts establish, and any truncation, stale evidence or disagreement.
Give it a small set of typed actions grounded in those observations.
A missing heading qualifier may justify a section read. An exact
error string absent from a bounded lexical result may justify a
retained scan. A changed file may justify an allowed live read when
the task depends on current bytes. Native code decides admissibility
and executes the action. Context cannot widen the scope or certify
coverage from its score alone. Keep the deterministic fallback for
model unavailability, abstention and deadline pressure.
[Answer model decisions](../../crates/airk-kernel/src/arbiter/answer/model.rs) · [Loop model roles](final-plan-fp16.md#122-how-the-models-take-part)

The checked code uses **8,192 bytes** as the ordinary Search default
and as the Search benchmark budget. It also uses 8,192 bytes in some
Answer References calls. Answer's broad internal Search already
allows up to 24,576 bytes, and some targeted follow-up excerpts use
3,072 bytes. Answer /3 defaults to a 256 KiB gathering-retention budget
and has a separate 128 KiB reply ceiling, subject to the request
budget. Context's 8,192 limit is **input tokens per model call** at
the profile ceiling. These limits govern different costs. Removing
the ordinary Search default alone would leave the other limits in
place. An older Windows gate found that roughly one fifth to one
quarter of agents' later reads went back to evidence Answer had
already delivered cut, despite median first replies of 82–91 KB.
That supports testing which excerpts get more room and which evidence
is worth delivering.
[Search default](../../crates/airk-mcp/src/lib.rs) · [Internal Search and excerpt recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Answer budget contract](../../crates/airk-core/src/arbiter/answer_loop.rs) · [Windows gate findings](answer-evaluation/04-gate-lg.md#6-what-was-bad)

**Replace fixed per-read defaults inside Answer with allocations based
on the evidence need and the remaining attempt budget.** Use small
discovery excerpts to locate candidates. Once a candidate matters to
a decision, let Answer spend more on the cited function, contract
section, test assertion and setup, or document block. Charge larger
reads against the shared attempt budget and give unused allocation
from satisfied needs to unresolved ones. Preserve source boundaries
and classify a clipped result as incomplete when the omitted lines
could change the decision. Keep three distinct accounts for retrieval
work and retained evidence, Context's scoring input, and the final
client reply. Context can compare compact, cited decision inputs
while Answer retains and returns a longer decisive excerpt. Fit the
reply by prioritizing complete evidence for the most important
needs and deduplicating overlapping text. Use a handle-only continuation
for useful remaining evidence when needed. Keep bounded transport
limits and verify actual client parity before proposing a larger
public frame.

The [Windows work order](completion-plan.md#3-stage-l7-with-a-windows-first-review)
includes two comparisons after the upstream index-quality decision.
In Change Set 1, hold model settings fixed and compare current
per-read allocations with evidence-driven allocation on pinned
inputs. Attribute every unrecovered fact to candidate discovery,
source-range truncation, retention, reply packing or the deadline.
Where a cut caused a reread, test whether expanding that range
inside Answer reduces external calls. In Change Set 2, hold the
retrieval and allocation policy fixed and compare deterministic
follow-ups with Context-guided decisions over the same eligible
actions, including direct-file fallback in the separately authorized
pilot. Hold Focus fixed for that comparison. Record correct task
finishes, external calls and request bytes, time to usable evidence,
internal scan cost, model tokens and reply bytes. Increase a specific
limit when the trace shows it is the binding constraint and the
added work saves the agent time or turns.

The completion plan owns the implementation order and downstream checks.
It asks whether the delivered evidence lets a fresh agent make the change,
cite the governing requirement, substantiate a review finding or select the
right regression test. Its index experiments cover exact and fielded lookup,
relationships, passage boundaries and alternative embedding inputs. The
decision to build a new full publication follows a measured small pilot and
discussion of its gain, cost and rollout.
[Agent task reviews](completion-plan.md#what-an-agent-must-be-able-to-do-with-the-reply) · [Index experiment choices](completion-plan.md#when-to-change-an-index)
