# Commit split: one commit per evaluation series

Evaluation tooling, never product. The Answer evaluation commits each series
(the baseline, each change set) as one signed-off commit that holds exactly
that series' code, so checking a commit out gives the code its results came
from ([plan section 5.6](../../../docs/arbiter/final-plan-fp16.md)). Work of
several series often sits uncommitted in one working tree, sometimes mixed
inside one file. `split.py` turns that tree into cumulative patch sets, one
per series, from a spec of file and hunk assignments. The primary agent
reviews the patches and makes the commits; the tool makes none.

Python 3.13, standard library only, plus `git` on `PATH`.

## Safety

- The real index and the working tree are never written. The status read
  runs with `GIT_OPTIONAL_LOCKS=0`; every index operation uses a temporary
  `GIT_INDEX_FILE` in the output folder, removed afterwards.
- Blobs are written into the repository's object database (`git hash-object -w`).
  Unreferenced blobs are ordinary loose objects that `git gc` prunes.
- Every changed or untracked path must match a rule or be listed as excluded;
  an unassigned path stops the run, so nothing is lost silently.
- Patches are LF only. A working-tree file with CRLF line endings is committed
  with LF and listed under `crlf_normalized`.

## Inputs

- `--repo`: the checkout. Its `HEAD` is the base of the first series.
- `--spec`: the split rules (JSON, schema `airk.answer-eval.commit-split-spec/1`).
  [`answer-eval-r1.json`](answer-eval-r1.json) holds the rules of the first
  evaluation round.
- `--private-root`: the folder that the spec's `roots` are relative to (for
  the Answer evaluation, its private folder `R`). Needed only by rules that
  read a retained snapshot.
- `--out`: output folder.
- `--export-check` (optional): an empty or missing folder for the export
  check below; it is deleted afterwards.

## Spec

```json
{
  "schema": "airk.answer-eval.commit-split-spec/1",
  "series": ["A", "B", "C"],
  "roots": {"name": {"dir": "relative/folder", "flatten": false}},
  "excluded": {"path": "reason"},
  "globs": {"scripts/tool/*": {"series": "A", "reason": "..."}},
  "files": {
    "path/whole.rs": {"series": "B", "reason": "..."},
    "path/mixed.rs": {"reason": "...", "hunks": {"A": "...", "B": "..."},
                      "states": {"A": "HEAD", "B": {"root": "name"}}}
  }
}
```

- `series` are applied in order: the first to `HEAD`, each later one to the
  state before it. The **last series is always the working tree**.
- `{"series": X}`: the whole change of the file lands in series X (the file
  keeps its `HEAD` content, or stays absent, before X). This also covers new
  and deleted files.
- `{"states": {...}}`: the file's content in each series before the last.
  A series not named keeps the previous content (`HEAD` before the first).
  Sources:
  - `"HEAD"`, `"prev"`, `"final"` (the working-tree content);
  - `{"root": name}`: the same path under a retained snapshot folder of
    `roots` (with `flatten`, `a/b/c.rs` is the file `a__b__c.rs`);
  - `{"from_final": {...}}`: the working-tree content with `revert` (indices
    of the `HEAD`-to-working-tree change blocks of Python `difflib`, in order)
    restored to `HEAD`, the 1-based inclusive working-tree line ranges of
    `drop` removed, then the exact text replacements `replace`
    (`[old, new, expected count]`) applied; `absent` lists text that must no
    longer occur.
- `files` wins over `globs`; the first matching glob applies (`*` also
  matches `/`).
- `hunks` and `reason` are free text copied into the manifest.

Line ranges and block indices refer to the working tree when the rule was
written. When a mixed file changes, re-check its rule: a range outside changed
text, a replacement count that no longer matches or a leftover `absent` text
stops the run. To prepare a later series, append its name to `series`, move
the rules of its files to it, and re-run.

## Outputs

In `--out`:

- `<series>.patch`: `git diff --binary --full-index` from the previous state
  to this series' state, one per series, applied in order with `git apply`.
- `manifest.json`: every assigned path with its rule, reason and hashes (for
  split files, the SHA-256 of each state), the excluded paths, the CRLF files,
  the git tree of each state, each patch's SHA-256, size and file count, and
  the verification results.

## Verification

Every run reapplies the patches in order with `git apply --cached` to a fresh
temporary index and requires the same trees, runs `git diff --check` on each
step and refuses a CR byte in any patch. `--export-check` also exports `HEAD`
with `git archive`, applies each patch outside the repository, compares the
export byte for byte with each state's tree, and compares the last state with
the working tree (only CRLF-to-LF differences are allowed).

Compiling each state is a separate step: run the scoped Clippy of the
[validation matrix](../../../CONTRIBUTING.md#agent-validation-matrix) on an
export of that state, as the Cargo owner.

## Use

```powershell
python -I -B scripts/arbiter-answer-eval/commit_split/split.py `
  --repo . --spec scripts/arbiter-answer-eval/commit_split/answer-eval-r1.json `
  --private-root <R> --out <R>\commit-prep --export-check <R>\commit-prep\export
```

Tests (synthetic git repositories only; set `AIRK_COMMIT_SPLIT_TEST_ROOT` to
create them under a chosen folder):

```powershell
python -I -B -m unittest discover -s scripts/arbiter-answer-eval/commit_split
```
