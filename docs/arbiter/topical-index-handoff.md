# Handoff: topical index research, stopping point of 27 November 2026

For the session that resumes this work. Read this page, then the
"Experiment protocol" and "Resume point" sections of
[topical-index-research.md](topical-index-research.md). The ledger's
entries of 27 November 2026 hold every result in full
([one-process-ledger.md](../simplify/one-process-ledger.md), from "Unaided
baseline and the measure" to the end).

## What the work is

Finding out whether a Gather Context tool can lower what an AI coding agent
spends on its model, and what design does it. The measure is the agent's
cost at one price; correctness is the guard. Experiments are evaluation
tooling in Python under `scripts/arbiter-answer-eval/`; the winning design
is migrated to the Rust kernel afterwards. The user directs the method:
one task at a time, sets of three to five single-hypothesis experiments,
improve while improvement is possible, then the next task, and report when
no real improvement remains.

## State at the stop

- Branch `feat/arbiter-final`, head `e1c23612` plus this page's commit.
  Nothing is pushed. Push, PR, merge to main and publication need the
  user's explicit word.
- No cost series is running. No fixture is running: lifetime
  `win-arbiter-brief-r1` was closed cleanly (stop file, exit 0, idle), on
  image `win-arbiter-brief-08fddfbe`. The oracle and live arms need no
  fixture.
- D: free 843,460,608,000 bytes; the floor is 214,748,364,800.
- **One loose end: the message-removal work.** A background agent of the
  stopped session was removing "Untrusted index; verify live bytes before
  edits" from the Rust guidance texts, the Workbench warning and what
  embeds them. When the session stopped it had uncommitted edits in the
  worktree `.claude/worktrees/agent-aeb77152b95228643` (branch
  `agent/remove-untrusted-message`, cut from `b11638dc`) and was still
  building. The new session cannot receive its report. Look at that
  worktree first: if the branch has commits beyond `b11638dc`, review
  and merge them; if it has only uncommitted edits, check that no cargo
  or rustc process is running, then finish the job there or redo it from
  the brief below. AGENTS.md and CLAUDE.md are already changed
  (`723ead9a`) and must stay byte-identical.

### Brief for the message-removal job, if it must be redone

Remove the distrust sentence, keep the sentences beside it:
`crates/airk-mcp/src/response.rs` and `unified.rs` (`GUIDANCE`, first
sentence); `crates/airk-kernel/src/workbench.rs` (the result warning's
first sentence; keep "Syntax relationships are candidates, not resolved
calls."); `crates/airk-core/src/verification.rs` `dated_warnings` (after a
successful source verification keep "Saved sources verified at the
observation time recorded in source_verification, against its exact
indexed revisions." without the distrust wording);
`packages/vscode/src/workbenchRender.ts`; the parity fixture
`fixtures/split/parity/airkctl_reads_print_what_airk_printed.json` through
its writer; the same advice in other words in tool descriptions, CLI help,
agent templates and the developer guides. Leave dated history documents
and uses of "untrusted" that are about security boundaries. Regenerate
generated outputs through their writers, run Clippy and the affected
tests, record the change in the owning CONTRACT files. Use Cargo target
`answer-loop/target-task-arg`.

## What is known

All figures at one price for every cache write, Sonnet, against the
unaided baseline.

1. **The unaided agent is strong.** 60 of 60 baseline sessions correct;
   about $0.50 a task; cost is 66 percent new content entering the
   context, 24 percent output, 10 percent re-read context.
2. **Over all 20 tasks a perfect reply saves 6 to 9 percent.** The room
   is concentrated: munarium-T1, munarium-N-W1, bookbuilder-N-C3,
   bookbuilder-C3 and munarium-C2 each show 22 to 32 percent; most
   others under 10 percent; the cheapest tasks lose even with a perfect
   reply.
3. **The index is not the weak point; how it was asked was.** By exact
   name the index's own lookups find 90 of 97 truth items. The kernel's
   Gather Context reply named 32 of 113 at their lines.
4. **The topical-index prototype finds far more** (74 of 113 at their
   lines, 99 files reached) with no embeddings and no stored file text,
   **and still costs 7 to 19 percent more than no tool** in its full
   layout, because agents read whole every file a reply names.
5. **Layout, on munarium-T1 with perfect content:** passages plus a line
   saying they need not be read again save 41 percent and are steady;
   passages alone 20 to 29; ranges without text 15 to 20.
6. **Guidance, same task:** the distrust sentence gives the worst result
   (9 percent) and a trust line the best (41); neighbouring conditions
   are within noise; with the prototype's own content, guidance makes no
   visible difference.
7. **The prototype's ranking is the current limit.** Its focused layout
   is at break-even on munarium-T1 because only two of five truth items
   make the short list; across tasks only 33 of 85 truth files are in the
   first six ranked.

## Next actions, in order

1. Settle the message-removal loose end above.
2. Improve the prototype's ranking of files and ranges: deterministic
   rules first, then the Qwen 4B reranker (`arbiter_judge_service.py
   --bits 8`, `gather_context_dose.py`). Check each change free with
   `live_gather.py replay --layout focus` before paying for a series.
3. Revise the decline rule (`gather_decline.py`): judge width from the
   task text alone, count files that define the named symbols. It must
   decline munarium-N-C2 and bookbuilder-N-C4 and let through
   munarium-T1, munarium-N-W1, bookbuilder-N-C3, bookbuilder-C3 and
   munarium-C2. Then serve it on the two cheap tasks.
4. Test description `d1-selective` on those two tasks and on
   munarium-T1, recording how often the tool is called; add rows to
   `scripts/arbiter-answer-eval/tool_descriptions/README.md`.
5. Run round 1's spot checks (bookbuilder-N-C3, munarium-N-C2) with the
   kept condition.
6. Judge correctness of every kept condition's sessions with
   `agent_cost_judge.py`. Only the baseline is judged so far.
7. Move to bookbuilder-N-C3.
8. When no real improvement remains, report to the user.

## How to run things

Campaign folder (private, never committed):
`artifacts\\v190\\final-plan\\exec-20261116T052942Z\\answer-eval-r1`, below
`$c`.

- A condition: `pwsh -NoProfile -File $c\\passes\\arbiter-brief-image-r1\\series.ps1
  -Series <name> -Arm plain|oracle|live -Tasks <task id> -Repeats 5
  -Workers 3 -Variant none|d0-build|d0b-no-untrusted|d1-selective
  [-Maps topical|topical-v3] [-Extra '--live-layout,focus']
  [-Extra '--oracle-layout,passages,--oracle-trust']`. Run it as a
  background task. It is a private helper, not maintained source.
- Compare conditions on a task: `python -B agent_cost_task.py --campaign
  $c --task <id> --conditions baseline-sonnet-r1:plain <series>:<arm> ...`
- What an agent did: `agent_cost_trace.py <session folder>`.
- Placement of a reply, free: `live_gather.py replay --campaign $c
  --calls $c\\replay\\first-calls-r1.json --maps $c\\topical --output <new
  dir> [--layout focus]`.
- Maps: `$c\\topical` (second version) and `$c\\topical-v3`.
- Series that exist: `baseline-sonnet-r1`, `oracle-sonnet-r1`,
  `live2-sonnet-r1`, `live3-sonnet-r1` (all 20 tasks); `t1-*` (munarium-T1
  conditions). `t1-oracle-passages-d0b` is a second run of passages with
  no guidance, not a guidance test.

## Rules learned the hard way

- Compare only at one price. A session's reported cost depends on the
  cache lifetime its account was on; `agent_cost_batch.py report` and
  `agent_cost_task.py` correct for it.
- Five repeats a condition. Two identical five-run sets differed by nine
  points; one repeat of a task varies by a quarter of its cost.
- Do not edit `live_gather.py`, `live_gather_v3.py`, `topical_index.py`,
  `agent_cost_run.py`, `agent_cost_batch.py` or `gather_context_mcp.py`
  while a series runs: sessions load them as they start.
- Name a variant on every series and read `gather_context.advertised` in
  one session's `session.json` before trusting a guidance result.
- No value is set from a task's truth. Changes must be general rules.
- Record each result in the ledger and the plan as it lands.

## Standing rules that still bind

Verify the human identity before shell or file work (`CLAUDE.md`,
"Execution identity"; `scripts/dev/assert-human-execution.ps1`). Commit
with `git commit -s -- <paths>`, the human's identity, no agent trailers
or attribution lines. Never commit `artifacts/`, `temp/` or `.airk/`. Keep
AGENTS.md and CLAUDE.md byte-identical. One Cargo owner per target; the
primary's target is `answer-loop/target-cuda`. Do not edit the
model-identity-hashed sources listed in the campaign instructions. Judge
correctness directly; no judge agents. Agent cost sessions leave empty
folders under the user's `.claude\\projects`; 127 were there at the stop
and the user has not asked for their removal.
