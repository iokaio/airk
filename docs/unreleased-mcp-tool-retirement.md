# Unreleased MCP tool retirement

The former `airk_history` endpoint was retired from the 1.3.0 source after the published 1.2.4 helper. AIRK 1.2 switched to one current index and no longer retains revisions for historical reads, so that endpoint could return only an unsupported error. The MCP catalog now has fifteen read-only tools. CLI and kernel history requests still return their existing unsupported errors; this change does not add historical evidence or alter collection data.

The VS Code helper check expects the fifteen-tool catalog from a newly built helper. Its upgrade fixture can still inspect older helpers by catalog size without implementing or naming the retired endpoint in application code. Agent clients that explicitly allowlist `airk_history` should remove that entry when switching to a helper built from this source. Published 1.2.4 binaries and historical qualification records retain their original sixteen-tool behavior.

This change is included in 1.3.0. The [release notes](release-1.3.0.md) record qualification limits; GitHub and Marketplace workflow records establish their separate publication status.
