# Coding-agent MCP setup

The **1.8.14 working implementation** connects clients directly to the shared
native HTTPS service on IPv4 loopback. MCP connections and reads do not start an
AIRK command, launcher, proxy or Node bridge. The listener URL comes from verified
native discovery: an occupied preferred port can select another saved port, so
copy the actual endpoint rather than assuming a machine-wide fixed port.

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
