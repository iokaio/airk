# 1.8.32 candidate: the service creates its index catalog (local, Windows)

**13 November 2026. Preparation.** 1.8.32 is 1.8.31 ([record](release-1.8.31.md)) with the fix
for the defect its manual install exposed. Native/extension manifests and lockfiles are
synchronized at 1.8.32 (`python scripts/release-version.py check --expected 1.8.32`). The scope is
still a Windows AIRK Service installer and VSIX only (U99).

**What 1.8.31 did.** The installed 1.8.31 service ran, but:

- `airkctl library list` and the Explorer's Index library failed with `invalid_registry_request`.
- `airkctl admin url` and **Open AIRK Admin** failed with `admin_listener_unavailable`.

So no index could be created. The cause was that the service state had no index catalog at all.
Before the split, the staged installer created and activated the catalog ("Genesis") before the
first service start. The split setup role does not, and the service never did, although the split
contracts assign "Genesis on first start" to it (contracts §4, I3;
`docs/split/inventory/dependencies.md` B1). Without an activated catalog:

- every catalog request fails;
- the service opens no HTTPS listener, so there is no AIRK Admin.

The integration tests did not catch this because their fixtures install through the pre-split
path, which still runs Genesis.

## What changed

- **The installed service creates its index catalog on its first start.** An admitted start (the
  installer's or sign-in startup's) on a service state without a catalog runs the store's
  unchanged Genesis: backup, migration, authority-1 promotion, an in-process qualification of the
  running executable and activation. This happens before any supervisor starts, and no other
  process is started. Later starts leave the catalog as it is. Development `serve` roots are
  unchanged.
- **A service state that cannot be given a catalog is refused clearly.** The start fails with
  `catalog_genesis_refused` instead of running without a catalog. This applies to a state that an
  earlier split build (1.8.24–1.8.31) already ran without one; Genesis admits only an empty
  registry with no history, and that rule is unchanged.
- **`airkctl` finds the installed service.** Commands that needed `--service-runtime-dir` on an
  installed product, such as `airkctl tools list`, now use the installation, as `library` and
  window reads already did.

## Installing over 1.8.31 (or any 1.8.24–1.8.31 split build)

The existing service state has no catalog and has already been used, so 1.8.32 cannot add one to
it. It holds no indexes.

1. Uninstall **AIRK Service** from Windows Settings › Apps. When asked "Also delete the AIRK index
   data, service state, cache and logs?", choose **Yes**.
2. Install the 1.8.32 VSIX and reload VS Code windows.
3. Run the 1.8.32 AIRK Service installer.

## Not in this candidate

The four items U102 moved out of 1.8.31 now move to a later candidate:

- the admin live event stream, rate limits and the operation-table coverage test;
- the Overview, Activity and Recovery page additions;
- code, document and evidence views beside admin Search (`packages/ui`);
- parity tests through the real session transport.

The known limitations of [1.8.31](release-1.8.31.md#known-limitations) still apply.
Installed-editor qualification, macOS/Linux installers, hosted CI and Phase 8 are not run, and
nothing is published.
