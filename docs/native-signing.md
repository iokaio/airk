# Native signing and macOS handoff

Signing is opt-in and operates on a fresh copy under ignored `artifacts/`. It does not import certificates, export keys, change the original build or publish a package. `scripts/sign-native.py` only writes a success receipt after OS signature verification succeeds. The native packager accepts `--signed-dir` and re-verifies the signature, source binary digest, target, commit and dependency lock before including the signed copy.

On Windows, provision a trusted code-signing identity in the current user's certificate store, then run:

```powershell
python scripts/sign-native.py --target x86_64-pc-windows-msvc --identity CERTIFICATE_THUMBPRINT --timestamp-url https://YOUR_RFC3161_SERVER --output artifacts/signed/windows-candidate
python scripts/package-native.py --target x86_64-pc-windows-msvc --signed-dir artifacts/signed/windows-candidate --output artifacts/native/windows-signed-candidate
```

`--signtool` may select the installed Windows SDK tool explicitly. Signing uses SHA-256 file and timestamp digests. Verification requires a valid Authenticode trust result, the exact requested signer and a timestamp. The [Microsoft SignTool reference](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool) describes the OS trust and timestamp checks.

The supplied Apple public certificate identifies `Developer ID Application: Ioka LLC (5PX7D99PXA)` and expires February 1, 2027. The certificate file contains no private key. The owner will continue on the original Mac holding the matching private key; no certificate or key is copied into this repository. Inspect that Mac's signing identities with `security find-identity -v -p codesigning` and use an existing `notarytool` keychain profile. Do not put credentials in command-line arguments or repository files.

After native tests/build on the Mac, choose its architecture and run:

```sh
cargo build --release --locked -p airk-cli --target aarch64-apple-darwin
python3 scripts/sign-native.py --target aarch64-apple-darwin \
  --identity 'Developer ID Application: Ioka LLC (5PX7D99PXA)' \
  --team-id 5PX7D99PXA --notarize --keychain-profile YOUR_EXISTING_PROFILE \
  --output artifacts/signed/mac-arm64-candidate
python3 scripts/package-native.py --target aarch64-apple-darwin \
  --signed-dir artifacts/signed/mac-arm64-candidate \
  --output artifacts/native/mac-arm64-signed-candidate
```

Use `x86_64-apple-darwin` on the Intel native runner. The signer enables hardened runtime and a trusted timestamp, verifies the Developer ID certificate/team, and uploads only when `--notarize` is supplied. It waits for Apple's explicit `Accepted` result and records the submission ID. A standalone CLI does not support stapling; the server-side ticket is not an offline Gatekeeper qualification. See [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

Feed the resulting native `publish/` directory to `package-extension.py --native-dir ...`. Candidate payload hashes then cover the signed executable. Existing hosted CI remains an unsigned build path; a signing runner/keychain is an owner-controlled release setup, not an inferred secret configuration. Neither real signing/notarization nor macOS installed-service/remote-editor qualification has been performed on this Windows workstation. Continue with [the macOS build guide](macos-build.md) and [release checklist](release-checklist-0.1.0.md), retaining actual logs for the final commit.
