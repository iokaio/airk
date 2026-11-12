# 1.8.29 candidate — stuck pre-split retirement and installer progress (local, Windows)

**12 November 2026; preparation.** 1.8.29 is 1.8.27
([record](release-1.8.27.md)) with one setup fix and one installer change.
Native/extension manifests and lockfiles are synchronized at 1.8.29
(`python scripts/release-version.py check --expected 1.8.29`). The bump
follows the standing per-installer authorization; 1.8.28 was not used. The
1.8.27 installer was started and cancelled on the "Earlier AIRK
installations" page; nothing was installed from it.

## What changed

- **An interrupted retirement of the pre-split AIRK no longer blocks every
  later installer.** Setup removes the pre-split `%LOCALAPPDATA%\AIRK`
  installation through its replacement journal
  (`%LOCALAPPDATA%\AIRK\.runtime.replacement-1\receipt.json`). The 1.8.24
  install wrote that journal, was refused before it stopped or moved anything,
  and its setup operation was deleted with the installer's temporary folder.
  Every later installer has a new operation identity, so it was refused with
  `replacement_in_progress: resume the original installation attempt` and
  the old installation was "left and reported". Now a journal that never
  reached its freeze (no executable admitted, every location at its original
  path with its recorded identity and owner files) is taken over by the next
  installer, which plans the removal afresh and shows it for acknowledgement.
  A journal past its freeze still resumes only under its own operation.
- **The installer shows progress while it checks the data locations.** After
  Next on "AIRK data locations", a "Checking AIRK data locations" page with a
  moving progress bar stays up until setup's check finishes, instead of the
  window appearing to do nothing.

The VS Code extension is unchanged from 1.8.27 apart from its version number.

## Installing over 1.8.24

Install the 1.8.29 VSIX in VS Code and reload (or close VS Code). Stop the
pre-split service if it is running, then run the installer with the same
folders. The "Earlier AIRK installations" page should list
`%LOCALAPPDATA%\AIRK\runtime` for removal without "cannot be removed
automatically"; acknowledge the removal.

## Not yet in this candidate

As for 1.8.27. Installed-editor, macOS/Linux and full release qualification
are pending; nothing is published.
