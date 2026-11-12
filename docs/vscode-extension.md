# Native VS Code extension

> **Split Phase 4 (non-shippable interim).** The extension and its management
> of the service are being reworked under the [split plan](split/split-plan.md)
> (Phase 6; the installer in Phase 7). This page describes the pre-split
> extension. On this branch the extension source still starts `airk service
> session` and `airk service console` and names pre-split `airk runtime …`
> commands; after Phase 4 `airk` is the service only and no longer accepts
> them as user commands. `airkctl` is the CLI: the window helper becomes
> `airkctl session`, and credential helpers become `airkctl agents credential`.

**1.8.16 source implementation; unpublished, with final native/installed-platform
qualification pending.** The extension presents the native Artifact Indexing and
Retrieval Kernel. Index contents, grants, jobs, process ownership and upgrades
remain native; an editor panel or saved preference does not become authority.
Current correction evidence belongs in the [1.8.16 record](releases/release-1.8.16.md);
the [1.8.14 working record](releases/release-1.8.14.md) retains the broader open gates.
Earlier results are preserved in the [dated pre-HTTP guide](reviews/vscode-extension-before-http-2026-11-07.md).

## Installation and evidence privacy

Every installed version before 1.8.16 uses the native clean-break path: review
preserved source selections and supported preferences, create fresh AIRK state
and rebuild indexes. Old index IDs, grants, credentials and pending operations
do not survive as authority; clients reconnect through fresh review. Original
sources stay untouched. Native process/ownership checks govern old-artifact
cleanup, and failures must remain visible. Supported 1.8.16+ upgrades require the
persisted state contract in addition to the version. The target public release
is 1.9.0 after qualification; the current local candidate is 1.8.22 (see the
[1.8.22 record](releases/release-1.8.22.md)), and 1.8.16 remains the state
baseline.

The [current reset contract](upgrade-rebuild-fallback.md) supersedes older
optional migration/rebuild instructions. Implementation and installed acceptance
of the revised policy are in progress.

Reset review uses complete saved metadata, bounded to 64 indexes/128 KiB. An
unsupported include/exclude policy or alias refuses before mutation instead of
widening source access. Every saved index gets one initial build and then its
saved pause/watch policy is restored. One unavailable index does not stop healthy
indexes from building; final cleanup waits for the required replacement
readiness. Complete native diagnostics and host-owned client-cleanup receipts
remain separate observations.

The in-progress HTTP disposal correction uses purpose-bound predecessor metadata
proof, separate from ordinary live-grant lookup. Complete original HTTP and host
client metadata must be durably captured before retirement; disposal never
renews credentials or enrolls a connection. Fresh-namespace collisions or unknown
ownership block cleanup. Earlier r34 checks remain historical for this change;
no current Mac or VSIX qualification is claimed.

Stop-related metadata, PATH and completion bookkeeping do not ensure or start a
runtime. Host cleanup handles Windows aliases and bare-ID saved metadata with
ownership checks. A flat managed trust-directory census permits at most 1,024
entries and 2 MiB total read work, including failed reads/overflow attempts;
unproven old-CA copies remain preserved with explicit blockers.

Use the platform VSIX containing the exact approved native payload. The host
checks the complete inventory before setup or execution. Modern runtime metadata
is schema 2 and binds one sealed `airk` image, with default MiniLM/tokenizer assets
and static inference. macOS keeps that image inside its signed `AIRK.app`.
Historical schema-1 helper/launcher layouts are predecessor inputs, not modern
package contents. See [installation](service-installation.md) and
[deployment](extension-deployment.md).

First-use **Set up** reviews native installation, optional terminal/PATH effects
and recognized existing agent configuration updates. **Customize…**, **Not now**
and **Don't set up automatically** retain their distinct choices. Setup does not
register or index an opened folder. All new indexes use **Indexing → Index
library** review; source/storage overlap and unowned occupied locations remain
refusals. Ordinary index/data deletion requires its own native review; the authenticated clean-break reset has its separate reviewed scope and owned-artifact retirement contract.

### Native execution control

`airk.runtime.nativeExecution: false` in User settings blocks AIRK discovery,
self-tests, native setup, administrative/evidence commands, credential resolution
and MCP definitions. Views disclose that execution is off. **AIRK: Enable Native
Execution** restores the ordinary verified path. Restricted Mode independently
blocks native work and secret resolution; navigation, trust-management guidance,
help and local output remain available.

After native clean-reset admission, old installation-level User Stop/lifecycle
state is discarded so the new runtime can perform its initial builds. A new
Stop during reset/rebuild stays authoritative and retry/resume cannot clear it.
This boundary never overrides `airk.runtime.nativeExecution: false` or workspace
trust; supported index pause/watch preferences remain a separate preserved policy.

`airk.cli.autoInstall: false` skips automatic installation; it is not a no-process
setting or an OS sandbox. Native discovery can still inspect the selected
installation. An isolated Code profile also does not create a separate OS user
or change native user defaults.

### Sensitive files and saved evidence

Indexing captures source-derived text. Configure exclusions before creating an
index and review hidden files, source roots and index storage explicitly.
Exclusions protect future capture; they do not erase a previously committed
publication or copied evidence. Source verification checks saved bytes, not
unsaved buffers or complete language understanding. The extension never edits
indexed sources as part of a read, refresh, model setup or upgrade.

Native control, TLS keys and credentials use protected owner-private files.
Index-data privacy is a separate concern: existing public storage/ancestor/ACL
limits remain visible in native inventory rather than being described as
private automatically. Copied result cards are a selected subset; complete
response copies/exports retain scope, provenance, readiness, coverage,
continuations, retry details and explicitly disclosed truncation. Logs/reports
have their own redacted or explicitly selected path scope.

### Data inventory and purge

**AIRK: Review and Remove AIRK Data** and native `runtime inventory` observe owned
locations without deleting them. Exact reviewed purge plans/digests govern
logs/cache cleanup. **Delete index** is logical native retirement; physical
cleanup completion is a separate observation. Runtime uninstall preserves
original sources and independent index data. Unknown files, links or ownership
changes are preserved. See [retirement](service-project-retirement.md) and
[installation lifecycle](service-installation.md).

<a id="airk-explorer-150"></a>

## AIRK Explorer

Management → Service → Runtime → **Complete runtime diagnostics** offers
**Open complete JSON**, **Copy complete JSON** and **Export complete JSON** for
the full cached native bootstrap and diagnostic observation, with its capture
timestamp. These host-owned actions do not start another native read. The saved
observation is historical, not current health or process-cleanup proof. Service
**Copy details** is a selected summary and points to the complete cached JSON.

One AIRK Explorer panel is owned per VS Code window, across repeated commands,
controllers and extension replacement. **×** dismisses update/connection/action
notices; their identities are persisted so the same notice remains hidden across
screens, panel reopen and window reload. New native work has its own identity;
dismissing a notice does not cancel it or change its receipt.

The managed extension loader detects a complete replacement payload, verifies
its sealed local module graph/assets, disposes the previous AIRK host and drains
local pending preference/session writes before activating the replacement. It
reopens Explorer only when that window had it open and retains the route through
the handoff. It does not reload VS Code or restart unrelated extensions. An
incomplete or failed replacement retains an explicit failure/retry state rather
than mixed old/new code. VS Code still owns declarative contribution updates.

This loader cannot retrofit behavior into an older version already loaded before
it existed. The first transition from such a host needs VS Code's supported
extension update/restart action; later transitions use the active loader.
Actual installed-editor acceptance is separate from source lifecycle fixtures.

The native execution host and saved search scope are shown independently of the
currently displayed source file. The saved index supplies Code outline,
Evidence/Inspector, documents, review and strategies; a source picker does not
silently grant access to another index. Help/About and local logs do not require
an agent session. Suggestions remain reviewed UI drafts, not autonomous source
edits or native indexing operations. See [Explorer documentation](explorer/README.md).

## Indexing and search selection

- **Search scope** drafts index/index-set choices. **Apply selection** validates
  current native metadata and installs that window's exact search selection.
- **Index library** reviews new source/storage setup, existing index changes,
  splits and **Delete index**. Opening/reordering folders creates no index.
- **Index sets** manages reusable native selections without duplicating evidence.

Stale/revoked choices, missing native metadata and unavailable scope do not fall
back to a broader legacy source. Genuine imported predecessors retain their
original snapshot provenance. A legacy multi-root MCP provider retains every
original member with no implicit default project. Saved preferences/checkboxes
alone do not create grants. See [index setup](service-project-setup.md).

## Workbench result selection

Workbench, Explorer and MCP use the same canonical native handlers and supported
inputs. Full structured results retain per-member origins, indexed/publication
identity, coverage, readiness, continuations and typed retry/refusal details.
Explicit cards/selections identify their subset. Reading a cached card does not
establish current source freshness or replay an accepted query/mutation.

Internal evidence sessions use the protected native HTTPS connection with
explicit Full preparation and additive issuer trust. They do not spawn an AIRK
stdio frontend. Explicit single/combined selections and genuine imported legacy
Workbench scopes keep their exact native authority. Narrowed combined readers
cannot inherit a wider credential. Administrative index setup/usage controls
remain separate native operations. The native CLI's direct tools/shell execute
the sealed main with the same selected installation and selector.

See [Workbench implementation](workbench/implementation.md) and
[read parity](read-interface-parity.md). Renderer/mock tests alone do not qualify
actual native CLI, MCP or client transport parity.

## Unified agent connection

**Connect Coding Agent** prepares the current native scope for the selected
client; **VS Code / Copilot MCP Controls** opens Code's own controls. One client's
configuration does not configure another application or silently change a
running agent's scope. New client files are explicit actions. Guarded migration
of recognized existing owned files preserves comments, other servers, disabled
settings, tool approval policies, unsaved edits and compare-before-save checks.

Modern clients connect directly to the actual native
`https://127.0.0.1:<bound-port>/mcp` URL. An occupied preferred port can cause a
new saved port. Connection/grant identity, native generation and the actual URL
remain distinct; a stale endpoint is not assumed to be the same owner.

The implemented serializers are Codex, Claude Code and VS Code. Other formats
remain `http_client_compatibility_unverified`; the picker does not justify
inventing another client's HTTP/auth schema. Native setup and an extension-side
probe are not proof that the chosen external client loaded its configuration.
Require actual initialization, tool discovery, status and a cited scoped read.

### Broken agent entries and Reconnect (1.8.22)

Explorer checks recognized AIRK entries in project files, user-level files that
AIRK provably wrote, and the reconnect ledger that a pre-1.9.0 replacement
captures before `--apply`. It warns once per set of findings when the window
starts and after an installation. Findings appear in an Explorer banner, the
status bar and Management → Agents.

**Reconnect** asks once, then creates or reuses the index for each recorded
folder and grants this window access. That grant makes the index the window's
search scope. It then enrolls HTTPS, rewrites each entry in place and
provisions trust and the token helper. Each entry reports a typed status. The
finding classes, the steps and the statuses are listed in
[agent setup](agent-integration.md#reconnecting-after-a-replacement).

### Protected VS Code credential input

Manual/migrated `.vscode/mcp.json` uses a command input reference such as
`${input:airkToken_<connection-id>}` and a matching `airk.httpCredential` input
definition. No raw bearer or caller-selected file path is written to the config.
The installed AIRK extension resolves only a validated connection ID, checks the
live native grant/expiry/verifier/private-file ownership and the exact matching
current workspace file, then reads its protected token. Changed/foreign URLs,
reused inputs, malformed arguments, lost trust or changed file preimages refuse
resolution. The built-in HTTP provider supplies the same protected credential
through the API. Recognized manual configuration owns its policy and suppresses
a duplicate provider definition.

This credential path needs no Code environment reload. It requires the AIRK
extension and a harness that supports command inputs. Portable/forwarded MCP
harnesses which reject input variables remain unqualified. Since 1.8.22,
Codex and Claude Code instead get a one-shot credential helper
(`http_headers_helper` / `headersHelper` running `airk runtime http-credential
… --format header-json`), so agents started outside VS Code receive their
token. Without a helper path that is safe to write, Codex falls back to its
environment reference. New terminals created by this window can receive
prepared additive CA/credential values. Existing apps, terminals,
headless processes and remote/container hosts do not inherit them from a saved
project file. See [agent setup](agent-integration.md) and [templates](../examples/mcp/README.md).
Credential-bearing terminal environment collections are nonpersistent: Code must
not cache the bearer values to disk, including after a later CLI repair.

### Certificate trust

Management also exposes complete native HTTPS issuer rotation receipts and an
explicit reviewed prepare/trust/activate/finalize/abort flow. This window's owned
client bundles use old/new overlap and exact preimage checks; unrelated roots,
config scope and original files remain intact. A finalized/aborted transaction
can retry this window's cleanup without inventing a new operation. External
hosts and other windows need their own reconnect/read observations. See
[the rotation contract](local-https-api.md#explicit-issuer-rotation).

HTTPS certificates validate normally; AIRK does not use an insecure-localhost
flag, disabled certificate verification or plain HTTP. Normal CA validation
preserves existing trusted roots and does not uniquely pin one issuer.
**Trust Local HTTPS Issuer** reviews the native public CA fingerprint/store;
install/remove recheck that digest. **Remove Owned Local HTTPS Trust** preserves
preexisting or unproven issuers. Interrupted/changed outcomes remain explicit.

Windows CurrentUser Root and macOS native-user login-keychain adapters exist in
source. Actual cold/running Code trust-cache behavior still needs platform
qualification: reconnecting or saving config alone does not prove cache refresh.
Linux automatic per-user OS-store enrollment is unsupported; an administrator
must provision the exact public issuer in the distro system and VS Code user's
NSS stores. Internal AIRK HTTPS success is not Linux Code MCP qualification.
See [official Code guidance](https://code.visualstudio.com/docs/setup/network#_ssl-certificates).

## CLI installation and PATH

### Current first-use review

Optional shell integration is separate from native/index setup. Its small
per-user terminal wrapper forwards to the approved sealed main, not to a copied
MCP launcher or embedding executable. The wrapper is a convenience for explicit
terminal commands; clients use HTTP. Existing custom shell/PATH state and marked
integration blocks have their own guarded ownership/removal checks.

The standalone Windows EXE has a separate per-user PATH owner: after its native
setup succeeds, it adds only the literal `{app}\runtime` directory. It is not a
VSIX action and does not install this extension. Its exact-entry receipt/type
preservation and confirmed successful-uninstall checks are documented in the
[Windows installer guide](../installers/windows/README.md); actual Inno and
registry round-trips remain unqualified. The reset's HTTP environment cleanup
preserves the VSIX's existing terminal PATH mutator.

### Terminal command

**Install or Repair CLI** verifies the matching payload/native selection and
reports actual setup stages and failure details. Within a supported 1.8.16+
state contract, forward upgrades retain data/Stop/watch preferences and use
native checkpoint/capability checks. Pre-baseline installation-level Stop is
handled by the separate admitted clean-break policy; a new Stop remains
authoritative. Version labels alone do not authorize a takeover.
For VSIX-managed terminal integration, `airk.cli.shellIntegration: off` avoids
external shell/PATH edits. New VS Code
terminals and separately launched applications can have different environments.

### Removing the terminal command and uninstalling

**Remove Terminal Command** removes owned external shell/PATH integration;
it does not delete indexes or stop accepted native work. Extension uninstall
uses the packaged bounded worker and recorded native ownership; it must retain
unknown/custom configuration and cleanup uncertainty. Native removal treats
manager ownership, old process exit, credentials/owned issuer trust and payload
cleanup as separate verified steps. Index data and sources are preserved unless
their separate explicit deletion/purge contracts apply. Do not infer complete
cleanup from the editor tab closing or an uninstall process starting.

## Runtime and distribution

### One native session per window (1.8.22)

The extension does not start one `airk` process per operation. Each window
runs one `airk service session`. It relays control requests and runs
allowlisted CLI commands in-process, with the same output as the one-shot
command and responses up to 16 MiB. It exits when the window's extension host
ends.

On owner loss, the session cancels read-only work. It lets installer or other
exclusive work that has already started finish within that request's own
deadline, at most 600 seconds. This is a documented exception. When the
window releases the session, the extension waits for that deadline plus 5
seconds before it kills the process. `airk.runtime.nativeExecution: false`
closes the window session.

The extension still starts a separate one-shot when a different runtime build
is involved or when no window session exists. It also starts the uninstall
worker and the `tools shell` terminal. The service itself, with its project,
cleanup and source-watch owners, is the only resident AIRK process. See
[process ownership](process-model.md).

The extension registers login startup on every installation, replacement and
upgrade, unless the user ran `airk runtime login uninstall`. The 1.8.20
restart after an idle exit is removed. When no OS registration exists, a
status check restarts a service that ended without a Stop request, once for
that stopped instance. See
[startup registration](service-installation.md#startup-registration-and-removal).

The in-progress request-tracking correction uses a private per-request journal
at `globalStorageUri/service-intents`, shared by console and extension actions.
Preferences remain in Memento. The journal keeps the existing 64 active-request
and 2-MiB aggregate bounds across namespaces. Immutable preparation must be
published before dispatch; later observations are separate. A bounded one-way
current-scope Memento import records its disposition. Only after authenticated
fresh Genesis establishes the exact operation/store/root/control/namespace
retirement fence do fully validated predecessor-namespace Memento rows,
including late nonces, stop consuming active quota or blocking a new Stop.
Those rows are never imported or replayed. Queued writers in other windows may
still publish; bounded exact-observed cleanup reports late removals and remaining
rows separately, without claiming the original capture included them. Before
the fence, and for nonretired, foreign, malformed or wrong-namespace state,
uncertainty remains preserved and blocking. Uncaptured physical journal files
remain blocked and untouched.

If local persistence or retirement fails after a native response, the complete
native reply and original tracking error remain available; another native
mutation is not authorized. Proven publication of the exact activated fresh
successor can retire captured predecessor request authority before physical
cleanup completes. All-index readiness, native/client cleanup and overall
installation completion remain separate gates. Ordinary compatible upgrades
retain uncertain outcomes. The backend and interrupted-retirement handling are
under correction and review; no new crash/platform qualification is claimed.
POSIX directory synchronization and atomic file publication do not establish
equivalent Windows directory-entry power-loss guarantees.

Installation/control/state/index data remain separately bound. Modern runtime
schema 2 distributes one main image with embedded default model assets/static
ORT. Inference status displays the in-service owner/generation, admission,
queued/running/callback/delivery/reference counters and failure. Null inference
PID is expected for this owner. Historical Helpers receipts remain distinct;
force-stop cannot be described as killing an in-process FFI thread.

Registry contract 13 retains schema-12 physical DDL but requires the newer
provider/default-source semantics. Native capability checks fence unsupported
rollback/downgrade before data work. Checkpoint journals retain original request
and accepted-work identities; legacy old-process recovery discloses its weaker
committed-store boundary rather than claiming a graceful protocol it lacked.
See [service lifecycle](service-installation.md).

## Remaining acceptance

Current source/TS contracts and dated predecessor results do not establish a
qualified 1.8.14 release. Final gates include fresh native images/import/model
checks, actual CLI/MCP/Workbench parity, isolated installed Code and external
client auth/trust/scope, old-version many-process migration and interruption,
Stop/uninstall ownership, final Windows VSIX and signed/notarized Mac Neo tests.
Preserve actual failures, skips and missing observations in the
[release record](releases/release-1.8.14.md).

## Historical feature and qualification records

The [pre-HTTP extension guide](reviews/vscode-extension-before-http-2026-11-07.md)
retains dated UI/worker/helper/storage behavior, commands and measured results.
Those records remain attributable to their original version and inputs; they do
not qualify the new HTTP/in-process runtime or its installers.
