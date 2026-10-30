# Correct CLI installation and removal instructions for the current launcher wrapper

The CLI setup/removal guide still describes the retired executable-copy layout. Following its cleanup instructions can leave the current wrapper and its PATH entry in place. It also misstates Windows PATH precedence and does not accurately describe the packaged uninstall hook.

## Version, environment and evidence

Source reviewed: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 development candidate, on 2026-10-30. These are documentation/code discrepancies verified by inspection, not an installed-editor reproduction.

| Documentation | Current implementation |
|---|---|
| [CLI installation guide, line 88](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/docs/vscode-extension.md#L88): helper copied to `cli/bin` | [Launcher publication](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/cliInstall.ts#L211) writes `service-cli/bin/airk` on Unix or `airk.cmd` on Windows; installation retires obsolete executable copies |
| [Windows PATH instructions, line 93](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/docs/vscode-extension.md#L93): directory is appended | [Registry update](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/cliInstall.ts#L138) prepends it |
| [Removal instructions, line 101](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/docs/vscode-extension.md#L101): discussion of no general uninstall hook and removal of `cli/bin`/`cli` | The extension ships a `vscode:uninstall` hook that launches [a detached cleanup worker](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/uninstallHook.ts). That worker removes native payloads but currently does not reverse profile/PATH changes or remove the current wrapper |

The documentation should distinguish the existence of the hook from its incomplete shell cleanup. Correcting these instructions need not wait for automatic cleanup to be implemented.

## Proposed acceptance criteria

- **AC1:** Current guidance names the actual wrapper path/filename on each platform, the separate native launcher, Windows PATH prepend behavior and any explicitly historical layouts.
- **AC2:** Manual cleanup instructions account for current and recognized legacy wrappers, owned profile blocks/user PATH entries, and unrelated entries that must be preserved. They do not direct users to delete indexes just to remove the command.
- **AC3:** The uninstall explanation matches the detached worker, completion/diagnostic location, intentional data retention and current manual steps for user-wide agent entries or shell integration.
- **AC4:** A source-to-document review and disposable-account checks on Windows and at least one supported POSIX host verify the instructions. Unrun platform checks remain open rather than counting as a pass.

Related work: automatic PATH/wrapper cleanup is a separate behavior change; this issue corrects the instructions for the behavior actually shipped.
