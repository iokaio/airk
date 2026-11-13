# Release records

The 1.8.23 manual installation reported an HTTP cleanup refusal. Its subsequent
native fix is an untested source checkpoint; no replacement installer was built.
Repair and qualification stopped at the user's direction on 10 November 2026.

- [1.8.32 split candidate](release-1.8.32.md): the installed service creates its index catalog on first start (1.8.31 never did, so no index could be created and AIRK Admin had no listener); `airkctl` commands find the installed service without `--service-runtime-dir`. Manifests synchronized at 1.8.32; the four U102 items move to a later candidate.
- [1.8.31 split candidate](release-1.8.31.md): index creation completed: admin Index library and Usage pages, `airkctl library` and service management, typed confirmation (U98), Storage cleanup, Windows sign-in startup, window search scopes, read-only Explorer Indexing with Open AIRK Admin (U97); `airk-cli` split into ten crates. Manifests synchronized at 1.8.31; four planned items moved to 1.8.32 (U102).
- [1.8.30 split candidate](release-1.8.30.md): 1.8.29 plus upgrades with VS Code open (I4 replacement notice), a setup process scan that tolerates exiting processes, cleanup of pre-split stdio agent entries and the downloads link. Manifests synchronized at 1.8.30; installed and platform qualification pending.
- [1.8.29 split candidate](release-1.8.29.md): 1.8.27 plus setup taking over a pre-split retirement that stopped before moving anything, and a progress page while the installer checks data locations. Manifests synchronized at 1.8.29 (1.8.28 unused); installed and platform qualification pending.
- [1.8.27 split candidate](release-1.8.27.md): 1.8.26 plus a version line on every Management tab that names the installed extension version even when no AIRK Service is found. Manifests synchronized at 1.8.27; installed and platform qualification pending.
- [1.8.26 split candidate](release-1.8.26.md): 1.8.25 plus setup's refusal while a pre-split AIRK extension runs and a Windows process scan that no longer refuses upgrades. Manifests synchronized at 1.8.26; installed and platform qualification pending.
- [1.8.25 split candidate](release-1.8.25.md): 1.8.24 plus the pre-split retirement fix and the installer's report of anything setup leaves behind. Manifests synchronized at 1.8.25; installed and platform qualification pending.
- [1.8.24 first split candidate](release-1.8.24.md): `airk` service only, `airkctl` client, UI-only universal VSIX, admin pages, locally signed Windows AIRK Service installer. Manifests synchronized at 1.8.24; installed and platform qualification pending.
- [1.8.23 Windows debug manual-test candidate](release-1.8.23.md): full owned uninstall through 1.9.0 inclusive, including the same build; empty successor without automatic indexing; ordinary reopen reconnects; Reconnect uses existing reviewed single/combined scopes. Source manifests and lockfiles are synchronized at 1.8.23; the Windows debug native package and VSIX are built and package-verified. Installed-editor and platform qualification remain pending.
- [1.8.22 local VSIX candidate](release-1.8.22.md) (failed manual-test artifact; corrected source prepared for 1.8.23): one resident `airk.exe` (the service), with workers inside it; one session per VS Code window; direct HTTPS agents and credential helper. Reconnect created an index before failing its scope check. Original artifact identity and evidence are retained.
- [1.8.20 local VSIX candidate](release-1.8.20.md) (dated history, corrected by 1.8.22): a fresh install registers login startup, so the service keeps running and starts again after a reboot; Explorer restarts an idle-exited service and flags agent configurations that name a removed launcher.
- [1.8.19 local VSIX candidate](release-1.8.19.md) (dated history; its "agent configurations outside VS Code are not edited" is reversed in 1.8.22): until 1.9.0, installing over an earlier version stops and removes it and installs fresh; no index is created until you add one.
- [1.8.18 local VSIX candidate](release-1.8.18.md): focused simplification closeout, unchanged state baseline and explicit qualification evidence for Windows manual testing.
- [1.8.16 clean-break baseline](release-1.8.16.md): persisted `airk-state/1` baseline and retained failed installed-editor acceptance.
- [1.8.14 implementation record](release-1.8.14.md): unpublished One AIRK implementation in progress; C0 source metadata and later acceptance evidence remain distinct.
- [1.8.12 manual-test working build](release-1.8.12.md): persistent Explorer notice dismissal, automatic AIRK extension replacement and current local qualification.
- [1.8.11 manual-test working build](release-1.8.11.md): simplified Indexing, index deletion and its original Windows package evidence.
- [1.8.8 manual-test checkpoint](release-1.8.8.md): retained phase identities and validation before later manual acceptance and promotion.
- [Planned 1.9.0](release-1.9.0.md): release target, original development checkpoints and performance/relevance controls; historical tested identities remain unchanged.
- [1.8.7](release-1.8.7.md): all unpublished 1.8.0-1.8.6 changes, complete CLI/MCP/Workbench read results, configured concurrency and dated validation.

Versioned release notes and dated release evidence live here. Each record retains its original version, date and qualification limits; a record alone does not establish publication. Current release procedures remain in the [release workflow](../release-workflow.md).

## Versions

- [1.7.6](./release-1.7.6.md) — published stable release (2026-10-31): review 02 follow-up (setup review, native execution control, uninstall cleanup, data purge, advisory gate)
- [1.7.3](./release-1.7.3.md) — published stable release: Marketplace images, release promotion and repository review fixes

- [1.7.2](./release-1.7.2.md) — Explorer toolbar and panel project selection candidate
- [1.7.1](./release-1.7.1.md) — compact Workbench controls candidate
- [1.7.0](./release-1.7.0.md) — unreleased Workbench candidate
- [1.6.4](./release-1.6.4.md)
- [1.6.3](./release-1.6.3.md)
- [1.6.2](./release-1.6.2.md)
- [1.6.1](./release-1.6.1.md)
- [1.6.0](./release-1.6.0.md)
- [1.5.12](./release-1.5.12.md)
- [1.5.11](./release-1.5.11.md)
- [1.5.10](./release-1.5.10.md)
- [1.5.9](./release-1.5.9.md)
- [1.5.8](./release-1.5.8.md)
- [1.5.7](./release-1.5.7.md)
- [1.5.6](./release-1.5.6.md)
- [1.5.5](./release-1.5.5.md)
- [1.5.4](./release-1.5.4.md)
- [1.5.3](./release-1.5.3.md)
- [1.5.2](./release-1.5.2.md)
- [1.5.1](./release-1.5.1.md)
- [1.5.0](./release-1.5.0.md)
- [1.4.4](./release-1.4.4.md)
- [1.4.3](./release-1.4.3.md)
- [1.4.2](./release-1.4.2.md)
- [1.4.1](./release-1.4.1.md)
- [1.4.0](./release-1.4.0.md)
- [1.3.2](./release-1.3.2.md)
- [1.3.1](./release-1.3.1.md)
- [1.3.0](./release-1.3.0.md)
- [1.2.5](./release-1.2.5.md)
- [1.2.4](./release-1.2.4.md)
- [1.2.3](./release-1.2.3.md)
- [1.2.2](./release-1.2.2.md)
- [1.2.1](./release-1.2.1.md)
- [1.2.0](./release-1.2.0.md)
- [1.1.4](./release-1.1.4.md)
- [1.1.3](./release-1.1.3.md)
- [1.1.2](./release-1.1.2.md)
- [1.1.1](./release-1.1.1.md)
- [1.1.0](./release-1.1.0.md)
- [1.0.0](./release-1.0.0.md)

## Dated records

- [October 17, 2026 release summary](releases-2026-10-17.md)
- [Historical 0.1.0 review and release checklist](release-checklist-0.1.0.md)

- [1.7.1 local candidate — compact Workbench and first opening](release-1.7.1.md)
