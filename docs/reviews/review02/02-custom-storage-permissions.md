# Verify privacy of indexes created in an existing custom storage directory

Draft for private security review under `SECURITY.md`. Conditional hardening concern; cross-user disclosure has not been reproduced.

Selecting an existing permissive storage directory can bypass the owner-private directory creation used for fresh AIRK storage. Because indexes contain source-derived text and terms, the selected directory's access policy matters even when source files themselves are private.

## Version, environment and evidence

Reviewed source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 development candidate, on 2026-10-30. The concern is an existing custom directory on a multi-user Unix host. It does not establish exposure in the normal newly created default directory.

[Storage preparation](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-cli/src/service_registration.rs#L21) creates missing directories with mode 0700 but does not check the owner or permissions of an already existing root. [SQLite creation](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-store/src/current/database.rs#L309) delegates file creation to the VFS without an explicit owner-only file policy in this path. [Lexical rows](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-store/src/current/lexical.rs#L179) contain source-derived terms; private blob tempfiles do not establish privacy of every database or sidecar.

## Investigation and expected outcome

Use two disposable local accounts, fictional source text, an existing permissive data directory, and recorded umask/ACL settings. Observe permissions and second-account access to the database and sidecars after registration, indexing and restart. Repeat with a freshly created default root as a control. This procedure was not run during review.

The investigation should establish whether the current path exposes evidence and propose a supported policy for unsafe custom roots. Rejecting the location or creating a private child directory are possible remedies; silently changing permissions on an unrelated shared directory is not an acceptable default. Implementing a remedy is a separate follow-up to this investigation.

## Proposed acceptance criteria

- **AC1:** A retained, sanitized reproduction records effective owner, permissions/ACLs and access results for existing custom and fresh default storage.
- **AC2:** If disclosure is confirmed, record the affected files/call paths and a scoped remediation proposal with a regression-test plan for owner-only access. If it is not confirmed, identify the controls that prevent disclosure and the environments in which they were verified.
- **AC3:** Assess preexisting data, directory redirection and concurrent directory creation, recording any unsupported cases. The investigation and proposed remedy preserve unrelated files and do not silently change shared-directory permissions.
- **AC4:** Windows behavior is assessed separately or explicitly left unqualified; Unix observations are not presented as Windows ACL proof.
