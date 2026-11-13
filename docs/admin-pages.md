# AIRK admin pages

The AIRK service serves a small management web interface at
`https://127.0.0.1:<port>/admin/` on its local HTTPS listener (split plan §7,
contract I3). The listener runs when the service's catalog has been
initialized; a control root that never completed catalog setup has no
listener.

## Opening the pages

```console
airkctl admin open    # opens the pages in your default browser
airkctl admin url     # prints a one-time sign-in URL (a secret; valid for 60 seconds, once)
```

`admin open` writes an owner-only redirect file and opens it, so the sign-in
code never appears in a command line. It refuses on a remote or headless
session. Bookmarks do not work: the port can change after a restart, and a
restart ends every session.

## How sign-in works

`airkctl` asks the service, over its owner-only control pipe, for a single-use
code. The page exchanges it once for a session cookie (`__Host-`, `Secure`,
`HttpOnly`, `SameSite=Strict`) and an anti-forgery token that every call
carries. Sessions end after 30 idle minutes or 12 hours, on sign-out, or when
the service restarts. Bearer tokens are refused on `/admin`, the admin cookie is
refused on `/mcp` and `/api/v1`, and nothing that returns a credential is ever
served over HTTP.

The service also checks that the browser's connection belongs to the same
local user (Windows: the connection's owning process; Linux:
`/proc/net/tcp`). **macOS has no such check yet, so the admin API answers
`admin_peer_refused` there.**

## What works in this version

Reads go through the same service handlers as `airkctl`: service health and
resources, the index list, workers, events, requests, jobs, recovery plans,
logs, HTTPS trust and rotation status, and agent connection details.

Ordinary changes run as `airkctl` runs them: indexing, configuration, retry,
pause and resume, plan creation, policy resume, revoke, stop and restart. The
service generates each request's identity (never the browser), journals it
where `airkctl` journals it, and the reply carries `request: {nonce, state}`.
If the service's answer is lost twice the page reports the outcome as unknown
instead of retrying. After **Restart** the service exits and every admin
session ends; open the pages again once it is running.

A search scope can be chosen among existing indexes (`search.select_scope`);
the grant lives only in the browser session.

**Index library** (U6, U97, U98). The index library lists indexes, index
sets and grants, with their sources, memberships, policy and split state.

- **Set up an index:** fill in the name, folder, data location, collection,
  extra folders and watching. **Browse…** beside each path opens a folder
  dialog served by the service (`files.browse`, M24): drives, your home folder
  and the AIRK data location to start from, folders only, links shown but
  never entered, existing index folders marked. The data location defaults to
  a folder named after the collection inside the installation's data
  location; the setup creates it. The service's full review is shown, then
  **Create index** applies exactly that review. Setup also creates an index
  set named after the index (1.8.34, U103), so agents and the Explorer can
  read it at once.
- **Make readable** (1.8.34): an index that belongs to no index set (created
  before 1.8.34) gets the set named after it, created or reused, with every
  enabled source root, through the same preview and apply as **Index sets**.
- **Indexing progress** (1.8.34, `indexes.progress`): on Overview and in an
  index's detail, the running job's phase, files committed of files found,
  publications, elapsed time, stalled and paused flags, and the pending and
  queued jobs; refreshed about every 2 seconds while work runs, with the
  complete result kept.
- **Index sets:** create, rename, add or remove member indexes, delete.
- **Index changes:** rename, source changes, policy resume, split and retire.
- **Grants:** draft and revoke.

Every change is drafted or previewed by the service and shown in full before
it is applied. The request identity stays in the service session, so the
page holds only a review token.

Removing or retiring something asks you to type the target's name (the
grant's id for a revoke; for a split, the original index's name). A wrong
name is refused and nothing is sent.

The page can be opened on a route: `airkctl admin open --page index-library
--for-folder <folder>` prefills a new index for that folder. The VS Code
Explorer's **Create an index for this folder** uses this route.

**Usage** shows whether usage recording is on and what it keeps, then the
report for a chosen index and period (1, 7, 14 or 30 days): calls and
failures, per-client and per-operation tables, a daily history with bars,
estimates labelled as estimates ("not available" when the service has none)
and recording health. Changing the recording mode is an `airkctl` command,
shown ready to copy.

**Agents** shows your MCP endpoint (URL, state and whether Windows trusts the
service's certificate, with the fix command when it does not), how to connect
an agent automatically (VS Code **Connect Coding Agent**, or `airkctl agents
connect`), and how to configure one by hand: ready-to-copy entries for Claude
Code (`.mcp.json`), Codex (`config.toml`), VS Code / Copilot (`mcp.json`) and
any other MCP client (URL, Streamable HTTP, `Authorization: Bearer` from
`airkctl agents credential`), each with its certificate trust step. They are
the same entries `airkctl agents connect` writes. An existing connection can be
looked up, verified and revoked. Creating a connection is never done over
HTTP.

**Activity** loads the recent events as a table (local time, what happened,
index, job), with an index filter and older pages; a job opens as a card with
retry or cancel; saved requests and pause or resume of an index chosen by
name.

**Help** lists the help topics and tools with a filter; a topic or a tool's
parameters are shown as readable text and tables.

Every page keeps the complete native result available under **Complete result
(JSON)**.

**Storage** lists the service's locations with their sizes (U100):
program, data, state, cache and logs.

- **Clean up the cache:** deletes every cache file.
- **Clean up closed logs:** deletes old log files, never the one being
  written.

Each purge first shows the exact plan, and runs only after you type the
location's path. Indexes are removed in the Index library, never here.

**Service** › **Start at sign-in** (Windows, U101) shows, turns on or turns
off login startup. Turning it off is refused while the service itself was
started at sign-in; stop it and run `airkctl service login uninstall`.

**Search** under the chosen scope is served: the service holds the scope as
an in-memory read authority, revoked on sign-out, expiry and re-selection,
and searches through the same canonical handlers as `airkctl` and VS Code.

**Not yet served** (the API answers `not_yet_served`): the live event
stream. Indexed evidence is always shown as text.

**Not in 1.8.31–1.8.34** (U102; planned for a later candidate):

- the Overview's version, activity and leftovers;
- plans on the Activity page;
- running a recovery plan from the Recovery page;
- code, document and evidence-inspector views beside Search (`packages/ui`);
- admin rate limits and the operation-table coverage test.

The pages are built from `packages/admin` (`npm run build`; `npm run check`
fails when the committed bundle in `crates/airk-cli/admin-assets/` differs
from its sources) and embedded in `airk`; they work offline.
