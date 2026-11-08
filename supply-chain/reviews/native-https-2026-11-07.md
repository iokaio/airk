<!-- SPDX-License-Identifier: Apache-2.0 -->

# Native HTTPS dependency source review, 7 November 2026

The primary independently reviewed and accepted **54 limited exact-version
exceptions** for the 1.8.14 native TLS/HTTP dependency change. These are explicit
exceptions to missing cargo-vet audit coverage, not full `safe-to-deploy` audits,
invented human approvals, publisher trust or current advisory clearance. The
3 November adoption baseline and its existing exemptions remain historical and
unchanged. Imported audits and `audits.toml` were not changed.

The [durable exact inventory](native-https-2026-11-07.json) records all 54 lock
identities, registry checksums, archive sizes, packaged-file tree digests,
normalized/original manifest digests, license/notice hashes, declared repository
and unverified packaged VCS metadata, build/procedural-macro execution and each
individual exception's rationale and limits. Its SHA-256 is
`c808069298d6bf3c42802fdb8b5b7e2dd8c2566afe6191db49ed3a5c70dbac5d`.
The independently reviewed proposal evidence digest is
`33741ae4555118efe8202c1a90af108f617fd0c33c424a9c6b6b785a2cd654e6`.
The Cargo.lock examined has SHA-256
`390e7352fc2235caafbd3046dc3f2c8ef7202f469fb1792c4fde9ff5d3ac38f2`.

All **12,023,230 archived bytes** match their exact lock checksums. All **2,035
regular packaged files** match the extracted local registry cache byte for byte;
the archives contain no links. Tree digests hash canonical JSON (UTF-8,
`sort_keys=True`, separators `,`/`:`) of path-sorted `{path, bytes, sha256}` rows.
Registry-byte identity does not authenticate publisher intent or prove a clean
upstream source checkout. No Cargo, network fetch, native runtime, advisory
research or platform build was performed for this source review.

Review covered manifests, provenance and license texts/notices; build scripts
and procedural-macro entry points; external I/O/process/environment surfaces;
target-specific linking; and targeted unsafe, TLS, HTTP, ASN.1 and time code.
TLS and HTTP groups received separate technical review followed by the
primary's review of the combined proposal. This is not exhaustive line-by-line
verification, cryptographic validation, a memory-model proof or legal approval.

Material limits retained in the exact exceptions:

* **try-lock 0.2.5 has a deprecated safe API documented as unsound for weak
  memory orderings.** The sole locked dependent is want; both inspected want
  calls use checked `try_lock_explicit(SeqCst, SeqCst)`. No deprecated callers
  were found in AIRK, Hyper or want. Acceptance is conditional on this checked
  reachable use, and cannot represent crate-wide soundness. Revisit this
  exception if dependencies or calls change.
* ring, http and http-body package VCS metadata report `dirty: true`. Their
  registry checksums establish the admitted bytes, not a clean Git rebuild.
  Ring compiles C/ASM and can consume shipped pregenerated objects; optional
  regeneration runs Perl/NASM. Eight Windows target crates link packaged
  binary OS import libraries. These were not independently regenerated.
* httparse probes the configured compiler version; oid-registry generates Rust
  from its fixed packaged database. Procedural macros execute in the compiler.
  No autonomous downloader was found in inspected new build scripts.
* RCGen private DER clones are not automatically zeroized with AIRK's declared
  features. Protected files do not prove erasure of all heap copies. rustls
  offers a secret-file logger that AIRK does not install. HeaderValue debug
  redaction is opt-in, not a default property of bearer headers.
* SSE buffering has no intrinsic byte limit. Plain body collection/size hints
  do not enforce budgets. Caller header/body/event/deadline/concurrency limits
  remain necessary. Optional Hyper proxy/client, HTTP2 and C-FFI paths are not
  qualified by the selected HTTP1/Tokio declarations. Manifest observations do
  not replace full Cargo feature resolution.
* A generic DER tagged combinator explicitly resets depth and ignores trailing
  bytes. Default parser recursion limits are not universal guarantees. Native
  owned CA inputs are separately bounded; parsing is not chain validation.
* Base64 defaults include unsafe SIMD. Time, formatting, bounded integers and
  other packages use unchecked range/layout operations. No blanket unsafe-free,
  cross-target soundness or constant-time claim is made.

Declared licenses are permissive MIT/Apache alternatives, MIT-only,
BSD-3-Clause, ISC, or ring's Apache-2.0 AND ISC. Preserve specific copyright
notices, ring's nested licenses and atomic-waker's third-party notice.
asn1-rs-impl declares MIT OR Apache-2.0 but packages no standalone license text;
its declaration is not independently authenticated upstream permission, and
its sibling packages' texts do not establish that its archive contains them.

No additional inspected finding established an active unacceptable AIRK path.
That limited conclusion is not a security clearance. Both source guard and
locked cargo-vet must still pass independently; final advisories, license
distribution, feature resolution and native/platform qualification remain
separate gates. No new cargo-vet audit attribution was created.
