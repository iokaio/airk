<!-- SPDX-License-Identifier: Apache-2.0 -->

# rustix 1.1.6 security update source review, 11 November 2026

Status: targeted review and one limited exact-version exception, reviewed by
the primary agent at the user's request ("Update the rustix dependency", 11
November 2026). No cargo-vet certification was created. The `safe-to-deploy`
exemption is an explicit policy exception with the limits below, not a full
audit, human approval or publisher trust.

## Why

RUSTSEC-2026-0486 (GHSA-jh2m-fhpj-3vr7, published 27 October 2026) reports
undefined behavior in rustix 1.1.5: the extended-attribute readers
(`getxattr`, `lgetxattr`, `fgetxattr`, `listxattr`, `llistxattr`,
`flistxattr`) marked as many bytes initialized as the system call reported even
when the buffer had zero length, which the OS treats as a size query. Patched
versions: `>=1.1.6`. AIRK reaches rustix only through `tempfile` on Unix targets
and does not call these functions directly, but the dependency audit and
cargo-deny correctly refuse the unsound version without a disposition.

## Identity

[The archive inventory](rustix-1.1.6-2026-11-11.json) records the checksum,
length, path-sorted packaged-file tree digest, manifest digests, license file
digests and unverified packaged VCS metadata. The archive downloaded from
`static.crates.io` matches the `Cargo.lock` SHA-256
`e5e76742e5294153d4dde1207dbec5bdd81eef6fabdc0d2118babadabfd858aa`. Every
member has the expected `rustix-1.1.6/` prefix, no parent traversal or absolute
path, and no links; extraction was confined to the ignored
`artifacts/security-rustix-2026-11-11/`. These bytes identify the reviewed
sources, not the publisher. Version 1.1.6 was published on 27 October 2026,
more than two weeks before this review.

| Package | Version | Declared license |
| --- | --- | --- |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |

## Lockfile change

Only the rustix entry's version and checksum change. Running
`cargo update -p rustix --precise 1.1.6` also moved seven unrelated crates from
`windows-sys` 0.61.2 to 0.52.0 or 0.60.2, so that resolution was discarded.
rustix 1.1.6 accepts `windows-sys >=0.52, <0.62`, and every other locked
dependency already satisfies its raised minimums (bitflags 2.13.2 ≥ 2.9.2,
libc 0.2.190 ≥ 0.2.182, linux-raw-sys 0.12.1). `cargo metadata --locked`
accepts the two-line change.

## Targeted review of the 1.1.5 → 1.1.6 diff (30 files)

- `src/fs/xattr.rs`: the fix. A zero-length buffer now reports an empty result
  instead of assuming initialization; size queries move to new
  `*getxattr_query`/`*listxattr_query` functions that pass a null buffer.
- `src/net/send_recv/msg.rs`: control-message lengths are clamped to the
  remaining buffer (macOS can report untruncated `cmsg_len`), the payload length
  uses `checked_sub`, and file-descriptor payloads round down to whole
  descriptors before `from_raw_parts_mut`.
- `src/types.rs` (new) and kernel structure definitions: padding fields become
  a zero-initializing `MaybeUninit` wrapper copied from libc.
- `src/backend/linux_raw/fs/types.rs`: `statvfs` flag constants are corrected
  to the `ST_*` values instead of `MS_*`.
- Remaining files: signatures and documentation following the above;
  `Cargo.toml` raises dependency minimums and removes one feature passthrough.
- `build.rs` is byte-identical to 1.1.5. No new dependency, build hook or
  network access was found.

Not reviewed exhaustively: every unsafe block in the crate, platform backends
AIRK does not build, and the correctness of the new constants against every
kernel.

## Disposition

- `supply-chain/config.toml`: the 1.1.5 exemption is replaced by an exact
  1.1.6 exemption that cites this review.
- `supply-chain/source-policy.json`: the locked identity is updated to 1.1.6.
