# Let users review persistent installation and indexing effects before first setup

A newly installed AIRK extension can use existing VS Code workspace trust to install a per-user native runtime, edit shell startup files, register folders and start indexing. A user trying a retrieval panel may not expect those persistent effects or background work after closing the editor.

## Current behavior and use case

Source reviewed: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. This is a product-control proposal, not an allegation that the documented automatic setup is malicious or violates an existing consent promise.

[Automatic setup](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/serviceExtension.ts#L450) runs in trusted workspaces unless User-scope `airk.cli.autoInstall` is false. [CLI setup](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/cliInstall.ts#L110) edits profiles for several shell families, even when they are not the user's current shell, or edits Windows user PATH. New folders are registered for indexing/watching. Recognized existing project AIRK agent entries can be migrated; this does not create arbitrary new user-wide agent connections. The service's lifetime is independent of an editor window. Login startup is a separate explicit operation.

The existing opt-out is useful but requires knowing the setting before activation. A first-use review should make the effects and intended source scope understandable before they occur.

## Proposed acceptance criteria

- **AC1:** Before a first installation/indexing action, the user can review native storage locations, source folders, background lifetime, shell/PATH changes and applicable agent-entry migration.
- **AC2:** The user can decline or defer setup without persistent installation, profile edits or new indexing registrations. Whether all native discovery also stops is a separate control decision.
- **AC3:** Shell integration can be selected independently of indexing; unused shell profiles need not be created or edited merely to use the extension.
- **AC4:** Saved choices are honored across windows and restarts; existing installations and organization-managed policies retain their ownership and lifecycle rules.
- **AC5:** Installed-editor tests demonstrate accepted, deferred and disabled setup in trusted/untrusted workspaces. Documentation distinguishes per-user service ownership from editor profiles and login startup.
- **AC6:** Maintainers record how home directories and filesystem/volume roots should be presented before registration. If a specific review or warning is adopted, tests show the scope before registration and let the user decline that root without declining other folders. No size-based pre-scan is assumed by this proposal.

The exact interaction and migration policy remain a maintainer decision; these criteria describe the proposed outcome.
