# Handoff: topical index research, ranking work, 27 November 2026

**Current finalist, 28 November 2026.** The [development comparison](topical-index-research.md#selected-prototype-for-independent-evaluation) is complete. The frozen candidate uses up to 40 bundled file judgments from one local Arbiter stage. It preserves the initial passages and fills spare reply space with adjacent source lines. All 800 development prompt hashes, 20 selected source sets and 20 final frames reproduced through the live policy callable. The [completed matched comparison](topical-index-research.md#completed-agent-cost-comparison-and-rust-decision) found equal final correctness and higher caller cost with AIRK available. The recommendation is to end this candidate campaign without a Rust port.

For Codex, picking up from Claude Code while the user is short of Claude
credits. Read this page, then the "Experiment protocol" and "Round 1"
sections of [topical-index-research.md](topical-index-research.md). The
earlier version of this page (Fable's stop of the same day) is in Git
history; everything it listed as a loose end is done.

## Subsequent Windows replay

The free trials described below have run. The [round 1 research results](topical-index-research.md#round-1-munarium-t1) record all conditions. Broader second-hop scoring lost a first-six reference. Sibling and project links improved only deep ranks. Giving each file a first passage opportunity before full-layout truncation produced 30 of 113 notes at the lines and 53 at the file, versus 28 and 43, with two passage regressions. On munarium-T1 even the full pre-truncation pool covers only two of five notes at their lines. The subsequent 20-task stage trace found two munarium-T1 ranges lost at the four-per-file cut (one full and one partial overlap) and one absent before selection. A need-based file nomination trial reduced first-six references from 47 to 41 and focused line hits from 30 to 26. The constrained oracle found 28 fully qualifying notes in the retained replies, a ceiling of 42 in the first six files and 70 in the first 40. Only a third of the recoverable gain lay inside the six, so the conditional passage-diversity trial did not run. The local CUDA Qwen top-40 run has now completed. Fully qualifying notes increased from 28 to 40, with 17 gains and five losses. Review the three regressing tasks and agent-cost evidence before any port decision. The maintained runner and usage are in [the harness README](../../scripts/arbiter-answer-eval/README.md#comparing-topical-ranking-experiments). The protected-leader rule was rejected and the plain-Qwen live MCP path has passed parity on all three Aardvark tasks. The user subsequently authorized Aardvark. All 22 sessions finished. On the primary task Qwen offered a 1.8% median saving, with zero fully passing patches in five runs versus one in five unaided. Both spot tasks bypassed the tool in all six offered sessions. See the [Aardvark results](topical-index-research.md#aardvark-sonnet-results) for the matched comparisons, evidence coverage and execution checks. Hold the Rust port. The next experiment should establish whether ideal evidence improves verified completion on this task.

The [Badger skill comparison](topical-index-research.md#badger-skill-results) is complete. Explicit invocation raised Gather use from zero of five to five of five on Munarium-T1, while fully passing patches fell from four to two. Stop this skill variant before natural activation or cross-client replication.

The ten [Caribou matched pairs and HTTPX failure reproduction](topical-index-research.md#caribou-paired-evidence-results) are complete. The proposed next comparison is one repair turn with full-module test feedback. The native fixture and local scorer are closed.

The [Dragonfly repair comparison](topical-index-research.md#dragonfly-matched-repair-results) is complete. The user retained the shipping skill and authorized one three-arm checkpoint on six prospective tasks across Munarium and Bookbuilder. Keep the skill naturally available in both AIRK conditions. Review verified outcomes and cost with the user after all 18 sessions, before any further repetitions or kernel port decision.

The [six-task checkpoint disposition](topical-index-research.md#six-task-end-to-end-checkpoint-disposition) supersedes the planned 18-session comparison above. Fourteen sessions ran before the historical native fixture was disqualified. Preserve their classifications and receipts. No current-Arbiter efficacy claim or Rust port decision follows from this panel.

## Authorization at the original handoff

**Run free checks only.** A cost series (`series.ps1`, `agent_cost_run.py`,
`agent_cost_batch.py run`) starts headless Claude sessions that spend the
user's API credits. Do not start one, and do not start judge or helper
agents, until the user says so in chat. Everything below can be done with
`rank_check.py` and `live_gather.py replay`, which call no model.

When the user does allow series again, name each after an animal or
insect, each starting with the next letter of the alphabet. The first is
Aardvark.

## What the work is

Finding out whether a Gather Context tool lowers what an AI coding agent
spends, and what design does it. The measure is the agent's cost at one
price; correctness is the guard. Experiments are Python evaluation tooling
under `scripts/arbiter-answer-eval/`; the winner is migrated to the Rust
kernel later. Method set by the user: one hypothesis at a time, general
rules only (never a value set from a task's truth), improve while
improvement is possible, report when no real improvement remains.

## State at the handoff

- Branch `feat/arbiter-final` (main checkout `D:\code\Github\airk`).
  Work was done on `agent/remove-untrusted-message` in the worktree
  `.claude/worktrees/agent-aeb77152b95228643` and merged into
  `feat/arbiter-final` after each step. Nothing is pushed; push, PR, merge
  to main and publication need the user's explicit word.
- No series, fixture or agent is running.
- Done today, all merged: distrust and editorial strings removed from
  replies and screens; the MCP surface cut to seven tools
  (`gather_context`, `search_passages`, `find_definition`,
  `find_identifier_occurrences`, `read_indexed_range`, `outline_file`,
  `list_indexed_files`), the CLI unchanged; scope configured per
  connection (`airkctl agents connect --scope`, header `X-AIRK-Scope`),
  never chosen by the agent; the one passage assurance ("These passages
  are exact text taken from the files referenced. There is no need to
  read these ranges again."); the decline rule and refusal
  (`gather_decline.py`); ranking rule 1 below.

## Ranking: where it stands

The prototype's ranking is the limit: the focused layout shows six
files, and too few truth files are among them. `rank_check.py` measures
this for free over the first recorded call of each of the 20 tasks (168
located truth files).

| Version | First 6 | 7 to 16 | Later | Unreached | Focus replay at the lines |
| --- | --- | --- | --- | --- | --- |
| Baseline | 43 | 23 | 77 | 25 | 26 of 113 |
| Rule 1: other-language siblings at 0.3 | 47 | 25 | 71 | 25 | 28 of 113 |
| Rule 2 (rejected): every second-hop term, weight 0.15 | 47 | 29 | 72 | 20 | 9 of 113 |

- *Rule 1.* When the task text names a language, code files in other
  languages rank at 0.3, but only under a top folder that also holds
  files in the named language (sibling clients, not the server a client
  calls). `LANGUAGES` lives in `live_gather.py`; `gather_decline.py`
  uses it.
- *Rule 2, rejected, not in the code.* The second hop searches only 24
  follow-up terms (`HOP_TERMS`), cut in alphabetical order within each
  kind. Searching all of them at weight 0.15 improved the file ranking
  (table) but the focus replay collapsed: 9 of 113 notes at the lines
  (was 28), 28 pointed at the file (was 43), mean time per call 10.3 s
  (was 5.3 s). Likely cause, not yet confirmed: every hop term also adds
  its lines to `held[path]`, which chooses the passages, so passages are
  cut around follow-up hits instead of the request's matches; and each
  held term raises the `1 + 0.25 * len(held)` multiplier in `fitted`,
  which the weight does not scale. A version worth testing first: let
  the extra hop terms add score only (no lines in `held`, no multiplier
  count). Also tried, by `rank_check.py` only: caps 48 and 1000 at
  weights 1.0, 0.5 and 0.3 (each lost one or two files from the first
  six); ordering hop terms by use in the top files (no difference).
- Lesson: `rank_check.py` measures files only. A rule that changes which
  lines a file holds must pass the focus replay too.

## Original experiment queue before Aardvark

1. **The 25 unreached files.** Try rule 2's score-only version
   first. Per file, `rank_check.py` prints which
   request terms reached the others. Patterns seen: dependency-injection
   registrations (`ServiceRegistration/*Extensions.cs`), message routers
   and handlers one call away (`MessageRouter.cs`, `Set*Handler.cs`),
   project files (`*.csproj`, `pyproject.toml`), generated code
   (`grpc_api_generated.rs`) and sibling tests of a reached source file
   (`AutoSaveServiceTests.cs`, `PreferencesHandlerTests.cs`). A candidate
   rule: a reached source file's test file by name (`X.cs` with
   `XTests.cs`, `x.py` with `test_x.py`) and its folder's project file
   join the ranking at a low weight.
2. **Documents.** 2 of 22 document truth files reach the first sixteen;
   `ROLE_WEIGHT["document"]` is 0.35. Test whether documents that match
   the request's own words (not hop terms) deserve more.
3. **The 72 files ranked later than 16.** Look at what holds the first six
   instead (`rank_check.py` prints the top six per task).
4. After deterministic rules stop helping: the Qwen 4B reranker
   (`arbiter_judge_service.py --bits 8`, `gather_context_dose.py`) over
   the first 40 ranked files. It runs locally on CUDA and spends no
   credits.
5. Record each result in `topical-index-research.md` (the "Ranking"
   entries under round 1) and the ledger
   [one-process-ledger.md](../simplify/one-process-ledger.md) as it lands.
6. With credits back and the user's word: series Aardvark, the focused
   layout on munarium-T1 with the improved ranking against
   `baseline-sonnet-r1` (five repeats, Sonnet, one price).

Accept a rule only when the first six do not lose a truth file and the
focus replay does not lose notes at the lines. Re-run both checks after
every change; they take about five minutes each.

## How to run things

PowerShell, from `scripts\arbiter-answer-eval` in the worktree or the main
checkout. Use Windows paths (`D:\...`); a `/d/...` path inside a
`python -c` string is not converted and fails.

```powershell
$c = 'D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1'
python -B rank_check.py --campaign $c --calls "$c\replay\first-calls-r1.json" --maps "$c\topical" > <scratch>\rank.txt
python -B live_gather.py replay --campaign $c --calls "$c\replay\first-calls-r1.json" --maps "$c\topical" --output "$c\replay\<new folder>" --layout focus
```

- The last line of `rank_check.py` is the totals; above it, per task,
  each truth file's rank and matched terms (`E` marks essential), then the
  top six.
- To try a constant without editing: `python -B -c "import sys,
  live_gather, rank_check; live_gather.HOP_TERMS=48;
  sys.exit(rank_check.main([...]))"`.
- Replay folders that exist: `live-focus-lang-r1` (rule 1, current),
  `live-focus-hop-r1` (rejected rule 2). Each new replay needs a new folder.
- Tests: `python -B -m unittest test_gather_decline test_passage_assurance
  test_tool_names`.

## Rules learned the hard way

- Compare cost only at one price (`agent_cost_batch.py report`,
  `agent_cost_task.py`); five repeats a condition.
- Do not edit `live_gather.py`, `topical_index.py`, `agent_cost_run.py`,
  `agent_cost_batch.py` or `gather_context_mcp.py` while a series runs.
- No value is set from a task's truth.
- Free checks first; a series confirms a rule, it is not the search loop.

## Standing rules that still bind

Verify the human identity before shell or file work (`AGENTS.md`,
"Execution identity"; `scripts/dev/assert-human-execution.ps1`; the human
is `TYAI\tdotn`). Check the owner of new outputs under the campaign
folder. Commit with `git commit -s -- <paths>`, the human's identity, no
agent trailers. Never commit `artifacts/`, `temp/` or `.airk/`. Keep
AGENTS.md and CLAUDE.md byte-identical. One Cargo owner per target. Judge
correctness directly; no judge agents. D: must stay above
214,748,364,800 bytes free (about 790 GB free at the handoff).
