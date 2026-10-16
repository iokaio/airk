# Google Antigravity MCP setup

Reviewed on **16 October 2026**. Google's [MCP documentation](https://antigravity.google/docs/mcp) describes local stdio servers with a `mcpServers` object containing `command` and `args`. AIRK already provides that transport and configuration shape. The documented connection needs no new AIRK binary, HTTP bridge or retrieval adapter. This is configuration compatibility, not an installed-Antigravity qualification result.

## Prepare the collection

Install and unpack the native ZIP for your platform from [AIRK releases](https://github.com/iokaio/airk/releases). Keep its executable in a stable location. The VSIX is optional for this connection; automatic MCP registration by the VS Code extension has not been verified in Antigravity.

Run setup with the intended workspace and state directory. This Windows example assumes `airk` is on PATH; otherwise invoke the unpacked executable by its absolute path:

~~~console
airk --root "D:/work/project-a" --data-dir "D:/indexes/airk" --collection project-a setup --client gemini
~~~

On macOS/Linux, substitute your absolute workspace and state paths. Setup indexes the collection, emits the launch configuration on stdout and writes its indexing summary to stderr. It does not modify client settings. The existing `gemini` choice supplies the compatible JSON format; it does not register Antigravity or select Antigravity's settings location. There is currently no `--client antigravity` option or Antigravity entry in AIRK's extension setup picker.

The generated configuration has this shape; keep the actual paths printed by setup:

~~~json
{
  "mcpServers": {
    "airk-project-a": {
      "command": "D:/tools/airk/airk.exe",
      "args": [
        "--root", "D:/work/project-a",
        "--data-dir", "D:/indexes/airk",
        "--collection", "project-a",
        "mcp"
      ]
    }
  }
}
~~~

Setup names the entry `airk`; the example renames it `airk-project-a` to distinguish collections. The server name does not change collection identity. Keep each argument separate. JSON Windows paths can use forward slashes as above or escaped backslashes. Absolute launch paths avoid depending on the editor's working directory or PATH. AIRK's local stdio server needs no API key or Google Cloud credentials.

## Register and connect

Google currently documents these [configuration locations and IDE controls](https://antigravity.google/docs/mcp#antigravity-ide):

- Workspace: `.agents/mcp_config.json` in the intended project.
- Global: `~/.gemini/config/mcp_config.json`, where `~` is your user home.
- IDE: agent panel **… → MCP Servers → Manage MCP Servers → View raw config**.

Prefer workspace configuration. Merge the generated server entry into the existing `mcpServers` object, preserving other servers and policies. For a different installed version, use its raw-config view to locate the active file. Gemini CLI's `.gemini/settings.json` is a different destination. Google's [CLI migration guide](https://antigravity.google/docs/cli/gcli-migration/) explains the separation.

Reload the server configuration and enable the AIRK entry in the client. Antigravity CLI exposes connection status, reload and logs through `/mcp`. Retain the client's tool approval policy; do not add blanket automatic approval just to establish a connection.

If using global configuration, assign unique names per collection and enable only the intended workspace's server. AIRK pins the configured root, data directory and collection; it does not automatically follow the editor's active folder. Reuse those same three values to share an existing index with another agent. See [shared collections](collections.md#multiple-agents-sharing-one-collection).

## Verify and maintain

1. Confirm the server connects and exposes AIRK's sixteen [read-only MCP tools](mcp.md).
2. Ask the agent to call `airk_status` and `airk_capabilities` through the configured server. Check the collection and reported capabilities.
3. Request a lexical `airk_search` for a known indexed phrase. Check its source path and original location against the intended workspace.
4. After source edits, update the same collection from a terminal:

~~~console
airk --root "D:/work/project-a" --data-dir "D:/indexes/airk" --collection project-a index
~~~

MCP retrieval does not trigger indexing. An existing [watch worker](native-scheduler.md) can maintain the collection. If tools are absent, check client enablement, executable permissions and the command path. If results are empty or stale, verify the scope arguments and indexing result before changing configuration.

AIRK's native stdio and client-format tests cover the server side. Connection, tool discovery, approvals and scope selection still need a smoke test in the installed Antigravity version on each intended OS. A named setup option or extension picker entry would be a later convenience change, not a prerequisite for this manual connection.
