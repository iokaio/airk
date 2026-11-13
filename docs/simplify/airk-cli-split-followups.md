# airk-cli split: follow-up crate work (after 1.8.31)

Status: **not started.** This work starts only after the 1.8.31 Windows installer and VSIX have
been tested, and only with the user's approval. It is Track S (structural) work under
docs/simplify-plan.md. It follows §4.1 (extraction criteria) and the "untangle in place before
extracting" rule. Recorded on 12 November 2026, from a review of `airk-installer` and
`airk-supervisor` after split steps 3e and 3f (docs/simplify/airk-cli-split.md).

## Conclusion of the review

Neither `airk-installer` nor `airk-supervisor` should be split into more crates. Each has one
misplaced dependency edge that should be fixed in place.

Measured with `python scripts/dev/module_graph.py --crate <crate>`:

| Crate | Production lines | Modules | Cycles |
|---|---|---|---|
| `airk-installer` | 22,331 | 8 | `runtime_install` ↔ `service_manager` ↔ `universal_launcher` ↔ `control_client` (19,328 lines) |
| `airk-supervisor` | 12,959 | 10 | none |

### Why no further extraction now (§4.1)

- **Cycle.** The installer cycle above passes through every candidate that would be worth
  separating: the OS service managers, the launcher and the control client.
- **No stable interface.** Neither crate has a `CONTRACT.md`.
- **No independent consumers.**
  - The installer's legacy cluster (`runtime_legacy_origin`, `legacy_startup`, `legacy_processes`,
    `manual_start`, and `runtime_install::{legacy_caches, legacy_launchers, legacy_maintenance,
    http_legacy}`, about 4.5k lines) is used only by the installer and `airk-cli`. It is live
    clean-break duty, not dead code.
  - The supervisor's worker modules (`project_worker`, `source_worker`, `derived_worker`,
    `worker_*`, about 4.2k lines) are used only by the supervisor and the role dispatcher. They run
    in the same binary, and the supervisor depends on them, so a separate crate would still build
    in series after them.
- **No testing benefit.** `airk-supervisor`'s 84 tests already run in 54 s. `airk-installer`'s 414
  tests run in 42 s.

## Work items

### F1. Installation lookups out of `airk-installer`

**Problem.** `airk-frontend`, `airk-https` and `airk-supervisor` depend on the 22k-line installer
only for read-side installation lookups and the control client:

- `runtime_install::{caller_install_root, installed_control, installed_control_cooperative,
  installed_metadata_control, rebuild_observation_for_control, default_project_data,
  InstallManifest}`
- `runtime_install::{SetupCancellation, SetupAdmission}`
- `runtime_install::http_legacy::{validate, validate_registry}`
- `control_client::{request, request_for_instance, request_for_version_and_instance}`

As a result, any installer edit recompiles those three crates and everything above them, and the
installer sits on the build's critical path before them.

**Steps.**

1. In place: give these lookups one owned module and facade inside `airk-installer`. Break the
   `control_client` ↔ `runtime_install` edge there. `control_client` currently uses
   `runtime_install::require_baseline_control` and `InitializedProbe`.
2. Re-measure the module graph. Once the module has no cycle and no dependency on installer
   lifecycle code, move it down into `airk-host`. That crate already owns `install_layout`,
   `service_locations` and `user_paths`. Do not create a new crate unless §4.1 is met and the
   user approves.
3. Move `SetupCancellation` / `SetupAdmission` and `http_legacy` validation only if the review
   in step 1 shows they belong with the lookups. Otherwise record why the edge stays.

**Done when:**

- `airk-frontend`, `airk-https` and `airk-supervisor` no longer list `airk-installer` as a
  dependency;
- `module_graph.py --crate airk-installer` reports no cycle through the moved code;
- library tests are identical by identity in the default and `local-inference` configurations;
- Cargo `--timings` reports from before and after are recorded. The compile-time benefit must be
  measured, not assumed.

### F2. `runtime_metadata` below the supervisor

**Problem.** `airk-frontend` depends on `airk-supervisor` only for
`runtime_metadata::{observe_process, unavailable}`, used in `project_client.rs`.
`runtime_metadata` (452 lines) uses `project_worker` four times.

**Steps.**

1. Separate the process-observation part that the frontend needs from the part that needs
   `project_worker`.
2. Move the observation part to the lowest crate that owns its inputs, probably `airk-host`.

**Done when:** `airk-frontend` no longer depends on `airk-supervisor`, and the test identities
are unchanged.

### F3. Component contracts

Write a `CONTRACT.md` for `airk-installer` and `airk-supervisor`. Each one covers:

- the interface;
- owned state;
- invariants;
- lock and fence order;
- the T0 command.

Without these, no sub-part of either crate can meet §4.1 criterion 2.

### F4. Admin bundle location

`crates/airk-cli/admin-assets/` stays in `airk-cli` for 1.8.31. `crates/airk-https/build.rs` and
`admin_plane/tests.rs` read it through `../airk-cli/admin-assets` (split step 3h). Moving it
under `airk-https` means updating all of the following in one step:

- `scripts/build-admin-assets.py`;
- the admin package's build output directory;
- `scripts/ci-plan.py` `ADMIN_PREFIXES`;
- `scripts/build-service-runtime.py` inputs;
- `docs/admin-pages.md`, `docs/ci-and-builds.md` and `admin_plane/CONTRACT.md`.

The digest check must still pass against the moved bundle.

### F5. Unused dependencies of the split crates

`extract_crate.py` gave each new crate the dependency set of the old `airk-cli`, reduced only to
workspace crates that come earlier in the order. As a result, `scripts/dev/boundary-exceptions.json`
allows edges that may have no user. For example, `airk-host` lists `airk-kernel`, `airk-mcp` and
`airk-retrieval`.

**Steps.** For each crate, remove each workspace and external dependency that no source references,
prove it with a clean Clippy run in both configurations, and shrink the allowed edges in the same
change.

**Done when:** every allowed edge of the ten crates has at least one user.

## Rules for this work

- Keep each change structural: moves, visibility and boundaries only, with no behavior change and
  no test assertion edits. Reproduce test identities with `scripts/dev/compare_test_sets.py` in
  both configurations.
- Keep one item per change, and record the evidence for each in docs/simplify/airk-cli-split.md.
- Do not delete legacy or clean-break code on the basis of its name. Delete only with the evidence
  the simplify plan requires.
