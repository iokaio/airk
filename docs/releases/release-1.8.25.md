# 1.8.25 candidate — split installer rebuild (local, Windows)

**12 November 2026; preparation.** 1.8.25 is 1.8.24
([record](release-1.8.24.md)) with one correction found by its manual
installation. Native/extension manifests and lockfiles are synchronized at
1.8.25 (`python scripts/release-version.py check --expected 1.8.25`). The bump
follows the standing per-installer authorization: 1.8.24 is installed on the
test machine, so the rebuilt payload takes a new patch number.

## What changed

- **Pre-split retirement works again.** The 1.8.24 install completed, but its
  setup left the pre-split installation in `%LOCALAPPDATA%\AIRK\runtime`
  running ("captured runtime manager command differs from its installation").
  The split's interim selector (U93) had changed the managed-service command,
  and setup compared every pre-split ownership receipt against the new form.
  Setup now accepts the exact pre-split command for ownership and removal
  only; it never starts a predecessor.
- **The installer reports what setup leaves.** When setup cannot remove an
  earlier installation it still completes, and the installer now shows the
  items left and why (`leftovers.txt`, written by setup on close).

Everything else is as described for 1.8.24, including the Windows default
locations under `%LOCALAPPDATA%\AIRK Service\` (U96).

## Before installing over 1.8.24

Install the 1.8.25 VSIX (or close VS Code) first, so the 1.8.23 extension's
`airk service session` does not start the old service again while setup
retires it. Running the installer again over 1.8.24 is an upgrade of the same
locations; setup retries the pre-split retirement.

## Not yet in this candidate

As for 1.8.24. The plan disclosure still predicts a predecessor's removal
without running the ownership check that removal applies (follow-up).
Installed-editor, macOS/Linux and full release qualification are pending;
nothing is published.
