<!-- SPDX-License-Identifier: Apache-2.0 -->

# rustls 0.23.46 security patch source review, 7 November 2026

The primary independently reviewed the complete proposed delta and critical
changed source, and accepted a **limited exact-version exception** replacing
rustls 0.23.45 with **0.23.46**. This is not a full cargo-vet audit, advisory
waiver, human signature, cryptographic proof or cross-platform qualification.
The [initial 54-package review](native-https-2026-11-07.md) remains an unchanged
dated record of the earlier .45 source admission, before fresh advisories were
fetched. Its absence of advisory clearance remains material.

The [exact delta inventory](rustls-0.23.46-2026-11-07.json) records all 119
packaged-file hashes,25 changed-file before/after hashes, exact old/new source
identities and accepted exception rationale. Its SHA-256 is
`82850a56477104d8aca7e2d35d0ff1b29c1bd2afa02670c7e365bd7fef758c27`.
Accepted proposal digest:
`8234c6214d7c374efb7a0dd0011c74e4e9c09e013d8db893d646f689031e101d`.

The official registry archive is 386,227 bytes, SHA-256
`48e13bd8c0e9365c43cfa5c9e8f9ad49d3c8444926c9aac819e0e4dc503c8fdf`,
matching the independently fetched crates.io index row (not yanked). All 119
paths are regular files, no archive links, same path set as .45. Packaged VCS
metadata names f04804fa13341a9b1dc02b5d1dd8a746f2c4b7fb without a dirty flag;
this does not independently reconstruct or authenticate that upstream commit.
Dependency/feature declarations, Rust 1.71 minimum, build.rs and all three license
texts are unchanged; normalized Cargo.toml differs only in package.version.
The archive's developer lock changes itertools and its own version, separate
from AIRK's resolved lock. AIRK's accepted lock digest is recorded in JSON.

All 2,373 lines of the complete 25-file packaged delta were reviewed, with a
separate client-side review independently regenerating its seven module deltas. No
new unsafe, filesystem, environment, subprocess or autonomous downloader was
found in added diff lines; no complete soundness or hostile-publisher proof is
asserted. The Apache/ISC/MIT licenses and original notices remain applicable.

The [official 0.23.46 release](https://github.com/rustls/rustls/releases/tag/v%2F0.23.46)
patches all four fresh RustSec findings:

* **RUSTSEC-2026-0346:** explicit client/server session Debug implementations
  omit secret/master_secret, with recognizable-secret regression assertions.
  Tickets, certificate metadata and application_data can still appear; this
  does not make arbitrary Debug logging confidential.
* **RUSTSEC-2026-0347:** ECH retry acceptance is retained consistently; reject
  contradictory final acceptance and handle ECH failure before enabling
  application traffic. New tests cover both retry-decision directions.
* **RUSTSEC-2026-0348:** Brotli strict state rejects large-window extension and
  requires exact input/output consumption. Zlib similarly rejects trailing
  input. Added tests cover oversized window and trailing data.
* **RUSTSEC-2026-0349:** move client keyshare-hint publication from unauthenticated
  ServerHello processing to after certificate/signature markers and constant-
  time Finished verification. No new direct hint-poisoning end-to-end regression
  was found in the changed packaged modules; cache eviction tests do not prove
  that scenario was exercised.

Additional reviewed behavior is relevant to native TLS: multi-record sequence
preflight and TLS 1.3 key-update completion; retained fatal errors on
write/write_vectored/flush; exact configured session-cache capacity; rejection
of encrypted ChangeCipherSpec; first-offered-PSK-only early-data acceptance;
client certificate request-context validation; buffered plaintext availability
and complete transport flush. AWS-LC HPKE only consumes its opening nonce after
successful authentication, accompanied by a failed-AAD regression; that provider
is outside AIRK's direct ring selection. These are substantive I/O/protocol
changes, not exclusively disabled-feature patches.

AIRK's production endpoint uses server ring/std/tls12 with no client certificate
authentication. No custom client session store, ECH configuration or Brotli
selection was found in direct AIRK declarations. These observed path limits do
not waive the advisories. Keep normal CA/hostname validation, no secret logging,
explicit caller deadlines and byte/concurrency bounds. Complete resolved feature
selection and actual TLS/gateway tests remain required.

No Cargo build, upstream test execution, native runtime or fresh advisory rerun
was performed by this review. Root owns those validation gates. Updating the
exact source identity and reviewed exception does not itself establish scanner
clearance. Existing imported audits, local audit store and all other exact
exceptions remain unchanged.
