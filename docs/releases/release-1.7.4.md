# 1.7.4 stable release candidate

Date: 2026-10-30. The native workspace, local Cargo lock entries, extension and npm lock entries are synchronized at 1.7.4. Publication requires green PR and exact-main CI plus the manual release workflows. The intended channel on both destinations is **stable**. This record describes unpublished source until those workflows succeed and their public outputs are verified.

1.7.4 addresses the outstanding items from [repository review 02](../reviews/review02/README.md); the [review response](../reviews/review02-response.md) maps each draft to its outcome.

## Behavior and contract changes

- **First-use review.** Before the first runtime installation, a trusted window shows the persistent effects of setup and offers set up, customize, defer or decline. Existing and managed installations are unaffected. Home folders and filesystem/volume roots are never registered automatically. The new `airk.cli.shellIntegration` setting (`auto`, `all` or `off`) controls startup-file edits; the `auto` default no longer creates profiles for shells that are not in use.
- **Native execution control.** `airk.runtime.nativeExecution=false` (User scope) starts no AIRK process from the extension. **AIRK: Enable Native Execution** resumes the normal path. The `airk.cli.autoInstall` description now states that native discovery still runs when only that setting is off.
- **Uninstall cleanup.** Uninstall removes AIRK-owned user PATH entries, exact profile blocks and the terminal wrapper, preserving and reporting modified files. **AIRK: Remove Terminal Command** does the same while the extension stays installed. The cleanup worker records `started` first, writes a `README.txt`, and mirrors its result to the native state directory. **AIRK: Show Last Uninstall Result** and a one-time warning at activation report interrupted or incomplete cleanups.
- **Data inventory and purge.** `airk runtime inventory` (read-only) and `airk runtime purge` (plan digest, then confirm) are new, together with **AIRK: Review and Remove AIRK Data**. Project indexes still go through unregister.
- **Sensitive data.** Common credential files are excluded by default; an ignore-file negation re-includes them. Turning off Workbench persistence now clears every project's saved sessions or excerpts.
- **Storage privacy (behavior change).** On Unix, an existing storage root that other accounts can reach is refused at registration, storage preparation and project startup with `storage_not_private`. Permissions are never changed automatically. Existing installations on such a root must move to private storage or restrict the directory themselves.
- **Extraction budgets and rebuilds (contract change).** Skip reports carry a `code`, and coverage reports `extraction_budget_exceeded`. A replacement rebuild may now publish when its only incomplete measurement is per-file extraction failures, which remain reported. The job outcome itself does not carry the count; read it from coverage. The eligibility policy version is bumped, so cached coverage inventories refresh once.
- **Explorer Assistance on Windows.** The extension replaces inherited ACL entries on its descriptor directory with the current user, SYSTEM and Administrators. The native forwarder refuses a launch descriptor that other accounts can read.

## Headless use

- `build.ps1` and `build.sh` at the repository root build an installation-free runtime for the build host under `builds/<UTC timestamp>-<platform>/` (`builds/` is ignored by Git). The generated README explains each process and a VS Code-free test setup. These builds are unsigned and for experiments only.
- `airk --help` now carries a guide written for people and agents: how the service and its workers fit together, how to select a project, a headless quick start, and how to learn the tools. Command descriptions now say what each command does. The same text reaches agents through `airk_help`.
- `airk projects` lists registered projects, matching the MCP `projects` tool. `airk explorer <tool>` provides every Explorer Assistance tool without an editor. Each requires `--output <new file>` and writes its result there: drafts are schema-checked and never admitted to an Inbox, templates come from a generated copy of the strategy pack, and `get-handoff` alone reaches a running window.
- The [CLI guide](../cli.md) and the generated [CLI reference](../cli-reference.md) document every command; a test fails when the reference drifts from the parser. `crates/airk-cli/tests/cli_commands.rs` gives every public command a test, either directly or by naming the native test or script that covers it.
- [Running AIRK headless](../headless-cli.md) maps every extension feature to the CLI. Guarded maintenance plans (rebuild, repair and verify previews) are still only available through raw `service rpc` JSON.

- Workbench citation links open in the main editor instead of a split. A same-version rebuilt VSIX is detected and offers **Reload to finish update**, and `scripts/install-local-vsix.ps1`/`.sh` force-replace a same-version local install, then verify it.

## Release and dependency controls

- `scripts/dependency-audit.py` and the separate dependency audit workflow check both lockfiles against advisory databases. Dated dispositions are recorded in [dependency advisories](../dependency-advisories.md): the unmaintained `paste` and `ttf-parser` crates are covered by exceptions until 2027-01-31, and the MSAL advisory was assessed against the Marketplace publish job. Routine CI is unchanged.
- The maintainer accepted the publisher license declarations of pdf-extract 0.12.1, adobe-cmap-parser 0.4.1 and type1-encoding-parser 0.1.1 for these exact versions. `license-notices.py --release`, which every sealed runtime payload uses, fails for any later package without a matching disposition.
- `scripts/verify-published-release.py` and [release verification](../release-verification.md) let users check a downloaded package against its published records. The published 1.7.3 Windows x64 package was verified this way (all inventories matched, Authenticode valid).

## Local validation and remaining qualification

To be completed from the final local run before the PR is opened. Checks that were not run stay listed as not run.

Open checks, not performed for this record (procedures in [validation](../validation.md)):

- the Unix two-account storage disclosure check;
- the Windows two-account Assistance descriptor check;
- installed-editor install, first-use review and uninstall on Windows and on a POSIX host, including an interrupted worker;
- verification of the other three published 1.7.3 platform packages;
- macOS and Linux qualification of every new behavior beyond what CI runs.

The Unix storage refusal tests are `cfg(unix)` and run only in CI or on a POSIX host.
