# CPU Focus row replay (L4 runtime.rs movement)

Reproduces the inverse-replay proof recorded in
`crates/airk-retrieval/src/arbiter/runtime/cpu_source_transition/CONTRACT.md`,
"Answer loop admission supplement": that the live `runtime.rs` differs from the
last reviewed row (`7b465076`, commit `f3978a12`) exactly by the private
evaluation routing (`179bb1da`, now removed), L5's admission (`881787f0`) and
the L4 hooks retained here as `l4-runtime-hooks.patch`.

- **Inputs:** the repository's Git history (commits `f3978a12`, `179bb1da`,
  `881787f0`), the live `runtime.rs` and `l4-runtime-hooks.patch`.
- **Outputs:** one line per check on stdout; exit 0 only when every
  LF-normalized SHA-256 matches its recorded value and the live source names
  no routing module. Nothing is written to the repository; copies are made in
  a fresh temporary directory that is removed on exit.
- **Use:** `python -I scripts/arbiter/evaluation/runtime_row_replay/replay.py --repo .`
  from the repository root, with `git` on the path.

A later change to `runtime.rs` makes the `live` check fail on purpose: such a
change needs a new reviewed row and an updated patch and expected values, never
a silent re-pin. The Rust control
`loop_admission_supplement_moves_the_runtime_row_without_relabelling_legacy_evidence`
pins the live source and the new row value; this script proves the relation to
the reviewed predecessors.
