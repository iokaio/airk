# MCP setup templates

The 1.8.14 candidate connects MCP clients directly to the shared native HTTPS
service. These are format templates, not ready-to-use connections. Obtain the
actual configuration from AIRK: Connect Coding Agent, or native `airk setup`
with an explicitly reviewed catalog selector. Replace the URL and connection
reference together from that authenticated native result. Never assume that the
preferred port is available: the persisted live endpoint can use another port.

- `vscode.json`: VS Code / GitHub Copilot Chat Local harness configuration. The AIRK extension resolves its exact native connection ID through `airk.httpCredential`; no host environment reload is needed. Forwarded/portable harnesses which reject command inputs are not qualified.
- `codex.toml`: Codex project configuration. `url` is literal; AIRK does not invent
  unsupported URL interpolation. `bearer_token_env_var` names the protected
  credential provisioned to the actual host process.
- `native.json`: Claude Code project configuration, retaining its historical
  filename. `${AIRK_MCP_TOKEN_...}` is Claude's header environment reference.
- `docker.json`: **historical predecessor stdio example only**. It is not a
  supported 1.8.14 shared HTTPS connection. A host cannot reach a container's
  private loopback listener by reusing the same URL. Container/remote deployment
  requires a separately qualified endpoint, trust, credentials and exact scope;
  preserve the old configuration until that migration is verified.

Native enrollment retains the selected reviewed grant or genuine imported
legacy snapshot; it does not infer authority from an open folder or this file.
The templates contain no token or private key. Do not paste the native
`credential_file` contents into a tracked configuration. Native internal clients
read protected credential references; external clients require their verified
host-specific provisioning. New VS Code terminals can receive the prepared
Codex/Claude environment. Already running terminals and remote hosts are not
changed by saving a configuration.

HTTPS certificate validation remains enabled. Provision the exact native local
issuer using an additive client CA bundle or supported per-user OS trust.
VS Code exposes **AIRK: Trust Local HTTPS Issuer**, with the actual native
fingerprint for review; **Remove Owned Local HTTPS Trust** preserves preexisting
issuers. A saved URL alone proves neither trust nor a working client connection.
Linux VS Code has no automatic per-user trust-store path here: an administrator must install the exact reviewed issuer in its distribution system store and user NSS store. An internal AIRK HTTPS check is not external Code qualification.
Actual client initialization, tool discovery, scope and a cited read remain
qualification gates. Other client schemas remain compatibility blockers until
verified; do not translate these formats by guessing.

See [agent integration](../../docs/agent-integration.md),
[service setup](../../docs/service-project-setup.md), and the
[implementation plan](../../docs/one-airk.md).
