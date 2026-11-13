# 1.8.35 candidate: agents from the Explorer, open connections, per-index results (local, Windows)

**Advanced split source checkpoint (13 November 2026).** Subsequent P0 source
work restores extracted component test ownership and the one-shot `airkctl`
command thread used by the pre-split CLI and session requests. The first P1
correction fences automatic scope switching/reselection and retains both
revocation obligations of an uncertain lease renewal. Exact-entry removal keeps
reviewed agent rows across confirmation and repair, revalidates live entries and
retains partial action errors. Admin job details and catalog pages fence stale
replies and retain retry successor receipts. Targeted Windows
checks are recorded in the [execution ledger](../simplify/one-process-ledger.md).
No installer or VSIX was rebuilt for this checkpoint; retained artifact receipts
continue to identify their original sources. The candidate version stays 1.8.35.

**13 November 2026. Preparation.** 1.8.35 is 1.8.34 ([record](release-1.8.34.md)) with the agent
connection and search scope changes its manual install asked for. Native/extension manifests and
lockfiles are synchronized at 1.8.35 (`python scripts/release-version.py check --expected 1.8.35`).
The scope is a Windows AIRK Service installer and VSIX only (U99). Decisions U104–U109
([split plan](../split/split-plan.md) §2.1; specification [open agent access](../split/open-agent-access.md))
amend U6, U77 and U97.

**What 1.8.34 did.**

- Connecting an index to a coding agent needed AIRK Admin's Agents page and a hand-made reviewed
  selection file. The Explorer's **Management › Agents** only linked to AIRK Admin.
- **Search scope › Apply selection** stayed disabled after checking the window's own index, and an
  index set plus an index outside it, or an index in no index set, was refused.

## What changed

- **Search scope is a union (U104).** Applying a scope reads everything in the checked index sets
  plus every checked index. Narrowing a set by checking some of its indexes is withdrawn.
  - **Apply selection** is enabled for any changed choice, including one that replaces the
    automatic scope.
  - Applying an index that is in no index set first creates a visible index set named after it,
    the same set index setup and **Make readable** create (`airkctl library set-create --index`).
  - **Save as index set** creates a new named index set holding the checked indexes and the
    indexes of the checked sets.
- **Open agent connections (U105, U106).** A new kind of connection whose token admits the agent to
  the MCP endpoint without a per-connection reviewed grant. Existing grant-bound connections keep
  working unchanged.
  - Each open connection has a **default scope** (indexes and index sets) stored by AIRK Service.
    A call naming nothing reads it. Changing it takes effect on the next call; no new credential,
    reconnect or configuration rewrite.
  - A named index is read whole (all of its enabled source roots); an index set is read as its
    member indexes, each whole. An index set is a grouping, not a read prerequisite, for these
    reads.
  - **Agent choice is off by default; never outside the assigned set (U109).** Off: a call naming
    nothing reads the whole assigned set (the default indexes plus the members of the default
    sets, at the current catalog revision). A call may narrow with `index_ids`/`index_set_ids` to
    a non-empty subset of the assigned set; a set is allowed when all of its members are inside.
    Anything outside is refused `agent_scope_choice_disabled` with a message naming the outside
    indexes, and `list_indexes` is neither listed nor served. On: read tools accept any indexes
    and index sets (union, at most 32 each) and `list_indexes` lists every index and index set with
    its id, name, description and indexed folders. **With choice on, any agent holding that token
    can read every index of this OS user.**
  - Names resolve by id (`idx_…`, `prj_…`), then exact name, then a unique case-insensitive name.
    Ambiguous or unknown names refuse `scope_name_ambiguous` / `scope_name_unknown`. An empty
    assigned set refuses `agent_default_scope_unset`.
  - **Open connections survive catalog changes (U106f).** Creating, describing or changing indexes
    and sets no longer closes an open HTTPS connection: each call re-resolves its scope at the
    current revision. A pending catalog fence, revocation and service end still deny or close it;
    grant-bound connections keep their existing fencing.
  - Agent reads have their own selection and worker budgets, so they never use the window's or
    AIRK Admin's capacity.
- **Agents from the Explorer.** **Management › Agents** connects Claude Code, Codex or VS Code /
  GitHub Copilot with one click: an open connection whose default scope is the window's search
  scope. A connection connected there follows the window's search scope when another one is
  applied. **Let the agent choose indexes** switches agent choice. **Disconnect** removes the
  entries naming a connection and revokes an open connection that no remaining entry names.
  Overview and Indexing link to the screen.
- **Results per index (U107, U108).** Multi-index results stay grouped by index (`members[]`).
  - An optional `limit_per_index` (1–100) on every read that takes `limit` and can span several
    indexes (combined, window/admin selection and open reads) gives every index exactly that many
    results. `limit` is then the overall cap and defaults to indexes × `limit_per_index`; a product
    above the cap or above 100 is refused `combined_limit_per_index_too_large`. Byte, deadline and
    verification budgets stay shared evenly. Absent, allocation, encodings and cursors are
    unchanged.
  - The member reads of one request run concurrently, up to the machine's query concurrency
    (concurrency 1 is the previous serial order). Results keep canonical member order.
  - Every read member reports `elapsed_ms` (timing, outside every identity), and members and ranked
    references carry `index_name` (display text only).
  - Still at most 32 indexes per read. Explorer Search scope, Save as index set, and AIRK Admin's
    index set and Search pages note when a scope covers more indexes than the query concurrency,
    so searches take several rounds. Informational only.
- **Workbench and Explorer Search.** The Workbench reads the window's search scope (default) or
  explicitly named indexes and index sets (by id or name) through the same service path as MCP and
  the CLI, with complete results, copies and exports; each turn records its selection and shows
  member `index_name` and `elapsed_ms`. **Max results per index** sends `limit_per_index` in the
  Workbench form and Explorer Search.
- **Descriptions.** Indexes and index sets have an optional description (at most 1 KiB), edited in
  the Explorer's Search scope or with `airkctl library describe`. Without one, `list_indexes`
  generates a description from the name and source paths; it is never stored.
- **New and extended `airkctl` commands:**
  - `airkctl agents connect --open`: enroll a new open connection and write the agent's entry;
  - `airkctl agents default --connection-id ID [--index …] [--set …] [--clear] [--agent-choice off|on]`;
  - `airkctl tools call --index/--set/--connection` and `airkctl tools list --connection`: the
    same service path as an agent's MCP calls (parity); `limit_per_index` goes in `--args`;
  - `airkctl library describe --index|--set … --text …|--clear --yes`;
  - `airkctl library set-create --index …`.
- **Fixes found on the way.** Multi-index open reads no longer require a Project for every hit, and
  discarded combined read failures are recorded in the service log.

## Stored state

- Credential ledgers without open connections serialize byte-identically (captured ledger golden).
  A ledger holding an open connection is refused by a 1.8.34 or older binary (fail closed).
- Descriptions live in a separate `catalog_descriptions` layer created only by the first
  description write. Untouched installations keep their `airk-state/1` identity (every retained
  layout fixture is recognized unchanged; one new reviewed fixture covers the layer).
- Existing wire fixtures and catalog items without the new fields are byte-identical.

## Installing over 1.8.34

1. Install the 1.8.35 VSIX and reload VS Code windows.
2. Run the 1.8.35 AIRK Service installer with the same folders. No uninstall is needed.
3. In the Explorer, apply a search scope, then connect agents from **Management › Agents**. Agents
   connected earlier through AIRK Admin keep working.

## Not in this candidate

- **Tool list change notifications.** Turning agent choice on or off does not send
  `notifications/tools/list_changed`; a running agent reloads its tool list itself (Claude Code
  `/mcp`, restart Codex).
- **AIRK Admin's Agents page** still connects reviewed (grant-bound) connections only; open
  connections are made from the Explorer or `airkctl`.
- **Explicit Workbench selections are not persisted** across reloads; the window scope is the
  default again after a reload.
- **The combined capabilities reply is near its 64 KiB bound.** Further catalog growth needs a
  bound or catalog review first.
- The U102 items and split follow-ups F1–F5.

The known limitations of [1.8.31](release-1.8.31.md#known-limitations) still apply. Installed-editor
qualification, real-client qualification of open connections, macOS/Linux installers, hosted CI and
Phase 8 are not run, and nothing is published.
