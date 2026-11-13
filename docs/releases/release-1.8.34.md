# 1.8.34 candidate: readable new indexes, indexing status in the Explorer (local, Windows)

**13 November 2026. Preparation.** 1.8.34 is 1.8.33 ([record](release-1.8.33.md)) with the fixes its
manual install asked for. Native/extension manifests and lockfiles are synchronized at 1.8.34
(`python scripts/release-version.py check --expected 1.8.34`). The scope is a Windows AIRK Service
installer and VSIX only (U99). Decision U103 ([split plan](../split/split-plan.md) §2.1) amends U97.

**What 1.8.33 did.** An index created in AIRK Admin was indexed, but nothing showed it:

- The Explorer header said **index unavailable**.
- No admin page showed progress.
- **Search scope › Apply selection** was enabled with nothing checked, and disabled once the only
  index was checked.

The cause: setup created no index set, and every read (Explorer, agents) goes through an index set.
The status read also reported this refusal as a worker failure.

## What changed

- **A new index is readable at once.** Index setup also creates an index set named after the index,
  containing the whole index, in the same step. Nothing about the setup receipt or the stored state
  format changes.
- **Make readable.** An index created before 1.8.34 belongs to no index set. AIRK Admin's Index
  library shows **Make readable** on it. It creates (or reuses) the set named after the index and
  adds the index with every enabled source root. Both changes go through the usual preview and
  apply, and every result is shown.
- **Live progress in AIRK Admin.** Overview and the Index library show the running job:
  - phase, and files committed of files found;
  - publications and elapsed time;
  - stalled and paused flags;
  - pending and queued jobs.

  The display refreshes about every 2 seconds while work runs. The complete result is kept.
- **The Explorer shows status and offers the everyday actions (U103).**
  - Without a chosen search scope, the Explorer reads the folder's index through the index sets
    that hold it. A scope you applied or cleared is never replaced.
  - The header shows **indexing N/M files**, **paused**, or **not readable: no index set**.
    **index unavailable** is kept for real unavailability.
  - The Overview shows live progress and job, watch and pause state. It also offers **Index now**,
    **Pause**/**Resume**, watching on/off, and **Retry** or **Cancel** of a job, using the same
    `airkctl` commands.
  - **Management › Usage** and **Management › Activity** show the usage report, recent job and
    status events, and the service log. Other management stays in AIRK Admin.
- **Search scope fixes.**
  - **Apply selection** is enabled only for a changed, valid choice.
  - Indexes checked alone are read through every index set that holds them. An index in no set
    offers **Make readable in AIRK Admin**.
  - Applying a set no longer checks all of its indexes.
  - Alerts reappear while their condition holds, and errors that were hidden are shown.
- **Honest status.** A read refused for lack of an index set answers
  `index_error.code = "index_read_unavailable"` with the index's current indexing progress, instead
  of `index_worker_unavailable`.
- **New `airkctl` commands:**
  - `airkctl progress`: the same observation the pages show;
  - `airkctl watch status`;
  - `airkctl jobs get|retry|cancel ID`.
- **The editor session forwards only what the operation table allows.** The `control` relay of
  `airkctl session` forwards exactly the operation-table rows marked `session`, plus the window
  plane's W5. Usage, logs, policy and split status, watch state, pause, watch, enqueue, retry and
  cancel were added to that column.

## Installing over 1.8.33

1. Install the 1.8.34 VSIX and reload VS Code windows.
2. Run the 1.8.34 AIRK Service installer with the same folders. No uninstall is needed.
3. In AIRK Admin › Index library, open your existing index and choose **Make readable** once.
   Indexes set up with 1.8.34 need nothing.

## Not in this candidate

- **Agents (MCP clients) still need a reviewed selection.** There is still no simple command to
  write one.
- **The Workbench still reads by project binding.** Only the Explorer uses the automatic scope.
- **Not yet in the Explorer:** service resources, workers and live agent verification.
- The U102 items and split follow-ups F1–F5.

The known limitations of [1.8.31](release-1.8.31.md#known-limitations) still apply. Installed-editor
qualification, macOS/Linux installers, hosted CI and Phase 8 are not run, and nothing is published.
