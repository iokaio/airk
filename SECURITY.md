# Security

AIRK is a local single-user tool. Index state includes source text. Collection boundaries are retrieval isolation within that user's process/data, not separate-user authentication or a filesystem security boundary.

Do not include private source, indexes, tokens or exploit details in public issues. If this repository has GitHub private vulnerability reporting enabled, use **Security → Report a vulnerability**. Otherwise open an issue requesting a private reporting route without disclosing the vulnerability or affected data. No response-time guarantee or security-audited release is currently offered.

Use read-only workspace mounts and per-user state storage. MCP tools retrieve indexed data and do not execute source instructions. Source content is untrusted; the consuming agent remains responsible for its instruction/permission boundaries. Default exclusions are not a secret scanner. Native extractors process only local files; they do not fetch SVG resources, OOXML relationships or XML entities.

The built-in PDF parser runs in a local child process with memory, CPU, wall-time and output limits; page text and page count are also bounded. Source reads use platform no-follow traversal and handle identity checks. These mechanisms do not sandbox explicitly provisioned providers or qualify every hostile filesystem/document scenario. Platform enforcement and remaining race assumptions are documented in [ingestion hardening](docs/ingest-hardening.md). Network-filesystem durability remains unqualified. See [operations](docs/operations.md) for supported recovery behavior.

[Retention](docs/retention.md) removes only derived AIRK files under the selected collection's policy and locks. Active evidence and live readers remain protected; pending removals and failures are visible. Reclamation is logical deletion, not secure erasure of disks, backups, relocated roots or external copies. Paths/hashes may remain in change metadata after old content is reclaimed, and active indexed secrets require a source change plus successful reindexing. Unknown or foreign maintenance markers never authorize arbitrary cleanup.
