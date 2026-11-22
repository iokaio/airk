# 00 Setup: the evaluation service, the indexes, the harness and the pilot

Written on 22 November 2026 (UTC) from the private receipts of 21 and 22
November. It goes into the baseline's commit with `01-baseline.md`.

## 1. Summary

Before Answer could be tested, the evaluation needed its own service, two
indexes that leave the benchmark's own files out, frozen scenarios and
rubrics, and a harness that runs agents and judges and counts their turns.
All of these now exist. The private evaluation service is the product's
service runtime built from commit `1fe7b600`, hosted in a test image. The
airk index is built from an export of that commit, with the plan, the
reports, the ledger, the assignment file and the harness denied by its own
source policy; a leak probe of 20 phrases found no hit in any of them. The
first bookbuilder index was built in place, read-only. It turned out to hold
two key-setting scripts (section 2.2). Their text reached two pilot sessions,
and the session logs have been deleted. bookbuilder is now served by a fresh
index, `bookbuilder-v2`, built from a snapshot with both scripts denied
before its first pass.

Building the indexes exposed a slow indexer. Three fixes to how vectors are
built cut the measured semantic indexing time of a 1,200-file sample from
1,939.4 s to 1,066.2 s with byte-identical index contents. Diagnosis then
found a search defect. In any index with a permanent deny, every search saw
only the first 4,096 paths in alphabetical order. That affected the airk
index from its first pass, so the pilot's airk results ran on truncated
search. A query-time fix is in the image the baseline runs on,
`baseline-1fe7b600-o2-fix3`, together with the indexer fixes (section 2.3a).

A pilot of four scenarios on five paths ran end to end. It produced no
evidence about Answer, because it ran while the indexes were still being
written: 16 of the 17 Answer calls with a model stage failed when a
publication changed under them. It did change the method: passes now run
only on a settled index, coverage needs at least five overlapping lines
(coverage rule 2), agents see the product's default reply mode, and the
report of every pass says that Parse's first call returns no evidence. The
user moved agents to Haiku and judges to Sonnet, both at medium effort. A
Sonnet re-judge of the pilot agreed with the earlier Opus grades on 19 of 20
results, and on all 20 under coverage rule 2.

## 2. What changed in this series

This series built the test, not a change to Answer. Answer's own code is as
it stood at commit `1fe7b600`. The baseline image differs from that commit
in three ways:

- test-support code that lets the private service read a source root outside
  the campaign folder;
- the three indexer fixes of section 2.3, which change how fast vectors are
  built and not what is stored;
- the search roster fix of section 2.3a. This is an evaluation-setup fix: the
  denies that triggered the defect are the benchmark's own.

### 2.1 The evaluation service

The user's installed AIRK is not used. The installed product refuses the
unqualified model profiles this work uses, and the branch has moved well
beyond it. The evaluation service is the product's service runtime started
by a test selector (`actual_private_arbiter_fixture_software_v2`) from a
frozen test binary. Every image was built with rustc 1.98.0 into the shared
target `D:\code\Github\airk\target`, `--locked --offline --timings`, and is
recorded in an `images.json` manifest with its source state, environment,
build log hashes and checks.

| Image | What it is | Service SHA-256 | Build |
| --- | --- | --- | --- |
| `baseline-1fe7b600` | Commit `1fe7b600`, opt level 0 | `8f5e4e7f…115ecf` | 120.0 s; a first attempt failed in 23.4 s with `AIRK_BUNDLED_MODEL_DIR` unset |
| `baseline-1fe7b600-o2` | The same at opt level 2, plus the external read-only source root change (test support only) | `8542a597…04951f3` | 398.7 s; Clippy clean; fixture tests 13 passed, 9 ignored |
| `baseline-1fe7b600-o2-fix` | Indexer fix 1 only, built from a commit export | `bce606e0…52cc93f` | 337.0 s. Never served: see section 6 |
| `baseline-1fe7b600-o2-fix2` | Indexer fixes 1 to 3, built from a commit export | `81a94045…01aabd5` | 294.2 s. The pilot ran on it; superseded by fix-3 for the baseline |
| `baseline-1fe7b600-o2-fix3` | From the commit export: the baseline emitter change, indexer patch r3 and the search roster fix; no change-set code | `ada54197…915c988` | 346.2 s, incremental off; fixture tests 13 passed, 9 ignored. **This is the image the baseline runs on** |
| `baseline-1fe7b600-cuda` | CUDA build for the GPU track (section 2.7) | `6e3c1762…cacc284` | 402.2 s; Clippy clean |
| `cs1d-d316beaf` | The working tree at 03:37 UTC on 22 November: the indexer fixes, the roster fix and the change sets behind their switches (diff digest `d316beaf…`) | `234e8e5b…ee81502f` | 54.2 s, incremental on; fixture tests 23 passed, 15 ignored. **The change-set-1 image** |
| `cs2prep-09edaadd` | Change set 1 plus change-set-2 preparation (`context_b`, `focus_keep`, `focus_threshold`), without the roster fix | `57a81be7…2281cfdb` | 418.3 s |
| `cs2search-b128abe0` | `cs2prep` plus the `search_v2` switch, without the roster fix | `6c249334…5ff6c7e4e` | 298.8 s |

Roles. The baseline runs on `baseline-1fe7b600-o2-fix3`. Change set 1 runs on
`cs1d-d316beaf`. `cs2prep` and `cs2search` are switch-off-identical carriers
for change-set-2 work. Their new code is reached only through switches
(`context_b` and the Focus values, `search_v2`). With those switches off they
take their base's paths. For search, the unit test
`switch_off_hybrid_search_is_the_product_search` holds this byte for byte.
Neither carrier has the roster fix, and neither is used for a judged pass. The earlier change-set
images (`cs1-f5d29b41`, `cs1b-8117a9c5`, `cs1b-cuda-860fa911` and
`cs1c-a4fcb934`) predate the roster fix and are superseded.

The command-line client `airkctl` is the same binary in every image
(`28d20587…711182d`). The manifests' SHA-256 values are in section 9.

The fixture that starts and stops the service, `scripts/arbiter-answer-eval/fixture.py`
([FIXTURE.md](../../../scripts/arbiter-answer-eval/FIXTURE.md)), runs one
service *lifetime* at a time with a deadline, and restarts the same
installation from the previous lifetime's settled closure. Fourteen
lifetimes ran between 15:39 UTC on 21 November and 22:44 UTC, four of them on
a fresh installation (section 6). `L14-fix2` started at 22:44 UTC on the
fix-2 image and served the pilot. Lifetimes `L15` to `L20` followed on 22
November. The current lifetime, `L21-fix3`, started at 03:41:48 UTC on the
fix-3 image, restarted from `L20-base`, with the stage configuration empty.

### 2.2 The two indexes

**airk is indexed from a commit export.** The service refuses a source root
that contains its own control root or index storage, and the fixture keeps
both under `artifacts/` inside the airk checkout. So the index reads
`fixture/sources/airk-1fe7b6003574/`, the tracked tree of commit
`1fe7b6003574e4e385896120c62b99acbca00a3a`, exported with `git archive` and
extracted with Python's `tarfile` data filter: 10,458 files in 23.7 s, from
an archive of 334,448,640 bytes (SHA-256 `ef5eb878…f8cb84e1a`, not retained).
This also fixes the indexed bytes at the scenarios' commit while the
checkout keeps changing under parallel work.

**The benchmark is denied through the index's own source policy**, before
indexing (17:08 UTC), with no ignore file added to the repository:

| Kind | Path |
| --- | --- |
| exact | `docs/arbiter/final-plan-fp16.md` |
| subtree | `docs/arbiter/answer-evaluation` |
| exact | `docs/simplify/one-process-ledger.md` |
| exact | `docs/agent-current-work.md` |
| subtree | `scripts/arbiter-answer-eval` |

**bookbuilder was first indexed in place**, read-only, at commit
`238b22a722ceb78447d72e39585fd6a7d4d3dd0c`. Its index storage and the
control root are under the airk campaign folder, disjoint from its root. That
index is now kept only for diagnosis (`bookbuilder@original`). The
bookbuilder scenarios are served by `bookbuilder-v2` (below).

| | airk | bookbuilder (original index) |
| --- | --- | --- |
| Index id | `idx_ac4e36f1c8b265719b0ae05d28894fb0` | `idx_b2fa0bfdec8fe0fdffcfa55b05e2288e` |
| Source | Commit export of `1fe7b600`, 10,458 files | In place at `238b22a7`; 8,250 tracked files, 0 untracked and not ignored |
| Files the index lists | 10,112 | 8,119 |
| Files selected for vectors | 10,109 | 8,117 |
| Initial index job | 2,447 s, finished 17:49 UTC | 1,469 s, finished 20:14 UTC |
| Chunks and vectors reported at 00:37 UTC on 22 November | 39,086 chunks, 164,930 vectors; 7,456 of 10,109 files done, semantic state partial | 65,238 chunks, 148,840 vectors; 8,096 of 8,117 files done, semantic state partial |
| Semantic state ready, served mode hybrid | Not yet at 00:37 UTC | Yes, at 00:41 UTC |
| Index storage at 00:48 UTC | 3,260,018,566 bytes | 2,001,177,122 bytes |

Both indexes use the bundled MiniLM model (`minilm-l6-v2-u8-v3`). Watching
is off. The indexes are not rebuilt between passes; the image switches of
section 2.3 restarted the service on the same installation and the same
indexes.

**Credential check.** Before bookbuilder was indexed, a precheck listed its
files with `git ls-files` (with `GIT_OPTIONAL_LOCKS=0`, no file contents
opened) against a credential name pattern (`aikeys.json`, `.env*`, `*.pem`,
`*.key`, `*.pfx`, `*.p12`, `secrets.json`, `credentials.json` and local or
secret `appsettings`). No tracked or untracked-unignored file matched. The
repository's `aikeys.json` exists, is untracked, and is ignored by line 428
of its `.gitignore`. After indexing, a scope check listed the index's 8,119
files (121 pages, no contents opened): `aikeys.json` is not listed, no file
matching that pattern is listed, and no build-output path is listed. For
airk, the export holds tracked files only; I found no separate credential
scan receipt for the airk index.

**That check missed two key scripts.** Its pattern did not match key-setting
scripts. A wider probe at 01:21 UTC on 22 November
(`fixture/credential-probe-bookbuilder.json`) listed the index's 8,119 paths
at publication 265, by name only. It matched
`ironspire/scripts/set-rss-key.ps1` and
`ironspire/scripts/set-sendgrid-key.ps1`, both tracked and indexed. The
other six matches are code that names secrets, and no `aikeys.json`,
`appsettings*.json`, `.env*` or `*.pfx` file was listed. The two scripts were
never opened by any agent. The security handling is described below.

**Security handling.** An exposure scan (`security/exposure-scan.json`,
02:03 UTC, paths and byte counts only, nothing printed or stored) searched
the session logs, passes and fixture records for text from the two scripts.
It found 360 bytes in four spans:

- two `airk_references` results returned single matching lines, 180 bytes
  each, in two pilot sessions: bookbuilder C2 on no model and on Focus, run
  with `claude-sonnet-5-5` from 22:46 to 22:54 UTC on 21 November;
- the same text was in those two sessions' transcripts, so it was sent to the
  model provider as tool results.

No judge packet and no brief contained it. No baseline session or Haiku
cost-pilot session was affected. On the user's authority of 22 November, the
two session folders were deleted at 02:46 UTC (`security/deletion-receipt.json`:
22 files and 639,124 bytes, and 23 files and 564,998 bytes). The pilot's
results, summary and grades, which hold no evidence text, were kept. A
rescan at 02:47 UTC (`security/exposure-scan-after-deletion.json`) found no
remaining text in 208 result files. Earlier, at 02:06 and 02:10 UTC, both
scripts had been denied by exact path in the original index (publications
266 and 267). A probe at 02:12 UTC
(`fixture/credential-probe-bookbuilder-after-exclusion.json`) listed 8,117
paths, without the scripts. **The user is
advised to rotate both keys**, because their text left the machine. This
report quotes none of it.

**The fresh bookbuilder-v2 index.** The original index's later publications
were affected by the search roster defect of section 2.3a. Because of that,
bookbuilder is now served by a new index, `bookbuilder-v2`
(`idx_27adb04a6fee60a70005f1502e4b5e45`, alias decision at 02:43 UTC in
`fixture/index-aliases.json`). It is built from a snapshot, not in place:
`git archive` of the same commit `238b22a7`, extracted with the `tarfile`
data filter, 8,250 files (`fixture/sources/bookbuilder-v2-238b22a722ce.json`).
The two scripts were denied by exact path at 02:41:33 and 02:41:35 UTC, before
the first index pass. That pass took 1,616.8 s and finished at 03:09:10 UTC.
At the start of `L21-fix3` its semantic state was still partial.

**Leak probe** (`fixture/leak-probe-1.json`, 19:23 UTC on 21 November,
lifetime `L9-index`). One distinctive phrase from each of the 20 frozen airk
instructions (the 10 development and the 10 sealed scenarios) was searched
in the airk index. Every phrase returned between 11 and 16 hits, and none
came from an excluded path. A full listing of the index (10,112 files, 135
pages) found none of the excluded paths indexed. The probe passed. It ran
once, while vectors were still being built; the index has not been rebuilt
since, so no later probe was required, and none has run.

### 2.3 Indexing speed: an indexer defect found and fixed

Vectors were being built slowly. Under the unfixed opt-level-2 image, from
21:37 to 22:03 UTC, the live service built 19,867 vectors an hour for airk
and 20,511 for bookbuilder, on about 1.9 CPU cores.

A diagnosis on samples of the airk export (300 and 1,200 files), timed with
the kernel's own `vector_performance` harness, found three causes. Each was
fixed in the private evaluation build:

1. **Inference and graph insertion ran one after the other.** Semantic
   preparation now runs inference of the next 32-unit batch while a helper
   thread inserts the previous batch into the DiskANN graph. Registry
   decisions, vectors, insertion order and occurrences are unchanged
   (`crates/airk-kernel/src/current/update.rs`).
2. **The proof cache was capped at 256 entries.** On the 1,200-file sample
   it evicted 9,054,736 times and read 9,192,670 base records. It is now
   bounded only by its existing 8 MiB payload bound
   (`crates/airk-store/src/current/graph.rs`).
3. **A service vectors turn published one batch with a 30-second soft
   deadline.** It now publishes up to 8 batches with a 120-second soft
   deadline. Cancellation and pause are still checked at checkpoints, and
   source hints are still honoured at batch boundaries
   (`crates/airk-kernel/src/service_enrichment.rs`).

The affected documents (`docs/current-index-runtime.md`,
`docs/service-jobs.md`, `docs/service-resources.md`,
`docs/service-vector-regeneration.md` and `docs/service-freshness.md`) were
updated with them. The patch is
`vectors-diag/indexer-fixes-r2.patch` (SHA-256 `7e11b481…f55f22843`).

**Measured gain**, semantic time on the 1,200-file sample (1,200 files,
21,796 vectors):

| Build | Semantic seconds | Graph ms per insert | Base record reads |
| --- | --- | --- | --- |
| Unfixed | 1,939.4 (a repeat: 1,953.3) | 40.96 | 9,192,670 |
| Fix 1 | 1,326.2 | 40.95 | 9,192,670 |
| Fixes 1 and 2 | 1,066.2 | 19.2 | 2,651,197 |

That is 45% less time with fixes 1 and 2. Fix 3 is not exercised by this
harness, which indexes through `Kernel::index` and not through service
turns; its gain is projected from per-job overhead and was not measured on
its own.

On the live service after the switch to the fix-2 image, from 22:07 to 22:44
UTC, the rates were 29,646 vectors an hour for airk and 65,858 for
bookbuilder, on about 2.3 cores. The two windows covered different file
ranges, so this is not a controlled comparison.

**Byte-identical proof.** The fixed and unfixed builds produced the same
index contents:

- 300-file sample: the same 5,814 graph records (identity, embedding key,
  liveness, record bytes with vectors and adjacency), 676 lexical documents,
  297 artifact objects, 297 source objects and 1 embedding pack.
- 1,200-file sample with fixes 1 and 2: the same 21,797 graph records, 2,664
  lexical documents, 1,170 artifact and 1,170 source objects and 1 embedding
  pack as the unfixed and fix-1 runs.
- Only the graph Merkle value and map buckets differ, and they differ the
  same way between two unfixed runs, because each collection gets a random
  scope incarnation.

**Checks of the fixes.** Clippy with `-D warnings` was clean (airk-kernel
with all features and with no default features, airk-store with all
features), and rustfmt and `git diff --check` were clean. Tests:

- airk-store `current::`: 176 passed. This includes a new test that the
  proof cache is bounded by payload bytes; with the 256 cap restored it
  fails.
- airk-kernel `current::` and `service_enrichment`: 43 passed, 1 ignored.
- `current_runtime`: 18 passed, 1 ignored. This includes a new test that
  pipelined batches share aliases across batch boundaries; it fails with
  the pending-key guard disabled.
- `service_enrichment`: 1 passed. It was adapted: a hinted turn stays
  partial, and an unhinted turn drains 39 inputs over 2 batches.
- `current_foundation`: 12 passed, 1 ignored.
- `vector_performance --ignored`: 3 passed.

**Product caveat for issue #64.** Issue #64 is that new and edited files are
not searchable while a vectors job runs. Fix 3 makes each vectors turn
longer: up to 8 batches and 120 seconds in place of 1 batch and 30 seconds.
When a source hint arrives, the turn still yields at the next batch
boundary. When no hint arrives, as with watching off, source reconciliation
waits behind a longer turn, so the window in which an edit is not yet
searchable can be wider than before. That is acceptable for a fixed
evaluation index, which is not edited during a pass. Before the fixes reach
the product they need a recorded decision that weighs this against #64.
They are uncommitted in the shared checkout and are not digest inputs.

The fix-3 image carries indexer patch r3 (`vectors-diag/indexer-fixes-r3.patch`).
Patch r3 is r2 plus pause handling: a paused multi-batch turn ends at a batch
boundary and returns admission instead of parking with its writer and model
(`crates/airk-kernel/src/current/enrichment.rs` and
`crates/airk-kernel/src/service_execution.rs`).

### 2.3a The search roster defect, found and fixed

**What it is.** If an index's source policy holds any permanent deny,
kernel search (`filtered_paths` in `crates/airk-kernel/src/current/search.rs`)
built a bounded roster of allowed paths. That roster holds at most 4,096 paths
(`MAX_SCOPED_PATHS`), taken in alphabetical order. Every search was then
limited to those first 4,096 paths, and the only sign was `truncated` in the
reply. The diagnosis agent found it on 22 November.

**Which indexes it affected.**

- The airk index, from its first pass: the benchmark's denies were applied
  before indexing (section 2.2).
- `bookbuilder-v2`, which also has denies.
- The original bookbuilder index, from publications 266 and 267, when the key
  scripts were denied.

The original bookbuilder index was unaffected up to publication 265, before
any deny. That covers the bookbuilder pilot, the bookbuilder depth measurement
and `baseline-bookbuilder-r2` (publication 265). **All of the pilot's airk results ran on truncated
search.** Each run below carries a `TRUNCATED-SEARCH.json` marker and is not
comparable evidence:

- the pilot's airk rows and its airk probes;
- on bookbuilder publication 267: the change-set-1 no-model replay
  (`change-set-1/replays/bookbuilder/all-noModel`), the stopped pass
  `baseline-bookbuilder-r3`, and two change-set-2 depth runs.

The `INVALID-FOR-COMPARISON.json` note for those publication-267 runs gives
the withdrawn vector-loss reason (below). Truncated search is the valid
reason.

The leak probe's 20 searches also ran on truncated airk search. Its full
listing of the index does not go through search, and it still shows no
excluded path.

**The fix.** A one-line, query-time change. Only a selected scope that omits
sources of the publication, or an explicit filter, builds a roster now. A
deny is still enforced by its removal publication, because reads are admitted
only after it is applied, and by the per-path checks of every search branch.
No reindex is needed. A new test,
`permanent_deny_searches_the_whole_retained_corpus_not_a_first_path_page`
(`crates/airk-kernel/tests/source_policy.rs`), indexes more than 4,096 paths
before its target. It checks that vector, hybrid and lexical search still
find a late retained path after a deny and never return the denied file. The
patch is `trim-diag/policy-search-roster-fix.patch`. The denies that
triggered the defect are the benchmark's own, so the fix is an
**evaluation-setup fix**. It is included in the baseline image
(`baseline-1fe7b600-o2-fix3`) and in the change-set image (`cs1d-d316beaf`).

**Product impact.** In the product the defect affects only indexes whose
source policy restricts paths: those with trims, split transfers or positive
partitions. Ordinary credential and `.gitignore` exclusions are applied at
discovery, not through a deny, so they do not trigger it. Product adoption is
recommended. `01-baseline.md` reports the fix with its test.

**A withdrawn reading.** An earlier reading, that the policy trim had lost
vectors, is withdrawn. The trim lost no vectors. Units with identical text
share one vector, which made the counts look short, and readiness was
correct.

### 2.3b A search finding: lexical search is conjunctive

Diagnosis for change set 2 (`design/search-v2.md`) found why "hybrid" search
does little for a prose instruction. The product lexical branch joins every
query term with `AND` and orders the matches by stable id, which is a hash,
not relevance (`crates/airk-store/src/current/lexical.rs`). Materialization
re-checks the same conjunction. A 40-to-70-word instruction matches no chunk,
so hybrid search of an instruction is the vector ranking alone, and vector
ranking of prose favours prose.

The bookbuilder depth measurement (`passes/baseline/depth/bookbuilder/depth.json`,
publication 265, before any deny, so the roster defect did not affect it) shows
the effect. It used 100-hit searches of the 10 instructions with
`max_bytes` 65,536. Every reply was truncated by its byte frame at 59 to 84
hits. Of 61 essential notes:

- 10 were covered under rule 2;
- 14 more had their file in the pool but not the noted region;
- 37 had no file in the pool.

(The design note counts 24 with the file in the pool; that count includes the
10 covered.) This is a product property, not a defect of the evaluation. Change
set 2's `search_v2` switch, which is off in the baseline, adds ranked any-term
lexical branches for prose queries.

### 2.4 The scenarios and their freeze

The 20 development scenarios, ten per repository in the plan's mix, are in
[scenarios.md](scenarios.md), with their instructions verbatim. Each has
between 8 and 10 reference notes (180 in all, 114 essential), written by
reading the repositories directly, never through the tools under test. A
sealed set of 20 more was written at the same time and has not been opened.

All 40 files were frozen at 15:56:26 UTC on 21 November in
`scenario-freeze.json` (SHA-256
`0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806`), with
the airk commit `1fe7b600` and the bookbuilder commit `238b22a7`. Every pass
checks each file against the freeze. No correction has been made since.

### 2.5 The harness

The harness is in [scripts/arbiter-answer-eval/](../../../scripts/arbiter-answer-eval/README.md)
(Python 3.13, standard library only, with its own tests on synthetic data).
It is excluded from the airk index. In outline:

- **The agent** is a fresh headless Claude Code session (the VS Code
  extension's binary, version 2.1.330), started in an empty working folder
  with `--setting-sources ""`, so it loads neither the repository's
  instructions nor the user's memory. Its only MCP server is a wrapper
  (`mcp_wrapper.py`) that offers the path's tools and nothing else. Built-in
  file, shell and web tools are denied. The child environment drops every
  `AIRK_*` variable.
- **The wrapper** injects the index id, enforces the first call (the
  instruction, unchanged, to `airk_answer`, `airk_search` or `airk_parse`)
  and the 12-turn cap, and logs every call: tool, arguments, bytes, time and
  the raw result with its SHA-256. For Answer it sets the plan's budget: 8
  calls, 3 rounds, fan-out 8, 8,192 input tokens, 32 candidates, 262,144
  bytes and 120 s.
- **The audit** discards a session that used or was offered anything outside
  the wrapper, checks that every `path:start-end` citation in the brief
  overlaps a logged result, and checks that the stream's tool calls match the
  wrapper's log. A discarded session is rerun once.
- **Coverage, judging and grading** follow [rubrics.md](rubrics.md).
- **Reuse.** A session is reused, not run again, when its path's canonical
  first call gives an evidence digest identical to an earlier completed
  session with the same tool set, the same agent model and the same effort.
- **The settled guard** (`settled.py`) refuses a judged pass unless the
  index is semantically ready, served in hybrid mode and free of index and
  vectors jobs, and invalidates the pass if its publication token changes.
- **The prompt freeze.** Every judged pass names
  `design/prompts-freeze.json` (SHA-256 `ef4dc528…428737ad7`, 00:40:15 UTC on
  22 November), which pins the three agent prompts, the judge prompt and its
  schema, the default models and efforts, and the hashes of 17 runtime
  harness files. `run_pass.py` refuses to run if any of them differs. The
  hashes are in [rubrics.md](rubrics.md). The citation is of that date: the
  freeze was re-made after it for harness changes, and the baseline ran under
  `b4044529…` (see [rubrics.md](rubrics.md)).

### 2.6 The models, as the user changed them

The plan first set Sonnet (`claude-sonnet-5-5`) agents and Opus
(`claude-opus-5-5`) judges. During execution on 21 November the user changed
this, to save cost, to **Haiku agents** (`claude-haiku-5-5`) and **Sonnet
judges**, both at `--effort medium`. The question therefore becomes whether
Answer helps a smaller agent. On the airk baseline only, the no-model
Answer control, Answer with Focus and the better of Parse and Search are
also run with a Sonnet agent, as a reference.

The user asked that Sonnet first re-judge the pilot's packets and that any
disagreement with the pilot's Opus verdicts be reported. `rejudge.py` ran
two Sonnet judges on the 20 identical pilot packets
(`passes/pilot/rejudge-sonnet.json`):

| Measure | Result |
| --- | --- |
| Invalid verdicts | 0 of 40 |
| Adjudicated grade agrees with Opus, coverage rule 1 | 19 of 20 |
| Adjudicated grade agrees with Opus, coverage rule 2 | 20 of 20 |
| Per-note verdicts that agree | 94.35% of 460 pairs |
| Mean Rubric B difference, Sonnet minus Opus | −0.056 |
| Cost | $1.98 for 20 results, about $0.10 a result |

The one disagreement is airk C1 on Answer with Focus. Under rule 1 both Opus
judges gave it 3, a correct finish at turn 2, and both Sonnet judges gave it
1, one fact short. Under rule 2 the evidence covers only two of its five
essential notes, and every judge's grade is 0. Per-note verdicts also
differed on four other results (airk A1 Search, airk C1 Parse, bookbuilder
A1 Parse, bookbuilder C2 Compact) without changing a grade. This is reported
for the user's decision, as the plan requires; the frozen judge is Sonnet.

What the switch saves, from the pilots: Sonnet agents cost $4.59 for 20
sessions ($0.19 to $0.29 a session by path), Haiku agents $0.23 for 12
sessions ($0.017 to $0.023 a session). Opus judging of the pilot cost $4.47.

### 2.7 The GPU track

The plan wants each Answer configuration on CPU and on the RTX A6000 (CUDA).

- **What ran.** A CUDA image (`baseline-1fe7b600-cuda`) with the reviewed
  CUDA runtime payload, and a GPU fixture (`gpu_fixture.py`) whose selector
  opens the CPU fixture's own installation and adds one CUDA Context profile
  to the evaluation catalog, reading its weights from pinned files without
  copying them. On a 7-file synthetic repository (not the evaluation
  indexes), the no-model and Focus-only calls returned 8 records, and the calls
  with Context on CUDA returned 7:

  | Lifetime | No model | Focus on CPU | Focus on CPU, Context on CUDA | GPU memory used |
  | --- | --- | --- | --- | --- |
  | Compact | 388.2 ms | 15,244.7 ms | 16,958.2 ms | 9,016 to 9,022 MiB |
  | Large | 377.9 ms | 15,369.1 ms | 18,022.7 ms (warm: 17,391.7 ms) | 17,164 to 17,174 MiB |

  Every Context assessment ran on `onnxruntime-cuda/1.30.0` and abstained,
  as Context does today under a calibration with no rules.
- **What did not.** Restarting a Large lifetime on the Compact lifetime's
  installation was refused ("Restart closure, profiles or saved
  configuration differ", exit 101), so Large ran on a fresh synthetic
  installation. No CUDA lifetime has yet run on the evaluation indexes.
- **Focus on CUDA is blocked.** There is no CUDA Focus route: in every GPU
  lifetime Focus runs on CPU. The plan listed this as its least certain gap.
- **The change-set-1 GPU cells were blocked before any build.** A CUDA
  Context profile with the provisional read rule needs a reviewed graph
  result naming its new manifest, and Large also pins that result's SHA-256
  in a digest-input file. Option A, approved at about 00:45 UTC on 22
  November, resolves this in the private build. The bounded-hybrid gate
  admits a variant that differs from the reviewed CUDA base only in its
  calibration and identity, and it checks graph, placement and adapter
  against the base. This changes the gate's validator identity, which needs
  a recorded decision before any product use. The variants were emitted with
  image `cs1b-cuda-860fa911`, which predates the roster fix. I found no
  receipt of a change-set-1 pass on CUDA.
- **Large on CPU keeps the 120-second product cap.** The plan's
  evaluation-only 180-second allowance was not built. Large on CPU runs on
  two airk scenarios (A1 and C3) as a timing measurement under the 120 s
  request deadline, with a 180 s client timeout.

### 2.8 The training track

The plan trains Focus weights on pairs mined from both repositories
(`R/training/`, private).

- **Pairs.** `scripts/arbiter/training/answer_eval_pairs/mine_pairs.py`
  mined three shapes (a commit's message with the regions it changed, a
  documentation section with the code it names, a test with the code under
  test) from commits before 15:00 UTC on 21 November. It left out the
  benchmark's paths, credential and generated files, and every pair whose
  evidence overlaps a judged or sealed scenario's notes. A hash-chosen 15% is
  held out.

  | Repository | Commit pairs, train / held out | Documentation | Test | Total |
  | --- | --- | --- | --- | --- |
  | airk | 618 / 94 | 1,182 / 195 | 7,203 / 1,287 | 9,003 / 1,576 |
  | bookbuilder | 442 / 88 | 1,201 / 240 | 5,816 / 989 | 7,459 / 1,317 |

- **Focus candidate 1.** A LoRA candidate of the Qwen3 Reranker 0.6B Focus
  model was trained on the A6000 on 13,403 rows from both repositories
  (4,642 positive, 8,761 negative), at width 1,024, with the software
  instruction. The owned run took 5,221.8 s and exited 0, with peak host
  memory of 12,772,188,160 bytes. Merging changed all 112 attention
  projections by a mean relative 0.35% (max 0.55%). The candidate was
  exported to the evaluation's static-1024 CPU profile shape (weight file
  1,117,980,672 bytes, SHA-256 `787ec07b…`).
- **Its scores** are Focus-level, pair by pair, and not yet Answer results:

  | Rows | Base AUROC | Candidate AUROC | Base hit@1 | Candidate hit@1 |
  | --- | --- | --- | --- | --- |
  | Held out by pair, both repositories (1,213 rows) | 0.623 | 0.919 | 0.267 | 0.896 |
  | Held out by path, evidence files never trained on (1,157 rows) | 0.583 | 0.905 | 0.215 | 0.851 |
  | Training sample (1,171 rows) | 0.613 | 0.935 | 0.242 | 0.868 |
  | Earlier development rows, existing ability (85 rows) | 0.543 | 0.597 | — | — |

  The memorization signal (the candidate's train-minus-held-out gap, less
  the base model's) is +0.026 AUROC by pair and −0.0003 by path, and within
  ±0.07 for hit@1 in each repository: below the gap the training plan
  proposed to call wide. On the 85 earlier rows the mean margin of
  positives fell from 4.11 to 0.50, which is worth watching.
- A second candidate was being trained when this report was written (the
  A6000's owner since 00:43 UTC on 22 November). The existing pilot
  checkpoint has not yet been compared in Answer.

## 3. What was tested

Only the pilot was run as a pass in this series.

| Item | Value |
| --- | --- |
| Scenarios | airk C1 and A1, bookbuilder C2 and A1 |
| Paths | Search, Parse, Answer with no model, Answer with Focus (CPU), Answer with Focus and Compact (CPU) |
| Agent | `claude-sonnet-5-5`, Claude Code 2.1.330 |
| Judges | Two `claude-opus-5-5` judges per result; later re-judged by two `claude-sonnet-5-5` judges |
| Image | `baseline-1fe7b600-o2-fix2`, lifetime `L14-fix2` |
| Index state | Lexical search served; vectors still being built (accepted for the pilot only) |
| Focus profile | `focus-static1024-cpu` (manifest `94c65d0c…`) |
| Compact profile | `context-compact-cpu` (manifest `6e7520aa…`) |
| Sessions | 20, from 22:52 UTC on 21 November; graded at 23:09 UTC, rescored under rule 2 at 23:24 UTC |

A second, Haiku pilot of the same four scenarios on Search, Parse and no
model ran from 23:25 to 23:29 UTC, with one Sonnet judge, to check the
plumbing and cost of the new models. It was marked invalid (section 6) and
is not a result.

## 4. Results

The pilot is not evidence about Answer: it ran while both indexes were being
written, and most of its model-stage Answer calls failed. Its results are
given so that the reader can see what the harness produced and why the
method changed.

### Against Parse and Search

Adjudicated grades under coverage rule 2 (rule 1 in brackets where it
differs). No result had a correct finish under rule 2. Under rule 1, airk C1
on Answer with Focus had one, at turn 2.

| Scenario | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- |
| airk C1 (coding) | 0 [1] | 0 | 0 [1] | 0 [3] | 0 [1] |
| airk A1 (architecture) | 0 | 0 | 0 | 0 | 0 |
| bookbuilder C2 (coding) | 0 | 0 | 0 | 0 | 0 |
| bookbuilder A1 (architecture) | 0 | 0 | 0 | 0 | 0 |

- No path covered any essential note with its first call: the first-call
  yield was 0 on all 20 results, under both rules.
- Essential notes covered by the end of the session, under rule 2: Search
  1/5, 0/5, 2/5 and 1/8; Parse 2/5, 2/5, 0/5 and 0/8; no model 1/5, 2/5, 0/5
  and 0/8; Focus 2/5, 0/5, 0/5 and 1/8; Compact 1/5, 0/5, 0/5 and 0/8 (in
  the table's scenario order).
- Rubric B, mean of the four questions over the four scenarios: Search 1.25
  and 1.31 (judge 1 and judge 2), Parse 0 and 0.06, no model 1.00 and 0.94.
  Focus and Compact scored 0, because their first calls failed and there was
  no reply to read.
- Mean wall time per session: Search 34.1 s, Parse 35.2 s, no model 51.4 s,
  Focus 147.2 s, Compact 162.8 s. Mean turns: 8, 5.5, 9.25, 9.25 and 8.75.
  Mean bytes returned per session: 112,778, 100,511, 147,898, 87,810 and
  78,963.
- Headroom: all four scenarios have room for a substantial win, since no
  comparator finished. Four is not a count that the rule of section 4.3 can
  use; the baseline counts it on all scenarios.

### What each model adds

On the four rows, under rule 2, no step added a correct finish or saved a
turn. The time each step added per session, on average: no model over the
better of Parse and Search, 13.2 s; Focus over no model, 98.4 s; Compact
over Focus, 15.6 s. The Focus and Compact times are mostly calls that ran
for up to 109 s and then failed.

### Reuse

No session was reused.

## 5. What was good

- **The benchmark is out of its own index.** Twenty leak-probe phrases, none
  with a hit from an excluded path, and no excluded path in the index
  listing.
- **The key-script exposure was found, bounded and closed without opening
  either file.** Scans worked by path and byte count only. The two
  affected session folders were deleted on the user's authority. The
  scripts are denied in both bookbuilder indexes, and in `bookbuilder-v2`
  before its first pass.
- **The search roster defect was found before any judged airk pass**, and
  it was fixed at query time with a regression test, without a reindex.
- **The indexer fixes** cut semantic indexing time by 45% on a 1,200-file
  sample with byte-identical contents, and are covered by new tests that
  fail when the fixes are undone.
- **The harness worked end to end.** All 20 pilot sessions completed, none
  was discarded, and all 40 judge verdicts were valid. One citation, in the
  airk A1 brief on Answer with Focus, was not backed by a logged result,
  and the audit flagged it. Across the briefs, 31 bare line references (a
  range without its path) could be counted but not audited; the frozen
  agent prompt asks for the full path with every range.
- **The pilot caught four problems before they could reach a judged pass**
  (section 7): an unsettled index, single-line coverage, the reply mode and
  Parse's empty first call.
- **Sonnet judges agree with Opus judges** on every grade once coverage
  rule 2 applies, at less than half the cost.
- **Focus candidate 1** separates evidence from non-evidence far better than
  the base model on held-out pairs, including pairs whose files it never
  saw, with no sign of memorization.

## 6. What was bad

- **Most pilot Answer calls with a model stage failed.** Of 17 Answer calls
  with Focus or Compact, 16 failed with `read_failed` ("joint source
  authority/publication changed before delivery"), after up to 109 s each.
  The no-model control's 5 Answer calls all succeeded. The pilot therefore
  says nothing about Focus or Compact.
- **The Haiku pilot was invalid.** In its four minutes both indexes
  published new revisions (airk from publication 226 to 228, bookbuilder
  from 202 to 206), and the settled guard marked the pass invalid.
- **Lifetimes got stuck, and fresh installations cost disk.** Fourteen
  lifetimes ran; four needed a fresh installation, about 24 GB each (the
  current one, `L8-fresh`, holds 24,162,723,690 bytes). The causes, from the
  lifetimes' handles:
  - a `verify` job left pending at shutdown made `L7` unrestartable;
  - a project still in setup was refused by the restart guard (`L4`);
  - a configure job was canceled (`L2`);
  - a lifetime expected exactly one airk source root (`L5`);
  - the service exited before readiness in `L3` and `L6`;
  - `L11` failed because the fix-1 image was built in a commit export,
    where the image's artifact root resolved inside the export, not to the
    real `artifacts` folder (os error 2). The `-o2-fix` image was never
    served. The fix-2 image was built from the same export after a junction
    to the real folder was added.
  - After the `L12` restart the service did not resume bookbuilder's
    vectors for 16 minutes, and one index pass was queued by hand.
- **The campaign overran its disk budget.** The primary agent recorded an
  overrun of about 30 GB on D:. I found no receipt stating that figure. The
  lowest free-space reading in the receipts I read is 1,055,823,269,888
  bytes at 16:49 UTC on 21 November, 19.9 GB below the 1,075,698,991,104
  bytes of the 15:19 UTC reading in the common brief. Duplicate copies of
  model weights were then reclaimed, each authenticated by SHA-256 against
  a retained original first:
  - 24,090,079,232 bytes from the retired calibration installation (15:55
    UTC);
  - 72,239,800,320 bytes from older candidate installations (16:49 UTC);
  - 18,423,260,952 bytes from the GPU track's synthetic installations and
    a copy of the Large weights (16:42 UTC).

  Free space on D: was 1,084,452,188,160 bytes at 00:48 UTC on 22 November.
- **Two `git status` calls were run in bookbuilder**, which is read-only for
  this campaign. I found no receipt naming them. `git status` can rewrite a
  repository's index file. bookbuilder's `.git/index` was last modified on
  28 October 2026 and its `HEAD` on 15 October, so neither call changed
  them. Later git queries in bookbuilder use `GIT_OPTIONAL_LOCKS=0`.
- **Eight failing tests in change set 1.** The change-set-1 image fails 8
  `cpu_source_transition` tests in airk-retrieval, because
  `crates/airk-retrieval/src/arbiter/runtime.rs`, which change set 1 edits,
  is a pinned source of the legacy release CPU Focus transition. They are a
  known failing check of the private evaluation build, owned by the primary
  agent. They are not re-pinned or weakened. Before any of change set 1
  reaches the product, its recorded decision must remove that routing or
  add a reviewed source row. The change-set-1 report carries this.
- **The CUDA Context reply differs from the Focus-only reply on the
  synthetic repository, for a known reason.** The no-model and Focus-only
  replies have the same evidence digest (`c2aaa39a…`, 8 records, 131,785
  bytes with Focus). The Compact and Large CUDA Context replies share
  another (`ef745835…`, 7 records, 135,625 to 135,627 bytes), although
  every Context assessment abstained. The cause is not the device. An
  enabled Context stage halves Focus's token allowance, so Focus keeps 7
  records against 8. The difference comes from having a Context stage, not
  from running it on CUDA. A CPU and a GPU cell with the same stages should
  therefore gather the same evidence. That still needs one run of each on
  the evaluation indexes.
- **The ONNX export of Focus candidate 1 was flagged.** Against PyTorch on
  24 rows, the largest raw logit error was 0.265 and the largest score error
  0.026. That is outside the parity check's tolerance, as was the
  unmodified base model (0.353 and 0.018).
- **The leak probe ran once, while vectors were being built and on
  truncated search** (section 2.3a), not on the finished index. Its index
  listing is unaffected.
- **Two key scripts were indexed and their text left the machine.** The
  bookbuilder credential check matched names only, and its pattern missed
  `set-rss-key.ps1` and `set-sendgrid-key.ps1` under `ironspire/scripts/`.
  360 bytes of their text, as single lines returned by `airk_references`,
  reached two pilot sessions and the model provider (section 2.2). The
  session folders were deleted on the user's authority. The user is advised
  to rotate both keys.
- **Every search of the airk index ran on its first 4,096 paths** until the
  roster fix (section 2.3a). The pilot's airk rows are not comparable
  evidence. The publication-267 bookbuilder runs were first stopped for the
  vector-loss reading, which has since been withdrawn.

## 7. Why

- **Publication drift.** The pilot accepted a queryable index whose vectors
  were still being built. Each vectors turn publishes a new revision. A
  model-stage Answer call holds its read context for tens of seconds while
  Focus and Compact run, and the service refuses to deliver evidence read
  under a revision that changed before delivery. The no-model call is fast
  enough to finish between publications. Hence the settled-index rule:
  semantic state ready, hybrid mode, no index or vectors job, and an
  unchanged publication token from start to end.
- **Single-line coverage.** Under the first coverage rule, one line of
  overlap between a returned span and a note's range counted as coverage. In
  the pilot, Search, no model and Compact covered all five essential notes
  of airk C1 that way. Under rule 2, which needs min(5, the note's length)
  overlapping lines, the same sessions cover one of five. A span that
  touches a range at its edge has not shown the agent the fact, so rule 2
  was adopted before any judged pass, and the pilot was rescored with it.
- **Reply mode.** The fixture's client environment sets
  `AIRK_MCP_RESPONSE_MODE=full`, and the pilot's agents received full
  replies. Agents in the product get the terse default. The harness now
  drops that variable for agent calls, digest replays and depth searches,
  and records each turn's effective mode.
- **Parse's first call.** With a prose instruction as its text, Parse
  prepares a plan that needs an identifier and returns no evidence: 1,794 to
  1,842 bytes and nothing to cover, in all four pilot scenarios. The Parse
  path is left as it is, because that is how the tool behaves, and every
  report states it.
- **Indexing speed.** Inference and DiskANN insertion were serialized, and a
  256-entry proof cache thrashed: on 1,200 files it evicted 9.05 million
  times. Insertion time per vector halved once the cache was bounded by
  bytes.
- **Stuck lifetimes.** The same-install restart refuses any pending or
  running job, and the service queues follow-up work after a cancel (a
  `verify` after `vectors`). The fixture now cancels, then waits for five
  quiet samples, waits for setup's own index job before cancelling it, and
  records whether the installation can restart before it exits.

## 8. What happens next

- **The baseline** (`01-baseline.md`) runs on the airk scenarios first, as
  the user directed, once the airk index is settled. At 00:37 UTC on 22
  November it was still building vectors (7,456 of 10,109 files). The
  bookbuilder index settled at 00:41 UTC, and a bookbuilder baseline pass
  started on it then. The baseline uses Haiku agents and two Sonnet judges
  per result, with the Sonnet reference arm on airk.
- **The no-agent depth measurement** of the plan records, for each
  essential note, the rank at which a 100-hit search first covers it. It
  decides whether shared search fusion enters a later change set.
- **The baseline runs on image `baseline-1fe7b600-o2-fix3`**, with the roster
  fix, and bookbuilder runs on `bookbuilder-v2` once it is settled.
- **Change set 1** runs on image `cs1d-d316beaf` after the baseline is
  judged. Its report carries the eight failing `cpu_source_transition` tests
  and the recorded digest-input changes (`runtime.rs`, `bounded_hybrid.rs`).
- **The GPU track** must get a CUDA lifetime running on the evaluation
  indexes. The CPU and GPU evidence difference is explained (section 6).
  Focus on CUDA stays blocked.
- **The user should rotate the two bookbuilder keys** (section 2.2).
- **The training track** continues with a second Focus candidate, and the
  first candidate is replayed through Answer.

## 9. Limits and receipts

**Limits.**

- The pilot is four scenarios on an unsettled index, with most model-stage
  calls failed. It shows that the harness runs, not how Answer performs.
- The indexing speed-up is measured on samples of the airk export with the
  kernel's harness. The live service rates come from uncontrolled windows.
  Fix 3 is not measured on its own.
- The GPU results come from a 7-file synthetic repository, not from the
  evaluation indexes.
- The roster fix is proven by a unit-level regression test and by reading
  the code. No airk search on the fix-3 image had been compared with the
  truncated one when this was written.
- The exposure scans looked only under the campaign folder. What the model
  provider retains is outside this evaluation's reach.
- Focus candidate 1's scores are pair-level Focus scores on mined pairs
  whose negatives are not judged. They are not Answer results.
- Everything ran on one Windows host with one RTX A6000. Nothing here is
  cross-platform evidence.

**Where the receipts are.** Every path below is under the git-ignored
campaign folder
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`, which is
not committed.

| Receipt | SHA-256 |
| --- | --- |
| `fixture/images/baseline-1fe7b600/images.json` | `2e84fc5e5eae885db17d3efa028064608e33b2e7100da22f9c6206f40dcbdc95` |
| `fixture/images/baseline-1fe7b600-o2/images.json` | `15cfa28ad676ba73a463ab12d880a45ccdce9448f07186f36823b9d5f592a3ef` |
| `fixture/images/baseline-1fe7b600-o2-fix/images.json` | `5d2fbd047ba0e7f2527a444f00476f4da802a74c06293b4065e8e680476b1b84` |
| `fixture/images/baseline-1fe7b600-o2-fix2/images.json` | `18b5ce6574e9274c13a75cb3d51fb347c591e324f399c856b18baff5605805d3` |
| `fixture/images/baseline-1fe7b600-o2-fix3/images.json` | `3f2cddb4d7f759835cf8f4d5c1b553f92779aeb34256400914d67fd47806bcc1` |
| `fixture/images/cs1d-d316beaf/images.json` | `78da9eddc68157424cdef1d4ba2f04b72384d3345b93b176e676f46ddb648259` |
| `fixture/images/cs2prep-09edaadd/images.json` | `219d97c33d9c1f833e971794837865351117bd6bc208bd9f8044316730369f6b` |
| `fixture/images/cs2search-b128abe0/images.json` | `2ee6a46805cff3832f0151e3dfded7accc9220a183e03f65597c444a441a3eab` |
| `fixture/images/baseline-1fe7b600-cuda/images.json` | `725e89b3514b919e1f338548b04b0003e0b951dbb16ab77ece5e63a4f420c121` |
| `trim-diag/policy-search-roster-fix.patch` | `5573907617736e2774f14be86850abd755313437a75d61668e8125b54a1d787c` |
| `vectors-diag/indexer-fixes-r3.patch` | `cf9b61062ef132f1299bf1e33189fd0dcbafd138e6a9b3bce37d4494ab5f201f` |
| `fixture/indexes/bookbuilder-v2.state.json` | `144805854fdaf3dd7ee8c402450ea7f8167f9f5baf3b7606ccad8eea59a36329` |
| `fixture/sources/bookbuilder-v2-238b22a722ce.json` (snapshot) | `13a05b1a25ee32e35d6a983d692b12c3ce466108c7e8bf3c0cc18fee51182ab5` |
| `fixture/index-aliases.json` | `c550b7ba7052d8fc885c028aab0ee13114bbd06cb6aac98a8cc5b46081e601d7` |
| `fixture/credential-probe-bookbuilder.json` | `69aea9bef6fabe10f78b80ebceb3eb4e61c6d2892562cc5f77bfe9f93ec6b7c8` |
| `fixture/credential-probe-bookbuilder-after-exclusion.json` | `a1fd2e80de12b39a7f6f072e45e19c41f82b06a8344b866e2e246e445d5d4166` |
| `SECURITY-NOTE.json` | `da3fafe448c604a0a9252ddbe7403b5c95b24c2cf0d09067f587f24fc2edcffd` |
| `security/exposure-scan.json` | `4b0d564a5ec37c9fe1bf8e187c7e1d4a3813bff3f8065b59495e006075765f3a` |
| `security/deletion-receipt.json` | `cfe9ef2941657292e4cb4fcd8bf1795a3418b1827d3a2495221555d409f83d12` |
| `security/exposure-scan-after-deletion.json` | `def1ac8a8bc60664697403a504516773107d8c2081bcdede3acb735dd8fa7e99` |
| `passes/baseline/depth/bookbuilder/depth.json` | `7e5702e16f50d37b21c89b6098e836cf43379dfff3af0fa56a22dff63a36495b` |
| `design/search-v2.md` | `0cae2d43c4f8d94f1ef3bbb521c0044102a530a17a2c34f9aee57747b468b112` |
| `design/change-set-1-decisions.md` (at 04:01 UTC) | `afedb9f009606c12fda2e1846fcce3bea10bf2f3a2c869694d0416361ee43ebe` |
| `gpu-track/runs/s1-focus-cpu.json` | `84e75f4f0692462ceb719d6e3556d7b1b29da45a3c321258ac073befa9b51bbe` |
| `fixture/sources/airk-1fe7b6003574.json` (commit export) | `0ddaba1d688c7dfe88ba88ad5ac49219f9be3c14f0fad1069af0499f6e1606b5` |
| `fixture/indexes/airk.state.json` | `f74724fc4abb1e1bfe1102285b9df496dc890ca6d5bcdd3e8c6034815d4e9230` |
| `fixture/indexes/bookbuilder.state.json` | `a1857f54a9d95627a6674ee3a9d3d3cb5dd6fa09ceb5083a2b46548af52a6e7a` |
| `fixture/bookbuilder-scope-precheck.json` | `006ebb2ff137e8a286ba2ab2c04b36285e9d7890ba21d667558893aba7377ade` |
| `fixture/bookbuilder-scope-check.json` | `51845888c00f18d16c97a5549d04fcb84b0a1bfa1959427759d064231e8650ae` |
| `fixture/leak-probe-1.json` | `d71ad886ca733a80145848a4846f104da9dd018d0a2be2abe008ce86f19ffccb` |
| `fixture/progress-notes.json` (image switches and live rates) | `95acda3841e5465f1b3952b0c204f370e29dd1f518303712f99d449e03343005` |
| `vectors-diag/indexer-fixes-r2.patch` | `7e11b481b4acb611f2b4ad529db051554795b430670ed82e61827f4f55f22843` |
| `vectors-diag/runs/o2-c1200b/index.json` (unfixed) | `43e9510f5a94e8f33a417a66a4a4543103973427cf769c4ac9f89dc97939fd35` |
| `vectors-diag/runs/o2fix2-c1200/index.json` (fix 1) | `1b93363225355397f97dea52c98238f2280f0d0c4ef994971d01e7e8499857b4` |
| `vectors-diag/runs/o2fix3-c1200/index.json` (fixes 1 and 2) | `3f520f1757a6cc194169c9f96f164b564fa21ab379f7d06f2bec66b173375d66` |
| `scenario-freeze.json` | `0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806` |
| `design/prompts-freeze.json` | `ef4dc528d530c4fa1bf510520d0bdd346212deb850295ccd78ec56b428737ad7` |
| `design/harness-conventions-accepted.md` | `4d9282f00a0dc21967df6cc8c31d80d4e1e3b904da1c9f97173091c0edf7b7c8` |
| `passes/pilot/pass-report.json` | `de290e253972f5d5a586a28c10d6b4399769b500eb6aae10589f3767b3af29cb` |
| `passes/pilot/summary-rule2.json` | `d7619751d85bd46151044d0d48689e535bcab871788063a012d4d1560e74d09b` |
| `passes/pilot/results-rule2.csv` | `73cc75327be798ca40c8c4cb52e99d2a05bcfc9f49ffce00f8b9f631ec23fd07` |
| `passes/pilot/rejudge-sonnet.json` | `28371538bb524c7fafe00c65665175eabb21e7f63e8c57151531e5ffdb466223` |
| `passes/pilot-haiku-cost/INVALID.json` | `d1c2d4b697963b5281f6c461747ae8e0ba3a68755951e36d2a03ccd35f1d5ec6` |
| `gpu-track/runs/s1-focus-cpu-compact-cuda.json` | `3b08e615277b681af806249b813f9600582dd7a39b8d078a0be31199fcee3053` |
| `gpu-track/runs/s3-focus-cpu-large-cuda.json` | `9e56a6b22ce3c4c6ae2361c9f9d3edb1840f0d15d0ad98dad446fc140482f005` |
| `gpu-track/reclaim.json` | `d71e8ddb61dcdc9940d6157c952a168780889d4124bd1c8f4e7ef17fb9d9b311` |
| `training/pairs-r1/stats.json` | `69d7d3fcbe207fc60410db22d4a50fb47b54e87f0b50dc805812a4bc22193467` |
| `training/focus-cand1/run/execution.json` | `f17cd183a94369791c3c015a3994ca1d4cf16b26d7924aa0ec71ed8c0ccdee76` |
| `training/focus-cand1/score.json` | `7fa5d67cb35db5c76902437184ddb1f17c70b25468e99ffcaeba6634e960cae6` |
| `training/focus-cand1/onnx-parity.json` | `f16829dcc49cea63b4be57d947f514a0924d9d536260ff5af3d07ea11766364f` |
| `cal100-reclaim-receipt.json` | `d45fe67efc4d94120a8e71b9865b2068599b78e71822bd59eadc2231d170decf` |
| `r154-reclaim-receipt.json` | `0edf0577d5ac27cf10a8d1ae7f2eafc0620717c088ff19e1d03098cd6edf897b` |

**Commands that reproduce it.** Python runs as
`python -I -B` from `D:\code\Github\airk`, as the verified human. `R` is the
campaign folder above.

```text
# a service lifetime on the baseline image, restarted from the previous one
python -I -B -S scripts/arbiter-answer-eval/fixture.py start --image R/fixture/images/baseline-1fe7b600-o2-fix2 \
    --lifetime <new> --restart-from <closed lifetime> --seconds 21000 --repositories airk,bookbuilder
# the leak probe (airk; phrases from R/scenario-freeze.json)
python -I -B -S scripts/arbiter-answer-eval/fixture.py leak-probe --lifetime <lifetime>
# the pilot, and its rule-2 rescore and Sonnet re-judge
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/pilot/spec.json
python -I -B scripts/arbiter-answer-eval/rescore.py ...      # writes summary-rule2.json, results-rule2.csv
python -I -B scripts/arbiter-answer-eval/rejudge.py ...      # writes rejudge-sonnet.json
# the harness's own tests (synthetic data; no model or service)
python -I -B -m unittest discover -s scripts/arbiter-answer-eval
```

The image builds use `cargo test -p airk-service -p airk-kernel --lib
--all-features --locked --offline --timings --no-run` with the environment
recorded in each `images.json`. The fix images were built from the commit
export at `R/vectors-diag/src-1fe7b600`: fix-2 with indexer patch r2, and
fix-3 with patch r3 and the roster patch. The change-set images were built
from the shared working tree, identified by its diff digest.
