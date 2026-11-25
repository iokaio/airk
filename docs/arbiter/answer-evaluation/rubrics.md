# Answer evaluation rubrics

This file fixes how every result of the Answer evaluation is judged. It was
frozen before the first judged pass (the baseline). The pilot of
[00-setup.md](00-setup.md) was judged while these rules were being settled,
and two of them (coverage rule 2 and the default response mode) came out of
it; the pilot's results are not evidence about Answer.

A **result** is one agent session: one scenario of [scenarios.md](scenarios.md)
run on one path (Answer in one configuration, Parse or Search) and, for
Answer, on one device. A session is a fresh headless Claude Code agent that
receives the instruction, may make up to 12 tool calls through a logging
wrapper that offers only its path's tools, and ends with a short work brief
under three headings: where it would work, what it must respect, and what it
still does not know. A **turn** is one tool call. A call the wrapper refuses
(a tool outside the path, a first call that breaks the first-call rule, or a
call after turn 12) is logged and is not a turn.

Two rubrics apply, in this order. Rubric A decides the outcome. Rubric B is
reported beside it and only breaks ties.

## Rubric A: fewer turns for an agent

### A correct finish

A result has a **correct finish** when both of these hold:

1. **The evidence establishes every essential fact.** After some turn, the
   tool results the agent has received cover every essential reference note
   of the scenario. A script decides this from the turn log, with the
   coverage rule below.
2. **The brief gets those facts right.** A judge marks every essential note
   "correct" in the agent's brief.

The **turn count** of a correct finish is the turn at which condition 1 was
first met. A session that meets condition 1 and fails condition 2 has no
correct finish: the evidence was there and the agent did not take it in.

### Grades

| Grade | Meaning |
| --- | --- |
| 4 | A correct finish after one call |
| 3 | A correct finish after two calls |
| 2 | A correct finish after three or four calls |
| 1 | A correct finish after five or more calls. Or one essential fact short: every fact was covered and the brief got one wrong, or one fact was never covered |
| 0 | More than one essential fact missing or wrong, misleading evidence that the agent followed, a failure or a timeout |

Each judge's verdict gives one grade. The script computes it this way
(`grade.py`, `grade_one`):

- A session that did not complete grades 0.
- A **deficit** is an essential note that the evidence never covered, or that
  the judge did not mark "correct" (a "missing" verdict counts like
  "wrong").
- If the judge found misleading evidence that the agent followed, the grade
  is 0.
- No deficit: a correct finish, graded 4, 3, 2 or 1 by the turn of
  condition 1.
- Exactly one deficit: grade 1, with no correct finish.
- More than one: grade 0.

### Also measured, from the turn log

- **First-call yield.** The share of essential notes covered by the first
  call alone.
- **Context cost.** Bytes returned to the agent over the session, tool time
  and wall time. Fewer turns bought with a very large or very slow reply is
  reported as such.
- **Misleading or noisy evidence.** Judged from the brief and the calls, as
  above.

### The coverage rule (rule 2)

Coverage is mechanical. It is implemented in
`scripts/arbiter-answer-eval/evidence.py` (`covers`, with
`COVERAGE_RULE = 2`) and applied turn by turn by `coverage.py`. Rule 2 was
set by the primary agent on 21 November 2026, before the first judged pass,
after the pilot showed that the earlier rule let a single overlapping line
count as coverage.

**What counts as evidence.** The script walks each tool result and collects
spans: a path, an optional line range, some text and a kind.

- A `text` span is evidence text at a place: a search, read, context or
  references hit with its text, a Parse evidence row that carries a hit, or
  an inner read of Answer.
- A `fact` span is an outline or symbol row: a name at a line range, without
  the text.
- A `pointer` is a location without text, such as a definition hint.
- A `path` entry is a path with no line range.
- Request arguments, plans, receipts, scope and readiness fields are never
  evidence.
- Only `text` spans cover by line overlap. Outline and symbol rows count only
  if `--count-fact-rows` is passed, and it is not passed in any judged pass.
  Pointers never count.

**When a span covers a note.** A note is tried at its own location (path,
line range, symbol) and then at each alternative (the alternative's path and
range, with the note's symbol when the alternative names none). For each
location:

1. Only spans on the same path are considered. Paths are compared after
   turning backslashes into slashes and dropping a leading `./`; they match
   when equal or when one ends with `/` followed by the other.
2. If the location has a line range of n lines, a `text` span covers it when
   it overlaps at least min(5, n) of those lines.
3. If the location has no line range, any `text` span on the path covers it.
4. Whatever the range, a `path` entry (a path with no line range) covers the
   location when its text contains the location's symbol. A path without a
   line span covers only this way.

A note is covered after turn k when any result of turns 1 to k covers it. A
failed call covers nothing. Rule 1 (one line of overlap) remains available
as `rule=1` for comparison only, through `rescore.py`.

The deciding lines of `evidence.py`:

```python
def required_overlap(lines: list, rule: int = COVERAGE_RULE) -> int:
    return 1 if rule == 1 else min(5, lines[1] - lines[0] + 1)


def covers(spans: list[Span], note: dict, count_fact_rows: bool = False, rule: int = COVERAGE_RULE) -> bool:
    """The coverage rule for one note over one result's spans (rule 2 unless ``rule=1``)."""
    line_kinds = {"text", "fact"} if count_fact_rows else {"text"}
    for location in _locations(note):
        same = [s for s in spans if path_matches(s.path, location["path"])]
        if not same:
            continue
        if location["lines"] is not None:
            need = required_overlap(location["lines"], rule)
            if any(s.kind in line_kinds and overlap_lines(s, location["lines"]) >= need for s in same):
                return True
        if location["lines"] is None and any(s.kind in line_kinds for s in same):
            return True
        # A path without a line span covers only by symbol.
        symbol = location["symbol"]
        if symbol and any(s.start is None and symbol in s.text for s in same):
            return True
    return False
```

### The judge

Fresh series use the OpenAI judges and runner specified in the
[completion plan](../completion-plan.md#agent-and-judge-models).
The following model IDs, client settings and prompt freezes document the
earlier series. Each historical result was judged by two independent
headless Claude Code sessions on `claude-sonnet-5-5` at medium effort, with
no tools. Each judge reads one packet: the instruction, the reference notes, the
agent's brief, the calls the agent made and the text of the first reply. Path
names are hidden where possible; Answer's reply has a recognizable shape, so
the blinding is partial. The judge returns JSON that a schema checks:

- for every essential note, "correct", "wrong" or "missing", with a one-line
  reason. "Correct" means the brief states the fact or clearly implies it
  without contradicting it; a location alone is not enough;
- whether misleading evidence was followed, which is true only if the brief
  or the calls show the agent acting on it;
- the four Rubric B scores of the first reply.

**Adjudication** (`grade.py`, `adjudicate`):

- Equal judge grades stand.
- Grades one apart take the lower grade, with that judge's verdict.
- Grades further apart stay pending until the primary agent settles them in
  the pass's `adjudications.json`, with a reason. Those settlements are the
  primary agent's own, and each report labels them as such.

## Cases that are not clear-cut

Choose the stronger of Parse and Search per scenario within the same agent
model and effort. A correct finish wins. When both finish correctly, choose
the one that took fewer turns, then Search on an exact tie. When neither
finishes correctly, choose the one that covered more essential notes across
the session, then the one that took fewer session turns, then Search. Use
the selected control's adjudicated grade in the task-kind average. The
22 November baseline report records this rule and the grader implements it.
[Baseline comparison](01-baseline.md) · [Comparator](../../../scripts/arbiter-answer-eval/grade.py) · [Comparator test](../../../scripts/arbiter-answer-eval/test_harness.py)

| Case | How it counts |
| --- | --- |
| Answer finishes correctly one turn sooner than the better of Parse and Search | Better, and reported as better. Not a substantial win |
| Answer and the better of them finish correctly on the same turn | A tie |
| Answer finishes correctly later than the better of them | Worse |
| Answer's evidence covers every essential fact and its brief gets one wrong | No correct finish for Answer. Not a win, whatever its turn count |
| Answer finishes correctly, and neither Parse nor Search does | A substantial win |
| Answer fails or times out, and Parse or Search finishes correctly | Worse |
| No path finishes correctly | No win. The scenario stays in the 20 and is listed as unsolved by every path |
| The verdict turns on a single turn | A second session is run. The verdict stands if both agree. If they differ, the scenario is reported as undecided and is not a win |

"Worse" means one of the two rows that say so. In the harness it also covers
Answer reaching no correct finish when the better of Parse and Search does.

A **close call**, which gets the second session, is a pair where both finish
correctly and the better-of turns minus Answer's turns is -1, 0, 1 or 2: one
turn either way would cross the win or the worse boundary. The second session
is run for the Answer configuration and for the better of Parse and Search.

## What "substantially improves" means

The user approved these thresholds on 21 November 2026.

- **A substantial win on a scenario:** Answer reaches a correct finish at
  least two turns sooner than the better of Parse and Search, or in one call
  where the better of them needed three or more, or at all where neither of
  them reached one.
- **A configuration substantially improves results** when it wins that way
  on at least 12 of the 20 scenarios, is worse on at most 2, wins at least 5
  in each repository, and is not worse on average in any of the four kinds
  of task. "Not worse on average in a kind" compares the configuration's
  mean adjudicated grade in that kind with the mean grade of the better of
  Parse and Search.
- **The second criterion** is reported beside it as the difference in Rubric
  B averages. It does not decide the outcome. It breaks ties and guides how
  Answer presents evidence.

**Headroom.** A substantial win is possible only where the better of Parse
and Search needs three or more calls, or never finishes correctly. The
baseline report counts those scenarios. With fewer than 12, no adjustment
can meet the rule. That does not stop the work: it is stated at the top of
the baseline report and repeated in every later report.

**The airk-only first round.** The user had the baseline and change set 1
run on airk's ten scenarios only. The rule needs 20 scenarios and five wins
in each repository, so it cannot be applied to that round. Those reports give
the airk results with the same per-scenario rules and say that the full rule
waits for the rounds with both repositories.

**What each model adds.** Every report also sets the configurations beside
each other on the same scenario rows: Answer with no model against the better
of Parse and Search, Focus against no model, Compact against Focus, and
Large against Compact. For each step it gives the correct finishes added,
the turns saved and the time added. A stage that adds time and no correct
finish has not earned its place.

## Rubric B: useful detail for a person

Judged on the first reply of each path, as a person would read it for the
original instruction. Four questions, each scored 0 to 4; the result's score
is their mean.

| Question | 0 | 2 | 4 |
| --- | --- | --- | --- |
| Can a person find the relevant detail quickly? (`find`) | Buried or absent | Present, needs hunting | Organized so the relevant parts stand out |
| Does it surface detail that would change a choice: constraints, rules, alternatives, risks? (`choice`) | None | Some, incidental | The governing rules and trade-offs are in view |
| Can each statement be traced to an exact source location, and is anything cut off disclosed? (`trace`) | No locations | Locations without ranges, or silent truncation | Exact locations, truncation disclosed |
| How much of it is useful? (`useful`) | Mostly noise or duplicates | Mixed | Little noise, little duplication |

When the packet holds no first reply, every question scores 0.

## Harness conventions accepted before any judged run

The plan leaves these open. The primary agent accepted them on 21 November
2026, before any judged run, as written in the harness:

- Outline and symbol rows are not evidence text by default; no judged pass
  uses `--count-fact-rows`.
- A fact missing from the brief counts as wrong for a correct finish.
- Grade 1 means exactly one essential deficit.
- Judge grades one apart take the lower; grades more than one apart are
  settled by the primary agent in `adjudications.json`, with a reason.
- "Worse" includes Answer having no correct finish where Parse or Search
  finishes.
- The per-kind comparison uses mean grades.
- A close call is a turn difference (better-of minus Answer) of -1 to 2.
- Parse defaults to `mode: execute`, which the agent may override. Answer's
  `schema` and `budget` are set by the wrapper and hidden from the agent;
  the budget is the plan's 8 calls, 3 rounds, fan-out 8, 8,192 input tokens,
  32 candidates, 262,144 bytes and 120 seconds.
- A discarded session (one that offered or used a tool outside the wrapper,
  or had another MCP server present) is rerun once. Failures and timeouts are
  results and are never retried.
- The Claude Code binary is pinned per pass to the VS Code extension binary
  2.1.330, because the npm install (2.1.118) refuses `claude-opus-5-5`. Every
  record stores the binary's path and version.

These were accepted against the harness files as then handed back. Coverage
rule 2 and the response-mode decision were added afterwards, also before the
first judged pass, so the frozen `evidence.py` and `grade.py` below differ
from the versions named in the acceptance note.

Further decisions of 21 November 2026, recorded in the harness README:

- **Settled index.** A judged pass runs only while every index it reads
  reports semantic readiness "ready" and served mode "hybrid", with no index
  or vectors job queued or running. A publication token that changes between
  the pass's first start and any later check invalidates the pass.
- **Default response mode.** Agent calls, digest replays and depth searches
  drop the fixture's `AIRK_MCP_RESPONSE_MODE` setting, so agents see the
  product's default reply. Every turn records its effective mode.
- **Parse is unchanged.** Its first call has a structural zero yield: the
  prose instruction prepares a plan that needs an identifier, and the call
  returns no evidence. Every report states this.
- **No session reuse across agent models or efforts.**

## Frozen prompts and harness files

The prompts, the judge's schema, the default models and the runtime harness
files are frozen in a private file, `design/prompts-freeze.json` of the
campaign folder. The values below are those of the freeze under which the
baseline ran (SHA-256
`b4044529440670ecd4c866244ff9fb746c97da8e5d32f54322b2656fc8ec9f59`,
frozen 22 November 2026 at 04:25:47 UTC). Every judged pass names the freeze
in force, and `run_pass.py` refuses to run if any prompt, the schema, a
default model or effort, or a runtime harness file differs from it. The
freeze was re-made whenever a harness file changed; the prompts and the
schema never changed. Its earlier states are `27d404a4…` (23:31 UTC on 21
November) and `ef4dc528…` (00:40:15 UTC on 22 November), and eleven
re-freezes lie between the last and this one, each recorded with its reason
in `design/prompts-freeze-history.jsonl`. Later passes name later freezes:
the bookbuilder-v2 baseline recorded `b6ac6cea…` (07:37:10 UTC), which
differs only in harness files.

| Prompt | SHA-256 |
| --- | --- |
| Agent, every Answer path (`noModel`, `focus`, `compact`, `large`, `answer`) | `c09581289884a22fbd5db379a09bf4a2ab19e3f21ba6d2c283fd06d57abc17c4` |
| Agent, Search path | `13ee39f1df2cee9fa97e8a84cacb2ba703aa071d7896df54fb53102d50117dff` |
| Agent, Parse path | `9690745f5be140a20d680d68a5fef9e28910ad372f9996e4fef6d1e2adb199de` |
| Judge prompt | `a2c72573f7034d2da7ee71e56e1c85e6ea150d8e9a199fdb347dee6738c5b46c` |
| Judge verdict schema | `4af26b00aa432eddb3752ddc06cf4a6bfdbe0d9c7cdc37d5045bf11a86c247ed` |

The three agent prompts differ only in the tool named for the first call.
Defaults in the freeze: agent `claude-haiku-5-5` at medium effort, judge
`claude-sonnet-5-5` at medium effort, 12 turns.

| Harness file | SHA-256 |
| --- | --- |
| `common.py` | `d6ed9f55f92d2f570b5d2f3d77143021fa51108f8ae7e9dddd36e95f0a696d55` |
| `coverage.py` | `35f9df4953596c4f4371c8f30709463213e0708e09bd3788096e60232e8027b8` |
| `cs_replay.py` | `367bd2742482d80e0b07e66558f0dae35fe14bf6cbf40e45d55138a10fe41d74` |
| `depth.py` | `9bc8dfd68669c952078fca6e5dc5ac24eea5b1f179fca8e1b21400cbbd47705a` |
| `evidence.py` | `803adbe349b130a8c0d2499086a4d42bb4861fb2973b6f943c6d57ae83ece4cb` |
| `grade.py` | `889135d5b8a693227c065c72031e2cb54bc0658b2d9d054f0b3e137132103bbb` |
| `judge.py` | `4fe4c74a96aaaf7aef24cd4d69f050a5be69a657e7e4d0d69be2f632ad0b813a` |
| `mcp_wrapper.py` | `2e552c4bea1697c94662a2d437b22f200b96178b9ce7bc3d41d1770dac15386d` |
| `pass_digests.py` | `88365781488fec78468e3f34c3d11174f695dc0c816bb708253f0c1d51721070` |
| `pass_report.py` | `ac8fa964ff25ea7b182be243fb70075d4c942bfcc9a3fca43a4b0653b88a59c6` |
| `prompts_freeze.py` | `dd2b6cfbe342fea8a9b7a566feb3f4afcd183e55ec58f54bad0491f4f28764a3` |
| `rejudge.py` | `cd4f2d474cb479f0859c48405c0fe582aa58ab4d8e008ec93c08c01d62b33f5e` |
| `replay.py` | `dcbad1183dd36396079b6c3ae20a85f8be1854cda331d75addee54396d211df2` |
| `rescore.py` | `f914e1f57df6efbea017bd91a7ca4a086454f28c365b3ff0bdb800595306a4f4` |
| `run_baseline.py` | `6d748afd115cb870aa1c4415a3979110d0dd2b1713693d6e6d9116472d58274d` |
| `run_pass.py` | `df113ffa2665f490dda8d0096583efaedc3582c28a05de55ae2c283436dfbc5d` |
| `run_session.py` | `2e1caf1003222796af3f45dcfa4890f24bff68c4f195a5e537b8b331f69c88d7` |
| `settled.py` | `4c7d81538b3788b25d3ade31be32c9dc82c28a2dd5d75c6f4c004f9ec83a9508` |

These are the harness files of the baseline's freeze. Fourteen of the 18
match the files in `scripts/arbiter-answer-eval/` when this was written;
four (`grade.py`, `prompts_freeze.py`, `run_baseline.py` and `run_pass.py`)
have changed since, in the later re-freezes of the history file. Reference
notes and instructions are frozen separately ([scenarios.md](scenarios.md)).
