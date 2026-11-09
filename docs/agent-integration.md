# Coding-agent MCP setup

The **1.8.14 working implementation** connects clients directly to the shared
native HTTPS service on IPv4 loopback. MCP connections and reads do not start an
AIRK command, launcher, proxy or Node bridge. The listener URL comes from verified
native discovery: an occupied preferred port can select another saved port, so
copy the actual endpoint rather than assuming a machine-wide fixed port.

The native service owns connections and protected credential files. VS Code is
an optional setup interface, not an MCP proxy: closing it does not put its
extension between a standalone client and AIRK. Headless reads still require the
native service to be running and the client process to have its reviewed
credential and CA trust. A saved configuration alone does not supply either.

## Select scope and provision the actual client

In Explorer **Indexing → Search scope**, review indexes/index sets and choose
**Apply selection**, then **Connect Coding Agent**. The host freezes the actual
native selector; saving a URL does not grant access. Missing/revoked authority,
a changed registration or stale review remains an explicit refusal. Imported
connections keep their original native grant and genuine owned configuration or
preexisting provider receipt. A multi-root predecessor retains the exact original
union and requires an explicit project; the active editor does not narrow it.

Historical helper-cache cleanup retains the native scope in its complete
receipt. A native default installation reports `native-defaults`, covering its
bounded native-discovered editor storage roots, the current editor context and
exact proved configuration origins. A custom/private installation reports
`context-only`: its supplied editor context and proved origins do not authorize
ambient user cache discovery. Native OS installation identity determines this
distinction; an editor path or test flag cannot promote it. Unknown cache
ownership or incomplete retirement remains an explicit blocker.

| Implemented adapter | Configuration | Host trust and credentials |
| --- | --- | --- |
| VS Code / Copilot Chat | `servers.airk` with `type: "http"`, literal actual `url`, and protected command input `${input:airkToken_<ID>}` with a matching top-level `inputs` definition. The built-in provider returns an HTTP definition. | Review the native per-user issuer trust action. The provider supplies its protected credential via the API; manual project files use the AIRK extension's protected native credential resolver, without a Code reload. |
| Codex | Literal `mcp_servers.airk.url`, `bearer_token_env_var`, and nested `http_headers`. URL interpolation is not invented. | Add the public issuer through `CODEX_CA_CERTIFICATE`, preserving an existing CA bundle/`SSL_CERT_FILE`. Credentials use a host environment reference. |
| Claude Code | `mcpServers.airk` with `type: "http"`, literal actual `url`, and `${AIRK_MCP_TOKEN_<ID>}` Authorization header. | Preserve `NODE_EXTRA_CA_CERTS` while adding the public native issuer; provision the credential to the actual process. |

These are implemented serializers/provisioning paths, not blanket client/version
or cross-platform qualification. Other formats remain
`http_client_compatibility_unverified`; existing configurations are preserved.
The [dated predecessor matrix](reviews/agent-integration-before-http-2026-11-07.md)
is historical evidence, not a supported current stdio setup path.

### Client classification for 1.8.14 (8 November 2026)

This matrix classifies all fifteen entries in AIRK's client catalog. **Implemented**
means AIRK emits the HTTPS configuration and protected credential reference; it
does not mean the named client has passed a scoped connection test. **Blocked**
means current HTTPS setup returns `http_client_compatibility_unverified` without
rewriting the existing configuration. For blocked clients, URL fields, trust,
credential storage, configuration location and reconnect behavior remain
unverified; historical stdio formats do not qualify those fields.

| Client | Current AIRK adapter / configuration location | Actual connecting-process evidence | Qualification still required |
| --- | --- | --- | --- |
| VS Code / Copilot Chat | Implemented: workspace `.vscode/mcp.json`; `servers.airk`, HTTP `url`, Authorization command input and top-level `inputs`. Built-in provider uses the same enrolled native authority. | Code **1.145.2** actual LocalProcess MCP host attempted connection on sealed debug **r24** and refused the certificate before tool discovery; the fixture issuer was absent from CurrentUser Root. Earlier installed-extension journeys exercise AIRK's own provider clients and retain that separate scope. | Review/provision actual host trust, then prove Code/Copilot initialization, tool discovery, scoped reads and reconnect against the final artifact on each claimed OS. |
| Claude Code | Implemented: project `.mcp.json`; `mcpServers.airk`, `type: "http"`, literal `url`, environment-reference Authorization header. | Windows **2.1.118** actual connecting-process acceptance passed on sealed native debug **r25**: credential-authenticated scoped reads, reconnect, additive trust and wrong-issuer rejection. Sources were preserved and native Stop completed; no model turn or OS trust mutation. Receipts `resume-actual-claude-r25-r1.json` / `resume-actual-claude-r25-result-r1.json` are recorded in the [implementation ledger](research/one-airk-implementation.md). Claude's control projection omits MCP `isError`, limiting that observation. | Repeat against the final optimized artifact and each other claimed OS; Windows debug acceptance is not release or cross-platform qualification. |
| Codex | Implemented: project `.codex/config.toml`; `mcp_servers.airk.url`, `bearer_token_env_var`, nested `http_headers`. | Windows **0.164.0-alpha.11.1** actual app-server acceptance passed on sealed native debug **r24**: 29 tools, credential-authenticated status/source reads with original publication, crossed-project refusal, fresh-process reconnect with unchanged configuration/authority, and preserved additive trust. An untrusted issuer received zero HTTP requests or Authorization headers. All owned clients exited and native Stop completed; no model turn or OS trust mutation. The earlier fictional TLS spike retains its separate scope. | Repeat against the final optimized artifact and each other claimed OS; debug acceptance is not release or cross-platform qualification. |
| Cursor | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Gemini | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Antigravity | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Cline | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Roo | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Junie | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Copilot CLI | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| OpenCode | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| OpenCode v2 | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Kilo | Blocked; HTTPS adapter unverified. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Amazon Q | Blocked; HTTPS adapter unverified. Historical connection details are not automatic modern setup. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |
| Windsurf | Blocked; HTTPS adapter unverified. Historical connection details are not automatic modern setup. | None retained for 1.8.14 HTTPS. | Complete trusted HTTPS/client qualification. |

Source owners are the [extension client catalog and serializers](../packages/vscode/src/model.ts),
[native setup serializer](../crates/airk-cli/src/main.rs),
[native setup contract tests](../crates/airk-cli/src/main_http_setup_tests.rs),
[owned configuration locations](../packages/vscode/src/agentMigration.ts),
[Codex project configuration](../packages/vscode/src/agentConnections.ts) and
[Claude project configuration](../packages/vscode/src/agentConfig.ts).
The [implementation ledger](research/one-airk-implementation.md) retains the
dated Codex TLS spike and installed-extension evidence with their original
artifact identities. Unsupported clients stay blocked; classification completes
the inventory, not the remaining C2 acceptance.

The failed Code host attempt prepared an additive child CA bundle whose issuer
matched the native service. A separate read-only Code **1.145.2** extension-host
probe confirmed that the environment value reached Node and the native issuer
appeared in its extra/default CA sets. Installed source inspection identifies a
separate MCP fetch path: its proxy dispatcher supplies bundled and system CAs,
which omit that extra-only issuer in the tested configuration. This is not an
environment-propagation failure. The tested default Code path still needs
reviewed per-user native trust enrollment and actual acceptance afterward;
neither investigation changed an OS trust store. Do not disable system CA
support or certificate verification to make the test pass.

All implemented adapters retain the original reviewed selector and native grant.
AIRK can refresh its owned endpoint definition after native discovery; external
clients may require their own reload or reconnect. An environment reference only
works in a process which received that credential and additive trust. Existing
client processes do not acquire new environment values when a file is saved.

VS Code command inputs require the AIRK extension and its Local harness support. Portable/forwarded harnesses that reject input/command variables remain unqualified; do not convert them by guessing. The resolver accepts only a connection ID, verifies current native grant/expiry/ACL and the actual matching workspace configuration, then reads the exact protected native token. No token or caller-nominated file path is saved in the JSON.

Recognized owned-file migration preserves comments, other servers, disabled
settings and tool policies. Exact file preimages, editor versions, unsaved edits,
canonical paths and authorization are checked around saves. Custom entries and
user-wide/client-private state need review. No raw token, private key or
`credential_file` contents belong in tracked files or copied snippets. See the
[configuration templates](../examples/mcp/README.md).

The extension can prepare additive CA/credential references for **new terminals
created by that VS Code window**. Existing terminals, separately launched apps,
headless sessions, SSH/remote hosts and containers do not receive those values
by saving a project file. Provision and qualify the process which will connect.
A host cannot reach a container's private loopback merely by copying its URL.

## Native setup output

### Standalone onboarding without VS Code

Standalone Windows, macOS and Linux installer support is implemented in source for the CLI and native
MCP/HTTPS service only. They do not install a VSIX or VS Code extension, run Code
actions, or require VS Code. The extension remains a separate optional
distribution. The intended path is native installation, then guided `airk setup`
and reviewed client enrollment. Installer signing, native installed journeys and
GitHub release-asset verification remain pending; no published installer is
claimed here. See [native signing](native-signing.md) and the
[release workflow](release-workflow.md).

The pending standalone release assets use these names (`<version>` is the
published release version; names alone do not establish availability):

| Platform | Installer asset | Intended entry point |
| --- | --- | --- |
| Windows x64 | `airk-<version>-x86_64-pc-windows-msvc-installer.exe` | Per-user native installer, then guided CLI setup |
| macOS Apple Silicon | `airk-<version>-aarch64-apple-darwin-installer.dmg` | Mount, open **Install AIRK.command** in Terminal |
| macOS Intel | `airk-<version>-x86_64-apple-darwin-installer.dmg` | Mount, open **Install AIRK.command** in Terminal |
| Linux x64 | `airk-<version>-x86_64-unknown-linux-gnu-installer.tar.gz` | Extract, run `./install.sh` |

Windows packaging uses the existing Azure signing method for the installer and
uninstaller as well as the native payload. Its per-user Installed Apps uninstall
entry delegates to native cleanup first; an incomplete native result must not
be reported as successful removal. The Mac disk images use the
existing Developer ID Application identity, accepted notarization and a stapled
ticket; they do not need a new Developer ID Installer certificate. The native
app remains inside its signed bundle. Linux remains unsigned by an OS signing
identity; its GitHub artifact-attestation integration verifies workflow provenance.
Verify the exact downloaded artifact using the [release verification guide](release-verification.md)
before running it. Actual signed/notarized installer and published-attestation
verification remain pending. These installers do not enroll arbitrary OS trust
or silently replace existing client configuration.

Packaging contracts passed 77/77 and release/site contracts 51/51 with mocked
platform tools. Those checks do not compile, sign or install an actual installer,
and no published installer or attestation is claimed. Retained actual-client
acceptance in the matrix above keeps its original older debug identity.

Guided setup is implemented in the working tree; validation and installed
acceptance are pending. In a terminal, `airk setup` asks which supported client
(Claude Code or Codex) and index scope to use. It discovers/starts the native
service, lists native indexes/index sets or offers explicit creation, reviews
scope, then requests native grants and credential references. Opening a folder
is not authorization to index it.

Setup prints the exact approved installed executable as `cli_executable`, with
setup/uninstall instructions. Native `airk setup` does not edit `PATH` or add an
MCP proxy. In the examples below, use that full path unless terminal integration
is available. The standalone Windows Inno package separately adds its literal
verified `{app}\runtime` directory to HKCU user `PATH` after native setup
succeeds. It preserves raw manual/variable-bearing entries and `REG_SZ`/
`REG_EXPAND_SZ` type, using a durable pending-to-owned receipt; its uninstall
removes only the exact owned literal after confirmation and successful native
uninstall. Existing terminals keep their inherited environment. See the
[Windows installer](../installers/windows/README.md). Actual Inno/registry
round-trips and signing remain unqualified; mocked/static tests do not establish
them. VSIX terminal integration has its separate reviewed ownership path.
On macOS, the printed executable is the installed copy, so the disk image can
be ejected after successful installation without editing PATH.

```console
airk setup
airk setup --client codex --project <native-project-id> --yes --launch
airk setup --client claude --create-root <source-folder> --storage <index-storage> --name <index-name> --yes --launch
airk uninstall
```

Without a terminal, supply `--client`, explicit `--project` or `--index` IDs and
`--yes`; `--index` can be repeated for a reviewed selection. `--create-root` uses
the native index-setup owner; `--storage`, `--name` and `--watch` are explicit
creation choices. Watching defaults to off. Index-job acceptance and search
readiness remain separate.

Installer bootstrapping pairs `--runtime-source <bundled-payload>` with
`--approved-sha256 <distributor-pinned-manifest-digest>`. The signed or attested
distribution supplies that pin; hashing arbitrary downloaded bytes does not
create approval. `--install-only --yes` installs without client enrollment.
Every installation before 1.8.16 must use the native clean break and rebuild;
there is no preserved-index migration choice at that boundary. Preserve only
reviewed source selections and supported preferences, then reconnect clients
with fresh authority. Supported 1.8.16+ upgrades require the persisted state
contract as well as the approved version. See the
[current reset contract](upgrade-rebuild-fallback.md); its historical optional
rebuild section is no longer current setup guidance. `airk uninstall [--yes]` uses the native owner to stop/remove owned
runtime files while preserving source files and index data; removing a downloaded
installer or ejecting its image is not uninstalling the service.

The native credential owner creates the bearer secret in an owner-protected
file. Setup returns complete native receipts and references without printing
the secret. `--launch -- <client-arguments>` starts the selected installed client
in the foreground with child-only credential variables and an additive CA bundle.
It does not modify OS trust stores.
Claude receives a private per-session MCP configuration; Codex receives a
per-process AIRK override and `--no-daemon`, so an older shared daemon cannot miss
the new environment. Existing project files and unrelated client settings remain
unchanged. Without `--launch`, use the returned provisioning instructions for the
actual process; saving project JSON does not update an existing process.
The VS Code `airk.httpCredential` resolver is a separate extension
mechanism and is not available to headless clients. Neither path writes a raw
bearer token to project configuration.

After an [upgrade rebuild](upgrade-rebuild-fallback.md), old client files and
credentials remain preserved but their original namespace does not grant access
to the successor. Review and enroll the replacement scope explicitly; do not
reuse an old namespace or silently broaden the client selection.

### Existing reviewed selector

Use the exact reviewed native selector, not invented IDs or folder-derived scope.
The following is a template with values supplied by authenticated native setup:

```console
airk --service-install-dir <installation> --catalog-session '<exact-reviewed-session-json>' setup --client codex
```

`--combined-session` is the corresponding reviewed combined input. Native setup
enrolls the existing selection and returns JSON containing configuration,
protected connection references and host provisioning guidance. It does not
print a bearer token, index sources or save client files. Root-only setup without
genuine predecessor lineage requires review. Reuse a returned connection ID for
an exact retry; an uncertain mutation does not authorize a new attempt identity.

## HTTPS trust and actual verification

AI tools validate the certificate normally; they do not ignore a self-signed
issuer. Native setup verifies the public CA and DER SHA256 fingerprint before
using an additive client bundle or supported per-user trust store. Existing
enterprise/public roots remain available. TLS bypass, plain HTTP and an arbitrary
replacement CA are not supported shortcuts. Normal CA validation is the approved
guarantee; it does not uniquely pin one issuer when another normally trusted CA
signs the same endpoint name.

**AIRK: Trust Local HTTPS Issuer** displays the native fingerprint and store for
review. The native command checks that digest again before changing Windows
CurrentUser Root or the current native user's macOS login keychain. Interrupted
outcomes remain unknown for review; preexisting issuers are never adopted.
**Remove Owned Local HTTPS Trust** removes only the generated issuer backed by
its native ownership receipt. Automatic per-user Linux OS-store enrollment is unsupported. For **Linux VS Code MCP**, an administrator must explicitly install the reviewed issuer in the distribution system trust store and the VS Code user's NSS store, following [official network guidance](https://code.visualstudio.com/docs/setup/network#_ssl-certificates). This prerequisite is not implemented automatically or qualified here. A client-specific additive bundle is an implemented path for Codex/Claude; internal AIRK HTTPS success does not qualify external VS Code.

Retry the actual client's connection after provisioning. Hosts can cache trust
or environment; use their actual result and refresh controls, without claiming a
saved file applied it. Require initialization and `tools/list`, then `status`
and a scoped read citing an original source path and indexed revision. Deferred
discovery can delay visibility. A badge, valid config or extension-side probe
alone does not prove that another agent loaded the connection. Preserve native
failure/retry receipts and repeat only reads authorized by their exact contract.
CLI, Workbench and MCP retain complete canonical results and selected scope.

Project instruction buttons still open existing `AGENTS.md`, `CLAUDE.md` or
GitHub Copilot instructions without overwriting unsaved edits; missing files
become unsaved drafts for review. Instructions do not alter permissions, client
approval policies, MCP configuration or scope.

See [index setup](service-project-setup.md), [the developer API](mcp-developer-guide.md),
[interface parity](read-interface-parity.md), and [the implementation plan](one-airk.md).
