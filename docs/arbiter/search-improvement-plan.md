# AIRK Search improvement plan

**Status:** Approved for sequential implementation and matched Search testing.

Improve the evidence returned by ordinary AIRK Search on Bookbuilder and Munarium. Test every proposal with Focus off and on. Keep Arbiter disabled in every run. The first five proposals reuse the settled indexes. The sixth needs a small experimental index only if the coverage audit justifies it. Steps 7 and 8 reuse the publications selected at the end of Step 6.

## What we know

- The current baseline is the first Search call from each of 20 development scenarios, ten per repository. Each call sent the full 40–68-word instruction and returned ten hits under an 8,192-byte response budget. Search returned qualifying citations for 14 of 113 essential notes.
- Of the 99 misses, 66 had no hit from an accepted source path, 30 had a hit from the right path with zero line overlap, and three had too little overlap to count. Bookbuilder accounted for 48 of the path misses. Munarium accounted for 23 of the wrong-span misses.
- The retained Search sessions found 41 more notes later. Additional searches first found 19 and targeted file reads first found 22. An older wider Search probe found the same 14 in its first ten and 16 more deeper in the list. The older probe used different publications, so it gives us a reason to measure depth on the current ones.
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

Use the same MiniLM weights and the same source sample for both builds. Compare passage coverage, exact vector ranks, index size and build cost. Consider any full rebuild only after the isolated test demonstrates a gain that the query-time changes leave unresolved.

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

## Evidence and implementation references

- [Matched Focus comparison and first-Search miss attribution](../simplify/one-process-ledger.md#25-november-2026--matched-ordinary-search-with-and-without-cuda-focus)
- [Change set 1 evaluation report and retained Search session coverage](answer-evaluation/06-loop-change-set-1.md)
- [Plain-store retrieval and private Search v2 contract](../../crates/airk-kernel/src/current/search/CONTRACT.md)
- [Current Search, fusion, snippets and native definition lookup](../../crates/airk-kernel/src/current/search.rs)
- [Vector passage construction](../../crates/airk-retrieval/src/vector_units.rs)
