# Terminal service-registry schema fixtures

Purpose: the expected `sqlite_master` objects of the terminal catalog layouts
of change-log entry 68 part 3 (reviewed grants removed). The Rust schema
identity tests in `crates/airk-store` compare the layouts that fresh genesis,
the policy and split promotions and the upgrade step produce with these files.

Inputs: the retained airk-state/1 captures in `../airk-state-1` (1.8.16,
`43d7005d`). They are never rewritten.

Outputs: one `objects.json` and one `meta.json` per terminal layout, and
`manifest.json` naming the removed tables and the captures each layout is
derived from. The `-schema13` layouts are those of registries at user version
13: an armed runtime contract, or a former standalone-adoption layout.

Use: `python scripts/dev/derive_terminal_registry_schema.py` writes them;
`--check` compares. The derivation removes, from each retained capture, every
object whose table is a removed table. It shares no code with the Rust upgrade
step, so the comparison is independent of the code under test.
