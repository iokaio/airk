# AIRK 1.8.19 local VSIX candidate

**Unpublished Windows x64 manual-test candidate, 10 November 2026.** Superseded
by [1.8.20](release-1.8.20.md): a fresh 1.8.19 installation registered no login
startup, so its on-demand service exited after three idle minutes and nothing
started it again.

## Installing over an earlier version replaces it

Until 1.9.0, AIRK does not upgrade an earlier installation in place. When the
installer finds an AIRK installation whose runtime differs from the one being
installed, it:

1. stops the processes running from that installation's folders;
2. moves AIRK's own folders aside and removes them;
3. installs fresh.

No index is created until you add one in **Indexing**. Your source folders are
never changed; only AIRK's own locations are removed. Every 1.8.x installer
behaves this way. In-place upgrades start once both the installed and the new
version are 1.9.0 or later.

The user made this decision on 10 November 2026 because there are almost no
installed predecessors, and the in-place clean break (capture, rebuild,
predecessor process proofs) kept refusing real installations with a long
history of failed updates. It supersedes the
[clean-break rebuild contract](../upgrade-rebuild-fallback.md) for installs
before 1.9.0.

What is removed:

- the installation folder and the state and log folders its layout record
  names;
- for a default installation, every default AIRK location for both channels
  and both the historical (`service-1.4`) and current layouts: runtime,
  launcher, settings, index data, state, cache and logs.

Before anything moves, each location must be provably AIRK's own: a default
AIRK location, the installation folder with its records, or a folder holding
AIRK control or state records. A damaged layout record that names an unrelated
folder is refused and nothing changes. If a program still holds a file open,
the installer stops with the folder and the error; nothing is half-removed,
and installing again retries. A folder that was moved aside but could not be
deleted is reported and is safe to delete later.

Index data in custom locations outside AIRK's folders is left in place.
Agent configurations outside VS Code (for example Codex or Claude Code) are
not edited; the installer reports that their AIRK entries need reconnecting
from **Management → Agents**.

The native command is `airk runtime replace-installation --install-dir <dir>
--candidate-sha256 <sha256> [--apply]`; without `--apply` it only reports the
plan.

## Carried forward from 1.8.18

The [1.8.18 record](release-1.8.18.md) lists the corrections made overnight
(planned-restart startup cooldown, setup review of AIRK-private locations,
saved search scope restoration, per-index Usage administration, bounded stdio
framing, retained-payload limits). They remain in this candidate. The
in-place clean-break code still exists but is no longer reached from the
installer before 1.9.0; its removal is a separate step.

## Qualification

See the [release-readiness ledger](../simplify/release-readiness.md). Results
are bound to this candidate's own artifacts; earlier 1.8.18 results do not
qualify it. Windows only; macOS and Linux were not built or tested.
