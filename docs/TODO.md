# AIRK follow-up work

## Index the same project at different locations for branch feature work

Recorded October 25, 2026. Status: deferred; no worktree-indexing behavior is implemented by this note. Keep this separate from the 1.4.5 redb cutover.

**Problem reported:** a feature checkout such as `D:\code\Github\airk-storagekv` needs to be indexed independently while the original AIRK project remains at another location and may be on another branch. AIRK should support both checkouts concurrently, including linked Git worktrees. The precise failing onboarding/configuration path still needs reproduction; this note does not claim that every second-root registration fails today.

Existing registrations bind canonical source root, data root and collection. Copied or inherited workspace/MCP configuration can therefore refer to the original checkout's opaque binding or storage destination. Reusing that identity for another checkout must not return the original checkout's evidence or overwrite its index. See [project registration](service-project-setup.md) and [source/data isolation](service-jobs.md).

**Likely solution to investigate later:** distinguish repository grouping from the identity of an indexed checkout. Give each canonical checkout/worktree root its own registered binding, collection/incarnation, durable jobs and derived storage. Use an opaque root-derived identifier or suffix to avoid default-name collisions. Git common-directory identity may group related checkouts in the UI; branch name alone must not select or own an index because branches can be renamed or switched in place.

During trusted onboarding, detect a saved binding whose canonical source differs from the open workspace. Offer an explicit, scoped new-checkout registration/configuration update rather than silently attaching to the old index or replacing the original registration. Preserve unrelated MCP servers/settings and existing source/data isolation checks. Branch switches at the same root should invalidate/reconcile that checkout's evidence through normal watching/indexing; changing branch names should not by itself create unlimited duplicate indexes. Handle worktree moves/removal through reviewed rebinding/retirement, preserving old state until the replacement is verified.

Before implementation, reproduce the issue with the current native service and the extension/agent setup path, then choose the smallest change within the existing registration architecture. Immutable model assets or embedding cache entries may be reused only through already validated identity/ownership contracts; mutable index state and revision tokens stay isolated.

**Acceptance criteria for the later change:**

- Two copies or linked worktrees of one repository, on different branches, index and query concurrently through the same per-user service.
- Each workspace advertises its own correct MCP binding; copied configuration is detected without changing the original checkout's binding.
- Citations resolve to the selected checkout's original files; revisions, cursors, jobs and results cannot cross between checkouts.
- Branch changes, dirty files, restarts, path aliases, moved/deleted worktrees and storage-name collisions have explicit tested behavior on supported platforms.
- Quota admission, protected state/source separation and owner-validated cleanup remain intact. Removing one checkout does not remove another checkout's state.
