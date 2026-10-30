# Document and demonstrate verification of a published package against its source

The source review did not verify a distributed VSIX or native archive. Users need a concrete procedure and retained evidence for connecting the exact package they install to its source revision and platform qualification.

## Scope and existing controls

The reviewed source was `c62c1e6936500801dbf41dffdd6e21453e48335d`, a 1.7.2 development candidate, on 2026-10-30. Published channels observed during review described different versions; those dated observations do not establish tampering or a release defect.

[Runtime verification](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/serviceBootstrap.ts#L114) checks payload integrity against the inventory shipped in the same VSIX and treats the distribution as its trust source. A package and its matching inventory are not independent authenticity evidence.

The repository already has [candidate manifests and producer checks](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/docs/ci-and-builds.md#L76), signing/release workflows and platform tests. Its documentation explicitly says a build manifest is not a signature or complete reproducible-build attestation. This issue requests verification of those controls on an actual published artifact, not an assumption that they are absent.

## Investigation completion criteria

- **AC1:** Select one exact public release/platform artifact and record its channel, version, cryptographic digest, publisher/signature status where applicable, and immutable source/build identity.
- **AC2:** Demonstrate the existing verification chain from trusted release/producer evidence through the archive/VSIX inventory to the installed payload. Explain the trust root and any unverified link; an internally consistent checksum alone does not close the chain.
- **AC3:** Record which relevant install, startup, upgrade and uninstall tests actually ran on that platform/version. Separate source tests, candidate tests and signed installed-artifact tests.
- **AC4:** Publish a reproducible verification procedure and evidence summary, with a clear conclusion and separately tracked unresolved gaps. Do not claim bit-for-bit reproducibility or universal platform qualification unless demonstrated.

No new attestation technology or build architecture is prescribed. If current controls establish the required chain, documenting that result can close the investigation.
