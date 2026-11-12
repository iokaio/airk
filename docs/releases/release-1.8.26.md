# 1.8.26 candidate — split installer rebuild (local, Windows)

**12 November 2026; preparation.** 1.8.26 is 1.8.25
([record](release-1.8.25.md)) with two corrections. Native/extension
manifests and lockfiles are synchronized at 1.8.26
(`python scripts/release-version.py check --expected 1.8.26`). The bump
follows the standing per-installer authorization. 1.8.25's installer was
built but not installed; it is superseded.

## What changed

- **Setup waits for an old AIRK extension.** A pre-split AIRK extension
  (1.8.23 and earlier) sets the old AIRK up again after setup removes it. While
  VS Code (or another editor) runs one, setup's plan reports it, the data page
  does not advance, and the preparation step refuses
  (`old_extension_running`). The data page also says to update the extension
  or close VS Code first, because setup cannot see an extension in a closed
  editor. The macOS and Linux installers stop at the same refusal.
- **Upgrades over an installed split build are no longer refused on
  Windows.** Before replacing installed files, setup checks that no process
  runs them. For a user without admin rights that check could never complete,
  because other accounts' and system processes cannot be opened, so every
  upgrade would have stopped with `process_observation_incomplete`. Processes
  of another account, and processes that exited during the check, no longer
  count as unobserved.

Everything else is as described for 1.8.25 and 1.8.24.

## Installing over 1.8.24

Install the 1.8.26 VSIX in VS Code and reload (or close VS Code), then run the
installer with the same folders. Setup then retires the pre-split
`%LOCALAPPDATA%\AIRK\runtime` installation.

## Not yet in this candidate

As for 1.8.25. Installed-editor, macOS/Linux and full release qualification
are pending; nothing is published.
