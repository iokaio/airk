# Reviewed legacy transition

The 1.4 **Connect Coding Agent** guide supports explicit review of a project JSON entry for Claude Code, Gemini CLI or Antigravity. Review does not execute the old command, stop an old worker, convert an index or change startup registration.

Only the same-named `airk-<collection>` stdio entry is eligible. Its absolute `airk`/`airk.exe` command and seven legacy arguments must match the selected source and collection exactly; its old data directory may differ from the fresh 1.4 directory. Extra arguments, URL transports, duplicate JSON keys, foreign roots, unknown commands, redirected directories and linked files require manual review. Reads are bounded to 1 MiB.

The guide opens an editor diff before offering **Replace reviewed entry**. The replacement changes the selected command and arguments to the registered stable-launcher binding, preserving other entry properties, other servers, comments and approval settings. It neither starts nor enables the selected agent. User configuration and TOML/other client formats remain manual merges using the displayed connection.

Approval binds the exact original and proposed bytes, client, source, collection and native binding. The editor checks disk contents, document version, unsaved changes and save outcome. A changed proposal, stale file, copied review object, closed guide or lost workspace trust refuses the write. A consumed review cannot be replayed. Tests cover these refusal boundaries and all three JSON destinations.

The native MCP check reads readiness and verifies its handshake without scheduling indexing. Actual agent enablement still needs the agent's own controls and verification prompt. Agent configuration replacement does not authorize stopping old workers or deleting derived state.

## Old startup ownership and removal

The trusted Service Console separately offers **Review old startup**. Select the old source, data directory, collection and executable. Review uses the verified new runtime; it never launches the selected old executable or opens its index. A bounded, same-user executable-path audit reports candidate PIDs and incomplete observations. Linux also recognizes the kernel's deleted-executable path suffix; use CLI paths or a saved review when the original file is gone. Candidate matches do not authorize process signals.

Removal requires the saved AIRK receipt, exact generated command/definition, unchanged file proofs and native manager observations. Windows compares the actual Task Scheduler definition; Linux compares typed user-bus properties. Unknown, changed or unverifiable registrations are preserved. Mac loaded registrations currently refuse removal because full native configuration proof remains unavailable.

**Remove reviewed startup** asks for separate approval naming the captured scope and warning that owned background work can be interrupted. It stops/disables only the verified registration, removes its exact saved startup files and preserves sources, project settings and indexes. Other old workers remain running. An absent or damaged old executable does not authorize executing a replacement for proof.

The new CLI exposes the same explicit metadata-only workflow:

```console
airk service legacy review --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection> --executable <old-airk>
airk service legacy remove-startup --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection> --executable <old-airk> --approval <review-approval>
airk service legacy list --runtime-dir <initialized-1.4-control>
airk service legacy show --runtime-dir <initialized-1.4-control> --approval <review-approval>
```

Accepted removal saves a checksummed, namespace-bound intent before OS effects. Explicit saved-receipt recovery resumes the captured phase after interruption; completed replay returns the original result without touching a later registration. Read-only review/list do not initialize retirement state. The journal retains at most sixteen 16 KiB intents; only completed receipts can be explicitly forgotten. Missing initialized metadata, corrupt phases, changed files, unknown links and reappearing definitions refuse further effects.

## Explicit old derived-state deletion

Once 1.3 is no longer needed, use the new runtime's separate offline CLI operation. Stop the old startup/worker first. This operation never opens an old SQLite database, queries old evidence, converts an index or runs an old command. It accepts only the canonical format-126 ownership header from 1.3 in `collections-1.2`, with an exact source/collection match. Other formats remain untouched.

```console
airk service legacy preview-derived --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection>
airk service legacy remove-derived --runtime-dir <initialized-1.4-control> --root <old-source> --data-dir <old-data> --collection <old-collection> --approval <preview-approval>
airk service legacy derived-receipts --runtime-dir <initialized-1.4-control>
```

Review the reported paths, file count and logical bytes before running `remove-derived`. Each explicit call removes at most 4,096 captured files. Repeat the exact command/approval while `complete` is false. `derived-receipts` retains captured previews and completion state for recovery; it does not inspect or execute the old index. `forget-derived --runtime-dir <control> --approval <approval>` removes only a completed receipt.

The store adapter holds the old scheduler worker, collection writer and exclusive reader gates. It captures native file identities, lengths and modification times, saves a checksummed intent in the new control namespace before deleting anything, and rechecks all remaining files on each turn. New/changed files, replacement directories, foreign quota allowances, hard links, redirects and unknown entries inside derived subtrees refuse further deletion. Sources, root-level user files, saved configuration and usage history remain. Empty owned directories and synchronization metadata may remain; this is derived-file cleanup, not removal of the user's data directory. Completed replay returns its receipt without touching a later index.

The operation bounds discovery to 65,536 entries and 1 TiB of logical derived bytes, with sixteen retained intents of at most 32 MiB each. Unknown journal files are preserved; reserved partial writes are swept only when their bytes match a prefix of the exact captured intent. Old and new quota namespaces stay separate. This administrative retirement adapter is never a normal project reader/writer fallback and does not make old jobs participate in the new service.

Windows development evidence includes the actual CLI read-only fixture, fault/replay tests, trusted-panel approval tests and an explicitly invoked unique Task Scheduler fixture. The latter registers a stopped task pointing at an invalid executable, verifies native ownership, removes it, checks preserved source/settings/index bytes and replays completion. It never starts the old command. Native Mac qualification remains unavailable.
