# MCP setup templates

MCP clients connect directly to the shared native service at
`http://127.0.0.1:<port>/mcp` (plain HTTP on the loopback address). These are format templates, not ready-to-use
connections. Obtain the actual entry from AIRK (Connect Coding Agent in the
Explorer or Admin, or `airkctl agents connect`), or replace the URL with the
live one from `airkctl agents endpoint --output json` (`url`). Never assume the
preferred port 57421 is available: the live endpoint can use another port, and
it can change after a restart or upgrade.

## No token (U110)

The loopback endpoint takes no credential. An entry contains only the URL and
the `X-AIRK-Response-Mode: terse` header: no `Authorization` header, no
`headersHelper` / `http_headers_helper`, no `bearer_token_env_var`, no
`${input:…}` token and no connection id. AIRK recognizes its entry by URL. Any
local process of any OS user that can reach 127.0.0.1 can read every index;
the service still refuses a wrong `Host`, a browser `Origin` and the Admin
cookie on this plane.

- `native.json`: Claude Code project configuration (`.mcp.json`), retaining its
  historical filename: `{"type": "http", "url": …, "headers": {…}}`.
- `vscode.json`: VS Code / GitHub Copilot Chat `mcp.json`: `type` `http`, `url`
  and `headers`. No extension command input is needed.
- `codex.toml`: Codex configuration: literal `url` plus
  `[mcp_servers.airk.http_headers]`. Codex does not substitute environment
  variables into the URL.
- `docker.json`: **historical predecessor stdio example only**. It is not a
  supported shared HTTP connection. A host cannot reach a container's private
  loopback listener by reusing the same URL; container or remote deployment
  needs a separately qualified endpoint.

## No certificate to trust

The endpoint is plain HTTP and listens only on 127.0.0.1, so a client needs no
CA bundle, certificate pin or OS trust-store entry. An older
`https://127.0.0.1:<port>/mcp` entry is legacy: regenerate it with
`airkctl agents connect` or replace its URL with the live one. A saved URL
alone does not prove a working connection.

## Scope is a per-call parameter

There is no per-connection scope. Every read tool takes a scope in its
arguments:

- `index_ids` and/or `index_set_ids`: ids or names (union, at most 32 each); or
- `workspace`: an absolute folder path. The default scope applied in the
  Explorer for the most specific stored folder containing it is used.

A read with neither is refused with `scope_required`. MCP clients that declare
the `roots` capability get `workspace` filled from their first `file:` root
when a read names no scope. Results report `read_scope.source` (`named` or
`workspace_default`).

Call `list_scopes` (formerly `list_indexes`) first: it lists the indexes,
index sets and stored workspace defaults; pass `workspace` to also get the
matching `workspace_default` (or null).

The raw JSON examples (`../local_http_client.py`, `.cjs`, `.ps1`) follow the
same rules: they read `<control>/http-1/endpoint.json`, send no
`Authorization` header, and merge `--index` / `--set` / `--workspace` into the
tool arguments.

Actual client initialization, tool discovery, scope and a cited read remain
qualification gates. Other client schemas remain compatibility blockers until
verified; do not translate these formats by guessing.

See [agent integration](../../docs/agent-integration.md),
[local HTTP API](../../docs/local-http-api.md),
[MCP tools](../../docs/mcp.md) and
[service setup](../../docs/service-project-setup.md).
