# Remove owned PATH entries and terminal wrappers during extension uninstall

AIRK installs persistent shell/PATH integration but its uninstall flow does not reverse it. The native launcher can be removed while the extension-storage `airk` wrapper and profile additions remain, leaving a broken command on PATH.

## Version, environment and evidence

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate, inspected on 2026-10-30. The code paths cover macOS/Linux profiles and Windows user PATH; no actual installation/uninstallation was run.

[CLI setup](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/cliInstall.ts#L110) appends marked blocks to `.profile`, `.bashrc`, an existing Bash login profile, Zsh profiles and Fish configuration, respecting relevant environment overrides. On Windows it prepends the directory to `HKCU\Environment\Path`. [Wrapper creation](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/cliInstall.ts#L211) writes `service-cli/bin/airk` or `airk.cmd` under extension global storage.

[Extension uninstall](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/extensionUninstall.ts#L100) removes recorded agent entries and Workbench sessions and delegates native removal. It contains no reversal of those shell/PATH changes or deletion of that wrapper. If editor storage remains, its directory still qualifies for the shell block's directory-existence check, even though the target launcher has gone.

## Minimal fictional verification, not yet run

In a disposable OS account, record profiles/user PATH, install into a trusted fictional workspace, verify `airk` command resolution in a new terminal, uninstall through VS Code and wait for native cleanup, then inspect the same profile entries, wrapper and command resolution in a fresh terminal. Expected: AIRK-owned integration is removed or explicitly reported as preserved. Current code predicts remaining additions and, where storage survives, a broken wrapper.

## Proposed acceptance criteria

- **AC1:** Successful uninstall removes the unchanged wrapper and only the profile blocks/PATH entries owned by that installation, preserving preexisting identical user entries, unrelated edits and other AIRK installations.
- **AC2:** Modified, redirected or ambiguous files are preserved with an actionable cleanup result; cleanup is safe to repeat after interruption.
- **AC3:** Tests cover fresh and existing profiles, environment-overridden paths, multiple installations, custom roots, Windows PATH value type, and retained editor storage.
- **AC4:** New terminals no longer resolve the removed installation through its former wrapper; newly created profile files are removed only when safe, otherwise any remaining harmless files are reported.
