<!-- SPDX-License-Identifier: Apache-2.0 -->

# Original-configuration TOML dependency review, 7 November 2026

This records targeted source inspection and **six limited exact-version
exceptions**, independently reviewed and accepted by the primary agent. These exceptions
satisfy the missing cargo-vet coverage policy through explicit, limited rationale;
they are not full `safe-to-deploy` audits, human approvals, publisher trust, legal
approval or fresh advisory clearance. No `certify` record was created. Existing
imported audits and the historical adoption baseline remain unchanged.

The [exact review inventory](toml-config-2026-11-07.json) records the locked
registry source/checksum, archive length, packaged-file tree digest, matching
extracted-cache observation, manifest and license hashes, unverified packaged VCS
metadata and individual rationale for:

| Package | Exact version | Declared license |
| --- | --- | --- |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 |
| winnow | 1.0.5 | MIT |

All six cached `.crate` files match their exact Cargo.lock SHA-256 checksums.
All **209 regular packaged files** match their extracted cache bytes; no archive
links or other nonregular members were admitted. File inventories hash canonical
JSON (`sort_keys=True`, separators `,`/`:`) of path-sorted `{path, bytes, sha256}`
rows. The local full observation has SHA-256
`4a60e5f1943cbab190619710dc203512bb1d16091b4ba46537d3e69c1deafc58`.
This demonstrates cached byte identity, not independent authentication of an
upstream publisher or source repository. No new network fetch was performed.
All six normalized manifests explicitly disable build scripts; no procedural
macro or native build step was found. Matching packaged license files are
recorded; compatibility is not a legal opinion.

AIRK pins `toml = "=1.1.6"` with default features disabled and `std`, `serde`,
`parse` requested. The native original-configuration reader supplies at most
1 MiB from an ownership-checked captured file. It parses the entire original
Codex document structurally, selects the original server table and retains its
complete configuration hash. Parser output does not establish executable or
scope ownership; those remain separate native checks. Parse failure uses a fixed
message rather than printing configuration or credential contents.

Targeted findings and limits:

- `toml`, `serde_spanned`, `toml_datetime` and `toml_writer` forbid unsafe code.
  The span serde visitor rejects duplicate/missing fields; a span is inert
  metadata, not a source-read capability. Date/time parsing uses fallible numeric
  conversion, field-width/range checks and exact input consumption. Internal
  parser invariants and panic freedom are not proved exhaustively.
- `toml` uses `RecursionGuard` with limit 80 for arrays/inline tables and checks
  dotted-key path depth. The `unbounded` feature is not requested. The lexer
  capacity estimate divides before multiplying and caps addressable capacity.
  Tokens and event vectors still amplify input memory; 1 MiB is a caller byte
  limit, not a hard parser allocation/time budget. No fuzz, worst-case complexity
  or denial-of-service certification is asserted.
- `toml_parser` gates unchecked source slicing, lexer consumption and string
  decoder operations behind its explicit `unsafe` feature. AIRK's configured
  dependency does not request that feature. The inspected checked branches use
  `get`/`split_at`; ASCII delimiter/UTF-8 span handling and event-receiver nesting
  refusal were examined. This does not certify arbitrary unsafe-feature callers.
- `winnow` exposes unchecked stream APIs whose contracts require valid bounds
  and UTF-8 boundaries. The inspected safe slice/token paths use checked offsets,
  `split_at`, `split_first` and `chars`; `TokenSlice` delegates unchecked methods
  without weakening their preconditions. Its full combinator implementation and
  every unsafe call site were not audited. Optional debug code reads `COLUMNS`
  and prints diagnostics; AIRK does not request debug. No filesystem, network,
  process launch or FFI call was found in the inspected library-source scan.
- `toml_writer` is present in the lock graph but AIRK does not request TOML's
  `display` feature. Its inspected formatter writes through caller-supplied
  formatting traits and escapes special/control characters. No complete
  formatting round-trip or resolved-feature-unification proof is asserted.

Only the six exact identities were added to source policy and cargo-vet
exceptions. A future identity or expanded unsafe/unbounded/debug feature use
needs renewed review. This review ran no Cargo build, upstream code/tests,
cargo-vet, fresh advisory check or platform runtime. The primary independently checked the native entrypoint, configured features,
source-policy diff, recursion bound and checked/unsafe stream boundaries. Final
dependency and platform validation gates remain separate.
