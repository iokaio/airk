# 1.8.31 candidate: index creation completed (local, Windows)

**12 November 2026. Preparation.** 1.8.31 is 1.8.30 ([record](release-1.8.30.md)) with the rest of the
split's index management. Native/extension manifests and lockfiles are synchronized at 1.8.31
(`python scripts/release-version.py check --expected 1.8.31`). The bump follows the standing
per-installer authorization. The scope is a Windows AIRK Service installer and VSIX only (U99).

**Why.** With 1.8.30 there was no way to create an index. The admin pages had no Index library,
and the Explorer's Indexing page failed with `catalog_control_connection_unavailable`. The gap
map and plan are in the [ledger](../simplify/one-process-ledger.md#split-completion-12-november-2026).

## What changed

- **Creating an index.**
  - **Admin Index library.** It lists indexes, index sets and grants. **Set up an index** shows the
    service's full review, and **Create index** applies exactly that review.
  - **`airkctl library`** offers the same operations: `list`, `show`, `setup`, `setup-apply`,
    `change`, and the set and index commands.
  - **Typed confirmation (U98).** Removing, retiring or purging something asks you to type the
    target's name, or the location's path for a purge. A wrong name is refused and nothing is sent.
- **The Explorer's Indexing view is read-only (U97).**
  - It shows indexes, index sets, sources, policy, split and job state, using the same service reads
    as `airkctl` and AIRK Admin.
  - **Open AIRK Admin** opens the management pages in VS Code's Simple Browser or in your default
    browser (setting `airk.admin.openIn`). The single-use sign-in link is never logged or stored.
  - With no index for the open folder, **Create an index for this folder** opens the Index library
    with that folder filled in.
  - The extension's pre-split management screens are removed.
- **Search scopes.**
  - A VS Code window can choose its search scope among existing index sets, optionally narrowed to
    some of their indexes. Searches, Workbench and Recommendations then read through that scope.
  - The service re-checks the scope on every read, and `airkctl` re-selects once if the index
    library changed (Q2–Q5).
  - AIRK Admin's Search reads under the admin session's scope.
- **Storage and service.**
  - **Storage** lists the service's locations with their sizes. You can clean up the cache and
    closed logs after reviewing the exact plan (U100).
  - **Service › Start at sign-in** shows, turns on or turns off login startup on Windows (U101).
  - `airkctl service inventory|purge-plan|purge|login` work the same way.
- **Usage page.** It shows the usage report, policy and settings.
- **Faster local installer builds.**
  - Local installer builds use a `release-local` profile and build `airk` and `airkctl` in one
    Cargo invocation.
  - The former `airk-cli` crate is split into ten crates
    ([record](../simplify/airk-cli-split.md)), with no behavior change: test identities are
    identical before and after.
  - Published releases are still built in CI with the release profile.

## Installing over 1.8.30

1. Install the 1.8.31 VSIX and reload VS Code windows.
2. Run the AIRK Service installer with the same folders. VS Code can stay open.

## Not in this candidate (U102; planned for 1.8.32)

- The admin live event stream, admin rate limits and the operation-table coverage test.
- The Overview's version, activity and leftovers; plans on the Activity page; running a recovery
  plan from the Recovery page.
- Code, document and evidence-inspector views beside admin Search (`packages/ui`).
- Parity tests between `airkctl`, AIRK Admin and the Explorer through the real session transport.

## Known limitations

- **Sign-in startup.** Turning it off while the service itself was started at sign-in answers
  `login_startup_in_use`. Stop the service and run `airkctl service login uninstall`.
- **Grant drafts in AIRK Admin** need a typed 64-hex context key.
- **Not run:** installed-editor qualification, macOS/Linux installers, hosted CI and Phase 8
  qualification.
- **Not published.** The downloads page still lists the pre-split assets.
