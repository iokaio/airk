# Dependency source review and cargo-vet baseline

Recorded 3 November 2026 from entry revision `7567bdc`, with the existing
1.8.8 Cargo dependency graph unchanged. This is an explicit adoption baseline,
not a certification that existing dependencies are safe. No AIRK-local audit,
reviewer signature or publisher-wide trust entry was fabricated.

The tool is [Mozilla cargo-vet 0.10.3](https://github.com/mozilla/cargo-vet/releases/tag/v0.10.3).
`tool.json` pins the official release archive URL and SHA-256 for each supported
host. The Windows tool was downloaded to ignored `artifacts/tools/cargo-vet/`,
verified against the official release digest, and executed successfully. It changes
no global Cargo installation or configuration. Other-host archive pins are recorded;
their installation is not established by the Windows result.

## What the two checks enforce

`guard.py` compares the complete lockfile with `source-policy.json`. Every
external package must retain its approved exact name, version, source URL/revision
and registry checksum. A new package, registry checksum replacement, custom
registry, Git URL/revision change or unknown local/path package refuses until its
source identity receives explicit review. The first-party allowance names only
the eight AIRK workspace packages at their recorded manifests. Synchronized AIRK
version increments do not automatically admit any new third-party source.

Cargo-vet then checks the audit criteria against the entire workspace's default
**all-features, unfiltered-target** metadata graph. This includes dependencies
conditional on Windows, macOS, Linux and other targets; it is not proof those
targets built or ran successfully. Keep its all-feature default and do not add
graph or target filters to make a check pass. Cargo-vet already invokes locked
Cargo metadata; adding a duplicate `--cargo-arg=--locked` fails with this version.

Use the pinned executable through Cargo (the direct executable requires Cargo's
`CARGO` environment contract). On Windows, add only the ignored tool directory to
this terminal's PATH, then run:

```console
python supply-chain/guard.py --lockfile Cargo.lock
cargo vet check --locked --no-minimize-exemptions --no-registry-suggestions --cache-dir artifacts/tools/cargo-vet/cache --output-format=json
```

`--locked` freezes imported audit records; `--no-minimize-exemptions` prevents
automatic exemption cleanup. The audit runner must verify input hashes before
and after, and refuse any mutation. Add `--frozen` only when deliberately using
offline cached evidence. A missing tool/cache or invalid result is unavailable,
not a clean review. The initial setup/import/format commands were deliberate
baseline preparation; they are not part of routine checks.

For a new/changed dependency, record the source review and update only its
justified `source-policy.json` identity. Cargo-vet must also have an applicable
trusted audit/import, a genuine full/delta review by its actual auditor, or an
explicit reviewed exception with rationale. Never run `regenerate exemptions`,
disable a source policy, add broad publisher trust or invent a `certify` entry
merely to turn a failure green. Changes to imports, criteria, source approvals
and exemptions belong in the same reviewed diff as the dependency change.
Retiring stale approvals can be a separate reviewed cleanup; this checker only
reports them and never rewrites the baseline.

## Existing coverage and remaining backlog

The following counts describe the **3 November 2026 adoption baseline**, not
the current 1.8.14 dependency graph. The later [7 November native HTTPS source
review](reviews/native-https-2026-11-07.md) admits 54 exact source identities and
54 explicitly limited reviewed exceptions, with per-package rationale and
evidence. It does not turn baseline exemptions into audits or certify current
advisory/platform qualification.

The [original-configuration TOML review](reviews/toml-config-2026-11-07.md)
records six further exact-version exceptions and their targeted parser/cache
evidence. Primary independent acceptance is pending; these are not full audits
or a replacement for advisory and actual native parser qualification.
The later [rustls 0.23.46 security-patch delta review](reviews/rustls-0.23.46-2026-11-07.md)
replaces only its active 0.23.45 source identity and limited exception after four
fresh advisories were found. The initial 54-package record remains historical;
the patch review does not waive advisory or runtime validation gates.

The actual default cargo-vet graph has **374 nodes**: eight first-party workspace
packages and **366 third-party package versions** (364 registry, two Git).
The locked baseline result is:

| Cargo-vet result | Package versions | Meaning |
| --- | ---: | --- |
| `vetted_fully` | 46 | Requirements satisfied by the selected trusted imported audit paths |
| `vetted_partially` | 1 | `hashbrown` 0.17.1 uses an imported delta based on exempt 0.16.1 |
| `vetted_with_exemptions` | 319 | Exact existing versions remain explicitly exempt from missing review |

There are exactly **319 exemption version entries across 308 crate names**.
The partial version's exempt base is already one of those entries, so it does
not create a 320th entry. **320 current versions depend on baseline exemption
evidence in whole or part**. Every exemption identifies itself as a mechanically
recorded, unreviewed existing dependency. Passing the adoption gate therefore
does not mean 366 dependencies were audited or independently reviewed by AIRK.

Both upstream Git packages are included as third-party with precise Git-version
exemptions, not silently excluded as first-party:

| Package | Existing pinned identity | Review status |
| --- | --- | --- |
| `libsqlite3-sys` | `0.38.1@git:2a71e35d94b2a02f7dd4a0c4cfd59c339370e747` | Unreviewed baseline exception |
| `rusqlite` | `0.40.1@git:2a71e35d94b2a02f7dd4a0c4cfd59c339370e747` | Unreviewed baseline exception |

Their `audit-as-crates-io = true` setting requests cargo-vet's exact Git delta
enforcement; it does not assert that the forks equal published crates. A registry
audit alone cannot qualify the changed source. The independent source guard also
binds the complete `git+https://github.com/rusqlite/rusqlite?rev=â€¦#â€¦` lock identity,
including the URL and commit even when the crate version stays unchanged.
See [cargo-vet's Git dependency semantics](https://mozilla.github.io/cargo-vet/first-party-code.html).

## Trusted imports and provenance

The three selected sources appear in the
[official cargo-vet registry](https://github.com/mozilla/cargo-vet/blob/v0.10.3/registry.toml).
They are explicit project trust choices: Mozilla maintains cargo-vet and publishes
shared audits, Google publishes its dependency reviews, and Bytecode Alliance
publishes Wasmtime's native/runtime reviews. Only the built-in `safe-to-run` and
`safe-to-deploy` criterion semantics are accepted implicitly; no weaker custom
criterion is mapped to production safety. `safe-to-deploy` implies `safe-to-run`;
these are scoped audit criteria, not an assertion of complete correctness.
See [criteria](https://mozilla.github.io/cargo-vet/built-in-criteria.html) and
[import semantics](https://mozilla.github.io/cargo-vet/importing-audits.html).

| Peer | Pinned source revision | SHA-256 of fetched audit file |
| --- | --- | --- |
| Mozilla | `4716047f88d72d0685224af330bf21d0c2435b69` | `77617e6e8d24664ed21af86b9e112489e37740280fafc293bc3a417449e87a4b` |
| Google | `8cec794c38751495cf7a6c083220c207af1303d6` | `eb1c1bf1ee616471bfb56965eea325815ee41b5e3fcf5d12537dff2791160bbd` |
| Bytecode Alliance / Wasmtime | `b112f2f24a58492c45cd958c2a8aa986df9e0634` | `2eef9862fa9182294a6b6f43b2779317614f751c557b22cc3e459d8d311276c3` |

`config.toml` contains the immutable raw URLs; `imports.lock` retains the selected
foreign records and their original attribution. It contains 93 explicit audit
records and nine crate/publisher/date-bounded upstream wildcard records, not a
new AIRK-wide publisher trust declaration. Accordingly `vetted_fully` means a
satisfied trusted audit path, not necessarily a new manual review of every byte
of that version by AIRK. No local `[trusted]` or wildcard audit was added.

The baseline was prepared with `cargo vet init`, then explicit `cargo vet import`
for each peer's immutable URL, all with the isolated `--cache-dir` above and no
dependency upgrades. A subsequent `cargo vet fmt` normalized the generated store.
The generated `config.toml` and `audits.toml` use cargo-vet's canonical headers:
its formatter removes custom SPDX comments and its locked checks reject them.
These two exact generated data files need the repository's narrow canonical-header
check; original Python/TOML source retains the normal SPDX rule. Keep imports
UTF-8 and the generated store LF-terminated on all platforms.

An isolated copied-store negative test removed only the `adobe-cmap-parser` 0.4.1
baseline exemption. Locked checking refused with `conclusion = "fail (vetting)"`
and missing `safe-to-deploy`; no project audit files were changed. Windows reports
the tool's nonzero exit as `-1`/`4294967295`, so consumers must classify the JSON
conclusion rather than assuming a policy failure always exits 1. Source-guard
regressions separately refuse new packages, same-version Git revision/URL changes,
checksum changes, malformed metadata and escaped local identities.

This gate complements advisory/license checks. It does not inspect npm packages,
downloaded ONNX binaries, model weights, signed distribution artifacts or private
source content, and it does not replace vulnerability scanning or the existing
release qualification matrix.
