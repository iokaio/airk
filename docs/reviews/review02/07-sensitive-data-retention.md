# Make sensitive-file capture and saved evidence controls visible before indexing

Indexing a project creates additional copies of source information, and Workbench can save retrieved excerpts. Users need an actionable way to understand and control those copies before indexing a repository containing credentials or other private material.

## Current behavior and use case

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. [Discovery](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-ingest/src/source_scope.rs#L7) includes hidden files subject to ignore policy. [Built-in exclusions](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-ingest/src/lib.rs#L1262) cover familiar secret locations/extensions but cannot identify credentials in ordinary JSON, configuration or source files. `.airkignore` is supported, and `SECURITY.md` correctly says exclusions are not a secret scanner.

[Workbench persistence settings](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/package.json#L561) default to saving sessions and result excerpts. [Session storage](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/workbenchSessions.ts#L58) uses private files and exposes deletion controls. Successful extension uninstall attempts to remove recorded Workbench sessions, while native index data is deliberately preserved.

Evidence returned through an agent may be sent to that agent's provider; this is distinct from AIRK itself making a network request. Deleting local data cannot retract external conversations, exports or backups. No actual credential capture or external transmission was observed in this review.

## Proposed acceptance criteria

- **AC1:** Before first capture, users can find source-scope/ignore controls and an explanation of index content, Workbench excerpts and connected-agent disclosure.
- **AC2:** A fictional credential-like value in an ordinary configuration file demonstrates the boundary: included files can be indexed; an applicable ignore rule prevents capture. Tests do not use real credentials or claim complete secret detection.
- **AC3:** Persistence controls identify the projects and copy types they affect, and tests verify their documented deletion behavior, including already-saved excerpts.
- **AC4:** Guidance explains how to retire already-indexed sensitive data and distinguishes logical local cleanup from erasure of backups, relocated roots and external agent copies.
- **AC5:** Maintainers assess common credential filenames for default exclusion, such as `.npmrc`, `.netrc`, `.pypirc`, `.git-credentials`, SSH private-key names, certificate containers, kubeconfig, Terraform state and `.docker/config.json`. The recorded decision covers false positives and whether/how a user can override an exclusion. Adopted defaults have fixtures; documentation states actual override behavior and retains the "not a secret scanner" limitation.

This is a privacy-control proposal. Changing persistence defaults or adding secret detection requires a separate explicit product decision.
