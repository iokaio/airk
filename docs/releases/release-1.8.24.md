# 1.8.24 candidate — first split build (local, Windows)

**12 November 2026; preparation.** 1.8.24 is the first candidate of the AIRK
split ([split plan](../split/split-plan.md)): `airk` is the service only,
`airkctl` is the command-line client, the VS Code extension is a UI-only
universal VSIX that reaches the service only through `airkctl`, and the service
serves the admin pages at `https://127.0.0.1:<port>/admin/`. It is also the
split threshold (`split_receipts.SPLIT_VERSION`): packagers and local wrappers
refuse earlier versions.

Native/extension manifests and lockfiles are synchronized at 1.8.24
(`python scripts/release-version.py check --expected 1.8.24`). The bump follows
the standing per-installer authorization (10 November 2026); it precedes the
local Windows installer and VSIX build.

## What this candidate contains

- **AIRK Service installer (Windows, per user, no elevation):** Inno Setup
  installer placing `bin\airkctl.exe` and `service\airk.exe`, signed locally
  with the CI Artifact Signing account through `scripts/local_signing.py`
  (U70; receipts say `provenance: local`). Setup runs the private setup role:
  placement verification (Authenticode publisher and identity EKU), install
  record and pointer, login startup, optional certificate trust with consent.
- **UI-only VSIX:** one universal package with no native content.
- **Signature checks:** `airkctl` verifies both installed executables against
  the install record on every discovery; revocation is not fetched.

**Installer rebuild (12 November 2026):** the first 1.8.24 installer stopped
on the data-locations page with `setup_operation_refused` when the user's Temp
folder granted another account access (here a Codex sandbox group): Inno's
temporary folder inherits that ACL, and setup correctly refuses an operation
file another account can change. The installer now keeps its operation folders
and the setup image under one folder below its temporary folder whose
inherited entries are removed (`icacls /inheritance:r`) and that grants only
the user, SYSTEM and Administrators; the image is copied there and verified
before it runs. Rebuilt at the same version (nothing had been installed).

**Second installer rebuild (12 November 2026):** the same machine then
refused every default location, because the Codex sandbox group's read access
is inherited throughout `%LOCALAPPDATA%`. Windows defaults now live under one
new root, `%LOCALAPPDATA%\AIRK Service\` (`program\`, `data\indexes`,
`state\service`, `cache\service`, `logs\service`), which the installer creates
private. On Windows, the existing parent of a location that does not exist yet
must only be not writable by others (read-only grants are accepted; a write
grant is refused with the account and the `icacls` command that removes it);
an existing location folder must still be private. Setup creates the program
folder owner-only before placement, as `airkctl` and the extension require
(split plan U96). macOS and Linux are unchanged. Rebuilt at the same version.

## Not yet in this candidate

Reads under a window or admin scope selection (W4/A2) answer
`not_yet_served`; the in-memory selection authority's written specification is under its
independent security review (U38/U55). macOS login startup
(SMAppService) and macOS/Linux installers are not built here. Release
workflows carry `TODO(split 10.5.2)` for split assembly. Installed-editor,
macOS/Linux and full release qualification are pending; nothing is published.

The [task ledger](../simplify/one-process-ledger.md) records each step's
evidence and the build receipts.
