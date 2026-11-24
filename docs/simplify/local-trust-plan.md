# Local trust simplification plan

Decided by the user on 23 November 2026 (about 20:15 to 20:40 UTC), to land
before L7 of the [Answer evaluation plan](../arbiter/final-plan-fp16.md).
This page is the recorded contract decision for the wire, connection and
state changes below. Each phase still records its exact changes in the
owning `CONTRACT.md` files and in the ledger.

## Why

AIRK's service listens on 127.0.0.1 only and exposes nothing publicly. Since
U110 (15 November 2026, [local HTTP API](../local-http-api.md)) the bearer
plane takes no credential: any local process can read every index. The
layers kept after U110 protect very little and cost reliability. Scope and
admission fences reported harmless races as permanent refusals
(change-log entries 65 and 66, and the `scoped_connection_unavailable` seen
in LH's acceptance), and loopback TLS makes opening Admin in a browser a
chore.

## Decisions

1. **Keep:**
   - the loopback-only listener;
   - the Host and `Origin` checks, which guard against a web page driving
     the API through the browser (cross-site requests, DNS rebinding);
   - Admin's browser guard: the one-time launch code issued over the owner
     pipe, the session cookie, the anti-forgery token, the same-user peer
     check and the `Origin` check.
2. **Serve loopback over plain HTTP.** Remove the installation CA, leaf
   certificates, issuer rotation and OS trust handling. Outbound HTTPS for
   model downloads is unchanged.
   - The Admin cookie drops the `__Host-` prefix and `Secure` (Safari does
     not keep `Secure` cookies on `http://127.0.0.1`). It keeps `HttpOnly`,
     `SameSite=Strict` and `Path=/`.
3. **Finish U110.** Remove the credential, bearer, connection-ID, issuance,
   revocation, open-connection enrollment and agent-choice machinery.
   - Also remove **reviewed grants entirely**: Admin's Grants tab, VS Code's
     grant picker, `--catalog-session` and `--combined-session`, and
     `CatalogGrantDraft`.
   - The grant tables in `service-1.4.sqlite3` and the credential ledger
     (`http-1/credentials.json` and its token files) are removed on upgrade
     (user, 23 November 2026, replacing the earlier "retained data"
     reading). Predecessor cleanup recognises AIRK's agent-config entries by
     their known shapes instead of reading the ledger.
   - Grant operation records are deleted on upgrade (user, 23 November
     2026): `CreateGrant` and `RevokeGrant` are removed outright, and the
     catalog registry deletes the `index_operation_receipts` rows that name
     them before any stored intent is decoded. These are application records
     of earlier operations, not index content, and this build was never
     deployed beyond this machine, so no backward compatibility is owed for
     them. The table schema and every other row are unchanged.
   - Kept, because live reads depend on it: per-call scope (`index_ids`,
     `index_set_ids`, `workspace`), Agent-plane leases, `lease_generation`,
     catalog fences, and the window and Admin in-memory selections.
   - Operation records (user, 23 November 2026), in the same catalog
     layout and upgrade step:
     - prune `index_operation_receipts` to the unfinished receipts plus a
       recent window, rotating the request epoch; terminal receipts were
       never evicted and the 1,024-row cap refused every catalog change
       after about 1,024 changes;
     - prune finished splits (capped at 64, never deleted);
     - delete retired `index_definitions` rows once their cleanup is
       acknowledged;
     - drop `index_context_imports`, `legacy_index_projects` and, if only
       pre-service adoption uses them, the adopted-authority tables;
     - remove the dead `index-catalog-upgrade.json` startup check and
       delete finished `rebuild.json` and clean-break cleanup journals.
4. **Fences are consistency checks.**
   - A changed authority is still a refusal.
   - A publication that moved under the same authority is read again, or
     answered with the retryable `publication_busy`.
   - A transient setup state (a worker busy, starting, draining or
     restarting, a catalog exchange pending) is typed and retryable, never a
     permanent `scoped_read_unavailable` or `scoped_connection_unavailable`.
   - Work that already spent model tokens is never replayed: its fence
     reports a typed, non-replayable "publication advanced" result.
   - Unknown setup outcomes (`catalog_connection_outcome_unknown`,
     `catalog_connection_owner_closed`, "catalog native outcome unknown")
     stay permanent refusals (user, 23 November 2026): an unknown worker
     outcome is never reported as pending, and retrying it could repeat
     work.
5. **Old installs migrate automatically.**
   - Saved `https://127.0.0.1` endpoints (the endpoint record, agent configs,
     VS Code settings) are recognised as "needs reconnect" and repaired by
     the service, `airkctl agents connect` and the extension.
   - The OS-trusted AIRK CA is removed on upgrade.
6. **Order:** phase A, then B, then C, one version bump and one installer at
   the end (D). L7 starts after D.

## Phases and acceptance

### A. Fences become consistency checks

Sites, from the 23 November inventory:
- kernel `scoped_reads.rs` `scoped_source_projection` and
  `scoped_index_metadata` (moved token), `compile_atoms`, `attach` and the
  root comparison;
- the combined frontend's `source_projection_expired`;
- the `usage_client` pagination check;
- the Answer, joint and Focus post-work fences (kernel `arbiter.rs`,
  `answer.rs`, `joint.rs`, `focus.rs`), including the unrendered
  `FocusPostExecutionStale`;
- the blanket `map_err` to `scoped_read_unavailable` in service
  `tools_call.rs`;
- `connection_failure_code`'s untyped fall-through and `setup_retryable`;
- the catalog-clock "missing or wrong namespace" conflation;
- the begin side of catalog admission (`pending` or no worker is
  retryable; revocation and ticket mismatch still refuse).

Acceptance:
- Each site gets a test that fails on the old code.
- The `admin_plane` and `combined_reads` Answer stress group passes 20 of 20.
- LH's switch sequence (neither, Focus, both, Arbiter, neither) passes on a
  CUDA fixture lifetime with no repeated step. This needs a fresh base, built
  in phase D.

### B. Credential and grant removal

Order (each step compiles and passes Clippy before the next):
1. **Gateway:** the local session replaces the synthetic credential. Remove
   `CredentialDelivery*`, `bind_credential`, `Gateway::revoke` and the
   non-Open `HttpSelection` arms.
2. **Startup:** stop opening or creating the credential ledger.
3. **Control and CLI surface:** remove `agents info|credential|revoke|verify|launch|default`,
   `HttpEnroll` and `HttpRevoke`, the `AgentConnection*` methods, the Admin
   `agents.connection` and `agents.ownership` rows, and the interim setup
   enrollment.
4. **Store and types:** remove `CredentialStore`, the ledger types,
   `agent_connection.rs`, agent choice, and `connection_id` in `open_reads`.
5. **Reviewed grants:** remove the Admin Grants tab, the VS Code grant UI,
   `--catalog-session`, `--combined-session`, `CatalogGrantDraft` and
   `CreateGrant`/`RevokeGrant`.
6. **The catalog-grant authority arm:** remove `Authority::Catalog`, and the
   `grant_id`, `grant_revision`, `provenance` and `grant` halves of
   `ScopedReadRef` and `ScopeObservation`. Regenerate the I3 contracts and
   TypeScript.
7. **Removed on upgrade:** the sqlite grant tables, the grant operation
   receipts and the credential ledger (see decision 3).

Deletion evidence follows the assignment's rule. First lift the
module-level `#![allow(dead_code)]` in `catalog_admission.rs`,
`catalog_authority.rs` and `catalog_coordinator.rs`, so the compiler shows
what is dead. Predecessor cleanup and legacy-entry recognition (installer
`http_cleanup.rs`, `airkctl` recognition of `Bearer` and `headersHelper`
entries, VS Code `airk.httpCredential`) stay as compatibility duties with
reviewed rows.

Acceptance: a clean compile and Clippy with all and default features
workspace-wide, no module-level dead-code allowances left in the touched
modules, the affected Rust, TypeScript and Python tests passing, and the
regenerated contracts checked.

### C. Loopback over plain HTTP

- **Gateway:** plain TCP. The delivery fences (`FencedIo`, `FlushTrackedIo`)
  move from under TLS to under TCP, with their flush tests.
- **Endpoint records:**
  - `LoopbackMcpUrl` becomes `http://127.0.0.1:<port>/mcp`, and an old
    `https` record decodes as "needs reconnect";
  - the descriptor drops `ca_sha256` and `certificate_sha256`, and
    `HttpConnectionInfo` drops `ca_certificate_file` and `credential_file`.
- **Removed:**
  - `airk-endpoint` `tls.rs`, issuer rotation and trust receipts;
  - `airkctl` trust and rotation commands;
  - the setup `ApplyTrust` and `RemoveTrust` steps and the installers'
    `/TRUST`, except that **upgrade removes the old CA from the OS store**;
  - the `https.trust_status` Admin row;
  - `rcgen` and `x509-parser`, and `rustls` and `tokio-rustls` outside
    `airk-service`'s model downloads.
- **Clients:**
  - `airkctl`: recognition, admin URL, and launch environment variables
    (no CA bundle);
  - `airk-cli` launch;
  - the test HTTP helpers;
  - VS Code: `serviceHttp`, the recognizer, `adminOpener`, reconnect and
    migration;
  - Admin: the trust UI and snippets;
  - `scripts/http_fixture.py` and the harnesses that import it;
  - `examples/`.
- **Docs:** `local-https-api.md` becomes the local HTTP API; also agent
  integration, `one-airk.md`, the CLI reference, service installation, the
  VS Code extension page, README, MCP help and the generated help manifest.

Acceptance:
- The workspace compiles and passes Clippy.
- The gateway, Admin plane, endpoint and CLI integration tests pass over
  HTTP.
- An upgrade test turns an old `https` install into a working `http` one and
  removes the CA.
- The Admin page opens in Edge or Chrome with no certificate step; Safari is
  checked on the Mac.
- The extension and Admin tests pass, and the docs check passes.

### D. Release and fresh bases

- One patch bump, synchronised everywhere, just before the installer build.
- Installer and VSIX candidate, for the user's manual check.
- Fresh fixture bases for L7. No spare gate root is left after LH's
  acceptance.
- Phase A's live switch-sequence check on a CUDA lifetime.

L7 starts after D.
