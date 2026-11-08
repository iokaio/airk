# Native signing and macOS handoff

Signing is opt-in and operates on a fresh copy under ignored `artifacts/`. It does not import certificates, export keys, change the original build or publish a package. `scripts/sign-native.py` only writes a success receipt after OS signature verification succeeds. The native packager accepts `--signed-dir` and re-verifies the signature, source binary digest, target, commit and dependency lock before including the signed copy.

## Current 1.8.14 signing contract

The unpublished modern runtime has one main executable, with embedded default
MiniLM/tokenizer assets and statically linked CPU ORT. On Mac it stays inside
`AIRK.app/Contents/MacOS/airk`. Sign the tested schema-2 runtime through
`--runtime-dir`; a bare no-feature `cargo build` is not the modern distributable.
The builder's development ad-hoc signature establishes no Developer ID or Apple
notarization trust. Release signing verifies the exact main and enclosing app,
Developer ID/team, hardened runtime and timestamp, then notarizes/staples the app
and reseals the complete external inventories. There is no modern embedding
executable, dynamic ORT library or separately copied launcher image to sign.
Legal notices, static startup resources, tickets and complete native/VSIX
verification remain required. See [the Mac build guide](macos-build.md).

Static ORT legal texts use bounded packaged filenames under
`notices/static-onnx-runtime/<component>/LICENSE-<source-path-sha256>.txt`.
Their schema-2 inventory preserves every original upstream path, byte count and
content hash, plus the producer receipt and dependency-lock hashes. Packaging
and release verification use the same mapping and complete dependency-coverage
checks. This keeps deep upstream documentation paths within the unchanged
native install path limit without omitting or modifying any legal text.

### Historical schema-1 layouts

Versions 1.8.8–1.8.13 carried an independently signed
`AIRK.app/Contents/MacOS/airk-launcher-image` alongside the main/helper and dynamic
inference library. Only that inventoried standalone image could be copied out;
the app-bound main depended on its resource envelope. Their signing and
verification checked the exact copied bytes, Developer ID/team/runtime/timestamp
and `--version` before recording `copied_launcher`. Earlier ad-hoc helper/library
load fixtures cover their own library-validation boundary, not modern static
inference. Historical universal-launcher publication/retirement uses its private
lock and exact image/pointer ownership; unknown bytes remain preserved during
migration. Those records and older inventories stay valid only for their
original inputs. See the [dated build guide](reviews/macos-build-before-static-main-2026-11-07.md)
and [implementation ledger](research/implementation-v190.md).

On Windows, provision a trusted code-signing identity in the current user's certificate store, then run:

```powershell
python scripts/sign-native.py --target x86_64-pc-windows-msvc --runtime-dir artifacts/runtime/x86_64-pc-windows-msvc --identity CERTIFICATE_THUMBPRINT --timestamp-url https://YOUR_RFC3161_SERVER --output artifacts/signed/windows-candidate
python scripts/package-native.py --target x86_64-pc-windows-msvc --signed-dir artifacts/signed/windows-candidate --output artifacts/native/windows-signed-candidate
```

`--signtool` may select the installed Windows SDK tool explicitly. Signing uses SHA-256 file and timestamp digests. Verification requires a valid Authenticode trust result, the exact requested signer and a timestamp. The [Microsoft SignTool reference](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool) describes the OS trust and timestamp checks.

The supplied Apple public certificate identifies `Developer ID Application: Ioka LLC (5PX7D99PXA)` and expires February 1, 2027. The certificate file contains no private key. The original Mac holds the matching private key; no certificate or key is copied into this repository. Inspect that Mac's signing identities with `security find-identity -v -p codesigning` and use AIRK’s separate `airk-notary` profile in its own signing keychain. Do not put credentials in command-line arguments or repository files.

After native tests/build on the Mac, choose its architecture and run:

```sh
python3 scripts/sign-native.py --target aarch64-apple-darwin \
  --runtime-dir artifacts/local/macos-1.8.14/runtime \
  --identity 'Developer ID Application: Ioka LLC (5PX7D99PXA)' \
  --team-id 5PX7D99PXA --notarize --keychain-profile airk-notary \
  --keychain "$HOME/Library/Keychains/airk-signing.keychain-db" \
  --output artifacts/signed/mac-arm64-candidate
python3 scripts/package-native.py --target aarch64-apple-darwin \
  --signed-dir artifacts/signed/mac-arm64-candidate \
  --output artifacts/native/mac-arm64-signed-candidate
```

Use `x86_64-apple-darwin` on the Intel native runner. The signer enables hardened runtime and a trusted timestamp, verifies the Developer ID certificate/team, and uploads only when `--notarize` is supplied. It waits for Apple's explicit `Accepted` result and records the submission ID. Modern runtime signing staples and validates the app ticket; a historical standalone CLI cannot carry a stapled app ticket. Neither outcome alone establishes offline extracted Gatekeeper acceptance. See [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

Signing replaces any linker-generated ad-hoc signature on the fresh artifact copy, then verifies the Developer ID signature, team, hardened runtime and timestamp. The tested source executable remains unchanged; the receipt records both source and signed hashes.

Feed the resulting native `publish/` directory to `package-extension.py --native-dir ...`. Candidate payload hashes then cover the signed executable. Continue with [the macOS build guide](macos-build.md) and [current release record](releases/release-1.8.14.md), retaining actual logs for the final commit. Signing/notarization does not qualify installed services, remote editors or the oldest supported macOS version.

The local AIRK keychain is `~/Library/Keychains/airk-signing.keychain-db`; its notary profile is `airk-notary`. It reuses the company’s Developer ID Application identity, with a separate keychain and profile from WritingForgePro. Unlock it in Keychain Access before signing. Its generated unlock password is kept in the login keychain as the generic-password item `airk-signing-keychain-password`, never in the repository.

## Manual GitHub-hosted signing

Both hosted Mac signing workflows use the same [ticket-network action](../.github/actions/macos-ticket-network/action.yml) before signing on Apple Silicon and Intel. It applies the runner provider's temporary [UDP/443 workaround](https://github.com/actions/runner-images/issues/14924) so Apple ticket requests use HTTPS over TCP. The job-level rule also covers later ticket validation during signed native packaging. All Developer ID, timestamp, accepted-notarization, staple and package checks remain required. This hosted-runner workaround does not change a developer's local firewall or the signed payload.

The [numbered GitHub release workflow](release-workflow.md#numbered-github-releases) also uses this Apple environment, signing the exact native candidates from its successful CI run. It requires accepted notarization for both architectures before any release is published.

[Signed macOS candidates](../.github/workflows/macos-signing.yml) is a separate, manual workflow for `main`. It builds/tests both Apple Silicon and Intel on their native hosted runners, then signs, notarizes and packages native ZIPs and stable VSIX files. Artifacts are retained for seven days. It does not publish a release or Marketplace extension. Ordinary PR CI continues to create development candidates. Extension release consumes the canonical GitHub release's verified signed native/VSIX assembly without rebuilding or re-signing; final Marketplace publication requires matching published GitHub assets. See the [handoff](release-workflow.md#build-manually).

Before dispatching, configure the repository environment **macos-signing** with a deployment branch policy allowing only the `main` branch (not tags), and these environment secrets:

| Secret | Value |
|---|---|
| `MACOS_CERTIFICATE_P12` | Single-line base64 of a password-encrypted PKCS#12 containing only the matching Developer ID Application certificate and private key |
| `MACOS_CERTIFICATE_PASSWORD` | Password protecting that PKCS#12 export |
| `APPLE_ID` | Apple account authorized to notarize for team `5PX7D99PXA` |
| `APPLE_APP_SPECIFIC_PASSWORD` | That account's app-specific password, validated against Apple's notary service |

The public `.cer` file cannot replace the PKCS#12 private key. Export the exact Developer ID Application identity through Keychain Access or the Security framework, rather than exporting every identity in the login keychain. Transfer values through GitHub's secret UI or `gh secret set --env macos-signing` using standard input; never paste secrets into commands, logs or repository files. Remove temporary exports after transfer. Rotate the certificate before its February 1, 2027 expiry. GitHub documents [certificate secrets and temporary runner keychains](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

The workflow exposes secrets only to [the CI signing helper](../scripts/ci-macos-sign.py), after build/test steps finish. It uses a private directory under `RUNNER_TEMP`, a fresh random keychain password, and explicit `--keychain` selection. It also adds the temporary keychain to the user's search list for certificate-chain resolution, preserving the existing entries and restoring the original list when signing exits. Passwords reach `security` through standard input and `notarytool` through its password prompt. Credential-tool diagnostics are suppressed to prevent accidental disclosure. The helper clears secrets from child-process environments, validates the imported identity and notary credentials, requires notarization `Accepted`, and deletes the temporary keychain on success or failure. An additional always-run cleanup step handles an interrupted signer. No compiler cache or artifact contains the credentials.

### Verify the exact Mac signing certificate

Both hosted Mac signing callers use the same post-sign execution gate in
`ci-macos-sign.py`. For a schema-2 runtime, it runs the canonical embedded-model
smoke against `runtime/AIRK.app/Contents/MacOS/airk`, then the existing private
`check-service-install.py` lifecycle fixture against that exact signed runtime.
The temporary keychain is deleted and its original search list restored before
either probe; remaining signing-secret environment variables cause refusal.
The probes neither extract the main from its app nor re-sign it. Complete
runtime inventory equality is checked afterward. Failure prevents the signing
step from succeeding and therefore blocks subsequent packaging/promotion.

`post-sign-validation.json` stays beside `signing.json`, outside the sealed
runtime. CI logs include its structured stages, model identity, hashes and
bounded traceback coordinates/error codes; arbitrary child output and bearer
bytes are omitted. Installation uses the canonical script's bounded native
operations and the existing job timeout. This source gate still needs an actual
authorized hosted run. It does not qualify GUI approval, agent-host trust,
oldest-supported macOS, reboot, or the complete historical migration matrix.

CI intentionally creates a different temporary keychain file on each runner.
The reproducible check is the certificate and matching private-key identity,
not equality with Neo's keychain path. The working source pins this public leaf
certificate SHA-256 in `ci-macos-sign.py`:

```text
4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1
```

That exact leaf, SHA-1 identity selector
`4d8496dcab73794378194d040d9177fb28a0f982`, and public key were observed on
Neo's signed 1.8.12 app/launcher and on the authenticated published 1.8.7 ARM
artifact from GitHub release run 41405017894. Historical artifact equality and
the environment secret's update timestamp support continuity; they do not
reveal or independently prove the present GitHub secret value.

The guard reads only public certificates from the imported temporary keychain,
requires one pinned leaf and its unambiguous valid private-key identity, then
passes the derived exact SHA-1 selector to `codesign`. Apple documents
[exact hash selection when identities share a name](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac).
The SHA-256 pin is the policy check; SHA-1 is Apple's identity selector. A same-name
certificate alone cannot satisfy the guard. Verification retains Developer ID,
team, hardened-runtime and timestamp requirements and adds the exact leaf
requirement for hash-selected identities.

After signing, the helper independently checks the public leaf SHA-256 on the
actual app and every signed component. It writes a public
`certificate-identity.json` beside `signing.json`, outside the signed runtime,
and prints the public fingerprints to the job log. A mismatch fails the step,
restores the original search list and removes the temporary keychain. This check
exports no private key and changes no signed payload or installer inventory.

The source guard does not prove a current hosted signing run has executed it.
The next authorized signing run must execute them to prove that its effective
`macos-signing/MACOS_CERTIFICATE_P12` import uses the pinned certificate and key
pair. Renewal or rotation requires a separately reviewed public fingerprint
change and the corresponding protected P12 update; a new certificate with the
same name or team must fail until reviewed. Do not bypass the pin during rotation.

Once the workflow change is merged to `main` and environment secrets are configured, use **Actions → Signed macOS candidates → Run workflow** on `main`. A failed signing or notarization step prevents candidate upload. Download `airk-signed-native-…` and `airk-signed-vsix-…` from that run; the native manifest includes the verified signature and Apple's submission ID. A local signing result does not establish that the hosted workflow has passed.

The environment, main-only deployment policy and four secrets were configured on 14 October 2026. See the [validation record](validation.md) for the accepted local Apple Silicon notarization and hosted-run limitations.

## Hosted Windows signing through Azure

The GitHub release workflow uses Microsoft's pinned [Artifact Signing action](https://github.com/Azure/artifact-signing-action) with Azure CLI OIDC login. It reuses VCP's existing service configuration: endpoint `https://wus2.codesigning.azure.net`, account `ioka-llc-signing`, and certificate profile `WritingForgePro`. AIRK has its own environment/federation boundary; it does not copy VCP implementation or private key material.

Configure environment **native-signing**, restricted to the `main` branch only, with `AZURE_SIGNING_CLIENT_ID`, `AZURE_SIGNING_TENANT_ID` and `AZURE_SIGNING_SUBSCRIPTION_ID` as variables. The existing Azure application already has **Artifact Signing Certificate Profile Signer** on this exact profile. Add a separate federated credential for AIRK; no broader Azure role is needed. AIRK currently uses GitHub's immutable OIDC subject:

~~~text
issuer: https://token.actions.githubusercontent.com
subject: repo:iokaio@152147161/airk@1418150478:environment:native-signing
audience: api://AzureADTokenExchange
~~~

Read `repos/iokaio/airk/actions/oidc/customization/sub` before provisioning rather than assuming the older repository-name-only subject format. See [GitHub's OIDC reference](https://docs.github.com/en/actions/reference/security/oidc).

The Windows job creates a fresh executable copy, signs only that file with SHA-256 and an RFC3161 timestamp, then checks Authenticode trust, the exact `Ioka LLC` publisher subject, the profile's stable identity EKU and timestamp. The receipt binds the original tested digest, signed digest, commit, lock and observed certificate thumbprint. Repackaging repeats OS verification. Certificate rotation may change the thumbprint between builds; the publisher and identity EKU remain required. Failed authentication, signing or verification prevents publication. Azure login is cleared after signing; no Azure client secret or local certificate export is needed.

On 14 October 2026, AIRK's `native-signing` and `github-releases` environments were created with branch policies allowing only `main`, the three Azure variables were configured, and `github-airk-native-signing` was added to the existing Azure application's federated credentials. Readback confirmed AIRK's immutable subject and VCP's existing entry. No Azure role was changed. Hosted AIRK signing/publication still requires a successful run of the merged workflow.
