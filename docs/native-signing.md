# Native signing and macOS handoff

Signing is opt-in and operates on a fresh copy under ignored `artifacts/`. It does not import certificates, export keys, change the original build or publish a package. `scripts/sign-native.py` only writes a success receipt after OS signature verification succeeds. The native packager accepts `--signed-dir` and re-verifies the signature, source binary digest, target, commit and dependency lock before including the signed copy.

On Windows, provision a trusted code-signing identity in the current user's certificate store, then run:

```powershell
python scripts/sign-native.py --target x86_64-pc-windows-msvc --identity CERTIFICATE_THUMBPRINT --timestamp-url https://YOUR_RFC3161_SERVER --output artifacts/signed/windows-candidate
python scripts/package-native.py --target x86_64-pc-windows-msvc --signed-dir artifacts/signed/windows-candidate --output artifacts/native/windows-signed-candidate
```

`--signtool` may select the installed Windows SDK tool explicitly. Signing uses SHA-256 file and timestamp digests. Verification requires a valid Authenticode trust result, the exact requested signer and a timestamp. The [Microsoft SignTool reference](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool) describes the OS trust and timestamp checks.

The supplied Apple public certificate identifies `Developer ID Application: Ioka LLC (5PX7D99PXA)` and expires February 1, 2027. The certificate file contains no private key. The original Mac holds the matching private key; no certificate or key is copied into this repository. Inspect that Mac's signing identities with `security find-identity -v -p codesigning` and use AIRK’s separate `airk-notary` profile in its own signing keychain. Do not put credentials in command-line arguments or repository files.

After native tests/build on the Mac, choose its architecture and run:

```sh
cargo build --release --locked -p airk-cli --target aarch64-apple-darwin
python3 scripts/sign-native.py --target aarch64-apple-darwin \
  --identity 'Developer ID Application: Ioka LLC (5PX7D99PXA)' \
  --team-id 5PX7D99PXA --notarize --keychain-profile airk-notary \
  --keychain "$HOME/Library/Keychains/airk-signing.keychain-db" \
  --output artifacts/signed/mac-arm64-candidate
python3 scripts/package-native.py --target aarch64-apple-darwin \
  --signed-dir artifacts/signed/mac-arm64-candidate \
  --output artifacts/native/mac-arm64-signed-candidate
```

Use `x86_64-apple-darwin` on the Intel native runner. The signer enables hardened runtime and a trusted timestamp, verifies the Developer ID certificate/team, and uploads only when `--notarize` is supplied. It waits for Apple's explicit `Accepted` result and records the submission ID. A standalone CLI does not support stapling; the server-side ticket is not an offline Gatekeeper qualification. See [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

Feed the resulting native `publish/` directory to `package-extension.py --native-dir ...`. Candidate payload hashes then cover the signed executable. Continue with [the macOS build guide](macos-build.md) and [release checklist](release-checklist-0.1.0.md), retaining actual logs for the final commit. Signing/notarization does not qualify installed services, remote editors or the oldest supported macOS version.

The local AIRK keychain is `~/Library/Keychains/airk-signing.keychain-db`; its notary profile is `airk-notary`. It reuses the company’s Developer ID Application identity, with a separate keychain and profile from WritingForgePro. Unlock it in Keychain Access before signing. Its generated unlock password is kept in the login keychain as the generic-password item `airk-signing-keychain-password`, never in the repository.

## Manual GitHub-hosted signing

The [numbered GitHub release workflow](release-workflow.md#numbered-github-releases) also uses this Apple environment, signing the exact native candidates from its successful CI run. It requires accepted notarization for both architectures before any release is published.

[Signed macOS candidates](../.github/workflows/macos-signing.yml) is a separate, manual workflow for `main`. It builds/tests both Apple Silicon and Intel on their native hosted runners, then signs, notarizes and packages native ZIPs and preview VSIX files. Artifacts are retained for seven days. It does not publish a release or Marketplace extension. Ordinary PR CI and Extension release continue to create unsigned candidates.

Before dispatching, configure the repository environment **macos-signing** with a deployment branch policy allowing only the `main` branch (not tags), and these environment secrets:

| Secret | Value |
|---|---|
| `MACOS_CERTIFICATE_P12` | Single-line base64 of a password-encrypted PKCS#12 containing only the matching Developer ID Application certificate and private key |
| `MACOS_CERTIFICATE_PASSWORD` | Password protecting that PKCS#12 export |
| `APPLE_ID` | Apple account authorized to notarize for team `5PX7D99PXA` |
| `APPLE_APP_SPECIFIC_PASSWORD` | That account's app-specific password, validated against Apple's notary service |

The public `.cer` file cannot replace the PKCS#12 private key. Export the exact Developer ID Application identity through Keychain Access or the Security framework, rather than exporting every identity in the login keychain. Transfer values through GitHub's secret UI or `gh secret set --env macos-signing` using standard input; never paste secrets into commands, logs or repository files. Remove temporary exports after transfer. Rotate the certificate before its February 1, 2027 expiry. GitHub documents [certificate secrets and temporary runner keychains](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

The workflow exposes secrets only to [the CI signing helper](../scripts/ci-macos-sign.py), after build/test steps finish. It uses a private directory under `RUNNER_TEMP`, a fresh random keychain password, and explicit `--keychain` selection. Passwords reach `security` through standard input and `notarytool` through its password prompt. Credential-tool diagnostics are suppressed to prevent accidental disclosure. The helper clears secrets from child-process environments, validates the imported identity and notary credentials, requires notarization `Accepted`, and deletes the temporary keychain on success or failure. An additional always-run cleanup step handles an interrupted signer. No compiler cache or artifact contains the credentials.

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
