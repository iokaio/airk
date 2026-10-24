# Reviewed legacy agent configuration

The 1.4 **Connect Coding Agent** guide supports explicit review of a project JSON entry for Claude Code, Gemini CLI or Antigravity. Review does not execute the old command, stop an old worker, convert an index or change startup registration.

Only the same-named `airk-<collection>` stdio entry is eligible. Its absolute `airk`/`airk.exe` command and seven legacy arguments must match the selected source and collection exactly; its old data directory may differ from the fresh 1.4 directory. Extra arguments, URL transports, duplicate JSON keys, foreign roots, unknown commands, redirected directories and linked files require manual review. Reads are bounded to 1 MiB.

The guide opens an editor diff before offering **Replace reviewed entry**. The replacement changes the selected command and arguments to the registered stable-launcher binding, preserving other entry properties, other servers, comments and approval settings. It neither starts nor enables the selected agent. User configuration and TOML/other client formats remain manual merges using the displayed connection.

Approval binds the exact original and proposed bytes, client, source, collection and native binding. The editor checks disk contents, document version, unsaved changes and save outcome. A changed proposal, stale file, copied review object, closed guide or lost workspace trust refuses the write. A consumed review cannot be replayed. Tests cover these refusal boundaries and all three JSON destinations.

The native MCP check reads readiness and verifies its handshake without scheduling indexing. Actual agent enablement still needs the agent's own controls and verification prompt. Unknown or modified old OS registrations and old derived state remain preserved; native ownership review and scoped cleanup are separate implementation gates.
