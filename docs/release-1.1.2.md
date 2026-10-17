# AIRK 1.1.2: coding-agent connection setup

Version 1.1.2 was developed on `feature/agent-connections-1.1.2`. No publication or installed-client qualification is implied.

## Problem and implementation plan

The previous **Manage Agent Connections** command opened VS Code's MCP controls. A user could start AIRK there and reasonably expect Codex to see it, although Codex reads separate configuration.

1. Route connection setup through an agent picker with recognizable names. Keep VS Code / Copilot controls explicit and separate.
2. Keep configuration, workspace identity, destination guidance, reload steps and verification instructions visible in one persistent guide.
3. Let Codex create a missing project `.codex/config.toml` on an explicit action. Preserve existing files and unsaved edits, refuse redirected directories, and open existing settings for manual merging. Install project JSON for Claude Code, Gemini CLI and Antigravity while preserving other settings. Enable Copilot registration and open its server controls. Keep remaining clients guided and remote-host setup manual.
4. Separate configuration creation, AIRK helper health and the agent's actual tool discovery. Supply a collection-specific verification prompt and retain failures in the guide.
5. Synchronize native/extension manifests and lockfiles at 1.1.2, update setup documentation, and run contributor checks and targeted regression tests.

## Implemented scope

- **Connect Coding Agent** and the existing **Manage Agent Connections** command open the same picker. Existing command IDs remain usable.
- A separate **VS Code / Copilot MCP Controls** command opens the editor's server controls.
- Codex configuration creation uses exclusive file creation. It never appends to, overwrites or changes policy in existing TOML. Existing files require manual merging, including conflict review against user-level configuration.
- Opening setup does not force indexing. The explicit helper check updates the selected collection and checks its stdio handshake and tool inventory.
- The guide escapes workspace content, accepts only client-specific actions, rejects stale workspace/storage bindings, and checks workspace trust on every action.
- Claude Code, Gemini CLI and Antigravity offer guarded project JSON installation with Microsoft’s pinned MIT-licensed `jsonc-parser` 3.3.1. The package includes only its runtime closure and original license. Other servers, comments and approval policies are preserved; conflicting entries require review.
- Copilot setup explicitly enables AIRK registration and opens VS Code’s MCP controls. Server start and trust prompts remain with VS Code. No network service, agent reasoning or MCP retrieval behavior is introduced.

## Acceptance and validation

- Automated fixtures cover configuration creation, concurrent creation, exact preservation of existing settings, redirected directories, unsaved edits, cancellation, wrong-client actions, remote setup, stale guides, trust loss, helper failure and HTML escaping. JSON fixtures also cover duplicate/invalid keys, comments, policies, idempotence, conflicts, redirected paths, hardlinks and editor save failures.
- Extension compilation and tests: 64 passed with the built 1.1.2 native helper, including real indexing, MCP and collection isolation. The old/new-helper upgrade fixture remains skipped because no previous helper was supplied. Targeted connection/controller tests also passed after adding workspace-alias coverage.
- Native workspace build, formatting and clippy passed. Repository and whitespace checks passed. Python packaging/release contracts: 85 passed, 1 Linux-only check skipped. License notices generated for 466 dependencies; the four previously documented missing standalone legal texts still require release review. Full native workspace tests and doc-tests passed on Windows, including the 199-test kernel integration suite. Explicitly ignored fixture/model/timing tests retain their existing requirements; Docker and cross-platform qualification were not run.
- Headless Edge rendered the Codex, Claude Code, Copilot, Gemini CLI and Antigravity guides at 900 and 360 CSS pixels without horizontal overflow. Actual DOM button events reached the webview bridge and disabled repeated clicks. This is renderer evidence, not an installed-agent connection test.
- Manual release acceptance: in an installed 1.1.2 extension, select Codex from **Manage Agent Connections**, create or merge its project config, restart Codex, then request `airk_status`, `airk_capabilities` and a known-phrase search. Confirm the exact collection and original source location.
- Also exercise Copilot's separate controls, an existing Codex config with unrelated servers/policies, multiple workspace folders, helper failure and a remote host. Linux/macOS and installed-agent behavior require their own evidence.

The requested version bump is already applied for this feature; do not bump it again for the same PR. The user authorized publication after green PR/main checks; use the protected release workflows and their exact-source candidate gates. Historical [1.1.1 qualification](release-1.1.1.md) remains unchanged.
