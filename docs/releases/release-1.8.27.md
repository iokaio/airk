# 1.8.27 candidate — Management version line (local, Windows)

**12 November 2026; preparation.** 1.8.27 is 1.8.26
([record](release-1.8.26.md)) with one extension change. Native/extension
manifests and lockfiles are synchronized at 1.8.27
(`python scripts/release-version.py check --expected 1.8.27`). The bump
follows the standing per-installer authorization. 1.8.26's VSIX was
installed against a 1.8.24 AIRK Service; its installer was not run.

## What changed

- **Every Management tab names the installed versions.** The first line reads
  `VS Code extension <version> · AIRK Service <version>`. The service version
  is the one a connected service reports in its health response. When no
  service is installed, running or reachable, the line still shows the
  extension version and reports the service as `not connected`; while the
  window is untrusted or the state is still loading it reports `not observed`.
  Before this change the extension version appeared only at the bottom of the
  Service tab, and only after the service state had loaded.

The native service and `airkctl` are unchanged from 1.8.26 apart from their
version number.

## Installing over 1.8.24 or 1.8.26

Install the 1.8.27 VSIX in VS Code and reload (or close VS Code), then run the
installer with the same folders. Setup retires the pre-split
`%LOCALAPPDATA%\AIRK\runtime` installation.

## Not yet in this candidate

As for 1.8.26. Installed-editor, macOS/Linux and full release qualification
are pending; nothing is published.
