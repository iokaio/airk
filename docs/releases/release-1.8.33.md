# 1.8.33 candidate: index setup works, folder browser, readable admin pages (local, Windows)

**13 November 2026. Preparation.** 1.8.33 is 1.8.32 ([record](release-1.8.32.md)) with the fixes and
pages its manual install asked for. Native/extension manifests and lockfiles are synchronized at
1.8.33 (`python scripts/release-version.py check --expected 1.8.33`). The scope is a Windows AIRK
Service installer and VSIX only (U99).

**What 1.8.32 did.**

- **Index setup failed.** With 1.8.32 installed, AIRK Admin's index library loaded, but **Review
  setup** failed with `invalid_registry_request` (HTTP 409) for any data root. The installation never
  created its data and cache folders, so the setup review could not find the data root's parent. The
  service reported only a generic code.
- **Path entry.** The setup form had no default data root and no way to browse for folders.
- **Pages.** Help, Usage, Agents and Activity mostly showed raw JSON.

## What changed

- **Index setup works on a new installation.**
  - Setup creates the installation's data and cache folders, private to your account. An existing
    folder is checked by the same rule as the installer's own location check.
  - A setup review that refuses its folders now says why: a folder is missing, the folders overlap,
    the data root is already in use, a path goes through a link, a path is a file, or an alias is
    taken. The page shows a readable sentence for each.
- **Browse… for folders (M24).**
  - Each path field of **Set up an index** has **Browse…**. It opens a folder dialog served by the
    service, because a browser page cannot learn a full path itself.
  - **Starting points:** your drives, your home folder and the AIRK data location.
  - **What it shows:** folders only, never files, with existing index folders and AIRK's own folders
    marked. Links and junctions are listed but never entered.
  - **Keyboard:** Escape closes the dialog, and focus stays inside it.
  - `airkctl folders [PATH]` gives the same listing.
- **Data root default.** The data root now defaults to a folder named after the collection inside the
  installation's data location, and setup creates it.
- **Readable admin pages.** Every page keeps the complete result under **Complete result (JSON)**.
  - **Agents** shows your MCP endpoint and whether Windows trusts its certificate.
    - It explains how to connect an agent automatically.
    - It gives ready-to-copy manual configuration for Claude Code, Codex, VS Code / Copilot and any
      other MCP client, using the same entries `airkctl agents connect` writes, each with its
      certificate trust step.
    - An existing connection can be looked up, verified and revoked.
  - **Usage** shows recording status, the report as tables and daily bars, estimates marked as
    estimates, and recording health.
  - **Activity** shows recent events as a table with index names, job cards with retry and cancel,
    saved requests, and pause or resume by index name.
  - **Help** shows topics and tool help as readable text, with parameter tables and a filter.
- **Service start robustness.**
  - The index catalog is created before anything else opens the service state, so an interrupted
    first start completes on the next start.
  - A Genesis that could not run answers `catalog_genesis_failed`; a state already used without a
    catalog answers `catalog_genesis_refused`.
  - `airkctl` commands such as `tools list` find the installed service without
    `--service-runtime-dir`. That change came in 1.8.32 and is unchanged here.

## Installing over 1.8.32

1. Install the 1.8.33 VSIX and reload VS Code windows.
2. Run the 1.8.33 AIRK Service installer with the same folders. It creates the missing data and cache
   folders, so a 1.8.32 installation needs no uninstall.

Coming from 1.8.31 or older: first uninstall AIRK Service and choose to delete its data, as the
[1.8.32 record](release-1.8.32.md#installing-over-1831-or-any-18241831-split-build) describes.

## Not in this candidate

- The four U102 items:
  - the admin events stream, rate limits and coverage test;
  - the Overview, Activity-plan and Recovery-execute additions;
  - `packages/ui` evidence views;
  - real-transport parity tests.
- **Creating an agent connection from AIRK Admin.** It is never done over HTTP. Without VS Code,
  `airkctl agents connect` needs a reviewed selection file, and there is no simple command to write
  one yet.
- **Changing usage recording from the page.** That is an `airkctl usage configure` command, shown
  ready to copy.

The known limitations of [1.8.31](release-1.8.31.md#known-limitations) still apply. Installed-editor
qualification, macOS/Linux installers, hosted CI and Phase 8 are not run, and nothing is published.
