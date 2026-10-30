# Provide a reviewable cleanup inventory and optional purge of retained AIRK data

Native uninstall deliberately preserves indexes and service metadata. Users who are retiring AIRK need a way to discover and remove its remaining data without guessing directory names or risking their source files.

## Current behavior and use case

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. [Native uninstall](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-cli/src/runtime_retirement.rs#L692) explicitly returns `project_data_preserved` and `service_metadata_preserved`. Existing/default/custom data roots, logs and installation metadata are not comprehensively erased by extension removal. [Extension cleanup](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/extensionUninstall.ts#L100) does attempt to remove recorded Workbench sessions and recognized project-local agent entries; user-wide manual entries are outside that sweep.

The separate [project removal contract](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/docs/service-project-retirement.md#L3) already deletes matched project indexes/settings after draining workers. A cleanup feature should reuse that ownership model. This request does not propose changing ordinary uninstall into automatic destructive deletion.

## Proposed acceptance criteria

- **AC1:** A read-only inventory identifies owned retained data, service state, logs, extension state/settings and installation metadata, including recorded custom/legacy roots. Unknown locations and incomplete inventories are explicit.
- **AC2:** Users can separately authorize removal of selected owned data, with paths, scope and consequences shown first; keeping indexes remains available.
- **AC3:** Cleanup drains the relevant owners, resumes safely after interruption and preserves source trees, unrelated files, other installations and organization-owned payloads. Unknown or changed items produce a visible refusal.
- **AC4:** The final result enumerates deleted, deliberately retained and unresolved items, including manual agent configurations that require user action and temporary uninstall diagnostics.
- **AC5:** Fictional tests cover default/custom roots, prior relocation, preserved damaged payloads, partial failure and a second cleanup attempt. Documentation makes clear that this is logical deletion, not secure erasure of backups or external copies.
