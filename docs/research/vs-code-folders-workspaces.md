# VS Code folders, workspaces and AIRK project identity

**Research note, 2 November 2026, against source version 1.8.7.** This note analyzes current behavior and proposes changes. It does not change any behavior. The code references below describe the tree at commit `0dbd2f0`. Statements about VS Code itself come from its documented extension API behavior and are marked where they still need confirmation in an installed editor.

## Summary

The extension treats **each VS Code workspace folder as one AIRK project**. On trusted activation, and again whenever folders change, it registers every open `file` or `vscode-remote` folder that is not already registered. Each one gets a collection ID derived from a hash of its canonical path, background watching, model setup and an index job. It does not consider:

- whether the folder is a repository, a container of repositories, or one part of a larger repository;
- whether the folder overlaps another registered project;
- whether the user is working through a saved `.code-workspace` file, an untitled multi-root window or a plain folder.

The native registry enforces separation between sources and AIRK's own storage. It does **not** compare one project's source root with another project's source root. Several common VS Code habits therefore produce outcomes the user did not ask for:

1. **Duplicate indexing.** Opening a parent folder such as `D:\code\Github`, then later opening one repository inside it, creates two independent projects that both index, embed and watch the same files.
2. **One undifferentiated index of many repositories.** Opening a container folder registers a single collection spanning every child repository. All of those repositories share one set of budgets (4 GiB eligible source and 100,000 extracted files on a 32 GiB machine). Results carry no repository boundary.
3. **Silent accumulation.** Every trusted folder ever opened stays registered and watched after it is removed from the workspace or its window closes. The only automatic guard covers the home folder and filesystem/volume roots.
4. **Multi-root agent connections that depend on folder order and on one folder's configuration.** In a multi-root window, the first folder becomes the primary `--root` of the single VS Code MCP definition. Separately, a manual `.vscode/mcp.json` AIRK entry in *any* registered folder suppresses the provider for the whole window.
5. **Window-scoped state that does not follow the project.** Explorer saved searches live in VS Code `workspaceState`, so saving a folder as a `.code-workspace` (or adding a second folder) hides them. Folder-level window-scoped settings stop applying in multi-root windows.

Saving a folder as a workspace does **not** by itself break indexing. Project bindings are keyed by canonical folder path in `globalState`, and the native registry is reconciled by source root, so the same folder keeps its existing project.

The proposed plan has two halves. First, add overlap and repository awareness to the native registration review, so CLI, Workbench and MCP observe it identically. Second, have the extension build one reviewed *workspace plan* for all open folders instead of registering them one at a time. Phases are listed in [Proposed plan](#proposed-plan).

## How a project is created today

### Folder enumeration

`AirkServiceController.folders()` returns `vscode.workspace.workspaceFolders` filtered to `file` and `vscode-remote` URIs ([serviceExtension.ts:180-182](../../packages/vscode/src/serviceExtension.ts#L180-L182)). `vscode.workspace.workspaceFile` is not consulted anywhere: a `.code-workspace` file, an untitled multi-root window and a single folder all produce the same folder list. Virtual workspaces are declared unsupported (`capabilities.virtualWorkspaces: false` in [package.json](../../packages/vscode/package.json)), and untrusted windows register nothing.

### Triggers

- Trusted activation (`onStartupFinished`), `onDidChangeWorkspaceFolders` and `onDidGrantWorkspaceTrust` all call `automaticInstall()` followed by `refresh()` ([serviceExtension.ts:1100-1101](../../packages/vscode/src/serviceExtension.ts#L1100-L1101)).
- Before the first installation, a one-time review lists the folders that will be registered. **Customize…** can deselect them ([serviceExtension.ts:574-614](../../packages/vscode/src/serviceExtension.ts#L574-L614)). Deselected folders go to `airk.setupDeclinedFolders.1`, and explicitly chosen broad roots to `airk.setupApprovedRoots.1`.
- After the first installation, there is **no review for newly added folders**. Each new folder is registered with `confirm: async()=>true` ([serviceExtension.ts:847-886](../../packages/vscode/src/serviceExtension.ts#L847-L886), line 871).

### Per-folder registration

For each open folder, in order ([serviceExtension.ts:851-882](../../packages/vscode/src/serviceExtension.ts#L851-L882)):

1. A missing folder is logged and skipped.
2. `ServiceWorkspace.reconcileStartup` looks for an active native project whose root equals the folder's `realpath`. If exactly one matches, or the saved choice selects one, the folder is bound to it ([serviceWorkspace.ts:93-121](https://github.com/iokaio/airk/blob/a6caa821/packages/vscode/src/serviceWorkspace.ts#L93-L121)). An inactive (removed) match yields `removed`, which is respected.
3. A folder declined during the first-use review is skipped. A **home folder or filesystem/volume root** is skipped unless it was approved ([serviceExtension.ts:564-568](../../packages/vscode/src/serviceExtension.ts#L564-L568)).
4. Otherwise the folder is registered with collection `collectionId(canonical)`. That is `project-` plus the first 24 hex digits of SHA-256 of the canonical path, lower-cased only on Windows ([model.ts:22-25](../../packages/vscode/src/model.ts#L22-L25)). Registration also enables automatic watching and schedules model setup and indexing ([service_registry.rs:795-811](../../crates/airk-store/src/service/registry.rs#L795-L811)).

Manual **Add Project** (`registerWorkspace`) uses the same path with a modal review of source, storage and collection, plus a choice between automatic watching and manual indexing ([serviceExtension.ts:297-317](../../packages/vscode/src/serviceExtension.ts#L297-L317)).

### What the native registry checks

`register_project` validates canonical identity, the data scope and source isolation ([service_registry.rs:682-791](../../crates/airk-store/src/service/registry.rs#L682-L791)). `service_source_isolation::validate` requires every proposed source to be disjoint from:

- service control and installer-private paths;
- its own data root;
- every registered project's **data** root.

It also requires the proposed data root to be disjoint from every registered project's sources ([service_source_isolation.rs:169-219](../../crates/airk-store/src/service/registry/source_isolation.rs#L169-L219)).

**Source-to-source overlap between projects is not checked.** The only other refusals are reuse of the same data root and collection pair, and the 1,024-project capacity. Overlap checks exist only *within* one collection's additional-root aliases ([roots.rs:170](../../crates/airk-store/src/roots.rs#L170), limit of 16 aliases at [roots.rs:12](../../crates/airk-store/src/roots.rs#L12)).

### What gets indexed under a root

Discovery walks the root with the `ignore` crate: `require_git(false)`, hidden files included, no symlink following, plus `.airkignore` ([source_scope.rs:34-42](../../crates/airk-ingest/src/source_scope.rs#L34-L42)). Hierarchical `.gitignore`, `.ignore`, `.airkignore`, parent and Git exclude rules apply (see [service-source-watching.md](../service-source-watching.md)). Directories named `.git`, `node_modules`, `target`, `.venv` and a few others are hard-excluded ([lib.rs:1285-1299](../../crates/airk-ingest/src/lib.rs#L1285-L1299)).

There is no notion of a repository boundary. A child repository is just a subdirectory, and a submodule's `.git` file is excluded like any other `.git` entry. VS Code's `files.exclude` and `search.exclude` settings are not read.

### What agents receive

- **VS Code's MCP provider** publishes one `AIRK` definition per window. Its first registered folder is the primary `--root`; every further folder is a `--additional-root`, plus `--no-default-project` ([serviceExtension.ts:996-999](../../packages/vscode/src/serviceExtension.ts#L996-L999), [model.ts:64-78](../../packages/vscode/src/model.ts#L64-L78)). Agents in a multi-root window must call `projects` and pass `project` ([unified-mcp.md](../unified-mcp.md)).
- **If any registered folder contains a recognized manual AIRK entry in `.vscode/mcp.json`**, the provider returns no definitions at all ([serviceExtension.ts:888-903](../../packages/vscode/src/serviceExtension.ts#L888-L903)).
- **Project-local agent files** (`.mcp.json`, `.codex/config.toml` and similar) are written or migrated inside each project root as single-root entries ([agentMigration.ts:68](../../packages/vscode/src/agentMigration.ts#L68)).
- **Explorer Assistance** attaches only when the agent's working directory lies inside exactly one window's folders ([vscode-extension.md](../vscode-extension.md), Explorer Assistance section).

### Where state is kept

| State | Store | Keyed by |
|---|---|---|
| Saved project bindings and selected binding | `globalState` (`airk.service.workspace.1.*`, `airk.service.workspace.active.1.*`) | install directory, binding, opened folder path |
| Declined folders, approved broad roots | `globalState` | canonical folder path |
| Explorer saved searches (recipes), startup notice | `workspaceState` | VS Code workspace identity |
| `airk.explorer.planner.directory`, `airk.explorer.review.testPathPattern` | `resource` settings | per folder |
| `airk.explorer.assistance.enabled`, `airk.workbench.persist*` | `window` settings (default scope) | window / workspace file |
| Storage, runtime, MCP registration | `machine` settings | machine |

## Scenario analysis

| # | What the user does in VS Code | What AIRK does today | Impact |
|---|---|---|---|
| 1 | Opens a repository root folder | Registers one project; collection from its path hash | Intended baseline. |
| 2 | **File › Save Workspace As…** with that one folder | Same folder list, so `reconcileStartup` finds the existing project by root | Indexing unaffected. `workspaceState` belongs to a new workspace identity, so Explorer saved searches and the startup notice don't carry over. |
| 3 | **Add Folder to Workspace…** with a sibling repository (untitled multi-root, then possibly saved) | `onDidChangeWorkspaceFolders` registers the new folder as a second project, without review | Two independent projects, which is reasonable for unrelated repositories. The MCP definition becomes multi-root with `--no-default-project`, so agents that never pass `project` lose a default. VS Code documents that changing the *first* folder restarts running extensions (`updateWorkspaceFolders`); whether converting a single-folder window to an untitled multi-root window also restarts the extension host should be confirmed in qualification. |
| 4 | Opens a **container folder** of many repositories (`D:\code\Github`, `~/src`) | Registers one project for the whole tree, unless it is the home folder or a volume root | One collection mixing unrelated repositories under a single set of budgets. Budget exhaustion fails the pass, not one repository. Search and facts have no repository boundary. Watcher scope covers everything. Large first index and storage cost the user never confirmed. |
| 5 | Has a container registered (case 4), then opens one child repository | The child's realpath doesn't equal any registered root, so a **second project** is registered | Every file in the child is read, extracted, embedded (embedding packs are collection-local) and watched twice. The footer picks "the most specific root" ([index-coverage.md](../index-coverage.md)), but agents see two indexes, and the two can be at different publications. |
| 6 | Opens a **subfolder** of a registered repository (a monorepo package, `docs/`) | Registers a new project for the subfolder | Same duplication as case 5, for a subset. The subfolder index loses imports and references that cross its boundary. The user probably wanted the existing index filtered to that path. |
| 7 | Multi-root window that contains both a parent and its child | Two overlapping projects; Explorer and the footer prefer the most specific folder | Duplicate cost as in case 5, plus a multi-root MCP definition whose two scopes overlap. Agents can get the same file from two projects with different revision tokens. |
| 8 | The same repository through two paths: junction or symlink, `subst` drive, mapped drive, `\\wsl$\…` | `realpath` resolves symlinks and junctions to one project. Behavior for `subst`, mapped and UNC forms is **not verified** | Possible duplicate projects for one directory, or a project whose root only resolves on some hosts. |
| 9 | **Git worktrees** or several clones of one repository | Each worktree or clone is its own project | Correct isolation, but full index and embedding cost for each, with no reuse between collections. |
| 10 | Repository with **submodules** or vendored nested repositories | Indexed as part of the parent; each `.git` entry excluded | Usually intended. Nested `.gitignore` files apply. Whether a parent `.gitignore` crosses into a nested repository follows `ignore` crate semantics and is not separately tested. |
| 11 | Removes a folder from the workspace, or closes the window | Nothing changes natively; watching and indexing continue by design | Registrations accumulate (up to 1,024). There is no "not used recently" signal. The only way out is an explicit **Remove project**, which deletes the index. |
| 12 | Reorders folders or renames a folder's display `name` in the `.code-workspace` | A new folder order changes the MCP definition arguments; the tree item name stays `path.basename` | MCP server restart on reorder. Display names in AIRK don't match the user's workspace names. |
| 13 | Moves or renames the repository on disk | The old project reports a missing source; the new path registers a **new** project automatically | Silent re-index from scratch. The old index stays as retained read-only evidence until removed. This is correct by the "redirected root does not acquire the old binding" rule ([service-project-setup.md](../service-project-setup.md)), but unexplained to the user. |
| 14 | Remote windows (WSL, SSH, Dev Containers) | `extensionKind: workspace`, so the remote host gets its own runtime, service and projects | Local and remote checkouts are separate indexes. In a dev container, the index lives in the container's user data unless storage is mounted, so a rebuild can lose it. |
| 15 | Multi-root window where one folder is untrusted | Workspace trust applies to the whole window, so AIRK registers nothing | Consistent and safe. Only the existing trust prompt explains it. |
| 16 | Hides folders with `files.exclude` or `search.exclude` | Not consulted | Users may assume hidden equals not indexed. Only `.gitignore`, `.ignore` and `.airkignore` apply. |
| 17 | Keeps AIRK settings in a folder's `.vscode/settings.json`, then goes multi-root | `resource` settings still apply per folder. VS Code ignores folder-level `window` settings in multi-root | `airk.explorer.assistance.enabled` and `airk.workbench.persist*` silently revert to the workspace or user value. |
| 18 | One folder has a manual `.vscode/mcp.json` AIRK entry inside a multi-root window | The provider publishes nothing for the whole window | Other folders lose their VS Code AIRK connection, with no message naming the cause. |
| 19 | Starts Claude Code or Codex in the folder that holds the `.code-workspace` file (often the container) | No project-local AIRK configuration exists there; Explorer attachment fails unless that folder is itself a workspace folder | The agent has no AIRK connection, or it uses a connection to a container project from case 4. |
| 20 | Several VS Code profiles, Insiders, or another VS Code build | Separate `globalState`; the native registry is shared and reconciled by root | Bindings recover correctly. Declined-folder choices made in one profile or build don't apply in another, so a declined folder can be registered elsewhere. (Profile scoping of `globalState` is VS Code behavior to confirm in qualification.) |

### Root causes

1. **Folder equals project, with no classification.** Nothing distinguishes a repository root, a container of repositories, a repository subfolder, or a folder already covered by another project.
2. **No source-overlap model in the native registry.** Isolation covers sources against AIRK's own private state, not sources against each other. Overlap is therefore invisible to every client: CLI `service prepare-registration`, Workbench Management and MCP `projects`/`status` all lack it.
3. **Unreviewed incremental registration.** After first use, folders are registered one at a time, as they appear, with no review and no workspace-level view.
4. **Collection identity is path identity.** A move, a different path form or a fresh clone creates a new project, which is correct for isolation but expensive and unexplained.
5. **Window-level connection assembly depends on order and on the presence of a single file.** It is not a stable function of the set of projects.
6. **Editor state keyed by window, not by project.** Saved searches and some settings follow the VS Code workspace identity rather than the AIRK project.

## Proposed plan

The plan keeps existing constraints: sources stay read-only, collection and root isolation is preserved, there is no automatic discovery of folders the user has not opened or named, there is no new crate without independent purpose, and **CLI, Workbench and MCP parity** holds for any new read field. Each phase can ship independently. Phases 1 and 2 address the costly failures (duplicate and container indexing). Later phases address ergonomics.

### Phase 0: correct documentation and add scenario fixtures

- Correct [vscode-extension.md](../vscode-extension.md) line 236. It says multi-root workspaces "register one MCP server per root", but the provider publishes one multi-root definition. Document cases 2, 11, 17 and 18 as current limitations.
- Add extension tests that drive `automaticSetup` with stubbed `workspaceFolders` for cases 3–7, 12 and 18. These record current behavior before it changes.

### Phase 1: native source-overlap and repository observations

Add a bounded **source relationship** observation to the canonical registration review and project rows, computed in `airk-kernel` and `airk-store` alongside source isolation:

- `relationships: [{binding, collection, relation: "same"|"contains"|"contained_by", root}]` against every registered and retained project root, including registered aliases. Bounds follow the existing 1,024-project and 8 MiB root-metadata limits. This is a metadata comparison of canonical paths. It never opens another project's sources or index.
- `repository_markers` for the proposed root and its direct children: presence of a `.git` directory, a `.git` file (worktree or submodule, with the `gitdir:` target reported but not followed), `.hg`, `.svn` or `.jj`. The scan is bounded to a fixed number of child entries and is metadata-only, with no Git execution. It describes markers; it does not claim to understand repositories.
- `broad_root` classification moves from the extension into native code (home, volume root, plus well-known per-user containers such as Desktop, Documents, Downloads, the OS temporary directory and synced-folder roots). Every client then gets the same answer.

Surface these identically in `service prepare-registration`, the Workbench registration review and Management project details, and the MCP `projects` and `status` (full) responses. Add parity regressions comparing full CLI and MCP output for an overlapping fixture.

**Policy:** registration does not hard-refuse overlap by default, because existing users may already rely on nested projects. Instead:

- an **unreviewed** (automatic) registration refuses `same`, `contains` and `contained_by` with a typed reason (`source_overlap_review_required`);
- an explicit, reviewed registration may accept overlap after the review shows it.

### Phase 2: workspace plan instead of per-folder auto-registration

Replace the per-folder loop in `automaticSetup` with one **workspace plan** over all open folders, computed from the Phase 1 observations:

| Folder classification | Default action |
|---|---|
| Already registered (`same`) | Bind, as today. |
| Inside a registered project (`contained_by`) | **Don't register.** Bind the folder to the containing project with a path scope (Phase 3). Explain this in the output and in Management. |
| Contains registered projects (`contains`) | Ask: index as one project and exclude the registered children (Phase 4); keep the children only and skip the container; or register as-is with duplication. |
| Repository root marker, no overlap | Register automatically, as today. |
| No root marker but several child repositories (a container) | **Ask.** Offer per-repository registration of selected children, or one combined project. Never auto-register. |
| Broad root | Skip unless approved, as today, with the wider native classification. |

The plan runs on activation and on `onDidChangeWorkspaceFolders`. When every folder is automatic, it stays silent. When any folder needs a choice, it shows one non-modal notification that opens a single review ("3 folders: 1 ready, 1 inside *airk*, 1 container of 12 repositories"), rather than a dialog per folder. Choices are saved as today, under canonical paths. Declined and approved lists should also move into the native registry as opt-out markers (removed projects already leave one), so other profiles, builds and the CLI honor them.

### Phase 3: path-scoped bindings for subfolders

When a folder is `contained_by` a registered project, bind the folder to that project with a **scope prefix** (its collection-relative path) instead of creating a project.

- Add an optional native connection scope, `--scope-prefix <relative>`, to the launcher and the MCP connection. The canonical request path applies it to every scoped read, as an intersection with any request `path_prefix`, so CLI tool calls, Workbench and MCP behave the same. `projects` and `status` report the prefix. Disclose the prefix as a configured subset in each response, using the existing truncation and selection disclosure.
- The footer, Explorer and Management show "airk › packages/vscode" instead of a separate project.
- References and imports keep resolving across the whole project, while listings and search default to the prefix. Where a tool deliberately crosses the prefix (for example `references` to a caller outside it), the response says so.

This removes duplication for monorepo packages and for case 6 without weakening isolation, because the scope narrows an existing authorized project.

### Phase 4: containers and nested registered projects

Offer two explicit shapes for case 4 and case 7:

1. **Per-repository projects.** Register selected child repositories as separate projects. This is the recommended default for unrelated repositories.
2. **One combined project that excludes registered descendants.** Add a native per-project option, `exclude_registered_descendants`. Discovery and the watcher then prune any directory that is the root of another registered project, and coverage lists each pruned root as `excluded: registered_project <binding>`. Without this, a combined project duplicates its children.

For users who want one index across *separate* folders (code plus specifications), the Service Console already provides **Add source root** aliases ([serviceConsole.ts:250](https://github.com/iokaio/airk/blob/a6caa821/packages/vscode/src/serviceConsole.ts#L250)). The multi-root review can offer "combine these folders into one index" using those aliases. The review must state the existing limits: 16 aliases, shared budgets, and rejection of imported LSP and ctags analysis for aliased collections ([code-facts-and-relationships.md](../code-facts-and-relationships.md)).

### Phase 5: stable, per-project agent connections in multi-root windows

- Build the provider definition from projects sorted by canonical root. With `--no-default-project`, no root is privileged, so folder order no longer changes arguments or restarts the server (case 12).
- Make manual-entry suppression per project. Omit only folders whose `.vscode/mcp.json` owns an AIRK entry, and report in the status bar and output which folder's file suppressed what (case 18).
- When `workspace.workspaceFile` is a saved file outside every folder (case 19), offer an explicit, previewed **Write multi-root agent configuration next to the workspace file**. The existing preview, refusal and no-overwrite rules apply. Never write it automatically.
- Remove the `folders[0]` fallback in [explorerWorkspaceIO.ts:164-170](../../packages/vscode/src/explorerWorkspaceIO.ts#L164-L170). When no preferred root matches in a multi-root window, ask the user to choose a folder.
- Use the VS Code folder `name` for display while keeping the canonical root as identity (case 12).

### Phase 6: lifecycle and identity

- **Unused projects (case 11).** Record a native, bounded `last_client_observation` per project (last MCP, CLI or Workbench read; no content). Management, `service projects` and `status` (full) show "not used for N days" with **Pause watching** and **Remove**. AIRK never pauses or removes automatically.
- **Moved repositories (case 13).** When a new folder would be registered and a registered project's source is missing, the review offers "This looks like *old path*: relink instead of re-indexing?". Relinking is an explicit, reviewed native operation that changes the root and invalidates the publication, analogous to storage relocation. The evidence offered is metadata only (same collection-relative file names and content hashes for a bounded sample), not a guessed identity.
- **Worktrees and clones (case 9).** Evaluate opt-in reuse of embedding results across collections that share a data root and model descriptor, keyed by exact input bytes and descriptor as today. This needs a separate ownership and retention design, because packs are currently collection-local and removal deletes owned objects. Treat it as research, not committed work.
- **Path forms (case 8).** Add Windows fixtures for `subst`, mapped drive and `\\wsl$` paths, plus a macOS case-insensitive volume fixture. `collectionId` lower-cases only on `win32`, so it is unverified whether two spellings of one macOS path yield one collection.

### Phase 7: editor state and settings

- Move Explorer saved searches from `workspaceState` to storage keyed by project binding, migrating existing entries on first read. They then follow the project across folder windows, workspace files and multi-root changes (case 2).
- Document that window-scoped AIRK settings must live in user or workspace-file settings in multi-root windows. Alternatively, change `airk.explorer.assistance.enabled` to `resource` scope if per-folder control is intended (case 17).
- Offer an explicit, previewed **Suggest `.airkignore` from `files.exclude`** action. Never derive index scope from editor settings silently (case 16).
- For dev containers (case 14), the first-use review should state where the index will live, and recommend a mounted `airk.storage.path` when the container's data directory is not persistent.

## Validation for each phase

Each phase needs the usual repository checks from [CONTRIBUTING.md](../../CONTRIBUTING.md). In addition:

- native fixtures for every overlap relation, including alias roots, retained missing roots and Windows case and Unicode aliases (extending `service_source_isolation_tests.rs`);
- parity regressions comparing full CLI, Workbench and MCP outputs and input catalogs for the new fields and for `--scope-prefix`, exercised through actual CLI and MCP transports rather than renderer mocks;
- extension tests for the workspace plan across cases 1–7, 12, 18 and 19, including folder changes while a plan is pending and lost trust during review;
- installed-editor qualification for single-folder, saved-workspace, untitled multi-root, container and remote windows on Windows, macOS and Linux. Local results must not be reported as cross-platform qualification.

## Open questions and unverified points

- Does VS Code copy `workspaceState` when an untitled multi-root workspace is saved to a file? This affects how much of case 2 also applies to case 3.
- Is extension `globalState` profile-scoped in the supported VS Code range? This affects case 20 and whether the declined-folder lists should move native.
- How do `realpath` results behave for `subst`, mapped drives and `\\wsl$` paths, and on case-insensitive macOS volumes? (case 8)
- Should parent `.gitignore` rules apply inside a nested repository? Current behavior follows the `ignore` crate and has no dedicated fixture. (case 10)
- Should `contained_by` registration be refused outright for reviewed registrations once Phase 3 exists, or remain an informed choice?
- For Phase 4, should pruning of registered descendants be the default for newly reviewed container projects?
