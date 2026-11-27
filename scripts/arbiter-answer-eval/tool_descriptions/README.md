# Tool descriptions under test

Evaluation inputs for the experiments of
[topical-index-research.md](../../../docs/arbiter/topical-index-research.md)
("Teach the agent when not to call"). An acting agent decides whether to
call Gather Context from two texts the server gives it: the tool's
description and the server's guidance. This folder holds each variant
tried, so a result can always be traced to the exact words.

## Files

Each variant is a pair, `<id>.description.txt` and `<id>.guidance.txt`.
A missing guidance file means the build's own guidance is used.

| Id | What it says | Source |
| --- | --- | --- |
| `d0-build` | "Use first for a task that needs evidence from several places"; guidance says "Multi-place task: gather_context first" | The build under test on 27 November 2026 (`crates/airk-mcp/src/lib.rs`, `unified.rs`), copied unchanged |
| `d0b-no-untrusted` | The build's text with the guidance's first sentence removed ("Untrusted index; verify live bytes before edits."). The user called that sentence an old artifact on 27 November 2026; this variant measures what removing it alone does | `d0-build`, one sentence removed |
| `d1-selective` | Says when to use the tool and when to open files directly; one call at the start; read the listed ranges, not whole files | First rewrite, from the cases where the tool cost more |

## Use

`agent_cost_run.py --description-file <file> --instructions-file <file>`
serves the variant in place of what the build advertises;
`agent_cost_batch.py` passes both through. The session records the
description's SHA-256 and length.

## Results

One row a variant and series, at one price for every cache write, against
the unaided baseline. "Called" is the share of sessions that called the
tool. A good description is called where the tool wins and not where it
loses.

| Variant | Reply served | Tasks | Called where it should be | Called where it should not | Cost against no tool | Series |
| --- | --- | --- | --- | --- | --- | --- |
| `d0-build` | kernel brief | all 20, first round trip only | 20 of 20 sessions called, on every task | same | not a cost series | `tool-firstcall-r1` |

## Later: descriptions that follow the repository

These fixed texts are for the prototype. The product may need to compose
the description from what the topical index finds, since a repository
that is mostly documents may need different words from one that is
mostly Python, C# or Rust. That experiment is noted in the plan
("Later experiment: a description that follows the repository") and
waits for a description that works on the current two repositories and
for repositories of a different make-up.

Rows are added as series finish. A variant is not called effective from
one task: the row says which tasks it was run on.
