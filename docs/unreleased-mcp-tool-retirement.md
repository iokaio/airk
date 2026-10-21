# Unreleased MCP tool retirement

The 1.2.5 source after the published 1.2.4 helper removes `airk_history` from the MCP catalog. The tool returned only an unsupported error after AIRK 1.2 switched to one current index. The MCP catalog now has fifteen read-only tools. CLI and kernel history requests still return their existing unsupported errors; this change does not add historical evidence or alter collection data.

The VS Code helper check expects the fifteen-tool catalog from a newly built helper. Its upgrade fixture can still inspect older helpers with fourteen, fifteen or sixteen tools. Agent clients that explicitly allowlist `airk_history` should remove that entry when switching to a helper built from this source. Published 1.2.4 binaries and historical qualification records retain their original sixteen-tool behavior.

This is a pending 1.2.5 change. No 1.2.5 release, VSIX or Marketplace listing has been published for it.
