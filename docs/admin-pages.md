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
logs, HTTPS trust and rotation status, and agent connection details. The pages
show the exact `airkctl` command for changes they cannot make yet.

**Not yet served** (the API answers `not_yet_served`): every mutation (pause,
retry, cancel, revoke, stop, restart), the live event stream, and search,
which needs the admin read grant. Indexed evidence is always shown as text.

The pages are built from `packages/admin` (`npm run build`; `npm run check`
fails when the committed bundle in `crates/airk-cli/admin-assets/` differs
from its sources) and embedded in `airk`; they work offline.
