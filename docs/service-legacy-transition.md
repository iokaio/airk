# Reviewed legacy transition

The 1.4 **Connect Coding Agent** guide supports explicit review of a project JSON entry for Claude Code, Gemini CLI or Antigravity. Review does not execute the old command, stop an old worker, convert an index or change startup registration.

Only the same-named `airk-<collection>` stdio entry is eligible. Its absolute `airk`/`airk.exe` command and seven legacy arguments must match the selected source and collection exactly; its old data directory may differ from the fresh 1.4 directory. Extra arguments, URL transports, duplicate JSON keys, foreign roots, unknown commands, redirected directories and linked files require manual review. Reads are bounded to 1 MiB.

The guide opens an editor diff before offering **Replace reviewed entry**. The replacement changes the selected command and arguments to the registered stable-launcher binding, preserving other entry properties, other servers, comments and approval settings. It neither starts nor enables the selected agent. User configuration and TOML/other client formats remain manual merges using the displayed connection.

Approval binds the exact original and proposed bytes, client, source, collection and native binding. The editor checks disk contents, document version, unsaved changes and save outcome. A changed proposal, stale file, copied review object, closed guide or lost workspace trust refuses the write. A consumed review cannot be replayed. Tests cover these refusal boundaries and all three JSON destinations.

The native MCP check reads readiness and verifies its handshake without scheduling indexing. Actual agent enablement still needs the agent's own controls and verification prompt. Agent configuration replacement does not authorize stopping old workers or deleting derived state.

## Old startup ownership and removal

The trusted Service Console separately offers **Review old startup**. Select the old source, data directory, collection and executable. Review uses the verified new runtime; it never launches the selected old executable or opens its index. A bounded, same-user executable-path audit reports candidate PIDs and incomplete observations. Candidate matches do not authorize process signals.

Removal requires the saved AIRK receipt, exact generated command/definition, unchanged file proofs and native manager observations. Windows compares the actual Task Scheduler definition; Linux compares typed user-bus properties. Unknown, changed or unverifiable registrations are preserved. Mac loaded registrations currently refuse removal because full native configuration proof remains unavailable.

**Remove reviewed startup** asks for separate approval naming the captured scope and warning that owned background work can be interrupted. It stops/disables only the verified registration, removes its exact saved startup files and preserves sources, project settings and indexes. Other old workers remain running. An absent or damaged old executable does not authorize executing a replacement for proof.

The new CLI exposes the same explicit metadata-only workflow:

```console
airk service legacy review --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection> --executable <old-airk>
airk service legacy remove-startup --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection> --executable <old-airk> --approval <review-approval>
airk service legacy list --runtime-dir <initialized-1.4-control>
airk service legacy show --runtime-dir <initialized-1.4-control> --approval <review-approval>
```

Accepted removal saves a checksummed, namespace-bound intent before OS effects. Explicit saved-receipt recovery resumes the captured phase after interruption; completed replay returns the original result without touching a later registration. Read-only review/list do not initialize retirement state. The journal retains at most sixteen 16 KiB intents; only completed receipts can be explicitly forgotten. Missing initialized metadata, corrupt phases, changed files, unknown links and reappearing definitions refuse further effects. Old derived-state deletion remains a separate implementation gate.

Windows development evidence includes the actual CLI read-only fixture, fault/replay tests, trusted-panel approval tests and an explicitly invoked unique Task Scheduler fixture. The latter registers a stopped task pointing at an invalid executable, verifies native ownership, removes it, checks preserved source/settings/index bytes and replays completion. It never starts the old command. Native Mac qualification remains unavailable.
