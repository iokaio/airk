# Verifying a published AIRK release

Use this procedure to tie the exact package you install to its source revision and to the records published with it. It covers one release and one platform at a time. It reads and hashes files; it never runs downloaded executables.

## What the chain shows, and what it does not

The trust root is the GitHub release for the tag, together with the producer workflow run it names. CI builds the native archive and the VSIX once. The release workflow authenticates those producer records again before publishing, signs Windows and macOS binaries, and publishes `SHA256SUMS` and `release.json` (see [release workflow](release-workflow.md) and [CI and builds](ci-and-builds.md#downloadable-candidates)).

The check confirms that:

- the downloaded files match `SHA256SUMS` and `release.json`;
- the VSIX and native candidate manifests name the same commit and tree as the release;
- every file inside the VSIX, including the bundled runtime, matches its inventory;
- the Windows binaries carry a valid Authenticode signature.

The check does **not** prove that rebuilding the source reproduces the same bytes. It does not prove the producer run was free of tampering beyond what GitHub records, and it does not show how the extension behaves when installed. A checksum that only matches the inventory shipped in the same package is not, on its own, evidence of authenticity. Matching the separately published records is what links the steps.

## Procedure

1. Choose the tag and the platform, and download into an empty directory, for example for Windows x64:

   ```text
   gh release download v1.7.3 --repo iokaio/airk --dir DIR --pattern SHA256SUMS --pattern release.json \
     --pattern "airk-1.7.3-win32-x64.vsix*" --pattern "airk-1.7.3-x86_64-pc-windows-msvc*"
   ```

   The macOS and Linux targets are `darwin-arm64`/`aarch64-apple-darwin`, `darwin-x64`/`x86_64-apple-darwin` and `linux-x64`/`x86_64-unknown-linux-gnu`.

2. Resolve the expected source commit from a clone you trust: `git rev-parse "v1.7.3^{commit}"`.

3. Run the read-only verifier from a checkout of this repository:

   ```text
   python scripts/verify-published-release.py --directory DIR --version 1.7.3 --platform win32-x64 --commit <commit> --authenticode
   ```

   It exits 0 only when every check passes, and it lists what it could not verify. `--authenticode` reads Windows signatures. On macOS, run `codesign --verify --strict --verbose=2` and `spctl --assess --type execute` on the extracted `AIRK.app` instead.

4. Confirm the producer run named in the output: `gh run view <run_id> --repo iokaio/airk`. Its workflow, head commit and conclusion should match the release.

5. Record the result, including which install, upgrade and uninstall tests actually ran on that platform. A source test, a CI candidate test and a test of the signed installed artifact are separate kinds of evidence.

## Recorded result: 1.7.6, Windows x64 and Linux x64 (2026-10-31)

| Item | Observed |
|---|---|
| Channel and tag | stable, `v1.7.6`, published on GitHub on 2026-10-31 |
| Source identity | commit `5e4f7bd518659ff1649ebce92849eb695e0c71c9` (expected commit supplied with `--commit`) |
| Producer run | `41163334759`, workflow "GitHub release" at the same commit, conclusion success |
| Windows x64 VSIX SHA-256 | `d1f0bc616dfb2e61ffbaed7b257d725bd1f270fc531dc53a3c8566f184ab5932`; all 22 checks passed; `airk.exe` and `airk-embed.exe` Authenticode `Valid` |
| Linux x64 VSIX SHA-256 | `a88c6ec79aef0353450e795ae185cc1d0f26ae3705db5937e5313f4030988dbe`; all 20 checks passed |
| Not verified | bit-for-bit reproduction from source; macOS packages (signed and notarized by the workflow, not re-checked locally); installed-editor behavior of these artifacts |

The first Linux check reported a mismatch for `LICENSE` and `README.md`. vsce stores them in the VSIX as `LICENSE.txt` and `readme.md`. The Linux CI manifest keeps the staged names, while signed promotions record the stored names. The bytes matched exactly. The verifier now applies the same name mapping as `package-extension.py` and `promote-vsix.py`.

## Recorded result: 1.7.3, Windows x64 (2026-10-30)

| Item | Observed |
|---|---|
| Channel and tag | stable, `v1.7.3`, published on GitHub on 2026-10-30 |
| Source identity | commit `73c72c7976c57ff368307431ac83ca9283f3215d`, tree `4bf8a6d2f276a912a14cf2414776abd22dfebd4d`, clean checkout |
| Producer run | `41131222230`, workflow "GitHub release", `workflow_dispatch` on `main` at the same commit, conclusion success |
| VSIX SHA-256 | `d8d73660cc636341019e27e29b5e4a0e28508e074eb44648984f28a7fa0e27ea` |
| Native archive SHA-256 | `f6b55b9ea14c126c1e9c95c30ea1696fc5a82ffaf12e04d6ae7f0751c7098843` |
| Inventories | all 758 VSIX payload files and all 674 bundled runtime files matched; no unlisted VSIX files |
| Signatures | `airk.exe` and `airk-embed.exe`: Authenticode `Valid`, signer `CN=Ioka LLC` |
| Not verified | bit-for-bit reproduction from source; installed-editor install, upgrade and uninstall of this artifact; the other three platforms |

The Marketplace copy was not compared in this record. Packages for other platforms and versions need their own run of this procedure.
