# Verifying a published AIRK release

Use this procedure to tie the exact package you install to its source revision and to the records published with it. It covers one release and one platform at a time. It reads and hashes files; it never runs downloaded executables.

## What the chain shows, and what it does not

The trust root is the GitHub release for the tag, together with the producer workflow run it names. CI builds the native archive and the VSIX once. The release workflow authenticates those producer records again before publishing, signs Windows and macOS binaries, and publishes `SHA256SUMS` and `release.json` (see [release workflow](release-workflow.md) and [CI and builds](ci-and-builds.md#downloadable-candidates)).

The check confirms that:

- the downloaded files match `SHA256SUMS` and `release.json`;
- the VSIX and native candidate manifests name the same commit and tree as the release;
- every file inside the native archive and VSIX matches its respective inventory;
- the Windows binaries carry a valid Authenticode signature.

The check does **not** prove that rebuilding the source reproduces the same bytes. It does not prove the producer run was free of tampering beyond what GitHub records, and it does not show how the extension behaves when installed. A checksum that only matches the inventory shipped in the same package is not, on its own, evidence of authenticity. Matching the separately published records is what links the steps.

## Procedure

1. Choose the tag and the platform, and download into an empty directory, for example for Windows x64:

   ```text
   gh release download v1.8.7 --repo iokaio/airk --dir DIR --pattern SHA256SUMS --pattern release.json \
     --pattern "airk-1.8.7-win32-x64.vsix*" --pattern "airk-1.8.7-x86_64-pc-windows-msvc*"
   ```

   The macOS and Linux targets are `darwin-arm64`/`aarch64-apple-darwin`, `darwin-x64`/`x86_64-apple-darwin` and `linux-x64`/`x86_64-unknown-linux-gnu`.

   From 1.8.24 (the split service and extension) a release has one universal VSIX, so download `airk-<version>.vsix*` instead of the platform VSIX; the platform patterns then also fetch the stable channel's AIRK Service installer and its manifest. For a split release the verifier checks that the VSIX has no native code, that the pair ZIP's `airk` and `airkctl` match the native manifest, and that the installer manifest binds the installer bytes, this pair and its pinned service payload inventory; for Linux it also reads every installer archive member. It does not open Windows or macOS installers.

2. Resolve the expected source commit from a clone you trust: `git rev-parse "v1.8.7^{commit}"`.

3. Run the read-only verifier from a checkout of this repository:

   ```text
   python scripts/verify-published-release.py --directory DIR --version 1.8.7 --platform win32-x64 --commit <commit> --authenticode
   ```

   It exits 0 only when every check passes, and it lists what it could not verify. `--authenticode` reads Windows signatures. On macOS, run `codesign --verify --strict --verbose=2` and `spctl --assess --type execute` on the extracted `AIRK.app` instead.

4. Confirm the producer run named in the output: `gh run view <run_id> --repo iokaio/airk`. Its workflow, head commit and conclusion should match the release.

5. Record the result, including which install, upgrade and uninstall tests actually ran on that platform. A source test, a CI candidate test and a test of the signed installed artifact are separate kinds of evidence.

## Portable publications and shared Help (1.8.35 and later)

The target filename pattern also downloads its separate portable ZIP and external manifest.
The read-only verifier checks every portable archive file, the included manuals and inventory,
and exact equality of `runtime/` bytes to the published native candidate. This does not
establish portable setup or process behavior on that host. Current `release.json` uses
`airk-github-release/2`, with an exact official installer capability map; old `/1` records
remain historical formats and do not imply an automatic headless installation capability.

After verifying the release and its live GitHub producer, check the published website against
that same downloaded universal VSIX:

```text
python scripts/check-published-help.py --release-dir DIR --report artifacts/published-help.json
```

This reads `https://airk.ioka.io/help/html.index`, follows its same-site directory redirect,
requires `text/html`, compares `/help/index.html` byte-for-byte, and checks every generated
article/reference/search asset plus the exact shipping Help manifest. Its report records
final URLs, content types, hashes and the published VSIX identity. It executes no downloaded
code. Browser rendering, keyboard/search behavior and native installer journeys require their
own checks. A local preview or an unpublished source manifest is not published Help evidence.

## Artifact attestations

GitHub artifact attestations for the standalone Linux installer and GitHub-hosted
VSIX packages are implemented in the release workflow source. Their generation and
published-artifact acceptance remain pending for 1.8.14. After an artifact and
its attestation have actually been published, verify the downloaded bytes:

```console
gh attestation verify <downloaded-artifact> -R iokaio/airk
```

For the release workflow's expected identity, constrain the signer and source
ref as well:

```console
gh attestation verify <downloaded-artifact> -R iokaio/airk --signer-workflow iokaio/airk/.github/workflows/github-release.yml --source-ref refs/heads/main
```

This checks signed workflow provenance. It is not a native OS signature, an
installed-behavior test, or proof of a reproducible build. Linux remains without
a native OS installer signature. Windows Azure signatures and macOS Developer ID
signing/notarization remain required independently. Marketplace's automatic
Microsoft extension signature is separate from the GitHub VSIX attestation.
Use a current GitHub CLI and inspect the verified workflow and source identity;
the [GitHub CLI verification reference](https://cli.github.com/manual/gh_attestation_verify)
describes repository, workflow and source-ref constraints.

## Historical schema-1 Mac installed-artifact and channel acceptance

Bundle verification alone does not qualify the installed launcher. The
[issue 66 review](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197)
identifies a signed 1.8.7 main executable that verifies inside `AIRK.app` but
fails after the installer copies it outside the bundle. Those schema-1 layouts used a
separately signed `AIRK.app/Contents/MacOS/airk-launcher-image`; service and worker
processes keep using the sealed app executables. Changing certificates does not
repair the historical copy defect. The modern 1.8.14 single-image layout keeps
its main inside the app bundle; use the [current signing contract](native-signing.md#current-1814-signing-contract)
and its own installed-artifact evidence rather than applying these historical
copied-launcher checks to a modern payload.

Before declaring the corrected Mac release accepted, retain these additional
results for Apple Silicon and Intel separately:

- Verify the final downloaded app's strict Developer ID signature, notarization
  assessment and stapled ticket. Check the standalone launcher's exact digest
  against the approved native and VSIX inventories.
- In a separate execution test of the approved package, copy the approved
  standalone image outside the app, verify its strict signature and run
  `--version`. Preserve a negative check showing that copying the sealed app
  main is rejected. `scripts/sign-native.py::verify_copied_launcher` performs
  the positive post-seal copy check during signing and native packaging.
- Install the actual signed/notarized VSIX on a clean standard-user account.
  Exercise setup, service health, indexing/search, collection status and an MCP
  handshake. Upgrade or repair a reproduction of the failing signed 1.8.7
  installation, reopen the editor, and verify that the invalid launcher is not
  recreated. A same-version payload-switch smoke is a different observation.
- Download both GitHub and Marketplace packages for the same version and
  architecture. Compare their native file inventories, executable permissions,
  digests and Apple signing identity. Record which published release and
  producer each download matches. Whole-VSIX hashes alone do not establish
  native payload equality; Marketplace extension signing is separate from Apple
  native signing. [Issue 67](https://github.com/iokaio/airk/issues/67) reports
  different native payloads for the historical 1.8.7 downloads.

The read-only procedure above does not execute these acceptance scenarios.
The [1.8.12 working-build record](releases/release-1.8.12.md) distinguishes
completed local Developer ID tests from outstanding notarized distributed-package,
historical upgrade, Intel and publication checks. Keep the issue open until its
required release evidence exists.

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


For 1.9.0 `/3` publications, retain all optional model part assets alongside the
published records when running `verify-published-release.py`. The checker uses
the matching source's native-emitted publication projection and checks each
original part's size/SHA-256 against native pins, `release.json` and
`SHA256SUMS`; changing downloaded bytes and resealing the local records cannot
replace those pins. Use the source revision identified by the release's live
producer. This check does not establish native concatenation/full logical asset
verification, provisioning, offline inference, resource fitness or platform
qualification. Those remain separate required native journeys.

The campaign's final publication nominates an earlier qualified signed run. Its
small assembly and separate bounded optional-part artifacts retain their
original run/attempt and actual GitHub artifact digests. Publication verifies
that complete original set and never rebuilds or resigns it. Read back the
actual published parts and their GitHub-computed digests after publication;
local staging success is not published-download verification.
