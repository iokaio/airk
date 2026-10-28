# AIRK 1.5.12 release record

28 October 2026. Native and extension manifests and lockfiles are synchronized at **1.5.12** on `feat/manageindices`. This record tracks local validation and subsequent CI/publication; no publication is implied by the version bump.

## Agent instruction guidance

Management → Agents now includes **Help your agent discover and use AIRK**, with the complete suggested Markdown, **Copy instructions**, and separate file buttons for Claude Code (`CLAUDE.md`), Codex (`AGENTS.md`) and GitHub Copilot (`.github/copilot-instructions.md`). The project selected in the Explorer header determines the root. Existing files and dirty editor buffers remain unchanged; missing files open as named, unsaved drafts. No configuration is rewritten and no indexing is started. Other assistants can use the same text in their documented project instructions or rules file.

The suggestion asks assistants to discover AIRK, including deferred tools where supported, check actual status and project binding, search scoped indexed evidence, verify live source before edits and explain limitations before falling back to local tools. It avoids treating configuration presence as tool availability or changing client settings without a task requirement. The **Project instructions for AI agents** help topic documents review, saving, client loading, trust and workspace limits. See [agent integration](agent-integration.md#project-instructions-1512).

## Included management and startup improvements

This release includes the branch's previously local 1.5.6–1.5.11 changes: tabbed index management; automatic Overview/Management observations; durable setup and bounded recovery; semantic setup and published coverage reported separately; stale queued maintenance settlement; raw diagnostic access and safe cleanup continuation; independent storage-format identities; one unified 26-tool MCP endpoint and migration of recognized old connections; compatible per-connection MCP negotiation; and current registration validation when preparing agent configurations. Earlier dated candidate records retain their own test scope.

The [1.5.11 candidate](release-1.5.11.md) adds immediate Explorer availability while runtime checks continue, reuse of healthy services, visible connection checks and distinct published-content/current-work cards. Its Windows installer was reported working by the user on the other machine before this change. That is user-reported acceptance of 1.5.11, not cross-platform qualification or a startup latency measurement for 1.5.12.

## Process lifetime and recognizable names

User-owned installations create verified `airk-service`, `airk-index` and `airk-source` executable copies locally (`.exe` on Windows). These identify the shared broker, project workers and source watcher. `airk-embed` remains the inference helper; `airk` remains the CLI/MCP entry point. The downloadable runtime and VSIX still contain one main engine and one inference helper. The extra installed copies use the main engine's approved digest, preserve the existing rejection of hard-linked/redirected files, and are included in owned runtime retirement. They sit outside the sealed macOS app bundle; helper and build metadata resolve through the original approved image. Read-only organization-managed and development payloads are unchanged and retain their original executable names.

MCP frontends and launcher forwarders now watch their originating client. EOF remains normal shutdown; a lost client also ends its frontend if inherited handles leave stdin open. Windows holds process handles and checks creation times through preceding launcher parents, including an older universal launcher; Unix detects reparenting. OS-supervised or PID-namespace sessions without an observable ordinary parent retain transport EOF/container shutdown. This does not time out idle live sessions or scan and kill unrelated processes. The shared service and index workers retain their independent service ownership. An identical verified universal/per-installation launcher can skip the intermediate forwarder; differing versions retain compatibility delegation. Separate agents and sessions can legitimately have separate MCP processes even in one VS Code window.

## Validation and publication

Windows local validation completed on 28 October 2026:

- Formatting, workspace/all-target/all-feature Clippy with warnings denied, repository checks, version consistency, license-notice generation and whitespace checks passed.
- Full locked workspace/all-feature Rust tests and doctests: **1,434 passed, 17 ignored, zero failures**.
- Extension compilation, webview/adapter type checks and tests against the new native executable: **381 passed, 2 skipped, zero failures**. The skipped cases cover POSIX shell startup and the separately configured cached-helper replacement fixture. A real installed VS Code UI session was not repeated locally for 1.5.12; instruction-file behavior uses the host fixtures and a rendered webview check.
- Python build/release contracts: **177 tests, 4 skipped, zero failures**.
- Real native clients, offline inference/vector retrieval, three simultaneous MCP clients, reconnect, protocol fallback and retained failure diagnostics passed. Source bytes were preserved.
- Installed role identities, helper resolution, abrupt client loss, concurrent-session preservation and normal EOF passed through both installed launcher paths. The shared service survived client shutdown. Installation, upgrade/rollback, launcher repair, interrupted retirement and resumable uninstall passed; modified/unknown files remained protected.
- A real **1.5.11 → 1.5.12** upgrade terminated the preceding connected client and preserved its registration and source. Read-only managed payload and external-supervisor ownership tests passed.

The first lifecycle-fixture runs exposed fixture setup errors (overlapping source/data paths, then missing project registration); those were corrected without relaxing production guards. Its child stderr is now retained in failures. An earlier paired build rejected source changes made during compilation; the final package used a fresh, consistent build.

The verified local Windows VSIX is **120,014,427 bytes**, SHA-256 `8bbba4408b094a19767ccbb001a6f11a925711ef65878403347f1647d9868b31`. It is **15,942 bytes larger** than the accepted 1.5.11 candidate and contains only the original engine/helper executable entries. The three install-local role copies add **522,258,432 bytes (about 498 MiB)** on this Windows build; they do not duplicate the inference helper or model assets.

This is an explicit self-review and Windows validation record, not independent review or macOS/Linux qualification. Required PR and exact-main CI, signed GitHub downloads and Marketplace publication are subsequent release gates; their authoritative results are the linked PR/workflow runs and published release artifacts.
