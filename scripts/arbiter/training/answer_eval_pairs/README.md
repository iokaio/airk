# Answer evaluation training pairs

Mines instruction/evidence pairs from git history for the Arbiter Answer
evaluation's training track (`docs/arbiter/final-plan-fp16.md`, sections 5.4
and 5.5). The pairs feed Focus (and later Context) weight candidates. They are
private training data and are never committed: this folder holds only the
miner and its synthetic tests.

## Pair kinds

| Kind | Instruction | Evidence |
| --- | --- | --- |
| `commit` | Commit message (subject and body, trailers removed) | New-side line ranges of the changed hunks, per file |
| `doc` | A Markdown section (heading and body) that names code symbols or paths | The named definitions (unique in the repository) or the head of the named non-documentation files |
| `test` | A test function or case: its name in words plus its doc comment, docstring or `describe`/`it` titles | The definitions it calls that share a word with its name (same language); otherwise its first resolvable call when that is not a shared helper; otherwise the file named by convention (`foo.test.ts` -> `foo.ts`, `FooTests.cs` -> `Foo.cs`) |

Each line of output is
`{"id", "repo", "kind", "commit", "instruction", "evidence": [{"path", "lines": [a, b]}], "split"}`.
Doc and test pairs also carry `source` (the section or test location), which
the scenario-note filter checks too. Lines are 1-based and inclusive at
`commit`. Doc and test pairs use the last commit before the cutoff. A
definition span is capped at 80 lines.

`split` is `heldout` for a deterministic 15% of pairs (first 32 bits of
SHA-256 of the id, modulo 100), `train` otherwise.

## Exclusions

- Merge commits. Commits after `--before` (committer date). Messages under 5
  words or 24 characters, or made only of generic words ("more fixes").
  Formatting, lockfile, version-bump and regeneration messages. More than
  `--max-files` (40) changed files, more than `--max-added-lines`, or more
  than `--max-evidence` hunk ranges.
- Paths: the airk benchmark paths (plan section 3.1), generated, vendored,
  build and lock files, content-addressed source copies, binary files.
- Credential and secrets files by path pattern (`aikeys.json`, `.env*`,
  `appsettings*.json`, keys and certificates, and others). These are dropped
  before any content is read. Commit diffs are requested only for the paths
  that survive.
- Pairs whose evidence or source overlaps a scenario note: same repository,
  same path, overlapping lines (`--overlap-margin` widens the check). Note
  alternatives count.

`stats.json` reports counts by repository, kind and split, and exclusions by
repository and reason.

## Inputs and outputs

- Inputs: git repositories, read only through `git log`, `show`, `ls-tree`,
  `rev-list`, `rev-parse` and `cat-file`, with optional locks off. Scenario
  JSON files (`airk.private.answer-eval-scenario/1`) for the overlap filter.
- Outputs, under `--out`, which must be inside the repository's git-ignored
  `artifacts/` folder:
  - `<repo>.jsonl`: filtered pairs with splits.
  - `stats.json`: counts, exclusions, cutoff and the note files used.
  - `unfiltered/<repo>.jsonl` and `unfiltered/mine.json`: the mined pairs
    before the note filter, so `filter` can be re-run without re-mining.

## Use

From the repository root, with Python 3.12 or later (standard library only):

```powershell
$R = 'artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1'
python -I -B scripts\arbiter\training\answer_eval_pairs\mine_pairs.py mine `
  --repo airk=D:\code\Github\airk --repo bookbuilder=D:\code\Github\bookbuilder `
  --out $R\training\pairs-r1 --before 2026-11-21T15:00:00Z `
  --exclude-notes "$R\scenarios\*\*.json" --exclude-notes "$R\sealed\*\*.json"

# After more scenario files exist: re-apply the note filter only.
python -I -B scripts\arbiter\training\answer_eval_pairs\mine_pairs.py filter `
  --out $R\training\pairs-r1 `
  --exclude-notes "$R\scenarios\*\*.json" --exclude-notes "$R\sealed\*\*.json"
```

Tests build a synthetic git repository and never read a real one.
`AIRK_PAIRS_TEST_ROOT` must name an existing private directory under
`artifacts/`:

```powershell
$env:AIRK_PAIRS_TEST_ROOT = "$R\training\pairs-test-tmp"
python -I -B scripts\arbiter\training\answer_eval_pairs\test_mine_pairs.py
```

## Limits

The definition index and span detection use line regexes and brace counting,
not a parser. A symbol resolves only when one file defines it. Test evidence
is a heuristic for "code under test". Doc and test pairs reflect one
snapshot, so their line ranges may differ from a scenario note taken at
another commit.
