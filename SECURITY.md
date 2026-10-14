# Security

AIRK is an early local single-user tool. Index state includes source text. Collection boundaries are retrieval isolation within that user's process/data, not separate-user authentication or a filesystem security boundary.

Do not include private source, indexes, tokens or exploit details in public issues. If this repository has GitHub private vulnerability reporting enabled, use **Security → Report a vulnerability**. Otherwise open an issue requesting a private reporting route without disclosing the vulnerability or affected data. No response-time guarantee or security-audited release is currently offered.

Use read-only workspace mounts and per-user state storage. MCP tools retrieve indexed data and do not execute source instructions. Source content is untrusted; the consuming agent remains responsible for its instruction/permission boundaries. Default exclusions are not a secret scanner. Native extractors process only local files; they do not fetch SVG resources, OOXML relationships or XML entities.

Current limits are a scaffold protection, not complete resource isolation for hostile files. PDF extraction may allocate before output limits can be checked. Concurrent malicious filesystem mutation and network-filesystem durability remain unqualified. See [operations](docs/operations.md) for supported recovery behavior.
