# AIRK sidebar renderings

The eleven `dashboard-*.png` images render the implemented 1.1.1 sidebar HTML/CSS with fictional workspaces and coverage observations, using a VS Code dark-theme palette. Viewport width is 360 CSS pixels, rendered at 2× resolution; images show the full scrollable panel. Separate installed-editor captures appear below.

Regenerated during the October 17, 2026 code review. Cards distinguish a completed indexing job from current coverage: stale files show **Changes detected**, an unapplied detail selection shows **Detail change pending**, and an absent or incomplete inventory does not claim that the index is current. The storage text explains why protected readers or pending cleanup may temporarily retain extra generations.

| State | Rendering |
|---|---|
| Detail change saved, previous detail still published | [Pending detail](images/dashboard-detail-pending.png) |
| Indexing with measured saved-file coverage | [Indexing](images/dashboard-indexing.png) |
| Automatic indexing paused | [Paused](images/dashboard-paused.png) |
| First run before coverage is measured | [First run](images/dashboard-first-run.png) |
| Failed indexing with previous revision retained | [Error](images/dashboard-error.png) |
| Two independent workspace collections | [Multiple roots](images/dashboard-multi-root.png) |
| Helper verified with tool count and timestamp | [MCP verified](images/dashboard-mcp-verified.png) |
| Native handshake in progress | [MCP checking](images/dashboard-mcp-checking.png) |
| Verification failed with an inline error | [MCP failed](images/dashboard-mcp-failed.png) |
| Storage capacity blocked with recovery controls | [Storage blocked](images/dashboard-storage-blocked.png) |
| VS Code registration off; prior helper check retained | [Registration off](images/dashboard-mcp-off.png) |

To regenerate with a local Chromium browser:

```console
npm run compile --prefix packages/vscode
node scripts/render-sidebar.cjs
```

The script uses Microsoft Edge's standard Windows location by default. Set `AIRK_BROWSER` to a Chromium/Chrome/Edge executable elsewhere. It starts a hidden headless browser with a separate profile under ignored `artifacts/`, loads only fictional fixture pages and writes the PNGs here. It also checks horizontal overflow and verifies that the rendered indexing-mode selector and registration switch send their expected webview messages. Installed-editor lifecycle and native-helper integration checks remain separate.

## Installed VS Code captures

The following PNGs are actual VS Code 1.142.0 workbench captures from the Windows AIRK 1.1.1 VSIX qualification on October 17, 2026, using private profiles and original fictional source files. They are separate from the renderer fixtures above.

- [Verified MCP helper and registration control](images/installed-editor-mcp.png): the panel's switch was clicked off/on, then Verify connection completed the native sixteen-tool handshake. Two windows shared this workspace and index storage.
- [Untrusted workspace](images/installed-editor-untrusted.png): disabled controls, no MCP definitions and the expected rejection of an indexing command.

See [installed-editor qualification](../windows-local-build.md#installed-extension-and-two-window-qualification) for the harness, isolation checks and limits. The private profiles, logs and index contents are not documentation assets or release payloads.
