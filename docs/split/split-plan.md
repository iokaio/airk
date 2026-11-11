# Split AIRK: Service, airkctl and VS Code Extension

**Status:** proposed plan, revision 2, 10 November 2026, on `feat/split`
(created from `feat/simplify` at `59dd3c721ca4c8301664827abaa9ea2ac6fd8dc8`).
This document authorizes no implementation, installer repair, build, signing,
version bump, commit, publication or merge. The inherited source version is
1.8.23.

| Revision | Change |
|---|---|
| 1 | Two products: a standalone service installer and a UI-only extension that discovers the service. |
| 2 | Adds `airkctl`, a service-only `airk`, browser admin pages on the HTTPS endpoint, an extension that reaches the service only through `airkctl`, a fixed-layout installer that removes pre-split installations, and code and installer maps. Records the user's decisions of 10 November 2026. |

**Reading order.** §2 lists what the user has decided and what still needs an
answer. §4 defines the boundary between extension code and service code. §5–§9
specify each piece. §10 and §9.5 are the code and installer maps. §11 is the
order of work.

## 1. Outcome

AIRK ships as two separately installed products built from four deliverables.

| Product | Contains | Installed by |
|---|---|---|
| **AIRK Service** | `airk` (the resident service, no CLI), `airkctl` (the whole CLI), the admin pages (embedded in `airk`), default models and notices | A signed per-platform installer from **https://airk.ioka.io**, with versioned assets in [iokaio/airk GitHub Releases](https://github.com/iokaio/airk/releases). Works without VS Code. |
| **AIRK VS Code Extension** | Explorer and Workbench for search, evidence, review and strategies; status; agent connection for the workspace | The Marketplace or a VSIX. Contains no native code and installs none. |

~~~mermaid
flowchart LR
    B[Local browser] -->|HTTPS /admin, signed-in session| H
    A[Coding agents and local programs] -->|HTTPS /mcp and /api/v1, bearer| H
    E[VS Code extension] -->|child process only| C[airkctl]
    U[Terminal user] --> C
    C -->|owner-checked named pipe or Unix socket| M
    I[Service installer] -->|signed setup step| S
    subgraph S[airk: one resident service per user]
        H[Loopback HTTPS listener]
        M[Control and query endpoints]
    end
~~~

The split stays in this repository. It needs two new Rust crates and two new
front-end packages (§10); revision 1's statement that no new crate is needed
no longer holds once `airkctl` must be lightweight.

Dependency direction and the single resident service in
[architecture](../architecture.md) stay in force. Contract, deletion and review
rules in [the simplification plan](../simplify-plan.md) apply to every step
here. This plan starts no unrelated simplification milestone.

## 2. Decisions

### 2.1 Recorded user decisions (10 November 2026)

These came from the user's request and answers. They are not agent choices.

| # | Decision |
|---|---|
| U1 | Two separately installed products. Installing or updating the extension never installs or replaces the service. |
| U2 | Windows and macOS installers are signed. Linux signing is optional. |
| U3 | `airkctl` is a separate, signed, lightweight CLI installed by the service installer. It carries the whole user CLI and talks to the service over the local pipe. |
| U4 | `airk` exposes no CLI. Started by hand, it reports that the service is already running, or starts it and explains how to stop it with `airkctl`. |
| U5 | Admin pages are served over HTTPS on the same host and port as the MCP endpoint, for a local browser user. |
| U6 | Management is admin-only. Index library, storage, service lifecycle, grants and recovery are managed in the admin pages and through `airkctl`, not in the extension. The extension keeps search, evidence, review, strategies and Workbench, shows status and opens the admin pages. |
| U7 | The first release of the admin pages also has search and evidence browsing. It has its own index selection and can use any index or index set the service holds. |
| U8 | The extension has no direct communication with the service. Everything, including Workbench reads, goes through `airkctl`. |
| U9 | The extension runs one long-lived `airkctl session` per window, plus one-shots for simple calls. |
| U10 | Explorer Assistance stays. Drafts reach the window through its `airkctl` session; the service never connects to the extension. |
| U11 | `airkctl` owns writing agent MCP configuration. The extension calls it. The admin pages manage connections and show the command to apply. The service never writes into project folders, and the installer never edits agent configuration. |
| U12 | The extension leaves any pre-split AIRK installation alone. It finds the split service and updates AIRK's MCP entries. If no service is installed it still removes the old AIRK MCP entries, and Explorer says that this happened. |
| U13 | The service installer stops old AIRK processes and services and deletes old indexes and old installed files. It first tells the user that this will happen, that indexes must be rebuilt, and that no source file is affected. |
| U14 | Removal is receipts-only. The installer removes only items that carry an AIRK ownership receipt, installs anyway, and reports what it left. |
| U15 | Fixed layout. The platform installer places files in one fixed per-user folder and replaces them in place. A small non-interactive setup step does old-install removal, login startup, certificate trust and start. Staged slots, rollback copies and organization-managed payloads are not carried into the split. |
| U16 | The executable that performs the setup step must be signed by CI. macOS rejects it otherwise. |
| U17 | One service per user account. A preview build replaces the stable one. |

### 2.2 Open decisions

Each needs the user's answer before the phase named. The proposal is the
agent's recommendation, not a decision.

| # | Question | Proposal | Needed before |
|---|---|---|---|
| O1 | Approve two new crates, as CLAUDE.md requires for extraction? | `airk-ipc` (library) and `airkctl` (binary). See §10.1 for the criteria check. | Phase 2 |
| O2 | Approve the contract changes under the simplification plan's D8? | New control methods, the `airkctl` output and session contract, the admin HTTP contract, and a two-executable install record (§4.2). No change to evidence tool results or digest inputs. | Phase 1 |
| O3 | Approve the HTTPS boundary change? Today HTTP exposes no management and refuses every browser `Origin`. | The admin plane in §7, after a dedicated security review. | Phase 1 |
| O4 | Does the split keep `airk-state/1`? | Yes for registry, catalog, collection and credential formats. New installs use state, data and cache folder names no pre-split build used. Any change to a layout pin that `airk-state/1` owns needs a schema-identity proof or a new state contract. | Phase 1 |
| O5 | What happens to indexes on later installs? | Every pre-split installation is removed with its indexes (U13). Split-to-split repair and upgrade keep indexes when the state contract matches. This narrows the "through 1.9.0, same build also resets" rule to pre-split installations. | Phase 7 |
| O6 | What does "update MCP entries" do when the service has no matching index, which is always true right after the first install? | Remove the dead entry, record it, and offer to re-add it once an index exists. Rewriting an entry to a live index keeps today's single confirmation because it creates a grant. | Phase 6 |
| O7 | Version and name of the first split release. | Not proposed here. | Phase 7 |
| O8 | Is the stopped installed-editor repair (D10) and the untested HTTP-cleanup correction superseded? | Yes: the VSIX stops installing and the staged engine is not carried forward (U15). Do not resume either. Keep their evidence. | Phase 0 review |
| O9 | Does uninstalling the split service delete indexes? | Keep today's behavior: indexes stay unless the user asks for a purge. | Phase 7 |
| O10 | Which signed executable runs the setup step (U16)? | A private, non-interactive role of the signed `airk` image. The alternative is a third signed `airk-setup` executable. | Phase 7 |
| O11 | Rename the `airk-cli` crate to `airk-service`? | Optional, scripted, last. | Phase 8 |
| O12 | One universal VSIX or the existing platform catalog? | Universal, once Marketplace targeting and promotion are audited. | Phase 6 |

### 2.3 Consequences that follow from the decisions

- **Start must exist outside the admin pages.** A stopped service cannot serve
  them. The extension and `airkctl service start` keep Start; running `airk`
  also starts it. Stop and restart live in the admin pages and `airkctl`.
- **Removing entries without a service leaves agents without AIRK.** If a
  pre-split service is still running, it keeps running with no clients until
  the service installer removes it. Explorer must say both things (U12).
- **The extension keeps removal-only configuration code.** When `airkctl` is
  absent, nothing else can remove the old entries. All creation and rewriting
  goes through `airkctl` (U11).
- **Features that end with the split, by user decision:** staged runtime
  slots, rollback and pinning, organization-managed payloads, side-by-side
  channels, extension-driven install and repair, shell integration written by
  the extension, and management screens inside the extension.
- **Receipts-only removal can leave things behind,** including an old login
  task that restarts an old service at sign-in. The installer reports this
  prominently (§9.3).
- **The extension loses its bundled digest as a trust anchor.** It verifies
  `airkctl` by signature on Windows and macOS. On Linux it can only check the
  owner-private install record, which is weaker and is documented as such.
- **Linux browsers will not trust the local certificate automatically.**
  Per-user Linux trust enrollment is unsupported today.
- **`airkctl` has four narrow uses that are not pipe requests** (§6.3):
  starting `airk`, reading the install record, the agent credential helper,
  and reading logs when the service is down.

## 3. Baseline (source review, 10 November 2026)

Three read-only investigations mapped the native code, the extension, and the
installers and release scripts. The primary agent spot-checked the claims
below. These are source observations, not test results.

**Native**

- The workspace builds one executable, `airk`
  (`crates/airk-cli/Cargo.toml`). `airk-launcher` and the role aliases are
  copies of it and are refused on current single-executable installs.
- The CLI is a clap tree of roughly 150 leaf commands in `main.rs`,
  `tools.rs` and `runtime_install.rs`. About 63 of them are `runtime`
  installation commands.
- Local IPC already exists: an owner-only named pipe on Windows and a
  peer-checked Unix socket elsewhere, with four-byte length-prefixed JSON
  frames (`local_ipc.rs`, `service_ipc.rs`). The control protocol has about 84
  methods (`airk-core/src/service.rs`).
- Read commands are already thin clients over the pipe. The CLI still does
  local work: it opens the service registry, plans registrations with the
  kernel, writes request journals, reads log and credential files, serves the
  tools catalog and help, and runs all installation code in-process.
- A second service start is refused with `service_already_running`
  (`service_runtime.rs`). `airk` with no arguments prints a clap usage error.
- Clients require the service version to equal their own
  (`project_client.rs`).
- The HTTPS listener binds `127.0.0.1`, preferring port 57421. It serves
  `/mcp`, `GET /api/v1/tools` and `POST /api/v1/tools/{name}` and nothing
  else. It refuses any request with an `Origin` header, requires a bearer on
  every request, and serves no HTML (`http_gateway.rs`,
  `airk-mcp/src/remote/http.rs`). It starts only when a catalog gate exists
  (`service_runtime/http.rs`).

**Extension**

- The VSIX is per-platform and carries the native runtime under `native/`.
  `serviceBootstrap.ts` verifies it and drives stage, activate, login startup,
  replacement and legacy cleanup. The extension is today's main installer.
- It reaches native code three ways: a per-window `airk service session`
  child (`nativeSession.ts`), a control relay or `service console` child
  (`serviceClient.ts`), and a persistent HTTPS MCP session for every Explorer
  and Workbench evidence read (`explorerEvidence.ts`, `mcpHttpSession.ts`).
- Explorer Assistance has the native side connect to a pipe hosted by the
  window (`explorerAssistanceHost.ts`).
- Agent configuration is written by about 2,100 lines of TypeScript. Entries
  are HTTPS-only and name an executable for the credential helper.
- The extension requires its version to equal the runtime's.
- The web front end is Preact, bundled by esbuild. `acquireVsCodeApi` appears
  in one file per webview. The real coupling is about 8,500 lines of host
  handlers that use `vscode.*` and the service client.
- `crates/airk-mcp/src/unified_explorer.json` and `explorer_templates.json`
  are generated from the extension's TypeScript, and the extension build
  reads `crates/airk-mcp/src/help/topics.rs`.

**Installers and release**

- All three installers are thin wrappers that run the packaged
  `airk setup --runtime-source … --approved-sha256 …`. Native code does the
  work.
- Predecessor removal covers one install root and refuses without ownership
  receipts. It does not sweep the preview channel, custom installs, 1.3
  schedulers, VS Code global-storage helper caches, the extension's CLI copy
  and PATH edits, or the Inno `standalone` folder.
- The one-executable rule is hard-coded in Rust (`runtime_pair.rs`,
  `install_files`), Python (`build-service-runtime.py`, `package-native.py`,
  `sign-native.py`, `package-standalone-linux.py`) and TypeScript.
- There is no signature check at run time. Trust is the SHA-256 approval of
  `airk-install.json`.
- The download site does not present service and extension as two steps, and
  `verify-published-release.py` does not check installers.

## 4. Target architecture and boundary

### 4.1 Components and ownership

| Component | Owns | Never |
|---|---|---|
| `airk` | Indexing, retrieval, storage, grants, jobs, watching, inference, the HTTPS listener, the admin API and embedded admin assets, the control and query endpoints, the request journal, the setup role | Parses user commands. Writes into project folders. Connects to an editor. |
| `airkctl` | The whole user CLI, text and JSON rendering, the per-window session, agent configuration files, service start | Opens indexes, registries or stores. Links the kernel, store, retrieval, ingest or MCP crates. Installs anything. |
| Admin pages | Management screens and browser search; view state only | Hold credentials, durable requests or authority in the browser. Load anything from another origin. |
| Extension | Explorer and Workbench presentation, window search scope, workspace files, editor integration, status and guidance | Ships or runs `airk`. Opens a pipe, socket or HTTPS connection to the service. Reads service credentials or state. Installs, repairs or removes the service. |
| Service installer | File placement, the signed setup step, old-install removal, PATH, login startup, uninstall | Installs a VSIX. Edits agent configuration. Touches source files. |

### 4.2 The four interfaces

Everything that crosses a component boundary uses one of these. Each is a
contract with one owner, reviewed and landed before its consumers.

| # | Interface | Between | Owner | Status |
|---|---|---|---|---|
| I1 | Local IPC: owner-checked pipe or socket, control and query protocols | `airkctl` ↔ `airk` | `airk-core` types, `airk-ipc` transport | Exists; gains methods for work the CLI does locally today |
| I2 | `airkctl` command contract: arguments, JSON output, exit codes, and the `session` stream | Extension and scripts ↔ `airkctl` | `airkctl` crate, with Rust-emitted fixtures | New |
| I3 | HTTPS: `/mcp` and `/api/v1` for agents and local programs; `/admin` for the browser | Agents, browser ↔ `airk` | `airk-mcp` (MCP), `airk-cli` (gateway, admin) | `/mcp` and `/api/v1` unchanged; `/admin` new |
| I4 | Install record: where the service is, what it contains, how to verify it | Installer → `airkctl`, extension | Installer contract | New |

I2 is the only interface the extension consumes. Its version handshake
replaces exact version equality between extension and service:
`airkctl version --output json` reports the product version, the I2 contract
version and a capability list. The extension decides compatibility from the
contract version and capabilities, never from version ordering alone.

`airk` and `airkctl` come from one installer and keep exact version equality
on I1.

### 4.3 Process model after the split

Exactly one `airk` stays resident per user: the service. This replaces the
list in [process ownership](../process-model.md).

| Process | Lifetime |
|---|---|
| `airk` service | Resident. Started by login startup, by `airkctl`, or by hand. |
| PDF parser child of `airk` | At most one, per document. Unchanged. |
| `airk` setup role | Installer and uninstaller one-shot. |
| `airkctl` one-shot | Exits when the command finishes. Includes agent credential helpers. |
| `airkctl session` | One per VS Code window. Exits with the window's extension host. Replaces `airk service session`. |
| `airkctl tools shell` | Opened and closed by the user. |

No other `airk` process may appear. `scripts/check-self-spawn.py` and
`scripts/check-process-topology.py` gain reviewed entries for `airkctl` and
for `airkctl` starting `airk`.

### 4.4 Extension code and service code

**Rule.** Extension code and service code share no runtime channel except I2
and no source except generated contract types.

| | Service side | Extension side |
|---|---|---|
| Source | `crates/`, `packages/admin/`, `installers/` | `packages/vscode/` |
| Shared | `packages/ui/` (presentation only, no transport) and generated I2 types | same |
| Ships in | Service installer | VSIX |
| Starts processes | `airk` (PDF child, setup role); `airkctl` starts `airk` | `airkctl` only |
| Network | Loopback listener in `airk` | None to the service |
| Files it may read outside its own storage | Source roots (read-only), its own state | The install record (I4) and workspace files |

**Enforcement, each a cheap check:**

- **VSIX contents.** `scripts/package-extension.py` and its verifier reject
  `native/`, executables, models and installers.
- **Process starts.** `packages/vscode/src/test/explorerBoundaries.test.ts`
  already lists every allowed process start. Its expected set becomes
  `airkctl` plus editor-owned tools. It is a reviewed, checked-in value.
- **No transport in the extension.** A boundary test fails on imports of
  `net`, `tls`, `http`, `https` or `http2` in extension host code outside
  tests, and on any reference to pipe names, the loopback endpoint, CA files
  or credential files.
- **One spawning module.** Only `packages/vscode/src/airkctl/` may start a
  process. Features receive a typed facade.
- **Crate edges.** `scripts/dev/boundary_check.py` and
  `boundary-exceptions.json` gain the reviewed edges in §10.1. `airkctl` may
  depend only on `airk-core`, `airk-process` and `airk-ipc`.
- **No native reads of extension source.** Native builds stop reading
  `packages/vscode`. The generated catalogs change owner (§10.4).

## 5. `airk`: service only

### 5.1 Invocation

| How `airk` is started | Result |
|---|---|
| No arguments, service running | Prints that the AIRK service is already running, with its version, and that `airkctl service status` shows details and `airkctl service stop` stops it. Exit 0. Starts nothing. |
| No arguments, service not running | Starts the service detached, waits for health under a deadline, then prints that the service has started, that it keeps running after the terminal closes, and that `airkctl service stop` stops it. Exit 0. |
| No arguments, start fails | Prints the typed failure and the log location. Non-zero exit. |
| Any user-style arguments | Prints that `airk` takes no commands and that `airkctl` is the CLI, plus the current running state. Non-zero exit. Starts nothing. |
| A private role selector | Runs that role: the service itself (from login startup or the detached start), the PDF parser child, or the setup role. |

A manual start is an explicit Start: it clears a saved explicit Stop, as
`runtime start` does today. Private role selectors are undocumented, are not a
user interface, and refuse to run without their parent's handshake or the
installer's owner-private operation file.

### 5.2 What moves into the service

`airkctl` cannot open state, so the service takes over the CLI's local work.
Each item is a control-plane addition with one handler (O2).

| Today in the CLI process | After the split |
|---|---|
| Resolve a project or selection from a root or binding (`service_frontend::saved`, kernel registration planning) | Control method; the service resolves |
| Prepare storage and reserve registrations (`service_registration::prepare_storage`) | Control method |
| Write the durable request journal before index, configure and usage mutations (`client_requests::prepare`) | The service owns the journal; the client supplies an idempotency key and can read receipts |
| Model catalog | Control method |
| Tools catalog and help, Explorer draft and template commands | Served by the service from `airk-mcp` |
| HTTPS trust, issuer rotation, login startup policy | Control methods run by the service; the setup role covers install and uninstall time |
| Extension-held request journal and intents (`privateIntentStore.ts`, `serviceIntents.ts`) | The same service journal, shared by `airkctl`, the session and the admin pages |
| Explorer Assistance delivery to a window-hosted pipe | A bounded per-window queue in the service, drained by that window's `airkctl session` |

### 5.3 What leaves `airk`

Removed only by the deletion rules: not compiled, no caller after a clean
compile and Clippy, or a reviewed compatibility-inventory row naming the
surviving owner.

- The user command tree and its rendering move to `airkctl`.
- `service session`, `service console` and `service rpc` are replaced by
  `airkctl session` and one-shots.
- Stdio MCP (`mcp`, `bridge`, `unified_mcp.rs`), the launchers
  (`universal_launcher.rs`, `runtime launch`) and role aliases are already
  refused on current installs.
- The staged installation engine (stage, activate, selection, rollback,
  managed payloads) is not used by a fixed layout (U15). The parts that read
  and remove pre-split installations survive as the retirement module (§9.3).

## 6. `airkctl`

### 6.1 Shape

- A new binary crate, `crates/airkctl`, depending on `airk-core`,
  `airk-process` and the new `airk-ipc`.
- "Lightweight" is checked by the crate-edge gate, not by a size limit.
  Binary size and cold-start time are measured and reported.
- Windows uses the existing owner-only named pipe. macOS and Linux use the
  existing peer-checked Unix socket. "Named pipes" in the request means this
  transport on each platform.
- Signed in CI with `airk`, and listed in every payload inventory.

### 6.2 Command surface

`airkctl` exposes the whole CLI. Phase 0 produces the reviewed mapping of
every current leaf command to one disposition. Surviving commands keep their
current names, arguments and output so documentation, scripts and golden
outputs carry over.

| Family | Disposition |
|---|---|
| Evidence reads (`search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, `status`, `parse`) and `tools list/help/call/shell` | Kept as they are |
| Index, job and project commands (`index`, `inspect`, `pause`, `resume`, `watch`, `configure`, `settings`, `setup-model`, `projects`, `model-catalog`) and `usage` | Kept; local work moves into the service |
| `explorer` draft and template commands | Kept; served by the service |
| `service health/logs/shutdown/register/requests …` | Kept under `airkctl service …`, with `start`, `stop`, `restart`, `status` |
| `runtime http-connection`, `http-credential`, `http-revoke`, `lookup-http-connection`, `setup --client` | Become `airkctl agents …` (§6.4) |
| `runtime http-trust`, `http-issuer-rotation`, `login …`, `inventory`, `purge`, `lifecycle` | Become `airkctl service …` subcommands backed by control methods |
| `runtime stage/activate/ensure/launch/managed/retire-*/replace-installation`, `setup`, `uninstall`, `service legacy …` | Retired with the staged engine, or owned by the setup role. Each gets a compatibility-inventory row. |
| `service session`, `service console`, `service rpc` | Replaced by `airkctl session` and one-shots |
| New: `airkctl admin open`, `airkctl admin url`, `airkctl version` | Sign-in link for the admin pages; the I2 handshake |

Every command accepts `--output json`. JSON is the complete native result,
never a reduced view.

### 6.3 Uses that are not pipe requests

`airkctl` talks to the service only over the pipe, with four documented
exceptions. Each is narrow and has a reason.

| Exception | Reason |
|---|---|
| Read the install record | To find and verify `airk` |
| Start `airk` detached | `airkctl service start`, and automatic start for commands that need the service. A saved explicit Stop is respected; only an explicit start clears it. |
| Agent credential helper reads the owner-private token file | Today's rule: the helper never starts the service and never enrolls |
| Read owner-private log files when the service is down | Diagnosing a service that will not start |

### 6.4 Agent configuration

One native implementation replaces the TypeScript writers.

- `airkctl agents list` classifies AIRK-owned entries in project and user
  files with today's classes (`stdio_retired`, `stdio_missing_command`,
  `https_unknown_connection`, `https_endpoint_mismatch`,
  `https_helper_missing`, `https_trust_missing`, `ok`).
- `airkctl agents connect` enrolls a reviewed selection and writes the entry.
  `repair` rewrites stale entries. `remove` deletes AIRK-owned entries.
- Credential helpers in written entries name the installed `airkctl`.
- Only recognized AIRK-owned entries are touched. Other servers, comments,
  disabled states and policies are preserved. User-level entries count only
  when AIRK provably wrote them. The installer's retirement receipt (§9.3)
  supplies that proof for pre-split entries.
- `airkctl` writes the files as the invoking user. The service never does.
- The extension keeps a removal-only recognizer for the no-service case. It
  is tested against shared cases emitted by the Rust implementation, so the
  two cannot drift.

### 6.5 Session mode

`airkctl session` is the per-window helper. It keeps today's session framing
and limits (length-prefixed JSON on stdin and stdout, at most eight requests
in flight, 16 MiB responses, cancellation).

- It carries every request the extension makes: evidence reads, Workbench
  plans, status, scope selection and agent commands.
- It keeps its pipe connections open, so a read does not pay process start.
- It adds a bounded event subscription for status and job changes, and
  delivery of Explorer Assistance drafts addressed to its window.
- It never returns bearer tokens. The VS Code MCP provider and the
  `airk.httpCredential` input resolve the token with a separate one-shot.
- On owner loss it cancels read-only work and exits. It no longer runs
  installer work, so the 600-second exception in the process model ends.

### 6.6 Parity

`airkctl` is the CLI in the parity rule. Equivalent reads through `airkctl`,
MCP, Workbench (through the session) and the admin pages use the canonical
handlers and return the same complete results and inputs. During Phase 3 the
old `airk` CLI still exists, so `airkctl` output is compared against goldens
captured from it: exact bytes for digested and persisted formats, parsed
equality elsewhere.

## 7. Admin endpoint and pages

### 7.1 Endpoint

- Same listener, certificate and origin as MCP:
  `https://127.0.0.1:<port>/admin/`. Static assets are embedded in `airk`.
  The API is under `/admin/api/v1/`.
- The listener must start with the service even when no index exists. Phase 0
  establishes whether a fresh empty catalog already provides the gate that
  starts it. `/mcp` and `/api/v1` keep their existing admission.
- `airkctl admin open` is the supported entry point. It prints or opens the
  current URL. The port can change after a restart, so bookmarks are not
  reliable.

### 7.2 Authorization (O3)

A browser is a new kind of client. Any web page can send requests to a
loopback port, so the admin plane must be strict.

| Rule | Detail |
|---|---|
| Sign-in | `airkctl admin open` asks the service, over the owner-checked pipe, for a single-use code with a short lifetime. The browser exchanges it for a session cookie and is redirected. The code is consumed on first use and never logged. The extension's **Open AIRK Admin** runs the same command. |
| Session | `__Host-` cookie, `Secure`, `HttpOnly`, `SameSite=Strict`. Idle and absolute expiry. Bound to the service instance, held in memory and bounded in number. A restart ends every session. |
| Planes stay separate | Bearer credentials are refused on `/admin`. Cookies are refused on `/mcp` and `/api/v1`, which keep refusing every `Origin`. |
| Origin and host | `Host` must match exactly, as today. On `/admin`, an `Origin` or `Sec-Fetch-Site` that is not the service's own origin is refused. No CORS headers are ever sent. |
| Mutations | `POST` only, with a session-bound anti-forgery token. Destructive actions use the existing native draft, preview and apply-with-digest flow plus a typed confirmation. |
| Content | Strict Content Security Policy with no inline script and no external origin, `frame-ancestors 'none'`, `nosniff`, `no-referrer`, and `no-store` on API responses. The pages work offline. |
| Never over HTTP | Anything that returns a bearer or private key, raw connection grants, upgrade checkpoints, uninstall. |

**One handler per operation.** The admin API dispatches to the same control
handlers the pipe uses. An operation table gives each operation its handler,
the surfaces allowed to call it, its authorization rule and its conformance
applicability. No second implementation exists for the browser.

**Certificate trust.** The browser must trust the installation-local issuer.
The setup step offers the existing reviewed per-user trust action on Windows
(CurrentUser Root) and macOS (login keychain), with consent. The service
never falls back to plain HTTP. Linux has no automatic per-user trust;
browsers there warn until the user provisions trust. Each browser is
qualified separately.

**Documents that change with O3:** the statements in
[architecture](../architecture.md) and the
[local HTTPS API](../local-https-api.md) that HTTP exposes no lifecycle or
index-management commands and that browser origins are refused.

### 7.3 Pages in the first release

| Area | Content | Comes from |
|---|---|---|
| Overview | Service state, version, indexes, activity, leftovers from old installs | Explorer Management → Overview |
| Index library and index sets | Reviewed index setup, sources, split, policies, retire | Explorer Indexing |
| Activity | Jobs, plans, pause, resume, cancel, retry | Management → Activity |
| Storage | Locations, limits, relocation, cleanup | Management → Storage |
| Recovery | Repair plans and their receipts | Management → Recovery |
| Service | Restart, stop, resources, login startup, HTTPS issuer trust and rotation | Management → Service |
| Agents | Connections, grants, revocation, health of known entries, and the exact `airkctl agents …` command to apply | Management → Agents |
| Usage | Reports and policy | Management → Usage |
| Logs | Bounded logs and the redacted diagnostic report | Management → Logs |
| Search and evidence | Query, code, documents and the evidence inspector | Explorer Search, Code, Documents, Evidence |
| Help | Help topics | Explorer Help |

**Search in the browser (U7).** The admin session picks its own selection
from every index and index set in the catalog. That selection belongs to the
browser session. It does not read or change a VS Code window's scope or any
agent connection's grant. Reads go through the canonical handlers with an
explicit selection, a grant issued for the admin session, revision identity
and budgets, and return complete results.

**Not in the browser:** opening a file in the editor, workspace trust, plan
files saved into a workspace, git-based review, the Suggestions Inbox. These
are editor features.

**After Stop** the page has no server. It shows how to start the service
again: run `airk`, `airkctl service start`, or use Start in the extension.

### 7.4 Front-end code

The management screens move out of the extension. Search and evidence views
are needed in both places, so they move to a shared package (§10.4). The
8,500 lines of extension-host management logic do not move as TypeScript:
their durable-request, confirmation, paging and validation behavior is
re-homed in the service behind the admin API. This is the largest single
piece of new work in the plan.

## 8. VS Code extension

### 8.1 What it does

| Keeps | Changes | Loses |
|---|---|---|
| Explorer search, code, documents, evidence, review, strategies and planner | All reads go through the `airkctl` session | Bundled runtime and every install, repair, update and uninstall path |
| Workbench, saved sessions, exports | Window search scope is chosen among existing indexes through the session | Index library, storage, service, recovery, usage and log screens (to the admin pages) |
| Suggestions Inbox and Explorer Assistance | Drafts arrive over the session | The window-hosted Assistance pipe and Node MCP server |
| Agent connection for the workspace | Calls `airkctl agents …`; keeps removal-only code | TypeScript writers, HTTPS enrollment, trust and rotation code |
| VS Code MCP provider | Supplies the definition; resolves the token with an `airkctl` one-shot. VS Code's own MCP client connects to `/mcp`; the extension does not. | Direct reads of CA and token files |
| Status and guidance | **Open AIRK Admin**, **Start**, **Download AIRK Service**, **Check again** | Stop, restart, upgrade, rollback |

Choosing a window's search scope stays in the extension because it selects
among existing indexes for that window. Creating or changing indexes, index
sets and grants is admin-only (U6).

### 8.2 Finding and trusting `airkctl`

The extension must not execute something because it is on `PATH`, in the
workspace, or claims a version.

1. Read the install record (I4) from its fixed per-user path. There is no
   workspace or user setting for the location. Test hosts inject a fixture
   through the existing test seam.
2. Require that the record and its folder are owned by the current user, are
   not writable by others, and contain no links or redirects.
3. Verify `airkctl` before the first execution. Windows: a valid Authenticode
   signature that meets the publisher policy in `scripts/sign-native.py`.
   macOS: `codesign --verify --strict` against the team requirement. Linux:
   the SHA-256 in the owner-private record, documented as weaker.
4. Cache the result by file identity and re-verify when it changes.
5. Run `airkctl version --output json` and apply the I2 compatibility check.

The expected publisher and team identity come from one checked-in signing
identity file that the signing scripts, installers and extension build all
read. They are not hand-copied. An unsigned development `airkctl` is accepted
only when the extension runs in development or test mode.

### 8.3 States

| Observed state | Explorer and Workbench behavior |
|---|---|
| Verified, compatible, running | Attach through the session. Show native readiness. |
| Service not installed | "AIRK Service is not installed", with **Download AIRK Service**, **Installation instructions** and **Check again**. The site opens only on user action. Old AIRK MCP entries are removed and listed (§8.4). |
| Only a pre-split installation exists | Same as not installed. Explorer adds that an older AIRK is still present, that this extension does not use or change it, and that the service installer will remove it. |
| Installed, service stopped | Show the stopped state and offer **Start**. An explicit Stop is shown as such; Start is the explicit action that clears it. |
| Incompatible contract | Show both versions and the reason. Point to the newer installer or extension. Never upgrade or downgrade silently. |
| Unverifiable or damaged `airkctl` | Refuse to execute. Explain the verification failure and point to the installer. |
| Reachable but refused | Keep the exact native error and retry details. Distinguish startup, credentials and absence. |
| Workspace untrusted or native execution disabled | Keep help and local UI. Do not run `airkctl` until the existing gate allows it. |

The multiple-installations state from revision 1 no longer exists (U17).

Recheck on request and on bounded recovery. If `airkctl` is replaced or
removed while a window is open, end the session, re-verify and reattach only
to the verified installation. Reopening the editor only reconnects; it never
creates an index or replays work.

The extension runs on the workspace's execution host. A remote workspace
needs the service installed on that host. A local service never stands in for
a remote workspace, and admin pages on a remote host are bound to that host's
loopback address. Unsupported remote arrangements are documented, not
claimed.

### 8.4 MCP entries (U12)

- **Service found.** The extension asks `airkctl agents list` for stale
  entries. Entries that match an existing index are rewritten by
  `airkctl agents repair` after one confirmation, because that creates a
  grant. Entries with no matching index are removed and recorded so Explorer
  can offer them again once an index exists (O6).
- **Service not found.** The extension removes recognized pre-split AIRK
  entries with its removal-only code and records them. Explorer lists what
  was removed, from which files, and why.
- Explorer never claims that a running agent reconnected.

### 8.5 Installation surfaces to retire

Every entry gets an explicit disposition in the Phase 0 inventory. A retained
command identifier may guide the user but must not install.

- Settings: `airk.cli.autoInstall`, `airk.cli.shellIntegration`,
  `airk.service.channel`, `airk.service.installDir`, the four
  `airk.service.managed*` settings, `airk.storage.path`.
- Commands: `installCli`, `setUp`, `removeTerminalCommand`, `reviewData`, the
  HTTPS trust and rotation commands, `serviceStop`, `serviceRestart`, and the
  indexing and storage commands that become admin links.
- `vscode:uninstall`: removes only editor-owned storage. It no longer runs a
  native uninstall.
- `extensionUpdateLoader.ts` hot-swaps host modules and seals
  `native/manifest.json`; its inventory drops the native entry.

## 9. Service installer

### 9.1 What the installer does

1. **Tell the user (U13).** Before any change, state that every earlier AIRK
   installation will be stopped and removed, that existing indexes are
   deleted and must be rebuilt, that no source file is read, changed or
   deleted, and that coding agents must be reconnected. List the locations
   found. An unattended install needs an explicit flag that acknowledges
   this.
2. **Place files** in the fixed layout, using the platform installer's own
   mechanism.
3. **Run the signed setup step** (U16): remove old installations, register
   login startup, offer certificate trust, start the service.
4. **Report.** Show what was removed, what was left and why, and how to open
   the admin pages.

The installer works with VS Code absent or closed. It installs no VSIX and
edits no agent configuration.

### 9.2 Later installs

| Found on the machine | Result (O5) |
|---|---|
| No AIRK | Fresh install, empty catalog |
| Any pre-split installation | Removed with its indexes, then fresh install |
| The same split build | Repair in place; indexes kept |
| An older split build, compatible state contract | Stop through `airkctl`, replace files, start; indexes kept |
| A split build with an incompatible state contract, or a newer build | Say so before proceeding; continue only as an explicit reset |

### 9.3 Removing old installations (U13, U14)

**Roster.** The setup step looks for every pre-split artifact, not only one
install root.

| Artifact | How it is found |
|---|---|
| Stable and preview install roots, with their state, data, cache, log and settings folders | Fixed per-user paths in `user_paths.rs` |
| Custom install roots | The universal-launcher pointer and the executable paths named by AIRK login registrations |
| Login registrations | Windows `airksvc-*` tasks and their saved XML, Linux systemd user units, macOS `io.iokaio.airk.service.*` agents, and 1.3 `airk-<hash>` schedulers |
| Universal launcher folder | Fixed per-user path |
| Inno `standalone` folder, its PATH entry and its uninstall registration | Fixed path and the `user-path-owner.txt` receipt |
| VS Code global-storage helper caches and CLI copies, for Code, Insiders and VSCodium | Paths in `runtime_legacy_origin.rs` |
| PATH and shell edits written by the old extension | Their ownership receipts |
| Custom index data folders | The old service registry |
| Per-user certificate trust added by AIRK | The trust ownership receipt |

**Rules.**

- **Receipts only.** An item is removed only when an AIRK ownership receipt
  proves it. This applies inside AIRK's own folders too.
- **Processes** are stopped by exact image path under a discovered root, on a
  verified handle. A name or a PID alone never authorizes termination.
- **No old binary is executed.** Old uninstallers are not run.
- **Links and redirects are never followed.** Sources are never touched.
- **Installation continues.** Anything unproven or locked is left, recorded
  with its reason, and shown at the end. The same list appears in
  `airkctl service status` and on the admin Overview page.
- **A leftover that can restart an old service,** such as an unproven login
  task, is reported first and in plain words.
- **The new service cannot adopt leftovers.** Its state, data and cache
  folders use names no pre-split build used (O4).
- **Retirement receipt.** The setup step records the removed executable
  paths, endpoint, issuer fingerprint and connection identifiers. `airkctl
  agents` uses this to prove that a stale entry was AIRK's.
- **Interrupted removal resumes** from its recorded operation and never
  resets the new installation.

Predecessor discovery, shutdown proof and resumable cleanup remain live
duties. The code that performs them is kept and moved, not deleted (§10.2).

### 9.4 Signing and delivery

| Platform | Delivery | Signed in CI |
|---|---|---|
| Windows x64 | Inno Setup EXE, per-user, no elevation | `airk.exe`, `airkctl.exe`, the uninstaller and the installer, with the existing Authenticode path |
| macOS arm64 and x64 | One DMG per architecture | `airk` and `airkctl` as nested code with hardened runtime, the `AIRK.app` bundle, and the DMG; notarized and stapled |
| Linux x64 | tar.gz with `install.sh` | Unsigned and labeled so; complete inventory digests, release checksums and GitHub attestations |

- The setup step runs from a CI-signed executable (U16). The wrapper checks
  the signature or pinned digest before the first native execution.
- Whether the macOS `Install AIRK.command` wrapper passes Gatekeeper from the
  notarized DMG is a qualification question. If it does not, the fallback is
  a signed installer package, which needs a different certificate.
- GitHub attestations and checksums are provenance and integrity evidence,
  not OS code signing. Marketplace signing is separate.
- Release assets keep the `airk-{version}-{target}-installer.*` names and
  manifests. `verify-published-release.py` gains installer checks.
- **airk.ioka.io** presents **Install AIRK Service** and **Install VS Code
  Extension** as two steps. It selects real published assets by platform,
  architecture, version and channel, with signature, checksum and
  compatibility information. Missing assets stay unavailable. A signed
  installer is advertised only after its published bytes pass verification.

### 9.5 Installer map

Paths are proposals for the Phase 0 review.

| Item | Windows | macOS | Linux |
|---|---|---|---|
| Program folder | `%LOCALAPPDATA%\Programs\AIRK\` | One fixed `AIRK.app` under `~/Library/Application Support/AIRK/`, outside every pre-split root | `~/.local/lib/airk-service/` |
| `airkctl` | `bin\airkctl.exe` | `AIRK.app/Contents/MacOS/airkctl` | `bin/airkctl` |
| `airk` | `service\airk.exe` | `AIRK.app/Contents/MacOS/airk` | `service/airk` |
| Notices and licenses | `notices\` | Inside the bundle | `notices/` |
| Install record (I4) | Program folder | Beside the bundle | Program folder |
| On `PATH` | `bin\` added to the user PATH under an ownership receipt. `airk.exe` is not on `PATH`. | A receipt-owned `airkctl` link in `~/.local/bin` when that folder exists; otherwise the path is printed | Same as macOS |
| State, data, cache, logs | New folder names under the existing per-user AIRK roots | Same | Same |
| Login startup | One Task Scheduler task | The bundle's SMAppService agent | One systemd user unit |
| Certificate trust | CurrentUser Root, with consent | Login keychain, with consent | Not automatic |
| Shortcut | Start menu: AIRK Admin | None | None |
| Uninstall | Inno uninstaller runs the setup role, then removes files and the PATH entry | `airkctl service uninstall` starts the setup role | Same as macOS |

No shell startup file is edited on any platform.

## 10. Code reorganization map

Every row is a candidate for the Phase 0 inventory to confirm. Moves are
mechanical steps, separate from behavior changes, generated by a committed
script, and reproduce the captured goldens.

### 10.1 Workspace

| Crate or package | After the split | AIRK dependencies |
|---|---|---|
| `airk-core` | Unchanged owner. Gains only reviewed contract additions. | None |
| `airk-process` | Unchanged | None |
| `airk-ipc` (new) | Local transport (pipe and socket, owner checks, framing) and typed clients for the control, query and session protocols | core |
| `airkctl` (new, binary) | Command tree, rendering, session mode, agent configuration, service start | core, process, ipc |
| `airk-ingest`, `airk-store`, `airk-retrieval`, `airk-kernel` | Unchanged | Unchanged |
| `airk-mcp` | Unchanged owner. Owns the tool and template catalogs it serves. | core, kernel |
| `airk-cli` | The service: role dispatcher, service runtime, HTTPS and admin planes, setup and retirement | Adds ipc |
| `packages/ui` (new) | Shared search and evidence views, styles and view contracts. No transport. | None |
| `packages/admin` (new) | Browser shell, admin screens, HTTPS adapter. Its build output is embedded in `airk`. | ui |
| `packages/vscode` | Extension host, webview shell, `airkctl` adapter | ui |

**Extraction criteria for O1** (simplification plan §4.1):

1. *No cycle.* IPC is the one cluster in `airk-cli` that is already outside
   the module cycles. The client code around it is not: it references the
   store's service lease, the kernel's broker and MCP types. It is untangled
   in place first (Phase 2).
2. *Stable interface.* `airk-ipc` gets a `CONTRACT.md` that Phase 2 uses
   unchanged before the crate is extracted.
3. *Independent consumers.* Two binaries consume it. This is a binary
   boundary.
4. *Testing benefit.* Its T0 command runs without the process stack. Phase 2
   measures this.

### 10.2 `crates/airk-cli/src`

| Cluster today | Disposition |
|---|---|
| `main.rs` command enums, `tools.rs`, `cli_io.rs`, `help_guide.txt` | Command tree and rendering move to `airkctl`. `main.rs` keeps the role dispatcher and the manual-start messages. |
| `local_ipc*.rs`, `service_ipc.rs` | Move to `airk-ipc` |
| `project_client*`, `console_client.rs`, `usage_client.rs`, `client_owner.rs`, `session_protocol*` | Client halves move to `airk-ipc` after untangling |
| `service_frontend.rs`, `combined_frontend*`, `client_requests.rs`, `service_registration.rs` | Registry- and kernel-dependent parts stay in the service behind new control methods; the thin client parts move |
| `session.rs`, `session_commands*` | Replaced by `airkctl session`; allowlist logic moves with it |
| `service_runtime*`, `service_management*`, `service_manager*`, `service_configuration.rs`, `service_logs*`, `service_resource_policy.rs`, `catalog_*`, `project_supervisor*`, `project_worker*`, `source_*`, `worker_*`, `embedding_*`, `inference_*`, `service_inference*`, `checkpoint_fence/`, `native_command/` | Stay |
| `http_endpoint*`, `http_gateway*`, `http_backend.rs`, `native_admin_connections.rs` | Stay. A new admin module beside them holds sessions, the anti-forgery check, static assets and admin dispatch. |
| `explorer_assistance.rs` | Replaced by the service-side Assistance queue |
| `unified_mcp.rs`, `universal_launcher.rs`, `runtime_pair*`, `process_roles.rs` aliases, `legacy_startup*` | Retire with evidence; already refused on current installs |
| `runtime_install*`, `standalone_setup*`, `install_files/`, `install_layout/`, `runtime_legacy_origin*`, `legacy_processes.rs`, `runtime_macos_login.rs` | Split three ways. **Setup:** a small fixed-layout role for login startup, trust, start and uninstall. **Retirement:** the readers and removers for pre-split installations (`replacement.rs`, `upgrade_stop`, legacy-origin and old-layout readers), kept and widened to the §9.3 roster. **Staged engine:** stage, activate, selection, rollback and managed payloads, removed with evidence once no caller remains. |

Each new owned module gets a `CONTRACT.md`: `airk-ipc`, `airkctl`, the admin
plane, setup and retirement.

### 10.3 `packages/vscode/src`

| Group today | Disposition |
|---|---|
| Install and update: `serviceBootstrap.ts`, `native.ts`, `cliInstall.ts`, `cliRemove.ts`, `runtimeRebuild.ts`, native parts of `extensionUninstall.ts` and `uninstallHook.ts` | Removed with evidence. `nativeGate.ts` stays and gates `airkctl`. |
| Transports: `nativeSession.ts`, `serviceClient.ts`, `serviceHttp.ts`, `mcpHttpSession.ts`, `mcpStdioSession.ts`, `privateDirectory.ts` | Replaced by `src/airkctl/`: discovery, verification, one-shot runner, session client, compatibility. `readRetry.ts` and `nativeOutcome.ts` stay as semantic rules tested against shared Rust cases. |
| Agent configuration: `agentConfig*.ts`, `agentMigration.ts`, `agentSetup.ts`, `agentToml.ts`, `unifiedAgentConfig.ts`, `globalAgentInventory.ts`, `agentConnections.ts`, `agentReconnect.ts`, `httpIssuerRotation.ts` | Writers, enrollment and rotation removed in favor of `airkctl agents`. A removal-only recognizer and the reconnect record stay. |
| Explorer evidence host: search, facts, inspect, review, strategies, suggestions, handoffs, plan files, workspace I/O, scope selection | Stay; reads use the session facade |
| Explorer management host: `explorerManagement.ts`, `indexingCatalog*.ts`, `indexingSplit.ts`, `serviceConsole*.ts`, `serviceDashboard.ts`, `usagePanel.ts`, `usageReport.ts`, `diagnostic*.ts`, `serviceIntents.ts`, `privateIntentStore.ts`, `intentAcceptedEvidence.ts`, `vectorModel.ts` | Leave the extension. Behavior is re-homed in the service and `packages/admin`. |
| Workbench host | Stays; reads use the session facade |
| `explorerAssistance/` Node MCP server, `explorerAssistanceHost.ts`, `explorerAssistanceLaunch.ts` | Replaced by session delivery |
| `explorerWebview/`, `workbenchWebview/` | Search and evidence views move to `packages/ui`. Management views move to `packages/admin`. The extension keeps a thin webview shell. |
| `serviceExtension.ts` (3,487 lines) | Split into a composition root plus discovery and status, agent connections, the MCP provider and views. Untangled in place before files move. |

Target layering, adapted from the simplification plan §4.3:

```
generated I2 types
  └─ airkctl/        the only module that starts a process
       ├─ agents/    airkctl agents calls, removal-only recognizer, MCP provider
       ├─ status/    discovery states, guidance, Open Admin, Start
       └─ features:  explorer · workbench · assistance
webview shell — mounts packages/ui views through a postMessage adapter
```

### 10.4 Front-end packages and generated catalogs

- **One view contract, two adapters.** The Explorer message contract
  (`explorerContracts.ts`, about 90 typed requests) becomes the transport
  interface of `packages/ui`. The extension adapter sends requests to the
  host, which calls `airkctl`. The browser adapter calls the admin API and
  reads pushed state from a same-origin event stream.
- **Theming.** The 74 `--vscode-*` uses become `--airk-*` tokens. The webview
  shell maps them to VS Code's variables. The browser shell supplies light
  and dark values.
- **Catalog ownership reverses.** Today the extension's TypeScript is the
  source of truth for `unified_explorer.json` and `explorer_templates.json`,
  and the template file embeds the extension version. After the split the
  service owns both, and they carry their own content version. Front ends
  read them from the service or consume a generated copy with a drift check.
  The extension build stops reading `topics.rs`.
- **Embedding.** `packages/admin` builds to a sealed asset set with a digest
  inventory. The service build embeds it. Phase 5 decides whether the bundle
  is checked in with a drift check, as the catalogs are today, or built at
  package time.

### 10.5 Scripts, CI and rosters

| Area | Change |
|---|---|
| Two executables in the payload | `build-service-runtime.py`, `package-native.py`, `sign-native.py`, `ci-windows-sign.py`, `ci-macos-sign.py`, `github-release.py`, `package-standalone-*.py`, `standalone-installer-contract.py`, `installers/*` |
| VSIX without native content | `package-extension.py` and its verifier, `promote-vsix.py` (no native subtree swap), `verify-extension-release.py` |
| Published-release checks | `verify-published-release.py` gains installers |
| Site | `build-downloads.py`, `site/index.html` |
| Process and boundary rosters | `check-self-spawn.py`, `check-process-topology.py`, `scripts/dev/boundary_check.py`, `boundary-exceptions.json`, `explorerBoundaries.test.ts` |
| Workflows | `ci.yml`, `github-release.yml`, `extension-release.yml`, `downloads.yml`, `macos-signing.yml` |

### 10.6 Documentation

Updated with the change that makes each statement true, not before:
[architecture](../architecture.md), [process ownership](../process-model.md),
[local HTTPS API](../local-https-api.md),
[installation](../service-installation.md),
[replacement policy](../upgrade-rebuild-fallback.md),
[agent setup](../agent-integration.md), [headless use](../headless-cli.md),
the CLI guide and generated reference, [extension](../vscode-extension.md),
[read parity](../read-interface-parity.md), [versioning](../versioning.md),
the README, the documentation index, release notes and the Marketplace
description. Dated records stay identifiable as history.

CLAUDE.md and AGENTS.md name the CLI, the window session and the parity
surfaces. Their wording changes only with the user's approval and stays
byte-identical.

## 11. Implementation sequence

Work proceeds one phase at a time. Contract changes land before their
consumers. Mechanical moves are separate from behavior changes. A behavior
step stays within about 25 files and 1,500 changed lines.

~~~mermaid
flowchart LR
    P0[0 Inventory and review] --> P1[1 Contracts]
    P1 --> P2[2 airk-ipc]
    P2 --> P3[3 airkctl]
    P3 --> P4[4 airk service-only]
    P4 --> P5[5 Admin plane and pages]
    P4 --> P6[6 Extension]
    P4 --> P7[7 Installers and release]
    P5 --> P8[8 Qualification]
    P6 --> P8
    P7 --> P8
~~~

| Phase | Work | Done when |
|---|---|---|
| **0. Inventory and review** | Disposition of every `airk` leaf command, extension command and setting. The predecessor roster with fixtures. Golden outputs captured from the current `airk`. Draft specifications of I1–I4. Whether an empty catalog starts the HTTPS listener. Proposed paths in §9.5. | The user has reviewed the inventory and answered O1–O4 and O8. **Stop here for review.** |
| **1. Contracts** | Control-plane additions (§5.2), the I2 output and session contract, the admin HTTP contract and operation table, the install record, and wire fixtures including malformed-input cases. | Each contract is reviewed and landed with fixtures. No consumer has changed. |
| **2. `airk-ipc`** | Give IPC and its clients an owned module and facade inside `airk-cli`. Move callers in small groups and re-measure the module graph. Extract the crate when the §10.1 criteria hold. | Goldens reproduce. Selected test identities and outcomes are unchanged. |
| **3. `airkctl`** | Port the command tree. Add the service-side handlers. Move agent configuration to Rust. Add session mode. The old `airk` CLI still exists. | `airkctl` reproduces the goldens for every surviving command. Parity regressions pass on actual transports. |
| **4. `airk` service-only** | Remove the user CLI. Add the manual-start behavior and private roles. Delete replaced code with evidence. Update the process rosters. | The `airk` invocation table in §5.1 is tested. No user command remains. |
| **5. Admin plane and pages** | Always-on listener, sign-in and sessions, admin dispatch, `packages/ui` and `packages/admin`, embedded assets, the first-release pages. | Security regressions pass. The dedicated security review is complete. Admin reads match the other surfaces. |
| **6. Extension** | `airkctl` discovery and verification, the session facade, evidence features on it, MCP entry handling, removal of install and management surfaces, UI-only packaging. | The boundary checks in §4.4 pass. No install path remains. |
| **7. Installers and release** | Fixed layout, the signed setup step, the removal roster, consent text, signing rosters, release assets and the site. | Installer contract tests pass against predecessor fixtures. |
| **8. Qualification and handoff** | The ordered validation in §12 on frozen inputs. Optional crate rename (O11). | Receipts are collected and remaining blockers reported. |

Phases 5, 6 and 7 own different components and can run in parallel after
Phase 4. Each delegated task names its component, its `CONTRACT.md`, the
files it may edit, its T0 command, the goldens to reproduce and its "done
when". Builds, tests and platform batches go through one owner. The primary
agent reviews the combined change.

Before a feature or fix PR, remind the user to increment the patch version
and synchronize manifests and lockfiles, honoring a bump already made for
that PR. Do not relabel existing artifacts or reuse qualification under a new
version or source identity.

## 12. Validation and acceptance

Find defects with the cheapest check that can expose them. Record exact
selections, non-zero selected-test counts, input identities and receipts in
the [task ledger](../simplify/one-process-ledger.md). Mocks establish
decision behavior, not installation or signed artifacts.

| Gate | Required observations |
|---|---|
| T0: boundaries | Crate edges; VSIX contents; extension process starts and transport imports; self-spawn and process-topology rosters. |
| T0: `airk` and `airkctl` | The §5.1 invocation table. Every surviving command against its golden. Session framing, limits, cancellation and owner loss. Version and contract handshake. |
| T0: admin plane | Pure-function tests of host, origin and fetch-site checks, cookie attributes, anti-forgery, plane separation, single-use sign-in codes, session expiry and the never-over-HTTP list. Every operation has one handler, an authorization rule and a conformance entry. |
| T0: extension | Every state in §8.3. No execution before verification. MCP entry removal with and without a service. No install-side call from activation, commands, update or uninstall. The removal recognizer against shared Rust cases. |
| T0: installer contracts | Against predecessor fixtures: proven items removed; unproven and locked items kept and reported; links not followed; sources untouched; installation continues; interrupted removal resumes. Two-executable inventories. Bad or missing Windows and macOS signatures and failed Linux integrity checks are rejected. |
| T0/T1: parity | Complete read and input parity across `airkctl`, MCP, Workbench through the session and the admin pages, including provenance, coverage, continuations, errors and retries, on canonical handlers and actual transports. |
| T1: component and repository | Formatting, scoped Clippy with timings, affected extension and Python tests, and the repository, boundary and whitespace checks from [CONTRIBUTING](../../CONTRIBUTING.md). |
| T2: installed products | Install the service without VS Code. Verify manual `airk` start, `airkctl`, MCP, the admin pages in a real browser, and login startup. Install the VSIX with and without a service. Exercise detection after a manual install, service removal and replacement while a window is open, several windows, and reconnect without index creation. |
| T2: predecessor removal | From a bundled-VSIX install, an old standalone install, a preview install and a custom root: complete proven cleanup, truthful leftovers, an empty successor, untouched sources. Locked files and interrupted recovery. Failed fixtures are preserved. |
| T3: final qualification | Workspace Clippy, tests and doctests, and the artifact and release gates from CONTRIBUTING. Installation evidence on Windows, Linux and both Mac architectures, with Windows and macOS signature verification and Linux integrity and provenance verification. Platform claims are limited to what was observed. |

Before any T2 or T3 run, review the combined diff and evidence, freeze the
consumed inputs, and record the exact commands, expected results and stop
conditions. Stop at the first failure and analyze the complete diagnostics
before correcting source. Two expensive failures for one component invoke the
root-cause stop rule.

**The split is complete when all of the following are demonstrated:**

- The service installer works on its own, with Windows and macOS signatures
  and Linux checksum and provenance verification, and the site selects its
  verified release asset for each host.
- Old AIRK installations are removed as §9.3 describes, the user was told
  beforehand, and no source file changed.
- `airk` takes no user command and behaves as §5.1 states. `airkctl` carries
  every surviving command with complete results.
- The admin pages manage the service and search any index, behind the §7.2
  rules, and `/mcp` and `/api/v1` behave as before.
- The VSIX contains no native payload, has no install path, and reaches the
  service only through `airkctl`.
- Read parity, native authority, source isolation, and revision and budget
  contracts are intact.
- Documentation and Marketplace text describe both products and their actual
  qualification status. Publication is claimed only after authorized
  workflows and published-artifact verification succeed.

## 13. References

| Area | Starting points |
|---|---|
| Native CLI and roles | `crates/airk-cli/src/main.rs`, `tools.rs`, `process_roles.rs`, `session_commands.rs`, `session_protocol.rs` |
| Local IPC and clients | `crates/airk-cli/src/local_ipc.rs`, `local_ipc_windows.rs`, `local_ipc_unix.rs`, `service_ipc.rs`, `project_client.rs`, `client_requests.rs`, `service_frontend.rs`; `crates/airk-core/src/service.rs` |
| HTTPS | `crates/airk-cli/src/http_gateway.rs`, `http_endpoint.rs`, `service_runtime/http.rs`; `crates/airk-mcp/src/remote/http.rs`; [local HTTPS API](../local-https-api.md) |
| Installation | [Installation guide](../service-installation.md), [layout contract](../../crates/airk-cli/src/install_layout/CONTRACT.md), [file contract](../../crates/airk-cli/src/install_files/CONTRACT.md), `crates/airk-cli/src/runtime_install/replacement.rs`, `standalone_setup.rs`, `user_paths.rs`, `service_manager.rs` |
| Packaging and signing | `installers/`, `scripts/package-standalone-*.py`, `standalone-installer-contract.py`, `build-service-runtime.py`, `sign-native.py`; [signing contract](../native-signing.md), [release verification](../release-verification.md) |
| Extension | `packages/vscode/src/serviceExtension.ts`, `serviceBootstrap.ts`, `nativeSession.ts`, `serviceClient.ts`, `explorerEvidence.ts`, `explorerContracts.ts`, `agentConfigHealth.ts`, `agentMigration.ts`; `packages/vscode/scripts/build-explorer.cjs` |
| Release and site | `.github/workflows/github-release.yml`, `extension-release.yml`, `downloads.yml`; `scripts/github-release.py`, `build-downloads.py`, `verify-published-release.py`; `site/` |
| Governing documents | [Architecture](../architecture.md), [simplification plan](../simplify-plan.md), [process ownership](../process-model.md), [replacement policy](../upgrade-rebuild-fallback.md), [agent setup](../agent-integration.md), [read parity](../read-interface-parity.md), [installer stop record](../research/installer-stop-2026-11-09.md) |

These are source-review starting points, not claims that tests, installers or
signatures have passed for the split. This planning change runs documentation
checks only.
