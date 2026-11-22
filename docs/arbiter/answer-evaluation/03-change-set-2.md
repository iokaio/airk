# 03 Change set 2: `search_v2`, a wider Context roster and Focus candidate 2, on airk and bookbuilder

Written on 22 November 2026 (UTC) from the private receipts of the change-set-2
replays (08:09 to 08:44 UTC), the judged airk and bookbuilder passes (sessions
08:15 to 09:27 UTC, judging to 09:36 UTC), the search replay (09:52 to 10:03
UTC, decision recorded in a file dated 10:03:31 UTC) and the lifetime handles of
L37 to L41. It goes into one commit with the
change set's code and [results/03-change-set-2.csv](results/03-change-set-2.csv).

> **Headroom: 10 of 10 airk scenarios and 10 of 10 bookbuilder-v2 scenarios**
> (20 of 20), as at the baseline: on neither repository does the better of Parse
> and Search finish any scenario correctly. Both repositories ran, so the full
> rule (12 of 20 scenarios, at least 5 wins in each repository) can be read for
> the first time. Answer has **0 substantial wins in 20**, in every
> configuration. The rule is not met.

Every "better of Parse and Search" in this report is chosen per scenario by the
primary agent's tie rule of 22 November, as in [01-baseline.md](01-baseline.md)
section 4.2: the path with a correct finish at fewer turns; with no finish, the
path that covered more essential notes over the session, then the one with fewer
session turns, then Search.

## 1. Summary

Change set 2 turned on `search_v2` (a new ranking for prose instructions in the
shared search, so Search, Parse and Answer all change), Context option B (a wider
Context roster that can take the best Read even when a terminal card scores
highest) and Focus candidate 2 weights, on top of change set 1's layers. It was
tested on airk's ten scenarios and on the ten scenarios of bookbuilder-v2, with
Haiku agents and two Sonnet judges per result, and compared with the baseline of
each repository (airk `baseline`, bookbuilder `baseline-bookbuilder-v2`).

**No correct finish, on either repository.** All 104 results graded 0 from both
judges, and the two judges gave the same grade on 104 of 104. Search, which
finished C1 and T1 on airk at the baseline, finished neither this time, so the
better of Parse and Search now finishes nothing anywhere. There was no close
call, so no second session ran.

**The airk Search drop (22 notes to 15, C1 and T1 lost) was session variance, not
`search_v2`.** The recorded Search queries of the baseline and of this pass were
replayed on the same index under the old and the new search
([section 4.5](#45-the-search-replay)). On the baseline's queries `search_v2` found
15 of 53 essential notes against 13 for the old search; on this pass's queries it
found 7 against 6. The Haiku sessions of this pass issued queries that find 6 to 7
notes under either search, against 13 to 15 for the baseline's queries. Two single
notes were lost, A1 on the baseline's queries (1 to 0) and C2 on this pass's
queries (2 to 1).

**What moved is small and mixed.**

| | airk (baseline) | bookbuilder (baseline) |
| --- | --- | --- |
| First Answer call covers, all three configurations | 4/53 (2) | 8/61 (4 to 5) |
| First call of Search covers | 3/53 (2) | 6/61 (4) |
| Session notes, no model | 15/53 (12) | 14/61 (15) |
| Session notes, Focus | 15/53 (11) | 14/61 (12) |
| Session notes, Compact | 10/53 (8) | 10/61 (10) |
| Session notes, Search | 15/53 (22) | 27/61 (26) |
| Session notes, Parse | 6/53 (7) | 18/61 (14) |

The first-call gain is real on both repositories but does not reach the
sessions' results. Every Answer configuration is still worse than Search in
notes on bookbuilder, and on airk it is ahead of Search only in writing and
architecture.

**Calls are slower again.** The no-model Answer call averaged 18.0 s on airk
(baseline 9.1 s), Focus 33.3 s (22.9 s) and Large 116.4 s (66.4 s); Compact's
mean was lower, 42.6 s (49.1 s), but its longest call was longer. On bookbuilder
one Compact call took 120.7 s and on airk the longest Large call took 165.2 s,
over the 120 s deadline the request sets.

**Two harness events** are reported in section 6.1: a plan that named results
`focus`, `compact` and `large` while the spec calls them `focus-cpu`,
`compact-cpu` and `large-cpu` ran zero Focus sessions and exited 0; and lifetime
L38 did not close cleanly, which needed a one-time, recorded restart exception
decided by the primary agent (not by the user) with a Python half and a Rust
half. The Rust half belongs to the change-set-3 commit.

The GPU-cell replays and the Context leverage measurement of
[02-change-set-1.md](02-change-set-1.md) sections 4.5 and 4.6 are not in the
receipts used for this report and stay pending.

## 2. What changed in this series

The changes live in the same private evaluation build as change set 1; they are
off in a product build. The image is `cs2-9d1b9aea`: commit `1fe7b600` plus the
uncommitted working tree (section 9). The pass ran with the layers
`all,pool_full=8` for Search, Parse, no model and Focus, and
`all,pool_full=8,context_b` for Compact and Large; `search_v2` is bound by the
service environment variable `AIRK_ARBITER_EVALUATION_SEARCH=search_v2` and is
compiled only with the kernel's `test-support` feature, which a product build
does not have. Every other setting is change set 1's: the same Answer budget
(calls 12, rounds 3, fanout 12, candidates 32, input tokens 8,192, max bytes
524,288, deadline 120,000 ms), the same 16-wide Focus profile shape with the
software instruction, and all of change set 1's layers on.

| # | Change | Cause it addressed | Expected effect | Measured effect | Kept or dropped |
| --- | --- | --- | --- | --- | --- |
| 1 | **`search_v2`** (design `design/search-v2.md`): for hybrid searches of 8 or more words, prose query rewriting into key and named words; two ranked any-term lexical branches; 300 candidates per branch before fusion; weighted fusion; a file-kind prior (generated 0.3, office documents 0.5, prose 0.75, source 1.0); per-file diversity; snippets centred on key words. It changes every caller of kernel search | At the baseline the lexical side of a prose instruction matched nothing, prose outranked code, one file could fill many places, and 35 of 53 airk notes had no file in the pool | Notes named in the instruction enter the pool; fewer prose and `.docx` hits | 100-hit depth on airk: notes in the pool under rule 2 went **5 to 8 of 53** (rule 1: 6 to 9), with the switch off identical to the baseline; mean depth call 3,577 to 5,949 ms. Search replays: **+2 on the baseline's queries, +1 on this pass's**, with two single-note losses. In the pass: Search first call 3/53 (2) on airk and 6/61 (4) on bookbuilder; bookbuilder Search 27/61 (26) and Parse 18/61 (14) over the session. A bookbuilder-v2 depth run with `search_v2` is not in the receipts | Kept. Change set 3 keeps it for Search, Parse and Answer |
| 2 | **Context option B** (layer `context_b`): a wider roster (8 source units where the product shows 4; 6 Read cards where it offers 5) and the best Read is chosen even when a terminal card scores highest, under its own task policy; new profiles `context-compact-cpu-cs2-context-b` and `context-large-cpu-cs2-context-b` | Option A (change set 1) only moved one of the first five queued reads to the front, and most offered reads run anyway | Context's choice changes what runs | First-call replays: coverage 4/53 on airk and 8/61 on bookbuilder, the same as no model and Focus. In airk replays on `search_v2` the option-B Compact first call took 27.1 s against 34.4 s for the provisional rule. In the pass Compact covered 10 of 53 on airk (change set 1: 14) and 10 of 61 on bookbuilder (baseline 10) | Kept in the pass. No coverage gain |
| 3 | **Focus candidate 2** weights, in the same 16-wide software profile `focus-static1024-cpu-cand2-software-w16` | Change set 1's candidate 1 was slower and covered the same notes as the base weights | A faster Focus at equal coverage | airk replays on `search_v2`: 4/53 for the base weights, candidate 1 and candidate 2, at 35.4 s, 35.0 s and 27.4 s mean first call. In the pass the Focus first reply took 28.4 s on airk (change set 1: 28.7 s) and 40.5 s on bookbuilder (baseline 22.1 s) | Kept in the pass |
| 4 | **Inputs r4**: `change-set-2/profiles/change-set-2-inputs-r4.json`, SHA-256 `4d058008…a7e3`, registers the profiles above | The new profiles need registering | None; it is bookkeeping | None | Used |

The build also carries two Focus tuning tokens (`focus_keep`, `focus_threshold`)
that the pass did not use. No change of change set 1 was removed.

## 3. What was tested

| Item | Value |
| --- | --- |
| Scenarios | The ten airk scenarios (53 essential notes) and the ten bookbuilder scenarios (61 essential notes) of [scenarios.md](scenarios.md) |
| Paths | Search and Parse (run again, not reused: the shared search changed); Answer with no model; with Focus (CPU); with Focus and Compact (CPU); with Focus and Large (CPU; A1 and C3 on airk, A1 and C2 on bookbuilder; timing only; 180 s client timeout) |
| Agents | `claude-haiku-5-5`, `--effort medium`, Claude Code 2.1.330, 12 turns at most, one session per scenario and path |
| Judges | Two independent `claude-sonnet-5-5` sessions per result, `--effort medium` |
| Answer budget, every call | calls 12, rounds 3, fanout 12, candidates 32, input tokens 8,192, max bytes 524,288, deadline 120,000 ms |
| Layers | `all,pool_full=8` (Search, Parse, no model, Focus); `all,pool_full=8,context_b` (Compact, Large) |
| Search | `search_v2` for every path |
| Focus profile | `focus-static1024-cpu-cand2-software-w16` |
| Context profiles | `context-compact-cpu-cs2-context-b`, `context-large-cpu-cs2-context-b` |
| Change-set inputs | `change-set-2/profiles/change-set-2-inputs-r4.json` |
| Image | `cs2-9d1b9aea` (service `7c8a931d…34b6`, CLI `28d20587…182d`, `images.json` `23385b7e…3bc`), from `1fe7b600` plus the uncommitted working tree described in section 9 |
| Lifetimes | L36-cs2a (A layers: both replay sets A, Search, Parse and no model), L37-cs2b (B layers: replay sets B, Compact and Large), L38-cs2a2 (A layers: Focus). All started from the previous lifetime and ready on the same publications |
| Indexes | airk `idx_ac4e36f1c8b265719b0ae05d28894fb0`, publication generation 330, token `airk-r1:d9feb47f…:330:5d1a2e84…f515dbc9da33`, the baseline's; bookbuilder-v2, generation 268, token `airk-r1:a8ee462e…:268:1b153149…ba5a6` (full tokens in the receipts) |
| Settled guard | 8 snapshots per pass (a start and an end for each of four session groups), all settled and all with the same token |
| Prompt freeze | Search, Parse and no model ran under `d86a78aa…` (frozen 08:08:23 UTC); Compact, Large and Focus under `f78d3dc0…` (frozen 08:35:26 UTC), the one the pass index records. Both re-freezes changed harness files only (`prompts_changed: false`); the reasons are in section 6.1 |
| Scenario freeze | `scenario-freeze.json`, SHA-256 `0a7d7b0a…e7806`, unchanged |
| Results | 104: 52 per repository, all run fresh and judged; none reused |
| Second sessions | None: no close call |
| Cost | airk: agents $0.7774, judges $8.3808, total $9.1582. bookbuilder: agents $1.3736, judges $9.0335, total $10.4071. Together $19.5653 |

The run was one plan in two parts. `cs2-plan.json` (08:08:35 to 08:44:44 UTC)
restarted L35 into L36, ran the A-layer replays and the A-layer Search, Parse and
no-model sessions of both repositories, restarted into L37 and ran the B-layer
replays; it stopped at the first Compact step (section 6.1).
`cs2-plan-2.json` (08:45:00 to 09:36:11 UTC) ran Compact and Large on L37, Focus
on L38, grading and judging, and the no-op second-session steps.

## 4. Results

### 4.1 Against Parse and Search

Each cell gives the essential notes covered by the end of the session and the
number of calls, with the baseline's in brackets. Every result graded 0.

**airk** (53 essential notes)

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 4/5, 9 (5, 11; finish at 9) | 1/5, 5 (0, 5) | 3/5, 10 (3, 7) | 2/5, 6 (1, 7) | 1/5, 9 (1, 7) | – |
| C2 coding | 2/6, 8 (4, 10) | 0/6, 6 (2, 4) | 3/6, 8 (2, 8) | 4/6, 8 (2, 11) | 2/6, 8 (2, 11) | – |
| C3 coding | 3/5, 10 (2, 10) | 1/5, 7 (2, 6) | 2/5, 6 (2, 7) | 2/5, 8 (3, 12) | 1/5, 7 (0, 8) | 2/5, 7 (0, 8) |
| W1 writing | 0/5, 3 (0, 3) | 0/5, 2 (0, 3) | 1/5, 7 (0, 12) | 3/5, 10 (0, 4) | 0/5, 9 (0, 9) | – |
| W2 writing | 0/6, 5 (0, 7) | 1/6, 7 (1, 7) | 0/6, 9 (1, 11) | 0/6, 10 (0, 11) | 0/6, 8 (1, 11) | – |
| T1 test | 4/5, 10 (5, 8; finish at 7) | 2/5, 6 (2, 5) | 2/5, 7 (2, 10) | 3/5, 7 (1, 11) | 4/5, 12 (2, 12) | – |
| T2 test | 0/4, 9 (1, 6) | 0/4, 3 (0, 3) | 0/4, 7 (0, 6) | 0/4, 7 (2, 8) | 0/4, 9 (0, 11) | – |
| A1 architecture | 2/5, 9 (1, 7) | 0/5, 8 (0, 5) | 2/5, 7 (0, 9) | 0/5, 12 (0, 9) | 0/5, 12 (0, 9) | 2/5, 10 (0, 12) |
| A2 architecture | 0/6, 6 (1, 6) | 1/6, 4 (0, 8) | 0/6, 10 (2, 9) | 0/6, 11 (2, 9) | 1/6, 11 (2, 9) | – |
| A3 architecture | 0/6, 10 (3, 8) | 0/6, 5 (0, 8) | 2/6, 9 (0, 10) | 1/6, 8 (0, 10) | 1/6, 9 (0, 10) | – |

Per path over its rows (airk), baseline in brackets:

| | Search | Parse | No model | Focus | Compact | Large (2 rows) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 0 (2) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) |
| Essential notes covered | 15/53 (22) | 6/53 (7) | 15/53 (12) | 15/53 (11) | 10/53 (8) | 4/10 (0) |
| Covered by the first call | 3/53 (2) | 0/53 (0) | 4/53 (2) | 4/53 (2) | 4/53 (2) | 0/10 (0) |
| Mean calls per session | 7.9 (7.6) | 5.3 (5.4) | 8.0 (8.9) | 8.7 (9.2) | 9.4 (9.7) | 8.5 (10.0) |
| Mean bytes per session | 104,046 (104,514) | 150,493 (150,070) | 152,223 (166,281) | 160,171 (166,316) | 161,237 (172,273) | 183,828 (155,234) |
| Mean first reply | 22,966 B in 8.1 s (22,644 B, 4.7 s) | 1,843 B in 0.3 s (1,843 B, 0.3 s) | 50,999 B in 18.8 s (51,326 B, 8.5 s) | 54,402 B in 28.4 s (51,369 B, 19.1 s) | 54,288 B in 49.6 s (51,047 B, 24.9 s) | 51,584 B in 91.8 s (45,430 B, 82.1 s) |
| Mean tool time per session | 22.4 s (17.6) | 19.1 s (18.9) | 29.6 s (20.3) | 54.9 s (36.7) | 65.3 s (40.7) | 296.5 s (112.1) |
| Mean wall time per session | 46.2 s (40.1) | 46.3 s (44.1) | 53.9 s (44.2) | 77.2 s (61.2) | 90.5 s (65.7) | 317.7 s (135.0) |
| Agent cost | $0.1434 | $0.1855 | $0.1725 | $0.1189 | $0.1341 | $0.0230 |
| Rubric B, mean of both judges | 1.575 (1.688) | 0.212 (0.188) | 1.625 (1.250) | 1.425 (1.275) | 1.413 (1.212) | 0.938 (0.562) |

The first-call notes are the same for all three Answer configurations: C1 1 of 5,
C2 2 of 6 and T1 1 of 5 (baseline: C2 1 and T1 1). Search's first call covers C1,
C2 and T1, one note each.

Bytes and time per scenario (bytes returned over the session, then tool time and
wall time):

| Scenario | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- |
| C1 | 195,452 B, 25.9 / 56.3 s | 135,704 B, 19.4 / 38.7 s | 161,277 B, 22.0 / 48.6 s | – |
| C2 | 143,461 B, 24.8 / 49.3 s | 183,571 B, 88.5 / 106.7 s | 136,197 B, 34.3 / 52.9 s | – |
| C3 | 170,094 B, 45.1 / 63.6 s | 122,006 B, 25.9 / 45.0 s | 156,422 B, 24.8 / 44.7 s | 183,556 B, 306.1 / 323.3 s |
| W1 | 140,021 B, 26.0 / 46.9 s | 199,700 B, 73.3 / 99.8 s | 169,219 B, 118.9 / 143.7 s | – |
| W2 | 134,372 B, 28.9 / 52.2 s | 142,477 B, 68.2 / 91.8 s | 162,411 B, 42.4 / 64.7 s | – |
| T1 | 118,725 B, 20.4 / 42.6 s | 163,163 B, 18.0 / 40.4 s | 172,821 B, 90.0 / 122.8 s | – |
| T2 | 118,731 B, 23.3 / 47.5 s | 156,161 B, 76.8 / 95.1 s | 143,320 B, 24.7 / 51.6 s | – |
| A1 | 123,525 B, 19.6 / 45.9 s | 183,349 B, 21.7 / 51.7 s | 194,932 B, 107.7 / 134.9 s | 184,100 B, 286.9 / 312.2 s |
| A2 | 168,898 B, 34.5 / 60.6 s | 174,686 B, 96.9 / 122.3 s | 151,292 B, 22.2 / 51.8 s | – |
| A3 | 208,952 B, 47.7 / 74.4 s | 140,890 B, 59.9 / 80.9 s | 164,479 B, 165.9 / 188.9 s | – |

**bookbuilder** (bookbuilder-v2 index, 61 essential notes)

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 4/7, 11 (5, 10) | 3/7, 5 (1, 6) | 2/7, 10 (2, 12) | 2/7, 9 (2, 9) | 2/7, 9 (2, 9) | – |
| C2 coding | 3/5, 9 (2, 11) | 0/5, 7 (1, 7) | 3/5, 7 (1, 9) | 2/5, 10 (2, 7) | 1/5, 12 (2, 7) | 2/5, 12 (3, 10) |
| C3 coding | 2/6, 9 (2, 10) | 2/6, 8 (3, 4) | 1/6, 10 (2, 10) | 2/6, 10 (2, 10) | 1/6, 11 (2, 10) | – |
| W1 writing | 2/5, 4 (1, 5) | 1/5, 7 (1, 5) | 2/5, 10 (1, 5) | 2/5, 9 (2, 9) | 3/5, 8 (1, 11) | – |
| W2 writing | 2/6, 11 (4, 11) | 2/6, 8 (3, 5) | 0/6, 12 (2, 12) | 1/6, 12 (2, 12) | 0/6, 12 (2, 12) | – |
| T1 test | 5/6, 12 (3, 11) | 3/6, 11 (2, 5) | 0/6, 12 (2, 12) | 2/6, 12 (0, 11) | 0/6, 11 (0, 11) | – |
| T2 test | 2/6, 6 (2, 10) | 2/6, 8 (1, 7) | 2/6, 4 (3, 12) | 0/6, 8 (0, 8) | 0/6, 11 (0, 12) | – |
| A1 architecture | 1/8, 10 (0, 9) | 1/8, 6 (0, 10) | 1/8, 12 (0, 12) | 0/8, 11 (0, 12) | 0/8, 12 (0, 12) | 0/8, 10 (0, 12) |
| A2 architecture | 3/6, 8 (2, 10) | 2/6, 10 (0, 8) | 0/6, 10 (0, 11) | 0/6, 11 (0, 10) | 0/6, 12 (0, 11) | – |
| A3 architecture | 3/6, 8 (5, 9) | 2/6, 6 (2, 6) | 3/6, 12 (2, 12) | 3/6, 10 (2, 10) | 3/6, 10 (1, 11) | – |

| | Search | Parse | No model | Focus | Compact | Large (2 rows) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) |
| Essential notes covered | 27/61 (26) | 18/61 (14) | 14/61 (15) | 14/61 (12) | 10/61 (10) | 2/13 (3) |
| Covered by the first call | 6/61 (4) | 0/61 (0) | 8/61 (4) | 8/61 (5) | 8/61 (4) | 1/13 (0) |
| Mean calls per session | 8.8 (9.6) | 7.6 (6.3) | 9.9 (10.7) | 10.2 (9.8) | 10.8 (10.6) | 11.0 (11.0) |
| Mean bytes per session | 130,479 (133,396) | 231,421 (193,893) | 190,541 (195,459) | 187,767 (197,390) | 193,944 (197,146) | 177,521 (201,330) |
| Mean first reply | 21,577 B in 5.4 s (20,921 B, 3.0 s) | 1,863 B in 0.4 s (1,863 B, 0.4 s) | 49,803 B in 15.5 s (49,739 B, 13.1 s) | 53,160 B in 40.5 s (47,601 B, 22.1 s) | 53,415 B in 36.8 s (47,702 B, 23.3 s) | 53,378 B in 90.8 s (54,515 B, 87.7 s) |
| Mean tool time per session | 17.8 s (16.1) | 25.1 s (21.3) | 30.8 s (26.5) | 60.5 s (70.5) | 56.9 s (59.4) | 97.9 s (96.2) |
| Mean wall time per session | 44.1 s (40.4) | 56.5 s (48.1) | 56.8 s (54.1) | 88.7 s (97.9) | 84.5 s (88.5) | 128.2 s (124.4) |
| Agent cost | $0.2118 | $0.5567 | $0.2032 | $0.1606 | $0.2043 | $0.0372 |
| Rubric B, mean of both judges | 1.837 (1.688) | 0.013 (0.000) | 1.637 (1.438) | 1.538 (1.350) | 1.500 (1.350) | 1.250 (1.062) |

The first-call notes are the same for all three Answer configurations: C1 1, C2
1, C3 1, W1 2 and A3 3 (baseline: C1 1, W1 1 or 2, W2 1 and A3 1, by
configuration). Search's first call covers one note each on C1, C2, C3 and W1 and
two on A3.

| Scenario | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- |
| C1 | 212,104 B, 21.9 / 49.4 s | 193,185 B, 16.6 / 45.1 s | 168,897 B, 17.4 / 44.5 s | – |
| C2 | 150,738 B, 25.6 / 46.9 s | 208,915 B, 27.0 / 52.1 s | 231,389 B, 18.7 / 52.8 s | 192,103 B, 92.2 / 126.3 s |
| C3 | 181,248 B, 21.6 / 48.4 s | 160,434 B, 36.5 / 62.4 s | 194,763 B, 88.1 / 119.4 s | – |
| W1 | 177,698 B, 30.7 / 58.1 s | 194,335 B, 113.5 / 143.4 s | 154,399 B, 14.5 / 37.6 s | – |
| W2 | 281,752 B, 59.4 / 86.4 s | 208,392 B, 66.0 / 101.3 s | 200,344 B, 181.6 / 216.2 s | – |
| T1 | 199,625 B, 28.3 / 57.1 s | 182,683 B, 31.7 / 61.9 s | 201,635 B, 25.1 / 52.2 s | – |
| T2 | 136,226 B, 44.9 / 65.5 s | 171,812 B, 35.4 / 58.7 s | 231,332 B, 39.4 / 63.8 s | – |
| A1 | 142,418 B, 23.3 / 52.1 s | 167,475 B, 48.9 / 80.1 s | 172,146 B, 23.5 / 48.8 s | 162,938 B, 103.6 / 130.2 s |
| A2 | 215,306 B, 19.1 / 43.0 s | 206,794 B, 98.2 / 126.1 s | 170,791 B, 127.2 / 154.5 s | – |
| A3 | 208,293 B, 32.8 / 60.7 s | 183,648 B, 131.5 / 155.4 s | 213,744 B, 33.0 / 55.5 s | – |

The baseline rows in brackets that were reused at the baseline carry their
source's values, as in the earlier reports. The baseline's compared rows are
[01-baseline.md](01-baseline.md) (airk) and its section 4.6 (bookbuilder-v2).

**Per kind of task** (essential notes covered, baseline in brackets; every Answer
grade is 0):

airk

| Kind | Notes | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- | --- |
| Coding (C1 to C3) | 16 | 9 (11) | 2 (4) | 8 (7) | 8 (6) | 4 (3) |
| Writing (W1, W2) | 11 | 0 (0) | 1 (1) | 1 (1) | 3 (0) | 0 (1) |
| Test (T1, T2) | 9 | 4 (6) | 2 (2) | 2 (2) | 3 (3) | 4 (2) |
| Architecture (A1 to A3) | 17 | 2 (5) | 1 (0) | 4 (2) | 1 (2) | 2 (2) |

bookbuilder

| Kind | Notes | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- | --- |
| Coding (C1 to C3) | 18 | 9 (9) | 5 (5) | 6 (5) | 6 (6) | 4 (6) |
| Writing (W1, W2) | 11 | 4 (5) | 3 (4) | 2 (3) | 3 (4) | 3 (3) |
| Test (T1, T2) | 12 | 7 (5) | 5 (3) | 2 (5) | 2 (0) | 0 (0) |
| Architecture (A1 to A3) | 20 | 7 (7) | 5 (2) | 4 (2) | 3 (2) | 3 (1) |

On airk, Answer is ahead of Search in notes in writing (Focus 3 against 0) and
architecture (no model 4 against 2), and behind it in coding (8 and 8 against 9)
and test (Compact ties at 4). On bookbuilder, Search is ahead of every Answer
configuration in every kind.

**Against the rule, per configuration** (the better of Parse and Search by the
tie rule; airk takes Search on A1, C1, C2, C3 and T1 and Parse on A2, A3, T2, W1
and W2; bookbuilder takes Search on A2, A3, C1, C2, T1, T2 and W1 and Parse on
A1, C3 and W2):

| Configuration | Substantial wins, airk / bookbuilder | Ties at grade 0 | Worse |
| --- | --- | --- | --- |
| No model | 0 / 0 | 10 / 10 | 0 / 0 |
| Focus | 0 / 0 | 10 / 10 | 0 / 0 |
| Compact | 0 / 0 | 10 / 10 | 0 / 0 |
| Large (2 rows each) | 0 / 0 | 2 / 2 | 0 / 0 |

There is nothing for Answer to be worse than: the better of Parse and Search has
no finish on any scenario, so on airk the baseline's two "worse" rows (C1 and T1)
became ties at grade 0.

Rubric B: the better-of comparator scores 0.912 on airk (baseline 1.475) and
1.300 on bookbuilder (1.512); it fell on airk because the tie rule now takes
Parse on five scenarios. On airk every Answer configuration (1.625, 1.425, 1.413)
is above it, and no model is above Search too (1.575). On bookbuilder Search
(1.837) is above every Answer configuration (1.637, 1.538, 1.500). On the Large
rows, airk Large scores 0.938 against 1.5 for the better of Parse and Search
there, and bookbuilder Large 1.250 against 0.750.

**Search lost its two finishes on airk, and its notes.** Search covered 4 of 5
notes on C1 and on T1 and neither brief was correct on all five. On C1 both
judges found the brief right on N1 and N3, missing on N2 and N4 and wrong on N5
(it pointed the Workbench work at help text and a generator script). On T1 both
judges found N3 missing (that the defaults apply after user ignore files, so a
whitelist wins), the one note that kept it from finishing. The query replay in
section 4.5 shows the search itself did not change this.

### 4.2 What each model adds

On the same rows, from the pass summaries (baseline in brackets). Time is the mean
tool time added per session. The comparator is the better of Parse and Search per
scenario, by the tie rule above. No step adds a correct finish, because no path
has one, and no step saves a turn, because there is no shared finish.

airk

| Step | Correct finishes added | Turns saved | Time added |
| --- | --- | --- | --- |
| No model against the better of Parse and Search | 0 | 0 | +9.3 s per session (92.9 s over 10 rows); +7.3 s against Search alone. Baseline +0.5 s, and it lost C1 and T1 |
| Focus against no model (CPU) | 0 | 0 | +25.2 s (252.5 s over 10 rows); baseline +16.3 s |
| Compact against Focus (CPU) | 0 | 0 | +10.4 s (104.1 s over 10 rows); baseline +4.0 s |
| Large against Compact (CPU, A1 and C3, timing only) | 0 | 0 | +230.3 s (460.5 s over 2 rows); baseline +31.4 s |

bookbuilder

| Step | Correct finishes added | Turns saved | Time added |
| --- | --- | --- | --- |
| No model against the better of Parse and Search | 0 | 0 | +11.2 s per session (111.8 s over 10 rows); baseline +10.6 s |
| Focus against no model (CPU) | 0 | 0 | +29.8 s (297.7 s over 10 rows); baseline +44.1 s |
| Compact against Focus (CPU, timing only) | 0 | 0 | −3.7 s (−36.5 s over 10 rows); baseline −11.1 s |
| Large against Compact (CPU, C2 and A1, timing only) | 0 | 0 | +76.8 s (153.6 s over 2 rows); baseline +69.8 s |

The Answer call itself, from the sessions that ran it:

| Configuration | airk calls | airk mean | airk longest | bookbuilder calls | bookbuilder mean | bookbuilder longest | Failed |
| --- | --- | --- | --- | --- | --- | --- | --- |
| No model | 14 | 18.0 s (9.1) | 23.4 s (17.0) | 17 | 14.6 s (12.5) | 19.9 s (18.0) | 0 |
| Focus | 15 | 33.3 s (22.9) | 59.2 s (47.9) | 15 | 35.7 s (31.6) | 92.0 s (67.6) | 0 |
| Compact | 14 | 42.6 s (49.1) | 105.9 s (97.7) | 13 | 38.1 s (42.9) | 120.7 s (86.4) | 0 |
| Large | 5 | 116.4 s (66.4) | 165.2 s (66.4) | 2 | 90.8 s (87.7) | 96.7 s (103.3) | 0 |

- **No model:** on airk it has the same notes as the better of Parse and Search
  on two rows (T2, A1), more on three (C2, W1, A3) and fewer on the other five,
  15 notes against 17. On bookbuilder it never covers more than the better-of
  (14 against 27). No finish.
- **Focus over no model:** the same notes on airk (15 and 15) and on bookbuilder
  (14 and 14). Its first-call notes equal no model's on both repositories
  (4/53 and 8/61). It cost 25.2 s (airk) and 29.8 s (bookbuilder) more tool time
  per session. Focus has not earned its place.
- **Compact over Focus:** fewer notes on both repositories (10 against 15 on
  airk, 10 against 14 on bookbuilder), no finish. Compact took 165.9 s of tool
  time on A3 airk and 181.6 s on W2 bookbuilder. One Compact
  call on bookbuilder took 120.7 s, 0.7 s over the 120 s deadline.
- **Large on CPU** (measurement only; Large never runs on CPU in production):
  airk C3 and A1 covered 2 notes each against Compact's 1 and 0; bookbuilder C2
  covered 2 against Compact's 1 and A1 none. Calls took up to 165.2 s on airk,
  over the deadline, with the 180 s client timeout; none failed. Mean tool time
  per session was 296.5 s on airk.

### 4.3 Reuse

None. Every one of the 104 results is a fresh session judged by two judges. The
reuse rule of [01-baseline.md](01-baseline.md) section 4.3 needs an earlier
completed session with an identical first-call evidence digest on the same tools.
Both specs list no earlier series (`prior_series` is empty), and they turn reuse
off for Search and Parse because change set 2 changes the shared search. Neither
pass has a `reuse.json`, and `reused_b` is 0 on every configuration in both
`compare-vs-baseline.json` files.

### 4.4 The replays

No agent. Each replay re-ran the first Answer call of the ten baseline sessions of
its repository on the change-set-2 image, and scored the reply with the pass's
coverage rule. All ran on the settled indexes (tokens unchanged at the start and
end), CPU only, with the change-set budget. 10 of 10 calls returned in every
replay (2 of 2 for Large) and none returned evidence equal to the recorded
baseline call (`same_evidence` 0).

First-call replays of this change set (notes covered, with the recorded
baseline's in brackets, then the mean first-call time):

| Replay | Lifetime | airk notes | airk mean | bookbuilder notes | bookbuilder mean |
| --- | --- | --- | --- | --- | --- |
| No model | L36-cs2a | 4/53 (2), 10 of 10 calls | 21,260 ms (8,546) | 8/61 (4), 10 of 10 | 17,197 ms (13,083) |
| Focus candidate 2 | L36-cs2a | 4/53 (2) | 36,133 ms (19,082) | 8/61 (5) | 23,964 ms (22,143) |
| Compact, option B | L37-cs2b | 4/53 (2) | 31,361 ms (24,916) | 8/61 (4) | 28,257 ms (23,310) |
| Large, option B (2 rows) | L37-cs2b | 0/10 (0), 2 of 2 calls | 102,064 ms (82,070) | 1/13 (0), 2 of 2 | 93,523 ms (87,734) |

The replays that fixed the change-set definition, airk only, on a `search_v2`
lifetime (07:18 to 07:33 UTC; the summary does not name the lifetime), all 10 of
10 calls and 4/53 notes each (recorded baseline 2):

| Replay | Mean first-call time |
| --- | --- |
| No model | 21,399 ms |
| Focus, base weights (w16) | 35,427 ms |
| Focus, candidate 1 | 35,049 ms |
| Focus, candidate 2 | 27,410 ms |
| Compact, provisional rule (change set 1) | 34,386 ms |
| Compact, option B | 27,122 ms |
| No model with `context_b` on | 19,564 ms |

100-hit depth on airk (`passes/cs2/depth`, limit 100): notes in the pool under
rule 2 5 of 53 at the baseline and **8 of 53** with `search_v2`; rule 1 6 and 9;
the same-path count 18 in both. With the switch off, the depth run is identical to
the baseline (5, 6 and 18, every scenario's ranks and evidence equal). Mean depth
call time rose from 3,577 ms to 5,949 ms. A bookbuilder-v2 depth run with
`search_v2` on is not in the receipts (the bookbuilder folder under
`passes/cs2/depth` holds only runs marked invalid for truncated search).

The first-call coverage is 4/53 with or without `search_v2` (this change set's
replays against change set 1's 4/53). The primary agent's decisions note of 22
November records that the pooled notes sit at ranks 22 to 40, beyond the 16 hits
an Answer first reply pools.

### 4.5 The search replay

No agent and no model. It answers one question the pass raised: did `search_v2`
lose notes, or did the Haiku sessions of this pass simply issue different
searches? The Search queries each series recorded were re-issued on the same
airk publication (generation 330, token unchanged) under the old and the new
search, and scored with the same coverage rule (rule 2). Two arms ran on image
`cs3x-769f8f3a`: the old search on L40-cs3def (no search switch) and `search_v2`
on L41-cs3v2 (section 6.1, 6.2). Essential notes found, of the scenario's notes:

| Scenario | Notes | Baseline queries: recorded, old, new | This pass's queries: recorded, old, new |
| --- | --- | --- | --- |
| A1 | 5 | 1, 1, **0** | 1, 1, 1 |
| A2 | 6 | 0, 0, 0 | 0, 0, 0 |
| A3 | 6 | 0, 0, 1 | 0, 0, 0 |
| C1 | 5 | 3, 3, 4 | 1, **0**, 1 |
| C2 | 6 | 3, 3, 3 | 1, 2, **1** |
| C3 | 5 | 2, 2, 2 | 3, 2, 3 |
| T1 | 5 | 3, 3, 4 | 1, 1, 1 |
| T2 | 4 | 1, 1, 1 | 0, 0, 0 |
| W1 | 5 | 0, 0, 0 | 0, 0, 0 |
| W2 | 6 | 0, 0, 0 | 0, 0, 0 |
| **Total of 53** | | **13, 13, 15** | **7, 6, 7** |

- **`search_v2` found more, not fewer.** On the baseline's queries 13 became 15
  of 53 (A3, C1 and T1 gained one note each; A1 lost one). On this pass's queries
  6 became 7 (C1 and C3 gained one each, C2 lost one, T1 equal). On the
  baseline's C1 and T1 queries it found one more note each (3 to 4 on both).
- **The queries moved, not the search.** The baseline's queries find 13 to 15
  notes; this pass's find 6 to 7 under either search. The C1 and T1 finishes were
  lost because this pass's Haiku sessions searched for other things, which is
  session variance, and the sessions' briefs then fell short (section 4.1).
- **Two losses are recorded:** A1 on the baseline's queries (1 to 0) and C2 on
  this pass's (2 to 1).
- **Each arm reproduces its own series' recorded coverage.** The old search on the
  baseline's queries gives 13, equal to the recorded 13, and `search_v2` on this
  pass's queries gives 7, equal to the recorded 7, on every scenario. The old
  search on this pass's queries (6) is a counterfactual: this pass's recorded
  searches ran on `search_v2`.
- **Decision** (`search-replay/decision.json`, by the harness agent under the
  primary agent's rule: keep `search_v2` unless it loses notes on the same
  queries; the file says `decided_utc` 10:05:00 and is dated 10:03:31 UTC):
  change set 3 keeps `search_v2` for Search, Parse and Answer.

An interrupted first attempt of the `search_v2` arm (outputs dated 09:36:44 to
09:37:12 UTC, all 20 queries at 0) was stopped by the L38 close (section 6.1); it
is kept as `search_v2.attempt-1-interrupted-by-L38-stop*` and is not used.

### 4.6 GPU cells and Context leverage: pending

The receipts used for this report hold no GPU scenario replay and no Context
leverage result, so sections 4.5 and 4.6 of [02-change-set-1.md](02-change-set-1.md)
are still pending. The change-set decisions record that change set 2 uses Context
option B, which the Context trainer design names as the remedy when the leverage
is below its no-go bar; the measurement itself is not in these receipts.

### 4.7 Judge agreement

| Pass | Results with two verdicts | Equal grades | Per-note verdicts that agree | Adjudications |
| --- | --- | --- | --- | --- |
| Change set 2, bookbuilder | 52 | 52 | 93.77% of 369 pairs | None needed |
| Change set 2, airk | 52 | 52 | 90.27% of 329 pairs | None needed |
| Change set 1, airk | 27 | 27 | 92.43% of 185 pairs | None needed |
| Baseline, airk | 71 | 71 | 93.02% of 487 pairs | None needed |

Grade agreement tests little when every grade is 0. About one note verdict in ten
differs between the judges on airk (one in sixteen on bookbuilder) without
changing a grade.

## 5. What was good

- **The first call covers more on both repositories.** Answer's first call covers
  4 of 53 notes on airk (baseline 2) and 8 of 61 on bookbuilder (4 to 5); Search's
  first call covers 3 of 53 and 6 of 61 (2 and 4).
- **`search_v2` helps retrieval and does not lose notes.** Depth on airk 5 to 8 of
  53, a clean switch-off, +2 and +1 in the replays, and on bookbuilder Search 27
  against 26 and Parse 18 against 14 over the sessions.
- **Over the sessions on airk, Answer covers more than at the baseline:** no model
  15 (12), Focus 15 (11), Compact 10 (8), Large 4 of 10 (0), with fewer calls per
  session (8.0 against 8.9 for no model).
- **Rubric B rose on every Answer path on both repositories** (airk: no model
  1.625 against 1.250, Focus 1.425 against 1.275, Compact 1.413 against 1.212,
  Large 0.938 against 0.562; bookbuilder: 1.637, 1.538, 1.500 and 1.250 against
  1.438, 1.350, 1.350 and 1.062).
- **No failed Answer call in either pass**, including the calls that ran past the
  120 s deadline (section 6).
- **The passes were clean.** One publication token per repository throughout (8
  settled snapshots each), every judge verdict valid, 104 of 104 grades equal, no
  adjudication pending, no close call.
- **The harness now refuses a filter that selects nothing** (section 6.1).

## 6. What was bad

- **No new correct finish, and Search lost its two.** Zero for every
  configuration on both repositories; 0 substantial wins in 20 against the rule's
  12. The better of Parse and Search is now a tie at grade 0 everywhere, so the
  baseline's two "worse" rows on airk (C1, T1) are no longer a measurable gap.
- **Answer is still behind Search in notes** on bookbuilder in every kind and on
  airk in coding. The better of Parse and Search covers 17 notes on airk (23 at
  the baseline) and 27 on bookbuilder (27); no model covers 15 and 14, Compact 10
  and 10.
- **The first-call gain does not convert.** The first call covers twice as much on
  both repositories, yet the session notes are 15 and 14 for no model. Per the
  decisions note, the airk notes now in the 100-hit pool lie at ranks 22 to 40,
  beyond a 16-hit pool.
- **Compact covers fewer notes than Focus** (10 against 15 on airk, 10 against 14
  on bookbuilder) and fewer than it did in change set 1 on airk (10 against 14;
  the search also changed). Context option B gave no coverage gain.
- **Calls are slower, and some are over the deadline.** The no-model Answer call is
  18.0 s on airk against 9.1 s; mean tool time per session rose from 20.3 to 29.6
  s (no model), 36.7 to 54.9 s (Focus), 40.7 to 65.3 s (Compact) and 112.1 to
  296.5 s (Large) on airk. A bookbuilder Compact call took 120.7 s and an airk
  Large call 165.2 s. The Large rows ran with a 180 s client timeout.
- **Parse calls fail more on bookbuilder.** 7 of 76 Parse calls failed (baseline
  4 of 63); on airk 4 of 53 (4 of 54).
- **The pass cannot attribute an effect to one change.** `search_v2`, Focus
  candidate 2 and Context option B all changed at once, on top of change set 1's
  layers, with one session per scenario and path; only the first-call replays and
  the search replay isolate anything, and only for the first call or the search
  calls.
- **Runs invalidated or repeated along the way:** the plan's naming error, the L38
  close and the interrupted search replay (section 6.1).

### 6.1 Run notes

**1. The plan named the wrong results.** `cs2-plan.json` named the results of its
Focus, Compact and Large session groups `focus`, `compact` and `large`. The
pass's result names carry the device: `focus-cpu`, `compact-cpu` and `large-cpu`
(Search, Parse and no model have none: `search`, `parse`, `noModel`).

- On L36 the Focus step of both repositories selected zero sessions and exited 0
  after 0.2 s each (08:24:36 UTC for airk and 08:34:01 UTC for bookbuilder; both
  step logs are empty files). The plan went on and restarted into L37 at 08:34:11.
- The harness agent then made `run_pass.check_names` refuse any `--only` or
  `--only-second` name that the spec does not have ("unknown result names …; the
  spec has …"), with the test `test_only_names_must_match_a_result`. The harness
  was re-frozen at 08:35:26 UTC (`d86a78aa…` to `f78d3dc0…`; the earlier
  re-freeze at 08:08:23 UTC added `run_plan.py`). Neither changed a prompt or the
  scoring.
- At 08:44:44 UTC the plan's Compact step on L37 failed in 0.2 s with exit 1
  (`unknown result names ['compact']`), as intended, and the plan stopped. The
  corrected `cs2-plan-2.json`, with `compact-cpu`, `large-cpu` and `focus-cpu`,
  started at 08:45:00 UTC.
- Effect: Focus ran on L38 (09:13:00 to 09:27:28 UTC) instead of on L36. Both are
  A-layer lifetimes on the same image and publications, so the only effect is
  the order. Every settled snapshot matched.

**2. L38 did not close cleanly, and a one-time restart exception was needed.**

- **What happened.** At 09:36:11 UTC the plan stopped L38-cs2a2 (started 09:11:38
  UTC, Focus sessions from 09:13:00). The service exited 0 (the fixture test ran
  1,487.71 s and passed) and the fixture's own `fixture/closed.json` says
  `shutdown_completed: true` and `service_lease_reacquired: true`, with no
  restart blockers and no cancelled jobs. But the final status (revision 141,
  `commands/091138-6239-0056-final-status.stdout`) showed Focus `queued 0,
  running 1` (task `airk.focus_rank.v1`) and Context `0, 0`. `fixture.py`'s worker
  therefore raised "fixture exit or stage settlement failed" (`final_stage_idle`
  was false) and left the handle in state `ready`, without a close time. The
  harness agent attributes the running item to a no-session Search replay it had
  started on L38 (the `search_v2` arm of section 4.5), still in flight at the
  stop; the receipts show the running Focus rank task and the replay's
  interrupted outputs, not which call it was.
- **Consequence.** `prior_settled` refuses a lifetime whose final status is not
  idle as a restart source, and the plan's next restart (the B-layer restart for
  second sessions) did not run. Both pass runs' second-session steps (09:35:57 to
  09:36:11 UTC) were no-ops, because there was no close call.
- **Rule taken.** No tool call on a lifetime that a plan step may stop.
- **The exception was decided by the primary agent, both halves, under the user's
  autonomy grant. The user did not decide it.** It is one-time and covers
  exactly one lifetime.
  - *Python half* (`scripts/arbiter-answer-eval/fixture.py`; the harness, which
    is not yet committed): `UNIDLE_CLOSE_EXCEPTIONS =
    {"L38-cs2a2": "f8bacf1cd2c7a24248b4f9052b66ef4cdf7c1cc31d7f992b83e79e4311083830"}`,
    the SHA-256 of L38's `fixture/closed.json`; `unidle_close_exception()`; and
    `start --accept-unidle-close L38-cs2a2=<digest>`. `prior_settled` is
    unchanged. The exception checks, from the receipts: the named lifetime and
    digest; `closed.json` has the same instance and both closure flags true; the
    refusal was for a non-idle final stage; service exit 0; no restart blockers;
    no cancelled jobs; the pinned final-status receipt, SHA-256
    `cbde25ff97d41ad3fbf043042742dcac8b30696c1e14a0d2415399972a6997ea`, shows
    Focus queued 0 and running 1 with only `airk.focus_rank.v1` tasks, and
    Context 0 and 0; and warm completions closed. The checked record is written
    to `<new lifetime>/restart-exception.json` and to the handle's
    `restart_exception`.
  - *Rust half* (`crates/airk-service/src/service_runtime/evaluation_fixture_tests/restart.rs`):
    a one-entry `UNIDLE_CLOSE_EXCEPTIONS` of (`L38-cs2a2`, final status
    `cbde25ff…6997ea`, `closed.json` `f8bacf1c…3830`), `unidle_exception_accepts()`
    and `unidle_exception()`, and the gate becomes `settled() ||
    unidle_exception()`; `settled()` is unchanged. Conditions: both digests; that
    lifetime's own `closed.json` complete (shutdown completed, lease reacquired,
    same control root); Focus queued 0 and running at most 1; Context 0 and 0; the
    origin closure and catalog clock checks still apply; restart blockers are
    refused separately. The test
    `unidle_close_exception_admits_only_the_recorded_lifetime_digests_and_one_focus_read`
    refuses another lifetime, another status digest, another `closed.json`,
    running 2, Focus queued, Context running, shutdown incomplete, lease not
    reacquired and a foreign control root. This is evaluation fixture code, not
    product code. **It belongs to the change-set-3 commit.**
  - *Order of events.* The Python half alone was tried first: L39-cs2def (image
    `cs2-9d1b9aea`, no search switch) started from L38 at 09:43:07 UTC, the
    harness accepted the exception and wrote `L39-cs2def/restart-exception.json`,
    and the service refused in 0.03 s ("Restart closure, profiles or saved
    configuration differ", test exit 101, before readiness) because the Rust gate
    `settled()` independently required every stage at queued 0 and running 0.
    L39 holds only that refused start (state `starting`). The primary agent then
    decided the Rust half. It was built once into image `cs3x-769f8f3a` (service
    `a2d1df0a…fa7f`; `images.json` `a0fd54e4…7e86`; the `cs3-fca239fc` tree plus
    `restart.rs`; image written 09:50:32 UTC, Cargo released 09:50:40 UTC). Its
    receipts: Clippy on `airk-service` (lib and tests, all features, `-D warnings`)
    exit 0; the restart tests 11 passed, 3 ignored, including the exception test;
    the catalog-only tests 29 passed, 15 ignored.
- **Post-restart verification.** L40-cs3def started from L38 at 09:50:56 UTC under
  both halves; its handle carries `restart_exception`
  (`restart-exception.json` `b4254fb7…3d33`) and the checked record names the two
  digests above. `verify_restart.py` passed before any other call (09:52:07
  UTC): the exact tokens, airk `…:330:…dbc9da33` and bookbuilder
  `…:268:…1b153149f408…`, and three consecutive idle stage samples (09:52:21,
  :31, :41) with Focus and Context at queued 0 and running 0. The receipt
  (`post-restart-verification.json`, SHA-256 `459e276f…7061`, `passed: true`, no
  problems) is attached to the handle. L40 ran the old-search arm of the search
  replay (09:52:50 to 09:56:31 UTC) and closed cleanly at 09:56:50 UTC (the test
  passed in 354.19 s). L41-cs3v2 restarted from L40 at 09:57:09 UTC with no
  exception, ran the `search_v2` arm (to 10:03:09 UTC) and closed cleanly at
  10:10:02 UTC (772.76 s). Its stage log shows Focus enabled again from 10:05:15
  UTC for other work, which this report does not cover.

**3. Second sessions.** The plan's second-session steps ran as no-ops (0.6 to 0.9
s each, exit 0), because neither pass has a close call
(`passes/cs2-airk/second-sessions.json` and the bookbuilder one are both `[]`).
The B-layer restart they would have needed was not started.

**4. The retrieval tests.** The retrieval test log
(`change-set-2/retrieval-runtime-tests.txt`) shows 9 failed of 229 run: the 8
`cpu_source_transition` tests that [02-change-set-1.md](02-change-set-1.md)
records and `embedded_catalog_tests::selected_local_test_profiles_are_admitted_at_1_8_201_and_refused_from_1_9_0`
("selected model compiler/task source policy is stale or inconsistent"). A
separate run of that one test (`retrieval-embedded-baseline.txt`) fails the same
way; the receipts do not show whether it also fails at `1fe7b600`. That ninth
failure is not explained here and needs an owner.

## 7. Why

From the turn logs (`coverage.json` per session), the first replies, the replays
and the search replay.

- **The search did not cost Search its finishes; the queries did.** The same
  recorded queries find 15 notes under `search_v2` against 13 under the old
  search (baseline queries) and 7 against 6 (this pass's), so the airk Search
  results moved with what Haiku asked, and the grades with what its brief then
  said (C1 N2, N4, N5; T1 N3). One session per scenario cannot separate a
  swing of about 7 notes from noise; bookbuilder Search, on the same search code,
  moved the other way (26 to 27).
- **Retrieval is still the bottleneck.** `search_v2` brought airk's pool from 5 to
  8 of 53 notes at 100 hits, but Answer's first reply pools 16, and the decisions
  note puts the pooled notes at ranks 22 to 40. First-call coverage is 4 of 53
  with or without it. Change set 3's wider first reply (pool to 48) targets this.
- **Context option B did not change what Compact covers.** The decisions note
  records that option-B Compact changed the evidence on 3 of 10 first calls and
  not the coverage, and over the sessions Compact covered fewer notes than Focus.
  The roster widening is not shown to help, and the pass cannot tell Context's
  choice from the search change that came with it.
- **Slower calls come from the CPU model stages.** The mean first reply took 18.8
  s with no model, 28.4 s with Focus, 49.6 s with Compact and 91.8 s with Large
  on airk, and 15.5, 40.5, 36.8 and 90.8 s on bookbuilder. The device gate that
  change set 1 added still lets a call run past 120 s (Compact 120.7 s on
  bookbuilder, Large 165.2 s on airk). Change set 3 lists a deadline fix.
- **The agent still repeats work Answer did,** as the earlier reports found; this
  pass did not measure it again.

## 8. What happens next

**Change set 3** (in build, per the primary agent's decisions of 22 November): a
frame-aware wider first reply (pool to 48), key-word follow-up searches and a
deadline fix for Compact overruns (146.6 s and 123.6 s in change set 1; 120.7 s
on bookbuilder and 165.2 s on airk Large here). It keeps `search_v2` on for Search,
Parse and Answer, by the search replay's decision.

Also open:

- The one-time restart exception (section 6.1) and its Rust half
  (`restart.rs`) land in the change-set-3 commit with `cs3x-769f8f3a`. The
  exception names one lifetime and does not apply to any other.
- The Context leverage measurement and the GPU cells of change set 1 (section
  4.6) are still pending.
- The ninth failing retrieval test (section 6.1, item 4) needs an owner before
  the "known failing tests" line of the change-set reports is final.
- A bookbuilder-v2 depth run with `search_v2` has not been run.

## 9. Limits and receipts

**Limits.**

- One session per scenario and path on each repository. The airk Search result
  (22 to 15 notes) shows how large session variance can be.
- Change set 2 changed three things on top of change set 1's layers and is
  compared with the baseline, not with change set 1. The pass cannot attribute a
  difference to one change.
- CPU only. No GPU cell ran. Large ran on two rows per repository, timing only.
- The replays measure the first Answer call only, and the search replay measures
  search calls only: not the agent's later reads or briefs.
- The two search-replay arms ran on image `cs3x-769f8f3a`, not on `cs2-9d1b9aea`.
  `decision.json` records that its search code (`crates/airk-kernel/src/current/search*`
  and `crates/airk-store/src/current/lexical*`) is byte-identical to the
  change-set-2 image's; this report did not re-check that.
- The old-search arm on this pass's queries is a counterfactual.
- The A-layer sessions ran under prompt freeze `d86a78aa…` and the rest under
  `f78d3dc0…`; both differ from the baseline's freeze in harness files only.
- All model times are for this one CPU host. Everything ran on one Windows host.
  Nothing here is cross-platform evidence.
- Coverage is the mechanical rule 2 of [rubrics.md](rubrics.md); symbol and
  outline rows never count.
- Grade agreement is a weak test with every grade 0.
- The image is the uncommitted working tree at the time of the build (06:44 UTC):
  `cs2-9d1b9aea`'s `images.json` records 76 changed files, the `git status` and a
  diff digest `9d1b9aea…0126`. This report's commit carries the change-set-2 code
  (`search_v2` in `crates/airk-kernel/src/current/search/` and
  `crates/airk-store/src/current/lexical/ranked.rs`, Context option B in
  `crates/airk-retrieval/src/arbiter/context_wide.rs` and the kernel's `joint`
  changes). How the working tree is split between the series commits is the
  primary agent's.
- The retrieval test logs for the change set date from 01:50 UTC and show the
  failures of section 6.1, item 4; the Rust exception's tests are in the
  `cs3x-build` logs.

**Receipts.** Every path below is under the git-ignored campaign folder
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/` (R), which is
not committed.

| Receipt | SHA-256 |
| --- | --- |
| `passes/cs2-airk/results.csv` | `955508bcbd23e0626f806658aef0c4c5141ea96237fd8db920804bee581bb050` |
| `passes/cs2-airk/baseline-summary.json` | `9d271e560143a62a7cf05dc6c4aa03a5058e010b7a05172fe2f48c6d1e0b1c3a` |
| `passes/cs2-airk/summary.json` | `54d0d72525d55ee0fd7b7c10684b76b151892decb342c9f7b386801a8d41caf7` |
| `passes/cs2-airk/compare-vs-baseline.json` | `214494966b716d1c8f5760793351e27a40bb5e117a85d72650ad735fc04262c8` |
| `passes/cs2-airk/pass-report.json` | `ccb7482a2e064f40b0f6b57511d75d76bdfe6f42f7567887c316f1ddf62e2a60` |
| `passes/cs2-airk/pass-index.json` | `09f4f92cecbe14a6295e742fafb1f013de7790c57613da01c7de0a5a9dfd740a` |
| `passes/cs2-airk/digests.json` | `45981e69837b82d1dffcdb2e3037a6e52e214fec1560b3dd530412a2c58b18b5` |
| `passes/cs2-airk/spec.json` | `2de797b98c3c2356915dd60c259cf1f1a15b574154397110cdb3255602cff24f` |
| `passes/cs2-airk/settled.jsonl` | `fef676615d73358b6aa505c167bb4d232ca8fe48bb3093df8d81c101bae1bc49` |
| `passes/cs2-airk/settled-start.json` | `bc05b353204c345bdddb711802cdcaaf9dd98c945906bae20fabc978be879d36` |
| `passes/cs2-airk/stages-run.jsonl` | `3c7696323c1d18d3be10b2501406e024e7fe3e917578032f566e7ea1be5c3420` |
| `passes/cs2-airk/pass-log.jsonl` | `dd5cff95707951d6b537c493c74327b15677ac452dfb9f0ac26d1c95c6781ded` |
| `passes/cs2-airk/second-sessions.json` | `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570` |
| `passes/cs2-bookbuilder/results.csv` | `0e10a52a4d983adc585c6f065e894fb3abac2b446ba4ec8cd2d7fba1ef015591` |
| `passes/cs2-bookbuilder/baseline-summary.json` | `be71a757db7ec84e9f532278f9e4889acf83129f7d63a22d19043429628aa339` |
| `passes/cs2-bookbuilder/summary.json` | `5eb6534449f542c43b56dc0b6ccd5c3c158ccbddc016f3ce618b8ca554c7bf4b` |
| `passes/cs2-bookbuilder/compare-vs-baseline.json` | `7afd0984e6ca06f52c68a19b77a54bcf993cc4c591b0be7b37fadd0454d54ee1` |
| `passes/cs2-bookbuilder/pass-report.json` | `7a57f6f52892d657296ffb12baefb67b1230bef78532252dc0390e68af965ebb` |
| `passes/cs2-bookbuilder/pass-index.json` | `4c048e9136f36c7a0290caa451c842819f31d504c1d366e73d4948f05c3f1191` |
| `passes/cs2-bookbuilder/digests.json` | `1e736f365e06e928a1c4dafcff45577f1e3f4d8b18228b2dffe78b5b85a98e32` |
| `passes/cs2-bookbuilder/spec.json` | `1f46f51b02158129a5717c8a2080b16e68ac24b9b81e9b71a607c8db4a7f35ff` |
| `passes/cs2-bookbuilder/settled.jsonl` | `975c77f84155641cfe77f0660f276db8e9c90d5393d0f4caa0b9a0e24f884685` |
| `passes/cs2-bookbuilder/settled-start.json` | `b5a75c2dc06177c79e83b8783ba3a19c05fb1be97008eb13aa6799eeefe1acb8` |
| `passes/cs2-bookbuilder/stages-run.jsonl` | `373a8b44bf7461d983d59924403dba1d3a945f0438fcbc0a07ef48071754a77b` |
| `passes/cs2-bookbuilder/pass-log.jsonl` | `a1ed2fe3249ea134eaaecb6f5c98214666d1b7117826da632af9419f225e6174` |
| `passes/cs2-bookbuilder/second-sessions.json` | `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570` |
| `passes/cs2/depth/airk-on-vs-baseline.json` | `48bc13db7782d9bf55e36a4ec1dd8fab4f80979f24644dbbc192c2f9263e4e20` |
| `passes/cs2/depth/airk-off-vs-baseline.json` | `9b4103cd08633e62a81489ad3f8bb565d1986e397d22a967e7f8be3ae0d179a4` |
| `change-set-2/replays/airk-a/replays-summary.jsonl` | `ad041060b4fea0b97115afeaeab317fab00273e1cd3ec59b90b3b38d217fc16e` |
| `change-set-2/replays/airk-b/replays-summary.jsonl` | `040afca7998ea19228eb084b8046e74de92db50fcbeb0231b40f9757005058af` |
| `change-set-2/replays/bookbuilder-a/replays-summary.jsonl` | `06f43800f5f3721b655154f9b8e0c312fcc117575ffc14348ba1ab702ddecb7c` |
| `change-set-2/replays/bookbuilder-b/replays-summary.jsonl` | `2b49c5e115469bc23d347cf0389df13bd6f1cf3d457cd51ef978a016885c942a` |
| `change-set-2/replays/airk-search-v2/replays-summary.jsonl` | `5b78fd7559e9326c5d9a11e14a433a7d2e9e07e56dcabf5b818de2b7f66bf40b` |
| `change-set-2/cs2-plan.json` | `85182c3b5e89e285aab24015aa8f549920c5fbfd7c450c50eba9e94427547438` |
| `change-set-2/cs2-plan-log.jsonl` | `117e105e3715ff0bf447a2ce87ad1c944325f615ee468ff6ee96c0748e41e38b` |
| `change-set-2/cs2-plan.out` | `70d2656374d4ff3cf6c4fc4b6b33cbfcc912c23dcf7be649388e0634d52ad36e` |
| `change-set-2/cs2-plan-2.json` | `9ad0375cb1eafa017342d686cb2ef7964ddb030654aaf53699762a6f89265e13` |
| `change-set-2/cs2-plan-2-log.jsonl` | `08c7cf1febba026fd2fe000559ebc26431e35d2dce361c177ac573da41109016` |
| `change-set-2/cs2-plan-2.out` | `9a21902b76e62c805a5468661d714ec14125b20f655d97eac91e91a394df3358` |
| `change-set-2/plan-steps/082436-sessions-focus.log` (empty; the zero-session step on airk) | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `change-set-2/plan-steps/083401-sessions-focus.log` (empty; the same on bookbuilder) | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `change-set-2/plan-steps/084444-sessions-compact.log` (the refused name) | `2c6dee261aee15a04bde303d99fa3a9b096c13de085fc95c255ad4bf17a8694d` |
| `change-set-2/search-replay/decision.json` | `3f9918613abbba3fe193bc1f93987c44c69fe7fefc96b3d77245a3fe2b861962` |
| `change-set-2/search-replay/combined.json` | `af7cd2036dc584dc09a3b254401364bd5d628ea3ade4acf39ad177f3907d069d` |
| `change-set-2/search-replay/default.out` | `4b6e2a6603f3e1b87454689be1d1cf58dc28c6b0af263c1db72a6da92ae67003` |
| `change-set-2/search-replay/search_v2.out` | `ac80e674601b53b13ca57780cbcdde89e44754b2e795ea54f43611179f6070ec` |
| `change-set-2/search-replay/search_v2.attempt-1-interrupted-by-L38-stop.out` | `045e9205922e6adcc59782ce49691fbcf157b798eb3c15b6b2323bd80bbba8dc` |
| `change-set-2/search-replay/stages-run.jsonl` | `5c5d0512b0941d5ed4f145d50ce01b58268e28774f35a5110f82f9dfb0f83628` |
| `change-set-2/profiles/change-set-2-inputs-r4.json` | `4d058008aac20172e024e8f2236f322e0300159abbef6a5f3d425c7c4c13a7e3` |
| `change-set-2/kernel-arbiter-tests.txt` | `01f54cfeed92301444ab6ce810033b9db48222eb91125122aca09ab991ac5c30` |
| `change-set-2/kernel-nodefault-software-tests.txt` | `5cdeab1c1e6acd443cda5a6d994b2c0e5adc46f96d272928d0c44f4bf07dae50` |
| `change-set-2/retrieval-runtime-tests.txt` | `13b6a11c7413c8e74a7022e2f325d2df817ad237cbc3cabed93c75189c11cde9` |
| `change-set-2/retrieval-embedded-baseline.txt` | `42210a0555210d004ec6975f59b7719d1a12293cb85b36a13d02220e0e3794c6` |
| `fixture/images/cs2-9d1b9aea/images.json` | `23385b7e96b6326724f87a78d2857a20bce16c91d1aad036552b782a1aec53bc` |
| `fixture/images/cs2-9d1b9aea/airk-service-fixture.exe` | `7c8a931dbdf71a0ca429300970ee553ff836a028a5c35e4a37d6a062a08334b6` |
| `fixture/images/cs2-9d1b9aea/airkctl.exe` | `28d20587de020f915df42219e9cc7902eb8c4deb1fd174ca9ab0a8dd4711182d` |
| `fixture/images/cs3x-769f8f3a/images.json` | `a0fd54e4ccccc02f649200aefc60e966a4e92bdf4b794eedd3dcf6ab2b5c7e86` |
| `fixture/images/cs3x-769f8f3a/airk-service-fixture.exe` | `a2d1df0ad7c3d248dccf4157156adbf461f81452e2fd647ff6feb2a4f2d1fa7f` |
| `fixture/images/cs3x-build/clippy-svc.log` | `fc1e4be68685f23f52124764fb3d423b2c44ba7ef03b48a81c620efd58447214` |
| `fixture/images/cs3x-build/restart-tests.log` | `bb59bcc1585e9dceac9f50c699e3ab3b64deaf56af97dae03ece3cad1af5a24c` |
| `fixture/images/cs3x-build/catalog-only-tests.log` | `fdd5cb9913c188f98853d616ea9f4862a1e2a0549da878b77e2ff9a5bcdddf33` |
| `fixture/CARGO-RELEASED-CS3X.json` | `4687fadb6b8cbd77b627a3b66690cd35e078d80fe935b50ebca895f79c2f5ada` |
| `fixture/L37-cs2b/handle.json` | `47d92e9b4a8c8f5337a8a53c818f7a9600a28df93b8d80b4af03f19c84d76c6c` |
| `fixture/L38-cs2a2/handle.json` | `8124eecca8def38c1c0a7c62e5e09c86d3ed7653af2a619dac0e1c465af47bfc` |
| `fixture/L38-cs2a2/fixture/closed.json` | `f8bacf1cd2c7a24248b4f9052b66ef4cdf7c1cc31d7f992b83e79e4311083830` |
| `fixture/L38-cs2a2/commands/091138-6239-0056-final-status.stdout` | `cbde25ff97d41ad3fbf043042742dcac8b30696c1e14a0d2415399972a6997ea` |
| `fixture/L38-cs2a2/outer.log` | `1521a7823fb1cbfcc8ebd3df6f2ea8da65f88cf1fc9e5d3915c39d9af414ff6a` |
| `fixture/L39-cs2def/handle.json` | `b0788e5787f101a3b10e004fb11928417a3ff845786559e6584b2b94e132752f` |
| `fixture/L39-cs2def/restart-exception.json` | `575458c1daec4681432492b3528411e63862a1ac56fa9be7707f61833513c7c2` |
| `fixture/L39-cs2def/service.log` | `879c27238a077c4c006165c8b1a3941b11fe92ec32fe355f1e5cfff6c3ad83f1` |
| `fixture/L40-cs3def/handle.json` | `70fe032295cb78c0cd052840a1ebfd86fb93f28651f24f39d71fd800409176ac` |
| `fixture/L40-cs3def/restart-exception.json` | `b4254fb76da5ef5a10ea00872be18db5c16ccbc6d2754b9e17963018bfc13d33` |
| `fixture/L40-cs3def/post-restart-verification.json` | `459e276fa7a89ed2b8f60277d1e29be88a3e9ec1766c3e8cf4c5626fa82d7061` |
| `fixture/L40-cs3def/fixture/closed.json` | `b2fd32558fa14fad45a3462625c7a559de539323ca035b771b640367ae61057a` |
| `fixture/L41-cs3v2/handle.json` | `e807d5449054d662e5ed23a4adb111d4f5139376b99c2e6d942cf87e97182b05` |
| `fixture/L41-cs3v2/fixture/closed.json` | `0d5d2031d9de2930fb48bf2b5c871835c36dd5fccb0f1919be52b1dd126c1d48` |
| `design/search-v2.md` (as of this report) | `c158de57b96ed16c67322b94c4beb3beb04a913b9f5a07183732b05d18124ddd` |
| `design/context-trainer.md` | `b0dc0d0d2523c657b2d5db98cc45beb7259343bdf01f954012cfa470718cd76d` |
| `design/change-set-1-decisions.md` (as of this report) | `53f861637464949f1c013df8e53ee3b576dace29e36557c25bfd481b2fa6d023` |
| `design/prompts-freeze.json` (08:35:26 UTC) | `f78d3dc0300f4383ddc8239f579d1b28b2b559f374c71cbc660567f19e106b6f` |
| `design/prompts-freeze-history.jsonl` | `01a2b0f0ddacead01252a82e4fba86ac1c66b5796af4ff1ab9a3e73600c51104` |
| `scenario-freeze.json` | `0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806` |

Repository files named in section 6.1 (SHA-256 at the time of writing; the
harness files are not yet committed): `crates/airk-service/src/service_runtime/evaluation_fixture_tests/restart.rs`
`9f596158dfc79b1dee3beb07fa916f81aa4588f18ff0f346a65bf2be1177a870` (equal to the
`cs3x-769f8f3a` `images.json` record), `scripts/arbiter-answer-eval/fixture.py`
`4e8bcb62e7f03c8f8de550667929ee93221088570cf3eb80806ea826ed910014`,
`verify_restart.py`
`0497fd2bab9575336d4546a113f19ce5e4b0e181a5eb83fc8dae724fafc54c4c`,
`run_pass.py`
`925689905030902f776bc21135342b64bdc6eb5dc349e0874fc96e39dde3bac4`,
`run_plan.py`
`8976805df0f0640d8943c724e0557c687a75ca7591c553e00f1405998e06db2a`,
`search_replay.py`
`935c2338b4f865c6ff31a4683c56c572d7b0d282e130cafcb2f178bedec2466c`.

The lifetime handles can be rewritten by later restarts; the SHA-256 above are
the handles as they stood when this report was written.

**The results file.** [results/03-change-set-2.csv](results/03-change-set-2.csv)
holds the harness's fixed columns for the 104 results, 52 airk (series
`cs2-airk`) and 52 bookbuilder (series `cs2-bookbuilder`), copied from the two
passes' `results.csv` unchanged, with the same columns as
[results/02-change-set-1.csv](results/02-change-set-1.csv). No row is reused, so
`reused_from` is empty on all of them. Its `elapsed_ms` column is tool time; wall
time is in `pass-report.json`. Its SHA-256 is
`8df95742e3cd7afa22487f0ba188dd24af5b6f7222f472116b30af0d5558c25a`.

**Commands that reproduce it.** Python runs as `python -I -B` from
`D:\code\Github\airk`, as the verified human. The service lifetimes are started by
`fixture.py` as in [00-setup.md](00-setup.md) section 9, on image
`cs2-9d1b9aea`, selector `change-set-1`, inputs `change-set-2-inputs-r4.json`,
`--search search_v2` and the layers above; the plans do the restarts themselves.

The change-set run, in two parts (`R` as above; the form of the command, the
plan and log paths are the receipts'):

```text
python -I -B scripts/arbiter-answer-eval/run_plan.py --plan R/change-set-2/cs2-plan.json --log R/change-set-2/cs2-plan-log.jsonl
python -I -B scripts/arbiter-answer-eval/run_plan.py --plan R/change-set-2/cs2-plan-2.json --log R/change-set-2/cs2-plan-2-log.jsonl
```

The first stopped at its Compact step (exit 1, section 6.1) and the second
ran the rest. They run, in order, the replays of `run_cs_replays.py`
(`change-set-2/replays/{airk,bookbuilder}-{a,b}`), the session groups of
`run_pass.py` for `passes/cs2-airk/spec.json` and
`passes/cs2-bookbuilder/spec.json`, then coverage, judging, grading,
`pass_report.py` and `baseline_summary.py` for each, and
`compare_passes.py` against `passes/baseline` and `passes/baseline-bookbuilder-v2`.

The restart check of section 6.1 (`verify_restart.py`, run by the harness after
L40's start with the two expected tokens) and the search replay
(`search_replay.py`, series `baseline,cs2-airk`, repository airk, once on L40 and
once on L41, then `--combine` of the two outputs into `combined.json`) are in
`fixture/L40-cs3def/post-restart-verification.json` and
`change-set-2/search-replay/`. L40 was started from L38 on
image `cs3x-769f8f3a` with the exception flag
`--accept-unidle-close L38-cs2a2=f8bacf1cd2c7a24248b4f9052b66ef4cdf7c1cc31d7f992b83e79e4311083830`;
its handle records the checked exception.
