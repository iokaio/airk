<!-- SPDX-License-Identifier: Apache-2.0 -->

# Candidate MCP dependency security review, 9 November 2026

Status: dependency and source review complete; fresh dependency audit, legal
bundle, scoped shipping Clippy and the corrected MCP regression batch pass.
Installed runtime/package/platform qualification remains separate and pending.
Owner: security task, integrated and validated by the primary agent.

The user approved the narrow D8 exception on 9 November 2026: modern MCP catalog
replies may add `ttlMs: 0` and `cacheScope: "private"`. Tool-call results,
digest inputs and persisted formats retain their contracts. The primary agent
reviewed the complete dependency/API/test/policy diff and the SDK dispatch,
schema-cache removal and build hook independently of the implementing agent.
Exact-version source exceptions remain limited review decisions, not full audits.

## Findings and invariant

At task entry the lock selected rmcp and rmcp-macros 3.2.0. Official RustSec records
[RUSTSEC-2026-0461](https://rustsec.org/advisories/RUSTSEC-2026-0461.html)
(server tool-schema cache, fixed in 3.6.0),
[RUSTSEC-2026-0457](https://rustsec.org/advisories/RUSTSEC-2026-0457.html)
(OAuth SSRF, fixed in 3.7.0), and
[RUSTSEC-2026-0462](https://rustsec.org/advisories/RUSTSEC-2026-0462.html)
(HTTP client response buffering, fixed in 3.9.0). The implemented exact 3.9.0
update covers these three published affected ranges without an advisory waiver.

The targeted lock update completed with Cargo 1.98.0 after sandboxed network
access failed and the authorized escalation succeeded. It updated only the SDK
pair and added sha2 0.11.1, block-buffer 0.12.1, const-oid 0.10.2,
crypto-common 0.2.2, digest 0.11.3 and hybrid-array 0.4.15. Lock ownership was
returned to the primary agent before synchronizing the candidate version.

AIRK disables SDK default features and enables server, macros, transport-io,
transport-streamable-http-server, and client. Client is used for local/duplex
canonical-handler calls; neither auth nor the built-in reqwest/Unix-socket HTTP
client transport is requested. The server-cache path is enabled. The CLI HTTPS
gateway authenticates and revalidates native scope before HttpReadService,
which bounds requests, sessions, authority, origin and native admission.
Authentication reduces exposure but does not justify retaining the SDK cache.

The correction updates dependency identities and the deprecated SDK type alias.
Apart from the approved modern catalog fields, AIRK's wire/schema
fixtures, canonical result data, full retry receipts, authority isolation,
bounded native admission and accepted-work lifetime must remain unchanged.
There is no persisted-state or digest-contract change. The SDK adds `ttlMs: 0`
and `cacheScope: "private"` to completed modern catalogs (2026-07-28+); legacy
catalogs and tool-call replies retain their field shape. The user approved this
narrow D8 exception on 9 November 2026. No fields are silently removed.

## Compatibility review and validation

The [official 3.9.0 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.9.0)
includes stateful worker cancellation, protocol reply cancellation, HTTP media
type/Accept handling and output-schema generation changes. Existing tests cover
legacy and inline negotiation, session isolation, DELETE/expiry, native refusals,
abandoned callers, request-local headers and canonical complete results.
All 171 MCP library tests passed against the updated SDK in the primary's
corrected 182-test batch; this is observed component evidence.

The primary ran scoped CLI/MCP shipping-feature Clippy through
`scripts/dev-check.py` with the established P10 target and retained timings,
then the named pinned Nextest selections. The corrected compilation and
regression batches passed on unchanged inputs. Actual CLI/MCP/HTTPS process
integration, service clients and installed tests remain separate identified
runtime/artifact gates. Existing fixture bytes remain the goldens.

Cargo-vet's 3.2.0 exemptions are an unreviewed historical baseline, not coverage
for 3.9.0. Their replacements and the six new dependency identities received
exact archive/source review and the primary's independent limited policy review.
No audit certification or automatic exemption was created. The fresh dependency
audit and release legal-notice bundle subsequently passed; they do not replace
runtime, package or installed qualification.

The [source review](../../supply-chain/reviews/rmcp-2026-11-09.md) and its
exact archive inventory cover all eight changed dependency identities. All
archive checksums matched Cargo.lock; regular-file inventory and packaged
license/manifest hashes were recorded. Source-policy and limited exact-version
exemptions received independent primary review, with no audit certification.
Raw downloaded source remains untracked in `artifacts/`.

New regressions (both passed in the corrected batch):

- `lifecycle::tests::catalog_cache_hints_are_modern_only_and_do_not_change_tool_results`;
- `remote::http::tests::modern_http_schema_validation_rechecks_unknown_and_changed_tools`.

The latter checks unknown → annotated → absent catalog transitions through the
actual SDK HTTP transport, including HTTP/error-code receipts and refusal before
canonical execution. It would expose the old persistent negative schema cache.
The corrected regression passes with SDK 3.9.0. The vulnerable 3.2.0 behavior
was traced in source and reported upstream; no local execution of this new
regression against 3.2.0 is claimed.

## First focused batch and correction

The primary's first focused library batch selected 204 tests: 199 passed and
five failed. All existing MCP tests and the modern catalog-hint regression
passed; the new HTTP schema regression failed on its annotated case, expecting
`-32020` but receiving `-32602`. This was a test-fixture defect, not evidence
of a production failure. The SDK's `is_legacy_request` calls
`validate_request_protocol_version_meta` before schema-header validation when
AIRK's legacy session support is enabled. The fixture omitted body protocol
metadata, so it never reached the cache lookup under test.

The coherent fixture correction supplies the matching body protocolVersion,
retains absent clientCapabilities to refuse later without native execution,
and observes catalog lookups on every request. Unknown/removed catalog cases
still require HTTP 400/`-32602`; a contradictory annotated header still requires
HTTP 400/`-32020`. Expectations were not weakened. No production source change
was needed. After collecting the complete first batch, the primary reviewed the
coherent correction and ran the corrected batch: all 182 selected tests passed,
including all 171 MCP library tests and both new regressions. The original
failed result remains retained evidence.

The canonical license generator scans each newly resolved crate's packaged
LICENSE before any retained fallback. Both 3.9.0 SDK archives contain the
12,227-byte LICENSE with SHA-256
`0382b0057770ca05e9c350a50aa3b1c1fea84da0bc81d723bf00b9aa841be58a`.
The 3.2.0 pinned fallback records are retained historical source identities;
they are not substituted for new archive license bytes.

## Evidence

Source review: Cargo workspace and component feature manifests; MCP bounded HTTP
adapter, stdio lifecycle and their tests; CLI HTTPS authentication/dispatch flow;
official advisory pages and release/source manifest. The delegated security
task ran no compile/test/scanner; primary-owned execution results are recorded
in the [release-readiness ledger](release-readiness.md#collected-evidence).
Fresh audit and legal gates pass; all 171 MCP
library tests, including both new regressions, pass in the corrected 182-test
batch. Receipts: `artifacts/readiness-1817/dependency-audit-report.json`,
`license-notices.json`, `fixture-clippy.json` and `fixture-regressions.json`.
The legal bundle includes 431 third-party packages and the packaged SDK 3.9.0
licenses. Existing dated advisory and publisher-declaration dispositions remain
explicit. Actual installed runtime and final artifact gates remain separate.
