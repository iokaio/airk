---
id: recovering-errors
title: Recover from an error
route:
area: guidance
aliases: error, retry, disconnected, timeout, stale, failure, recover, invalid file, permission, lost acknowledgment
related_airk_help: troubleshooting, index-health, request-recovery, freshness, maintenance
---
# Recover from an error

Errors keep their diagnostic code while the UI explains the next useful step. Query failures leave forms editable. Correct the input or connection, then retry deliberately. A failed or timed-out read does not justify rebuilding a collection or replaying a management operation.

## Common cases

- **Disconnected service**: open Management > Service and choose Connect / reconnect. Use Start service only when you intend to start it; reconnect does not clear an explicit Stop preference.
- **No selected project**: review the workspace registration in Management > Projects. Projects outside this window are listed for management but cannot be selected as this window's evidence source.
- **Untrusted workspace**: review VS Code's trust decision. AIRK does not start native evidence work or accept management requests until authorized. Bundled help remains available.
- **Stale binding, citation or publication**: confirm the current project, refresh status and run the originating query or strategy again. Old handles and shared snapshots cannot be retargeted to another project. Review existing draft work before applying a new proposal.
- **Vectors unavailable**: use lexical search, or review Vector search/model setup in Management > Indexing. Querying does not install a model or start enrichment automatically.
- **Invalid imported plan or strategy**: check the schema, file format, size and reported field. Loading leaves the current draft intact until a validated replacement is explicitly accepted. AIRK Markdown plans import as draft tasks without complete evidence metadata; JSON exports preserve exact state/provenance.
- **Save refused**: choose a local workspace destination, resolve unsaved editor changes or review a changed/linked target. Remote folders use Open as draft and the editor's Save command. A refused save is not permission to bypass containment or overwrite another editor's work.
- **Expired/conflicted suggestion or share token**: ask for a new proposal or explicitly share a fresh reviewed snapshot. Keep the current draft when its revision changed.
- **Request queue full**: wait for pending work, narrow queries or review/discard unneeded Inbox proposals. Repeated clicks do not increase capacity.

## A read timed out

Read requests stop waiting after three minutes; interactive dialog/action requests have a longer deadline. The UI clears its waiting state and ignores late replies. You may retry a read or narrow its scope. Check Activity when native work may still be running.

## An action's outcome is unknown

An action can have effects even when its acknowledgment was lost. The UI reports REQUEST_OUTCOME_UNKNOWN instead of claiming that nothing happened. Check Management > Activity, saved files or proposal status first. Durable native requests are recovered through their saved request identity; do not create a duplicate mutation to test whether the first worked.

The UI never automatically replays saves, configuration changes, proposal application or repair. Preserve logs and exact request identities while diagnosing. Keep original files and useful index state; use Repair only after ordinary status/reconnect checks reveal a reason.

## Help and support

Help search and bundled engine guides work offline. If Live reference fails, choose Back to the bundled topic. About AIRK links to the Ioka Discord community, GitHub Issues and the contribution guide. Review diagnostics for private paths, source content and tokens before sharing them.
