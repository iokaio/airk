# webpki-roots 1.0.9 source review

Status: exact source review independently accepted by Root on 15 November 2026. Subsequent actual cargo-vet R109 failed only this package. Corrected semantics and the following limited exact-version exception proposal require separate Root acceptance. No full safe-to-deploy audit, human auditor attribution or local publisher trust is claimed.

## Exact source and corrected audit-coverage assessment

Admit only `webpki-roots` 1.0.9, crates.io registry, checksum `7dcd9d09a39985f5344844e66b0c530a33843579125f23e21e9f0f220850f22a`. The cached 217462-byte archive and all twelve packaged files match the retained local provenance inventory, with no links. Packaged VCS metadata identifies commit `0a553dbc8b3f18ea05c4f881cffa3f2d005d0d30`, path `webpki-roots`; upstream release v/1.0.9 names that commit. VCS metadata alone is not publisher authentication.

The retained original [official crates.io API response](https://crates.io/api/v1/crates/webpki-roots/1.0.9) independently supplies that exact checksum, non-yanked state, publication `2026-07-18T09:30:44.802479Z`, and publisher user ID 2751 (`ctz`). Its local SHA is `e096ef49175f0e37c852f938b52262b99332cc84a0b1d0cd5b401108df17124f`.

The pinned [Mozilla import](https://raw.githubusercontent.com/mozilla/supply-chain/4716047f88d72d0685224af330bf21d0c2435b69/audits.toml), SHA `77617e6e8d24664ed21af86b9e112489e37740280fafc293bc3a417449e87a4b`, contains a publisher2751/date-matching `trusted.webpki-roots` heuristic. The original review incorrectly treated that imported trusted entry as usable audit coverage. Actual R108 import refresh and R109 locked check disproved this: only webpki-roots1.0.9 lacks safe-to-deploy. Version0.10.3 [resolver.rs](https://raw.githubusercontent.com/mozilla/cargo-vet/v0.10.3/src/resolver.rs) takes usable trusted entries solely from local `store.audits.trusted` (lines1061-1067), while imported trusted entries inform suggestions (lines1608-1614). A trusted entry is not an imported wildcard audit. No manual imports.lock rewrite or fabricated foreign audit can legitimately supply that missing path.

Bytecode Alliance's 1.0.7-to-1.0.10 delta does not directly audit intermediate1.0.9. No applicable imported full/delta/wildcard audit has been established. Existing policy expressly permits reviewed limited exact-version exceptions. Proposed fallback: one exception for this exact1.0.9 based on the bounded source review below, with all scope limits explicit; no local publisher trust, no new import, no invented auditor and no broader version range. This needs Root approval and a subsequent actual locked cargo-vet check.

No previously AIRK-reviewed webpki-roots version exists in the current store. Cached1.0.0/1.0.5 are not reviewed delta bases. This review instead inspects the exact current source without claiming an applicable imported audit. The [upstream release](https://github.com/rustls/webpki-roots/releases/tag/v%2F1.0.9) describes four added Telia/SECOM roots and removed Atos TrustedRoot2011, plus lock update. Those release statements explain the update; they do not establish independent CA trustworthiness.

## Source and use assessment

The only production Rust source is a no_std, unsafe-forbidden static `TLS_SERVER_ROOTS` array of121 TrustAnchor values; no executable functions, network, process, filesystem, build script, proc macro or unsafe code. Cargo.toml disables build scripts and has only normal dependency `rustls-pki-types` with defaults disabled. Root data's CDLA-Permissive-2.0 notice requires making its agreement available when sharing; preserve LICENSE in dependency notices. No license exception is proposed.

All121 embedded PEM fingerprints were independently recomputed, unique, and their DER certificate subject and subject-public-key-info compared with the actual compiled byte literals using cryptography. Name constraints remain upstream byte declarations; bounded checks do not establish CA policy, revocation freshness, certificate validation or cryptographic correctness. A historical root's nonpositive serial emits cryptography's deprecation warning; parsing and matching succeed. Trust anchors are not validated as newly issued end-entity certificates by these checks.

Reviewed tests/verify.rs exercises permitted/excluded DNS name constraints with rcgen/webpki and the historical tubitak fixture. Reviewed tests/codegen.rs deterministically fetches CCADB, extracts trust anchors and compares generated source; it writes src/lib.rs then fails on change. It was deliberately not executed against frozen cached source. Dependency dev-tests/generator are not normal AIRK runtime code.

Only AIRK consumer is service_runtime/arbiter/download.rs:47, in the service's explicit optional model asset acquisition. It gives these roots to rustls's normal ring-backed safe-default TLS verifier, and verifies the actual requested hostname. Initial URLs are exact HTTPS GitHub AIRK release paths; only bounded approved CDN redirects are allowed, no credentials/non443 endpoints, and every body has compiled exact length/SHA plus idle/whole deadlines. No custom verifier bypass is added. Source/index authority does not derive from this CA bundle. Existing source tests cover arbitrary URL/redirect refusal and stream hash/length failure without asset publication; they were read, not executed in this review.

Residual limitation: compiled Mozilla roots are static and do not provide native OS root changes, native constraints or revocation data. Upstream documents this; regular security/dependency updates and actual release TLS/resource qualification remain required. The proposed exception documents missing audit coverage; it is not a claim of complete security or platform qualification.

## Proposed delta and bounded evidence

The accepted source-policy addition remains exact. Proposed config delta adds only `[[exemptions.webpki-roots]]`, version1.0.9, criteria safe-to-deploy, with notes explicitly stating review scope and missing full audit. `audits.toml`, imports, policies and all existing exceptions remain unchanged. No publisher trust is added. Root must inspect and accept this single justified exception before mounting it.

`bounded_review_checks.py` completed eight grouped controls: (1)all packaged source hashes; (2)immutable Mozilla audit hash; (3)actual publisher/date/criteria; (4)registry checksum/non-yanked; (5)121 unique root fingerprints and exact certificate subject/SPKI literals; (6)static/no_std/build-disabled/normal dependency inventory; (7)baseline source-policy refusal and proposed exact-policy success; (8)independent wrong-checksum refusal. Guard success covers432 external packages(430registry/2Git),19first-party packages, with3unused historical approvals. No Cargo, model, upstream generator, network TLS test, advisory scan or qualification executed. Original registry API acquisition was read-only HTTPS; the bounded tests are offline.

The original eight grouped checks establish source/provenance and policy behavior only. In particular, publisher/date matching does NOT establish cargo-vet coverage. A new TOML structural control proves the proposed config differs only by the one exact-version exception and preserves imports, policies and existing exemptions. No Cargo was run for this correction.

Root acceptance recommendation: accept this limited exact-version exception on the documented source/use assessment, or retain the failure pending an actual independent full/delta audit. Do not relabel the earlier source checks or Mozilla heuristic as a full audit. Actual locked vet success is still required after any approved mount.


## Root acceptance,15November2026

Root independently reviewed the exact archive/source identity, static trust-anchor implementation, sole bounded hash-pinned download consumer and stated limitations. The one exact1.0.9 limited reviewed exception is accepted under the standing supply-chain policy. This is not a human/full security audit or publisher trust. The prior imported-trust coverage assertion is retracted; actual R109 correctly failed. The unnecessary generated import selection and automatic exemption cleanup were not accepted; original imports and all prior exceptions are restored. A fresh unfiltered locked cargo-vet check remains required.

The subsequent actual R111 check passed with the default all-feature,
unfiltered-target graph and all configuration, audit, import, source-policy and
Cargo.lock bytes unchanged. Command: `cargo vet check --locked
--no-minimize-exemptions --no-registry-suggestions --cache-dir
artifacts/tools/cargo-vet/cache --output-format=json`. Retained evidence:
`artifacts/advanced-split/receipts/cargo-vet-check-r111/{result.json,check.log}`.
The exact-source guard also passed432 external and19 first-party packages.
This result includes the disclosed limited exception; it does not transform it
into a full audit or establish runtime, platform or advisory qualification.
