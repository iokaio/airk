# Clarify autoInstall semantics and offer an explicit no-native-execution mode

Setting `airk.cli.autoInstall=false` suppresses automatic setup but does not prevent AIRK's bundled executable from running. Users evaluating the extension need to distinguish disabling installation from disabling native execution.

## Version and observed call path

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. Static review traced trusted activation through `start → refresh → discover`. [Discovery](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/serviceBootstrap.ts#L192) invokes the bundled native binary for `runtime self-test` and `runtime location`. The automatic-install setting guards a different setup path. This was not exercised in an installed editor during review.

Trusted activation also starts the [Explorer Assistance local endpoint](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/serviceExtension.ts#L704) when enabled and a usable workspace folder exists. That endpoint is governed by `airk.explorer.assistance.enabled`, not `airk.cli.autoInstall`. It is editor-hosted local IPC, not a native AIRK process.

The setting does not promise a sandbox, and executing a self-test is not evidence of indexing or service installation. The issue is the missing distinction for a user who wants to inspect settings/help before authorizing native processes.

## Proposed acceptance criteria

- **AC1:** The setting description and first-use documentation state that disabling automatic installation still allows native discovery/self-tests, and distinguish the separately controlled Assistance endpoint.
- **AC2:** A separately named explicit mode can keep trusted activation, refresh and read-only UI navigation from spawning native AIRK processes until the user enables execution.
- **AC3:** An installed-editor or process-spawn interception test covers activation, restored views, refresh, workspace changes and restart under that mode; unavailable native observations are shown as unavailable rather than inferred.
- **AC4:** Enabling execution resumes the normal verified-runtime path without changing workspace trust or bypassing managed installation policy.

The additional mode is a proposal requiring a product decision. Its scope is process execution control, not OS isolation or protection from arbitrary extension code.
