# AIRK 1.5.0

October 26, 2026 — source candidate on `feat/airkexplorer`; not yet merged or published. Publication remains controlled by protected-main CI and the release workflows.

1.5.0 adds the **AIRK Explorer**, an editor workbench that replaces the Service Console, and **Explorer Assistance**, a separate extension-owned MCP server that helps coding agents use it. The design is [AIRK Explorer: Final Design](explorer/AIRK_Explorer_Final_Design.md); user-facing behavior is in the [extension guide](vscode-extension.md#airk-explorer-150).

## What changed

**AIRK Explorer.** **AIRK: Open Explorer** opens an editor tab with eight workspaces and a Help drawer:

- **Overview**: status of the selected project from `airk_status`
- **Search**: Query, plus Recipes saved in workspace state
- **Code**: Symbols, References and Outline
- **Documents**: Facts
- **Evidence**: Inspector, Checker and Changes
- **Review**: Changes, Impact and Tests, using the editor's Git extension
- **Strategies**: Library, Studio and Planner
- **Management**: nine tabs replacing the Service Console

The AIRK activity bar's **Status & Search** view and a status-bar word summarize readiness. Evidence comes from the Explorer's own read-only MCP session, attributed to the new `explorer` usage client. Management drives the existing console controller, so requests, confirmations and paging are unchanged. Screens that are not built are labeled; nothing is simulated.

**Strategies.**
- **Library.** 25 core templates (`airk.core/NAME@1.0.0`). Investigate & fix a bug, Change an API contract and Explain a component are Runnable; the others are Preview, each with its reason.
- **Studio.** Edits strategies from a closed operator catalog and runs them against one pinned publication, stopping if the index is republished. A run is a verified snapshot only when every cited file's saved bytes match the index. Studio also compiles a deterministic, editable prompt.
- **Planner.** Saves Markdown plans through the save dialog, by default in `.airk/plans`. A one-task handoff prompt is offered only after the save is confirmed.

**Explorer Assistance.** An MCP server named `airk-explorer`, bundled as `out/explorerAssistance.js`, with nine tools:
- help search and help get, served from the catalog bundled with this version, offline and with the service stopped
- UI status
- five draft tools: search, strategy, plan, review and management guidance
- proposal status

Drafts go to the attached window's **Suggestions Inbox** and are never accepted, applied or executed on arrival. Management drafts can only open a screen with fixed guidance. Assistance never calls AIRK Retrieval or the service, never runs commands, changes settings or writes files, and never reads source.

How agents reach it:
- **VS Code agents:** the second MCP provider, `airk.explorerAssistance`.
- **External agents:** a second agent connection, `airk_explorer`, through the stable launcher and the new hidden native subcommand `airk explorer-assistance --descriptor`. That subcommand verifies the extension's launch descriptor and the server's SHA-256, then starts the server with VS Code's own runtime, so no system Node is needed.
- **Window attachment:** each trusted window publishes a private endpoint with a rotating credential.

**Connect a Coding Agent** now configures and checks AIRK Retrieval and Explorer Assistance as two separately labeled connections.

**Native.**
- Version-free universal launcher with a pointer file, published by `runtime ensure` and reported by `runtime inspect` as `universal_launcher`.
- New hidden subcommand `explorer-assistance --descriptor` ([service installation](service-installation.md)).
- New `explorer` client family (`airk-explorer`) in usage reports.
- No new MCP tools: AIRK Retrieval still exposes exactly sixteen.

**Removed.**
- The Service Console webview view, its HTML renderer, the `airk.explorer.showServiceConsole` setting and the focus-forcing console command helper. Former console commands open the matching Management tab.
- Thirteen extension modules that the extension entry no longer reached and that were never packaged, together with their tests.

**New settings.**
- `airk.explorer.assistance.enabled`
- `airk.explorer.planner.directory` (workspace)
- `airk.explorer.suggestions.ttlHours`
- `airk.explorer.review.testPathPattern` (workspace)

**New commands.**
- **AIRK: Open Explorer**
- **AIRK: Search Indexed Evidence**
- **AIRK: Show Agent Suggestions**

## Upgrade and rollback

- **Namespace.** The native service namespace remains `service-1.4`. No reindex is needed, and project registrations, indexes and settings are kept.
- **Upgrade.** Activation upgrades an installed 1.4.x runtime to the bundled 1.5.0 runtime automatically, as it did for 1.4 patches; downgrades still need an explicit, reviewed transition.
- **Rolling back to 1.4.** A 1.5 extension refuses a reviewed rollback to a 1.4 runtime, because rollback and resume require the same major.minor. To return to 1.4, install the 1.4 extension.
- **Usage after rollback.** The usage store records client families by name, and a 1.4.x runtime does not know the new `explorer` family. After Explorer use, a 1.4.x runtime may fail to read or may drop those usage records. This was not tested.
- **Universal launcher.** Agent configurations no longer name the versioned install root. For the native default installation, `runtime ensure` publishes a version-free universal launcher (`…/AIRK/launcher/airk-launcher`) with an `airk-launcher.json` pointer to the current install root. It reads the pointer on every launch and hands off to that installation's own verified launcher, so a future installation only rewrites the pointer. See [service installation](service-installation.md).
- **Agent configurations.** Existing configurations keep working. On Install or Repair CLI and trusted activation, recognized entries for this installation, including 1.4 per-installation launcher entries, are moved to the universal launcher, and so are `airk_explorer` entries this extension wrote. Unrecognized entries are left untouched. The `airk_explorer` connection is added only when a user installs it from Connect a Coding Agent.

## Design differences recorded during implementation

- **Windows named pipe.** Node cannot set a named pipe's ACL. The per-window pipe has a random name, and the credential in the user-only window descriptor authenticates callers; a wrong credential gets no reply.
- **Not implemented:**
  - Runnable templates are not exposed as MCP prompts.
  - Plan export as `SKILL.md`.
  - UI strings are English and do not go through `vscode.l10n`.
  - Phase P6: code map, documentation drift, quality rules and reports.

## Validation

Local runs on Windows 11 x64 on this branch, after the review fixes unless noted:

**Rust.**
- `cargo fmt --all --check` passes.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passes.
- `airk-core`: 61 tests pass.
- `airk-cli` library: 150 tests pass, 1 ignored.
- `airk-cli` integration tests: `explorer_assistance` 2, `usage_native` 2, `mcp` 8, `tools` 21 and `service_runtime` 3, all passing. The Rust results predate the review fixes, which changed no Rust code.

**Extension tests, with mocks.** 245 tests: 235 pass, 10 platform or native skips.

**Native lane.** With `AIRK_TEST_BINARY` set to a 1.5.0 debug runtime pair: 243 pass, 2 skipped. The lane covers:
- installed launcher → runtime forwarder → Assistance tools
- Retrieval still at 16 tools
- Strategies `bug`, `api` and `explain` against a real indexed fixture: a verified snapshot, and partial after a live edit

**Python contracts.** 172 tests pass, 4 skipped.

**Repository checks.**
- Repository and whitespace checks pass.
- `release-version.py check --expected 1.5.0` passes.

**Installed-editor harness.** Run on a staged 1.5.0 VSIX tree with the `multi`, `untrusted`, `different` and `same` scenarios:
- Before the review fixes: all four passed.
- After the fixes: all four passed on a rebuilt runtime pair whose native candidate and extension stage were verified against this source identity.

**Independent review.** A separate read-only review of the complete diff reported three medium and six low findings, with no authority-boundary or path-containment defects. All nine were fixed and covered by tests in the review-fix commit:
- stale Studio parameters
- publication pinning requiring a leading status step
- crashed-window descriptors causing ambiguity
- unchecked citations labeled valid
- the vscode.git status map
- credential rotation on write failure
- silent task truncation
- leftover Service Console text
- small listener leaks

## Not qualified

None of the following was run for this candidate:
- macOS and Linux: installed editor, Unix socket and permission paths, and the forwarder's Unix owner checks. Linux and macOS runs of the forwarder integration test are expected in CI.
- Remote SSH, WSL and dev containers.
- Real Claude Code, Codex and Gemini CLI sessions using the `airk_explorer` connection.
- An installed 1.4.4 → 1.5.0 upgrade in an editor (no 1.4.4 VSIX was available locally; the upgrade gate is unit-tested).
- Rollback to a 1.4 runtime after Explorer use.
- A full accessibility audit.
- The complete Rust workspace suite (routine CI runs it on Linux).

## Explorer review follow-up (26 October 2026)

The subsequent local follow-up expands Explorer Assistance to fourteen tools (templates list/get, dry-run validation, navigation proposals and explicit handoff retrieval), with richer UI/proposal status and paired project installation/backfill. Explorer now has unified offline Help search over native engine and UI guidance, four role guides, purpose/format indexing advice, contextual help, paged live references and About with Ioka/Munarium history, Discord, issues and contribution links. Planner loads local Markdown as unverified drafts or exact JSON; Studio import/replacement and dialog focus are guarded. Friendly errors, bounded UI requests and unknown-action-outcome guidance support deliberate recovery. The candidate remains 1.5.0; updated build/test/installer receipts follow in the action plan. Earlier installer receipts describe their earlier source checkpoint.

This local 1.5.0 candidate implements the eleven findings in the Explorer action plan and the code improvements for Changes pagination, deletion/rename review, bounded transport queues, project selection and keyboard focus. Save checks reject redirected/linked targets and concurrent review changes; remote plan saves use Open as draft and the editor Save command. Studio discards old-run completions, invalidates generated prompts after edits and refuses truncated Planner snapshots. Review and Checker require consistent publications and preserve unavailable failures. Proposals are authorized before changing local drafts. Budgets measure serialized UTF-8 bytes.

The earlier validation and independent-review paragraphs above describe the earlier branch checkpoint. Current follow-up results and remaining cross-platform, remote, real-agent and upgrade/rollback qualification are recorded in docs/explorer/code-review-action-plan.md. The version remains 1.5.0 for the requested local installer; nothing is published by this build.
