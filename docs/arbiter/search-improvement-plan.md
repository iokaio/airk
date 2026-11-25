# AIRK Search improvement plan

**Status:** Approved for sequential implementation and matched Search testing.

Improve the evidence returned by ordinary AIRK Search on Bookbuilder and Munarium. Test every proposal with Focus off and on. Keep Arbiter disabled in every run. The first five proposals reuse the settled indexes. The sixth needs a small experimental index only if the coverage audit justifies it.

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

## Comparison method

1. Pin both existing publications and freeze the original 20 queries and 113 note locations. Use the same ten-hit and 8,192-byte limits in each main comparison. Keep the note labels out of query construction and ranking.
2. Keep one original baseline pair. Add each proposal to the preceding implementation and collect one Focus-off and one Focus-on result. Retain that pair as the baseline for the next step. Compare each step with both the preceding pair and the original. Keep the Focus profile, device and scoring limits fixed. For the two isolated indexes in step 6, pin each publication and use identical source revisions.
s’s effect with Focus off, its effect with Focus on, the on/off difference within each version, and whether Focus changes the size or direction of the proposal’s benefit. For each run, report top-three and top-ten essential coverage, gains and losses by repository and scenario, elapsed median and p95, and response bytes. Record differences in publications or served modes. Alternate measurement order when assessing latency.
4. Trace each note from branch candidates through fused rank, the candidates presented to Focus, and the returned source lines. Use the existing overlap rule consistently. Retain Focus receipts with the active profile and runtime, original and final order, scored candidates and scores when available, and any reason Focus did not apply. Explain each material on/off difference using the trace, and label explanations that remain hypotheses. Keep each measured result and its receipt with the corresponding implementation milestone.
5. Advance a change when it improves final essential coverage or materially improves early rank with a measured latency cost and understood losses. If candidate recall rises but returned coverage stays flat, use the trace to choose the next change. Keep source scope and citation integrity as acceptance requirements.

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

## Evidence and implementation references

- [Matched Focus comparison and first-Search miss attribution](../simplify/one-process-ledger.md#25-november-2026--matched-ordinary-search-with-and-without-cuda-focus)
- [Change set 1 evaluation report and retained Search session coverage](answer-evaluation/06-loop-change-set-1.md)
- [Plain-store retrieval and private Search v2 contract](../../crates/airk-kernel/src/current/search/CONTRACT.md)
- [Current Search, fusion, snippets and native definition lookup](../../crates/airk-kernel/src/current/search.rs)
- [Vector passage construction](../../crates/airk-retrieval/src/vector_units.rs)
