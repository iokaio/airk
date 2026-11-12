# 1.8.30 candidate — upgrades with VS Code open (local, Windows)

**12 November 2026; preparation.** 1.8.30 is 1.8.29
([record](release-1.8.29.md)) with the fixes from its manual install.
Native/extension manifests and lockfiles are synchronized at 1.8.30
(`python scripts/release-version.py check --expected 1.8.30`). The bump
follows the standing per-installer authorization. The 1.8.29 installer removed
the pre-split AIRK on the test machine and then stopped before placing files;
nothing from it was installed.

## What changed

- **Upgrading no longer needs VS Code closed.** Before setup stops an
  installed AIRK Service it will replace or remove, it writes a replacement
  notice (`replacing.json` beside the install pointer; I4
  `airk-install-replacing/1`, at most ten minutes). Open VS Code windows with
  this extension end their `airkctl` session, show "AIRK Service is being
  updated" and run nothing until setup finishes; then they verify the new
  installation and reconnect. Setup removes the notice when it completes or
  fails. A fresh installation never needed VS Code closed. An extension older
  than 1.8.30 does not read the notice, so update the extension first (or
  close VS Code) when upgrading from an earlier split build.
- **Setup's check for running AIRK programs no longer fails on a process that
  exits while it looks.** A process that exits mid-scan is not counted as
  unobserved, and an incomplete scan is repeated within the same 15 seconds
  instead of stopping the installer (the 1.8.29 "process_observation_incomplete"
  refusal).
- **Old agent entries are cleaned up.** With AIRK Service found, the extension
  removes this window's pre-split stdio AIRK entries (for example
  `airk-launcher.exe … mcp` in `.mcp.json` or `.codex/config.toml`) whose
  folder has no index, records them and says so. The service serves HTTPS
  only; reconnect agents through AIRK Admin and **Connect Coding Agent** once
  an index exists. An entry whose folder already has an index is left for that
  command.
- **Download links.** The Download command, Explorer's not-installed message,
  the Marketplace description and README point to
  <https://airk.ioka.io/#downloads>.
- **Installer wording.** The data-locations page says to update the extension
  (or close VS Code) first, and why.

## Installing over the 1.8.24 service

1. Install the 1.8.30 VSIX and reload VS Code windows.
2. Run the installer with the same folders. VS Code can stay open.

## Not yet in this candidate

As for 1.8.29. The downloads page itself still lists the pre-split v1.8.7
assets and no AIRK Service installer; publishing is a separate, authorized
step. Installed-editor, macOS/Linux and full release qualification are
pending; nothing is published.
