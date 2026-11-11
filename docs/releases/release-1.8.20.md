# AIRK 1.8.20 local VSIX candidate

**Unpublished Windows x64 manual-test candidate, 10 November 2026.** Superseded
by [1.8.22](release-1.8.22.md), which corrects this candidate's agent
configuration and process regressions. This record remains dated history.

1.8.20 keeps the [1.8.19](release-1.8.19.md) rule that, until 1.9.0, an
installer replaces an earlier installation instead of upgrading it. It fixes
the service not running after that replacement.

## The service now starts at sign-in

**Problem in 1.8.19.** A fresh installation started the service on demand with
a 180-second idle exit and registered nothing with the operating system. With
no project indexed, nothing kept it running: three minutes later it exited, and
nothing started it again.
- Explorer only observed status. It reported "The service connection is
  unavailable", and its advice to choose **Connect / reconnect** could not
  start the service.
- Agents outside VS Code connect directly to the local HTTPS address and cannot
  start AIRK themselves.
- The repeating short-lived `airk.exe` processes were Explorer's status checks
  (`runtime inspect`, `diagnose`, `snapshot`), not a crash loop.

**Fresh installations register login startup.** After a fresh install
(including one that just replaced an earlier version), the installer runs
`airk runtime login --install-dir <dir> install`. That registers the per-user
OS startup entry and starts the service through it:
- Windows: Task Scheduler, current user, `InteractiveToken`, `LeastPrivilege`.
- Linux: a user systemd unit.

The registered entry runs `runtime managed-serve`. Unlike the on-demand
service, it does not exit when idle. It keeps running after VS Code closes and
starts again when you sign in after a reboot. Afterwards, `runtime ensure`
starts it through the OS manager and never starts a competing process. The
standalone `airk setup` installs fresh the same way, and both use one shared
native implementation.

If the operating system refuses the registration, AIRK keeps the on-demand
service and reports the native error. Possible causes are a policy, or the
macOS shared LaunchAgent adapter, which is not qualified yet. A refusal never
fails the installation. Explicit **Stop service** is still honored at the next
sign-in, until **Start service** is chosen. Uninstall removes the registration.

**No console window at sign-in (Windows).** Task Scheduler gives a console
program started at sign-in its own visible terminal window, and closing that
window would end the service. This was confirmed on the development machine
with a temporary task. The sign-in entry now detaches from a console that no
other process shares. A console opened by a person or a parent process is left
alone.

**Replacement removes the old startup entry.** Before moving an earlier
installation's folders aside, the installer removes that installation's
startup registration, proven by its ownership receipt. Without this, the next
sign-in would run a removed program and the fresh install could not register
its own. An entry without an ownership receipt is never altered. If removal
fails, nothing is moved.

## Explorer

- **Restart after an idle exit.** When the on-demand service (no login
  registration) has exited because it was idle, the next status check starts
  it again once for that exited instance. It does not do this after an
  explicit Stop, with a pending stop or restart request, when the OS or an
  organization manages the service, or for a different runtime version.
- **Clearer recovery advice.** The unavailable-service hint now says: choose
  **Start service** when the service is not running; **Connect / reconnect**
  only reattaches to a running service.
- **Broken agent configuration check.** Management → Agents lists recognized
  AIRK entries in the project's agent configuration files whose command no
  longer exists. Examples are a Claude Code `.mcp.json` or a Codex
  `.codex/config.toml` that still names the launcher removed by the
  replacement. The list suggests **Connect a coding agent…** to replace each
  entry with the HTTPS connection.

## Qualification

See the [release-readiness ledger](../simplify/release-readiness.md). Results
are bound to this candidate's own artifacts; 1.8.19 results do not qualify it.
Windows only; macOS and Linux were not built or tested, and the macOS login
adapter remains unqualified.
