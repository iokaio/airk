# AIRK 1.8.208 local source candidate

Prepared 24 November 2026 (UTC) for phase D of the
[local trust plan](../simplify/local-trust-plan.md). Native and extension
manifests and lockfiles are synchronized at 1.8.208. This is a Windows
candidate for the user's manual check; 1.9.0 remains reserved for final
delivery after its applicable gates pass. Package files, hashes, signing and
installed outcomes are recorded separately in the
[execution ledger](../simplify/one-process-ledger.md).

## Implemented source behavior

- **Local trust simplification** ([plan](../simplify/local-trust-plan.md),
  phases A to C):
  - the service, Admin and agents talk over plain HTTP on loopback
    (`http://127.0.0.1:<port>/mcp`); there is no AIRK certificate authority,
    no TLS issuer and no certificate step in the browser. The
    [local HTTP API](../local-http-api.md) replaces the HTTPS API page;
  - reads need no token, credential or grant: they are authorized by the
    window and Admin selections and open reads, and each read names its
    indexes, index sets or a workspace default. The credential ledger, its
    token files, the grant tables and the grant UI are removed;
  - the former scope fences are consistency checks, not access control;
  - upgrading an old `https` install is automatic: the service publishes the
    same port over `http`, setup removes the old AIRK CA from the current
    user's certificate store (Windows CurrentUser\Root, macOS login
    keychain) by its exact hash and deletes the old TLS files, and
    `airkctl agents repair` or **Reconnect** rewrites agent entries that
    still name `https://`. `airkctl service trust`, `issuer-rotation`, the
    `runtime http-trust` and `http-issuer-rotation` commands and the
    installer's `/TRUST` option are gone.
- **Operation-record pruning:** terminal index operation receipts are kept to
  the newest 256 beyond the unfinished and still-needed ones, finished splits
  to the newest 8; acknowledged retired definitions, the dead catalog-upgrade
  refusal file and finished clean-break journals are deleted on upgrade.
- **Answer jobs:** the service owns Answer work as jobs. A job holds the read
  authority its caller redeemed and ends with that selection's revocation,
  lease expiry or close, not with a query session's teardown. The caller's
  wait (`wait_ms`) is separate from the work budget (`deadline_ms`); `fresh`,
  `invocation_key` and `interactive` options and a per-delivery record come
  with every job reply; attended jobs dispatch first, and a supervised
  deadline holds the slot until the work settles. Answer runs as the local
  multi-round loop, with Focus and Arbiter as separate switches.

## Qualification limits

Scoped regressions, workspace Clippy and the upgrade test recorded in the
ledger and the [Answer loop change log](../arbiter/answer-loop-changes.md)
(entries 68 to 71) retain their exact source identities. The installer and
VSIX built from this version are candidates for the user's manual check; no
installed journey, macOS or Linux qualification, model qualification or
publication is claimed by this record. No public upload, deployment, PR or
release publication is authorized.