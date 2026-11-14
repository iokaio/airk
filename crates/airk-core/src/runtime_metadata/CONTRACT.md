# Split runtime metadata (C7)

Owner: `airk-core::runtime_metadata`. This pure Rust owner consumes the existing
C7 `airk-runtime.json` schema 3, without adding fields or changing its meaning.
The independently authored `fixtures/split/receipts/pair_metadata_3.json`
predates this native consumer. Python artifact validators remain consumers,
checked against Rust-emitted `runtime_metadata_cases.json` semantic cases.

`SplitRuntimeMetadata` requires exactly `schema`, `version`, `os`, `arch`,
`airk_sha256`, `airkctl_sha256`, `i1_protocol`, `i2_contract` and
`admin_inventory_sha256`. Unknown, duplicate or missing fields refuse.
Schema is 3; version is three canonical unsigned decimal components; supported
targets are Windows/Linux/macOS x86_64 and macOS aarch64. Both interfaces are
integers 1–65535. Service, CLI and Admin inventory digests use the Core canonical
lowercase SHA-256 type, and service/CLI identities must differ.

Interface fields are declared artifact identities, not proof that the current
client implements those interface versions. Capability/admission consumers
retain their own version checks. `validate_target` compares an explicitly
supplied approved version and host target; Core reads neither environment nor
filesystem and performs no signature, ownership or execution checks.

The bounded decoder/encoder accepts at most 4096 bytes. Canonical encoding is
compact UTF-8 JSON in declaration order with no trailing newline. Packaging may
indent the file; native maintenance identity uses canonical full metadata.
Schema 1/2 remain owned by Host and keep their exact old serialization and
maintenance digest domain. No new persisted state, wire envelope or digest
domain is introduced here.

The fixed payload image paths are also owned here: Windows `airk.exe` and
`airkctl.exe`, Linux `airk` and `airkctl`, and the corresponding names under
`AIRK.app/Contents/MacOS/` on macOS. They never come from caller-supplied paths.
Host must validate actual sibling executable bytes; metadata itself does not
authorize execution or prove embedded Admin assets.

Focused tests: `cargo nextest run --profile ci -p airk-core --no-default-features --lib --locked --timings -E 'test(runtime_metadata::tests::)'`.
Generate the semantic fixture explicitly with
`AIRK_WRITE_RUNTIME_METADATA_CASES=1` and the single test
`runtime_metadata_independent_c7_cases_and_emitted_semantics`, then rerun with
that variable absent to check drift. Each selection must be nonzero. Existing
C7 fixture outcomes remain independently specified rather than generated from
the new Rust implementation.
