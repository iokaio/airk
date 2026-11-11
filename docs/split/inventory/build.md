# Split inventory: build, CI and signing (P0.8, P0.9)

Status: Phase 0 draft, 11 November 2026; source review only (no build, test,
Cargo, npm or signing command was run). The draft authorizes no deletion and no
contract change. Decisions U1–U71 (split-plan §2.1) are taken as final. Line
numbers refer to `feat/split` at `caab8ec9`.

## Sources and conventions

- Inputs: split-plan §9.4, §10.5.1–§10.5.5 and §12; U19, U21, U22, U29, U33,
  U37, U44, U46, U60, U66 and U70. Ledger entries S0.2 (Inno Setup) and S0.3
  (Artifact Signing client) in `docs/simplify/one-process-ledger.md`.
- The AIRK MCP server failed to connect in this session (connection closed). The
  review therefore used local reads and `rg`.
- **[V]** means the line was read in source. The primary agent read the runtime
  builder, packager, transfer, signer, Windows CI signer, Windows standalone
  packager and authenticode verifier in full. Three read-only reviewers read the
  remaining files and cited lines. The primary agent spot-checked about 25 of
  those citations; all matched.
- **[I]** means an inference or a proposal. It needs review before
  implementation.
- Classes:
  - **S**: service-only `airk`.
  - **C**: CLI-only `airkctl`.
  - **P**: matched pair (both images, same version and source).
  - **V**: universal UI-only VSIX (U29).
  - **A**: admin bundle (U19).
  - **U**: unaffected.
- Payload names. Until I4 decides the payload layout, this file writes the pair
  as Windows `airk.exe`/`airkctl.exe`, Linux `airk`/`airkctl` and macOS
  `AIRK.app/Contents/MacOS/{airk,airkctl}` [I]. The §9.5 installed layout puts
  them in `service\` and `bin\`. Whether the runtime payload uses that layout
  or stays flat, with the installer doing the mapping, is an I4 decision.

---

# Part A — Build, CI and harness roster (P0.8)

## A.1 Root single-image roster: `scripts/build-service-runtime.py` (P, A)

All runtime, packaging, transfer, signing and release verifiers import this
module. Change it first, in the contract step for the I4/receipt changes. Test
file: `test_service_runtime_build.py`.

| Line | Current assumption | Required change |
|---|---|---|
| :48-65 `cargo_binary(name, profile, features)` | Hard-codes `-p airk-cli --bin <name>`; one call at :462 with `"airk"`, `local-inference` [V] | Add a `package` parameter. Build `airk` (`airk-cli`, later `airk-service` per U34) with `local-inference`. Build `airkctl` from its own crate (U27) with no features, in the same staging and under the same source identity [I]. |
| :68-76 `check_lightweight_dependencies()` | Defined but **never called** in `main()`; only mocked at `test_service_runtime_build.py:335`. Checks `airk-cli --no-default-features` [V] | Retarget it to `-p airkctl` and call it during the build. Forbid `ort*`, `tokenizers`, `tract-*`. Enforce the U27 edge set (`airk-core`, `airk-process`, `airk-ipc` only) against a checked-in expectation [I]. |
| :40-45 `verify_embedded_models`; :223-234 `verify_model_smoke` | Run `executables[0]` with `--verify-bundled-models` / `--verify-model-smoke` [V] | Pin both to the **service** image by name, not by index. Index 0 must not silently become `airkctl` after a reorder (S). |
| :207-220 `verify_static_imports` | Called once at :475 on `executables[0]` [V] | Run it on both images. `airk`: no external ORT. `airkctl`: no ORT import and no inference symbols (S + C). |
| :242-263 `source_identity()` | Roster is Cargo files, `crates`, a few scripts, `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, `third_party`. **No frontend inputs** [V] | Add `packages/ui/**`, `packages/admin/**`, their `package.json`/lock files, the esbuild config, the contract-generator inputs and outputs, the admin build script, the Node toolchain pin, and the admin asset inventory (A, §10.5.4). Any frontend-only change must change this digest. |
| :280-293 `seal_install_payload` | `airk-install.json` schema 1 inventories every staged file [V] | Both executables enter automatically once staged. Add the admin inventory digest to the build receipt, not the install manifest, unless I4 says otherwise [I]. |
| :296-308 `write_pair_metadata` | `airk-runtime.json` schema 2 holds `airk_sha256` only. `--version` must print `"<stem> <version>"` [V] | Define a new pair schema (3) with `airk_sha256` and `airkctl_sha256`. `airkctl --version` must print `airkctl <version>`, which is an I2 output contract. Gate it by version; never infer a missing image. |
| :311-335 `runtime_names`, `single_image_version`, `runtime_executables` (`names[:1]`), `runtime_metadata_matches` | ≥1.8.14 means one main image [V] | Add a split-version predicate. Every pre-split version keeps today's decoder unchanged. The split version returns the pair, and `runtime_metadata_matches` checks the schema-3 key set exactly. No pre-split receipt can qualify a split payload (§10.5.4). |
| :338-345 `signing_names` | Equals `runtime_executables` (+ launcher/dylib for historical macOS) [V] | Returns both images on Windows and macOS. Consumers: sign-native, ci-windows-sign, github-release. |
| :379-396 `macos_bundle_files` | `CFBundleExecutable=airk`; LaunchAgents run `airk runtime login-serve` [V] | `airkctl` is nested code in `Contents/MacOS` (§9.5). The LaunchAgent arguments follow the I1 role catalog (the `login-serve` role name is an I1 item) [I]. |
| :399-416 `arrange_macos_bundle` | Moves `("airk","airk-embed")` into the bundle [V] | Move `airk` and `airkctl`. |
| :418-435 `sign_development_macos_bundle` | Ad-hoc signs the bundle [V] | Ad-hoc sign the nested `airkctl` before the bundle. |
| :483-490 build receipt (`airk-build.json`, schema 1, ≤16 KiB) | Records source, profile, runtime, rustc, ORT and smoke results [V] | Add an `admin_assets` block: bundle digest, inventory digest, Node/npm versions, lock digests. Add an `airkctl` import report. Check the 16 KiB bound again (A). |
| :462-495 `main()` | One compile, then the checks [V] | Order: verify the sealed admin bundle → build `airk` (embedding it) → build `airkctl` → arrange → checks → metadata → seal (§A.17). |

## A.2 Packaging and transfer (P)

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `package-native.py:124-188` `validate_runtime` | Allowlist = `runtime_executables` + launcher + metadata + legal + notices. `main = executables[0]` at :142 [V] | P | The allowlist comes from the split roster (both images). Require the schema-3 pair. Reject a payload missing either image or carrying `airkctl` without `airk`. | `test_package_native.py` |
| `package-native.py:186` `runtime_metadata_matches` | Single digest [V] | P | Check both digests. | same |
| `package-native.py:239` `executable = executables[0]`; :268-272 `--version` loop; :273 `runtime self-test`; :276 `model-catalog`; :282-283 model checks | Every post-extract probe runs on the main image [V] | S/P | Run `--version` on both images. `runtime self-test`, `model-catalog` and the model checks stay on `airk` **only if I1 keeps them as private service roles**. Otherwise call `airkctl` for the catalog and keep the probes on `airk`. Select images by name [I]. | same |
| `package-native.py:286-301` candidate receipt `airk-native-candidate/2` | `vsix_target`, single payload; text "native CLI candidate" [V] | P/V | Move to `airk-native-candidate/3`: both image hashes, the admin inventory digest, no `vsix_target` (U29). Fix the wording. | same, `test_github_release.py` |
| `package-native.py:19-24` `TARGETS` | Rust triple → VSIX platform [V] | V | Keep the triples for native targets. Drop the VSIX-platform values (read by `package-extension.py`, `verify-extension-release.py`, `release-version.py`, `ci-runtime.py:160`). | several |
| `ci-runtime.py:44-62` `validate`/`seal`; :75-115 `restore`; :118-154 `resume` | Roster through `package.validate_runtime`. :93 executables = `runtime_executables` + launcher. Receipt `airk-ci-runtime/1` [V] | P/A | Move to `airk-ci-runtime/2`, binding both executables and the admin bundle digest from the build receipt. `restore` re-verifies the admin digest against the current `source_identity()`. `resume` must reject a stale admin input: a changed frontend source changes the source identity and fails `validate` [I]. | `test_ci_runtime.py` |

## A.3 Local and Windows build entry points

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `local-build.py:132-150` | `runtime_executables(...)[0]` (:134). Single-main check and message "single-main inventory" (:138-141). Forbidden set (:142). One `BIN` placeholder (:146-150) [V] | P/A | Build the admin bundle first, through the builder. Check the pair. Add `CTL_BIN`. `airkctl` must not be in the forbidden set. Update `scripts/local-build-README.md`. | `test_local_build.py` (asserts `runtime/airk.exe`) |
| `build.ps1:20`, `build.sh:13-15` | Require only cargo and python [V] | A | Require pinned Node/npm when the admin bundle is built. Fix the stale "each process" comment. | none (add a static case to `test_windows_static_entrypoints.py`) |
| `build-windows.ps1:37` | `npm ci` for `packages/vscode` only [V] | A | Run admin/ui `npm ci` and the admin build before clippy/test/runtime build (:50-56). | `test_windows_static_entrypoints.py` |
| `build-windows.ps1:57` | `AIRK_TEST_BINARY = runtime/airk.exe` [V] | P | Use the split variables (§A.14). | same |
| `build-windows.ps1:60-65` | `--binary` to check-transformer-model, benchmark-code, check-document-tables, check-storage-lifecycle, check-usage-inputs [V] | C/P | Use `--runtime-dir` (§A.14). | same |
| `build-windows.ps1:77` | `package-extension.py --target --native-dir` [V] | V | Universal VSIX, no native input. | same |
| `build-windows.ps1:104,108` | `result.json` and summary name one native executable [V] | P | Name both images. | same |
| `dev-check.py:40-58` | fmt and clippy only [V] | A | If any crate embeds the admin bundle, add the canonical admin build (input-digest cached) before clippy, printed in `--dry-run`. No shipping stub [I]. | `test_dev_check.py` |
| `ci-local.ps1:64,68,69,81` | Builder (:64), presence of `airk.exe` only (:68), VSIX compile (:69), `AIRK_TEST_BINARY` (:81) [V] | P/A/V | Admin build before :64. Require `airkctl.exe`. Split the test variables. Universal VSIX. | `test_windows_static_entrypoints.py` |
| `scripts/dev/receipt_runner.py:153,173-175` | Sets `AIRK_TEST_BINARY`/`AIRK_NATIVE_TEST_BINARY` to `runtime/airk` [V] | P | Set both split variables. | `test_receipt_runner.py` |
| `scripts/dev/cli_golden.py:11,140,186,198` | `program = binary.stem`, captured from `airk` [V] | C | Capture against `airkctl`. The original `airk` goldens keep their source identity (Phase 4 done-when). | `test_cli_golden.py` |
| New `build-local-installers.ps1`, `build-local-service-installer.ps1`, `build-local-vsix.ps1` (U37) | Not present [V] | P/A/V | Thin wrappers over the canonical owners (§A.17, Part B). | new cases in `test_local_build.py` or a new `test_local_wrappers.py` (§10.5.5) |

## A.4 Signing (P) — the local adapter is in Part B

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `sign-native.py:20-26` `AZURE_SIGNING` | Hard-coded identity [V] | P | Derive it from the checked-in identity file (B.2). The dict stays byte-identical. | `test_native_signing.py` |
| `sign-native.py:95-142` `read_verified_pair` | `main = runtime_executables(...)[0]` (:108); components = `signing_names` (:109-113) [V] | P | The pair roster brings in both images. The macOS bundle team check uses `components["…/airk"]` by name. | same |
| `sign-native.py:194-267` `prepare_pair`/`finish_pair` | Rewrites `airk_sha256` (:249) and schema-1 `embed_sha256` (:250-251) [V] | P | Rewrite `airkctl_sha256` for schema 3. All components share one provenance (B.3). | same |
| `sign-native.py:270-319` `sign_pair` | Spawns one child per `signing_names` entry with `--identity` (:280-291). macOS bundle sealing at :301 [V] | P | Both images signed (nested `airkctl` before the bundle on macOS). Pass the local mode through (B.3). | same |
| `sign-native.py:353-361` single-file mode | Default `target/<t>/release/airk(.exe)`. Allowed names `airk(.exe)`, `airk-embed`, … [V] | P | Add `airkctl(.exe)`. The `airk-embed` names stay historical. | same |
| `ci-windows-sign.py:18-25` `guard()` | Hosted main + `workflow_dispatch` + exact workflow ref [V] | U | **Unchanged** (U70). | `test_ci_windows_signing.py:42` |
| `ci-windows-sign.py:28-42,54-73` single-helper path | `airk.exe` only (:29,41,59) [V] | P | Historical non-pair path. The pair path is used for the split. Keep it or delete it with evidence. | same |
| `ci-windows-sign.py:76-96,99-111` `record_pair`/`signing_files` | Iterate `signing_names` [V] | P | Get both images through the roster; no other edit. | same (:18) |
| `ci-macos-sign.py:121` component regex `(?:airk\|airk-embed\|airk-launcher-image\|libonnxruntime\.dylib)` | [V] | P | Admit `airkctl`. | `test_ci_macos_signing.py` |
| `ci-macos-sign.py:126,180,191,196` | `paths=["airk"]`, one `executable_sha256`, model smoke, `check-service-install.py` call [V] | P/S | Add the `airkctl` digest. Smoke stays on `airk`. Inherits A.9. | same (:342) |
| `verify-authenticode.ps1` | Per-binary publisher/EKU/timestamp check [V] | U | Reused unchanged for both images, the uninstaller and the installer. | `test_ci_windows_signing.py:69` |

## A.5 Installers and standalone packagers

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `package-standalone-windows.py:44-58` `native_inputs` | Requires the "modern single-executable runtime" (:50-51); payload has no VSIX [V] | P | Require the split pair version; refuse a pre-split pair. | `test_standalone_windows.py` |
| `package-standalone-windows.py:73-79` `compile_installer` | Defines `AirkBinarySHA = payload["airk.exe"]` (:78) [V] | P | Add `AirkCtlBinarySHA = payload["airkctl.exe"]`, plus the U39 location defaults if they are compile-time. | same |
| `package-standalone-windows.py:61-70` `compiler_identity` | Requires `ProductVersion` to match `6.7.1(\.\d+)?`. **Genuine 6.7.1 `ISCC.exe` reports `0.0.0.0`** (ledger S0.2), so a correct compiler is refused [V] | P | Replace with a reviewed identity (A.5.1). | same (:28,40,52,161 mock it) |
| `package-standalone-windows.py:141-150,153-203` sign stages | Assume an external Azure action between prepare→build→record [V] | P | Add a local signing stage (B.4). Hosted flow unchanged. | same (:145-233) |
| `installers/windows/airk.iss:14-16` | `DefaultDirName={localappdata}\AIRK\standalone`, `DisableDirPage=yes`, `PrivilegesRequired=lowest` [V] | P | §9.5 default `%LOCALAPPDATA%\Programs\AIRK\`. `DisableDirPage=no`. Add the U39 data-location page. Validate that the chosen folder is owned by the user and not shared; machine folders are refused without elevation. Keep `lowest`; no `PrivilegesRequiredOverridesAllowed` (U39 forbids elevation) [I]. | same (:71-138) |
| `airk.iss:10,30` | `AppName`/`UninstallDisplayName` "AIRK CLI and MCP server" [V] | P | Rename to "AIRK Service" (U31). | same |
| `airk.iss:35` | One wildcard `[Files]` entry into `{app}\runtime` [V] | P | Explicit entries: `airk.exe` → `service\`, `airkctl.exe` → `bin\`, notices → `notices\` (§9.5), per I4. | same |
| `airk.iss:39` | Shortcut runs `airk.exe setup --runtime-source … --approved-sha256` [V] | S | Setup stays an `airk` role (U33). §9.5 asks for the "AIRK Admin" Start-menu shortcut instead. | same |
| `airk.iss:42-48` `UserPathEntry = {app}\runtime` | Puts `airk.exe` on PATH [V] | C | Only `{app}\bin` goes on PATH; `airk.exe` must not be on PATH (§9.5). | same (:86-123) |
| `airk.iss:231-237` `NativeBinary` | Hashes `airk.exe` against `AirkBinarySHA` only [V] | P | Verify both images before setup runs or PATH is published. | same |
| `airk.iss:275-280` | `Exec(NativeBinary, 'uninstall --yes')` [V] | S | Becomes the setup role's uninstall (§9.5 "Inno uninstaller runs the setup role"). The argument is an I1 item. | same (:124) |
| `airk.iss:21-22` `SignedUninstaller` | [V] | U | Keep the sign→embed→sign order (§10.5.4). | same (:220) |
| `installers/windows/README.md:5,10,29-35` | `airk` on PATH; one verified binary [V] | C | Reword for `airkctl` on PATH and two verified images. | none |
| `package-standalone-macos.py:53-61,142,151` | One `EXECUTABLE_SHA256`; "single-executable setup runtime" message; `files["…/MacOS/airk"]` [V] | P | Add `CTL_EXECUTABLE_SHA256` and check both. | `test_standalone_macos.py` |
| `installers/macos/Install AIRK.command.in:8,15,28-35,38` | Checks `MacOS/airk` only; one digest; `codesign` on the app; `airk setup` [V] | P/S | Check both digests. Setup stays on `airk`. U40 fallback (setup from `AIRK.app` on first launch) if Gatekeeper rejects the wrapper. | same |
| `installers/macos/README.txt.in:10-13` | Talks about "the installed executable" and uninstall [V] | C | Name `airkctl service uninstall` (U20). | none |
| `package-standalone-linux.py:80,96,98,131,149,193` | `verify_archive(…,"airk")`; chmod only `airk`; `executables == ("airk",)`; mode for `runtime/airk`; qualification text [V] | P | Pair roster everywhere. Label it unsigned (U2). | `test_package_standalone_linux.py` (:26,47) |
| `installers/linux/install.sh.in:25,29-30` | `-x runtime/airk`; `exec runtime/airk setup` [V] | P/S | Check both images. Setup stays on `airk`. | same |
| `installers/linux/README.txt.in:45-46` | `airk setup` / `airk uninstall` [V] | C | Use the `airkctl` verbs. | none |
| `standalone-installer-contract.py:6` | Artifact names only [V] | U | Names are kept (§9.4). | **none — add a case** |

### A.5.1 Reviewed Inno Setup compiler identity [I]

Problem [V]: `compiler_identity()` trusts `VersionInfo.ProductVersion`. The
official 6.7.1 `ISCC.exe`, `ISCmplr.dll` and `Compil32.exe` all report `0.0.0.0`
(ledger S0.2). Version metadata is also not an identity. The current check
would refuse the genuine compiler, and it would accept a modified one that
claims 6.7.1.

Proposal:

1. Add a checked-in `scripts/inno-setup-lock.json` (schema `airk-inno-lock/1`)
   containing:
   - `version: "6.7.1"`
   - the official installer URL and full SHA-256 (the ledger has the prefix
     `4D11E805…A7B0`; copy the full value from the S0.2 receipt)
   - the installer's Authenticode signer subject (ledger: Pyrsys B.V.)
   - a `files` map of SHA-256 for every compiler file ISCC loads during a
     build: `ISCC.exe`, `ISCmplr.dll`, `ISPP.dll`, `Setup.e32`, `SetupLdr.e32`,
     plus any language and `.isl` files AIRK uses
   - the `license.txt` SHA-256
2. `compiler_identity(iscc)` then:
   - checks `plain()` on each file;
   - requires each listed file under `iscc.parent` to match its pinned digest;
   - if the compiler files themselves are signed, requires Valid Authenticode
     with the pinned subject (whether `ISCC.exe` is signed is not verified [I]);
   - records `ProductVersion` only as an observation;
   - returns `{"version": lock.version, "sha256": ISCC digest, "files_sha256":
     canonical digest of the map, "lock_sha256": …}`.
3. Hosted parity. `github-release.yml` currently uses the runner image's Inno
   Setup at the default path (`package-standalone-windows.py:212`); no install
   step pins it [V by grep]. Install the same pinned installer
   (`/CURRENTUSER /VERYSILENT`, verify the digest first) in the Windows packaging
   job, so local and hosted compilers have identical digests. Alternatively, add
   one reviewed digest row per accepted source. Do not accept "any 6.7.1".
4. `load_context()` (:131) already re-checks `compiler` equality between stages.
   It keeps working with the richer dict. The receipt's `compiler` shape
   changes, so check `github-release.py` for consumers of it (none found by
   grep [I]).
5. Tests (`test_standalone_windows.py`):
   - a `0.0.0.0` ProductVersion with the pinned digests is accepted;
   - a `6.7.1` ProductVersion with one changed DLL is refused;
   - a missing listed file is refused;
   - a lock-file change between prepare and build is refused.

## A.6 VSIX (V) — universal, UI-only (U29)

| File:line | Assumption | Required change | Tests |
|---|---|---|---|
| `package-extension.py:2,19-20,79,107,150-153` | Platform VSIX; `TARGETS`; `native/manifest.json` in the update inventory; `vsix_target` check [V] | Build a universal VSIX. The update-inventory schema/version gate must agree with the extension's update loader. Historical verification is version-gated. | `test_package_extension.py` (:60-75, :84, :111-156, :286-333) |
| `package-extension.py:274,293-294,302-303` `verify_vsix` | `executable`/`platform` parameters; permission list of native entries; `TargetPlatform == platform` [V] | Fail if any `extension/native/` entry exists. Require that `TargetPlatform` is absent. | same |
| `package-extension.py:321-322,327-398,428-450` | `--target` (required), `--native-dir`; reads `airk-native-candidate/2`; stages `native/runtime`; receipt native digests; name `airk-{ver}-{platform}.vsix` [V] | Remove all of it. Name `airk-{ver}.vsix`. New receipt records the UI/host inventory and supported I2 contracts (§10.5.4). | same |
| `promote-vsix.py:21-158` | Exists to splice signed native into a VSIX [V] | Reduce it to the channel rewrite, or delete it with evidence (D2/U61). | `test_release_promotion.py` (only the :342 channel logic survives) |
| `verify-extension-release.py:27-31,48,53-88` | "exactly four platform receipts"; per-platform name; embedded native provenance [V] | Exactly one receipt and name. Fail if any native entry exists. | `test_extension_release.py` (:18-116) |
| `packages/vscode/package.json:51` `files: "native/**"` | [V] | Remove it. | `test_package_extension.py` |
| `packages/vscode/scripts/package.cjs:4-9` | Requires a `target` argument [V] | Remove the target argument. | same (:74) |
| `packages/vscode/scripts/publish.cjs:19-20` | "at most four platform packages" [V] | Exactly one. | `test_release_version.py:208` (workflow order only) |
| `packages/vscode/scripts/check-native-editor.cjs:104,110`; `check-service-console-client.cjs:30,58,131`; `check-service-frontend.cjs:19,79,86`; `check-service-runtime-transitions.cjs:20,78,106`; `check-agent-install.cjs:40` | Find native code inside the VSIX/stage [V] | Take the service install from outside the VSIX, or retire with the install path (Phase 6). | `scripts/tests/*.test.cjs` |

## A.7 CI selection, candidates and release (P/V/A)

| File:line | Assumption | Required change | Tests |
|---|---|---|---|
| `ci-plan.py:14-21` `NATIVE_ROWS.executable` | One executable per row (`airk`, `airk.exe`, `AIRK.app/Contents/MacOS/airk`) [V] | Add `cli_executable`, or prefer `--runtime-dir` consumers (§A.14). | `test_ci_plan.py` |
| `ci-plan.py:22-23` `PR_TARGETS`/`MAIN_TARGETS` | [V] | Unchanged (host policy §10.5.3). | same |
| `ci-plan.py:26-31` `FULL_MAIN_INPUTS`/`PREFIXES` | Cargo/toolchain/CI/ORT inputs [V] | Add the frontend lock files and Node pin if a main-only Node cache is added [I]. | same (:155) |
| `ci-plan.py:83-106` `classify()` | Plan keys `native/container/extension/contracts`. `packages/vscode/` → extension, and also native unless `.md`/`media/`. `scripts/` (non-test) → everything. `docs/images/` → extension+contracts. Docs `.md` → nothing. **`packages/ui/`, `packages/admin/` are not classified, so they fall to "everything"** [V] | Add reviewed rules: `packages/ui/` → extension + admin + native + container; `packages/admin/` → admin + native + container. `packages/vscode/` → extension, and drop native once the VSIX is UI-only and the I2 session integration lives in native/CLI tests (§10.5.3). Add the new paths, deletions, renames and unknown-path cases to `test_ci_plan.py` **before** narrowing. | same |
| `ci-plan.py:80,84,121,131-135,146` | Hard-coded plan key set; gate rejects any other key set [V] | Add an `admin` key only if a separate job is created (§A.17). | same (:79 asserts the `required.needs` list) |
| `ci-candidates.py:52-56,108-110,114,143,169-171,200,240-248,270-281,418-434,476,488` | `kinds=("native","vsix")` × targets. "Exactly eight artifacts". Rehearsal `airk-vsix-<target>`. Release inputs "exactly twelve". `len(assets) != 16`. `--kind vsix --target` [V] | 4 native + 1 universal VSIX. Recount release inputs (4 native + 4 source + 1 VSIX = 9 [I]) and assets (installers included). Native stays per target. Authenticate run/attempt, commit/tree and digest set. | `test_ci_candidates.py` (:38, :278-282, :380-382), `test_release_promotion.py` |
| `github-release.py:33,89-157` `verify_native` | `vsix_target`; `airk-<v>-<t>.zip`; `runtime_executables` or legacy name (:104-106); `executable_names` (:156) [V] | Version-gated pair: require `airkctl` and the admin digest. | `test_github_release.py` |
| `github-release.py:158-209` signing receipts | Exact schema `airk-native-signing/1` / `airk-service-pair-signing/1`; `azure == AZURE_SIGNING`; team `5PX7D99PXA` [V] | Keep the exact-schema rejection. Add a regression that local-schema receipts (B.3) are refused. | same |
| `github-release.py:213-233,364` | Restore and chmod one executable; macOS installer bootstrap uses `MacOS/airk` [V] | Both images. | same |
| `github-release.py:303,325-340` | Standalone installer receipt `airk-standalone-installer/1`; Windows wrapper set `{notices/inno-setup/LICENSE.txt}`; Azure identity on installer and uninstaller [V] | Keep. Reject the local installer schema (B.4). | `test_github_release.py:294`, `test_standalone_windows.py` |
| `github-release.py:411-438,469-476,520-523,578-605` | Per-target VSIX assembly; VSIX↔native binding; release notes "matching platform VSIX"; per-target Marketplace staging [V] | One VSIX, bound to commit/tree and not to native. Rewrite the notes. | same |
| `release-version.py:21,84-94,115,126,192-203,221-249` | `PLATFORMS` (4); `targetPlatform` required; "four-platform VSIX inventory"; `vspackage?targetPlatform=` [V] | Accept one universal entry for split versions and keep historical parsing. | `test_release_version.py` |
| `release-version.py:37-48,137` | Version agreement and bump list = `Cargo.toml`, `Cargo.lock`, `packages/vscode/package.json`, `packages/vscode/package-lock.json` [V] | Add the `packages/admin` and `packages/ui` manifests and locks (or a root workspace lock) to both lists, keeping versions synchronized (U46). | same |
| `verify-published-release.py:24-25,49-52,74-76,88-96,101,116` | Per-platform VSIX; "VSIX manifest binds the native archive"; reads `extension/native/…`; `("airk.exe","airk-embed.exe")` (already stale) [V] | Verify the installers (§9.4), the pair inventory from the native ZIP and the single VSIX. | **none — add `test_verify_published_release.py`** |
| `build-downloads.py:16-21,47-50,123,138` | Per-platform VSIX assets; "Portable runtime ZIP"; "native CLI and MCP runtime" [V] | Two steps: service installer and extension (§9.4). Only verified assets. Historical releases still parse. | `test_downloads_site.py` |
| `site/index.html:38,64,82,83,87` | "CLI + MCP", "Native helper included", portable ZIPs, "optional VS Code extension" [V] | Rewrite: AIRK Service + AIRK VS Code Extension. | `test_downloads_site.py` (indirect) |

## A.8 Process, spawn and installed-runtime checks

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `check-self-spawn.py:2-10,34,135` | "own executable"; `SELF = current_exe(` [V] | P | Detect sibling-image starts (`airkctl` starting `airk`, U4/§6.3). Record which image each roster entry starts. Keep expectations checked in. | `test_check_self_spawn.py` |
| `check-self-spawn.py:83-110` | self-test (S); on-demand `serve` (now `airkctl`→`airk`); `--version` helper; macOS service-manager/setup/launcher entries [V] | S/P | Reassign each entry to its owning image. | same |
| `check-process-topology.py:39` `MAIN_IMAGES={"airk.exe","airk"}`; :40-51, :100-123, :145-146, :172-215, :272-323 | One image; classification by command words only [V] | P | Split into `SERVICE_IMAGES` and `CLI_IMAGES`. Classify by image plus role: `serve` only on `airk`; one-shot and session only on `airkctl`. The `--airk` burst option becomes `--airkctl`. | `test_check_process_topology.py` (:16,41) |
| `check-service-clients.py:389` | Schema-2 key set `{schema,version,os,arch,airk_sha256}` [V] | P | Schema 3. | `test_service_clients.py` (:113, :268, :768) |
| `check-service-clients.py:439-441,502-560,693-698,781,788` | One `self.binary`; `run()` used for `runtime stage/activate/inspect`, `serve`, `service health/rpc/requests`, reads, `mcp` [V] | P | `self.service` and `self.cli`. `serve` and private roles go to `airk`. User and `service *` commands go to `airkctl`. `runtime stage/activate` are pre-split staging (U15 retires slots), so retire them per the D2 inventory [I]. | same |
| `check-service-install.py:126-139,362-364,397,235-474` | Single-image branch; `check_image` and `runtime self-test` on main; forbidden list; `service rpc`, `index`, `prepare-client`, `lifecycle`, `start`, `shutdown` through `airk` [V] | P | `check_image` both images. `airkctl --version`. User verbs through `airkctl`. Define which image `executable` in inspection results names. | `test_service_install.py` (:85, :228) |
| Outside the brief (grep hits): `check-runtime-upgrade.py:36`, `check-managed-runtime.py:32,40`, `check-runtime-activation.py:56`, `setup-local-test.py:109,596,843-851` [V] | Staged/managed runtime model | P/retire | Most exercise slots and managed payloads that end with the split (§2.3, U15). Route them through the compatibility inventory (U61), not a blind port [I]. | `test_runtime_upgrade.py`, `test_local_test_setup.py` |

## A.9 Container

| File:line | Assumption | Class | Required change | Tests |
|---|---|---|---|---|
| `build-service-container.py:78,120` | Allowed trees `{crates, third_party/models, third_party/licenses}`; required COPY set [V] | A | Add the admin/ui source and lock inputs. | `test_container_runtime_build.py`, `test_container_inputs.py` |
| `build-service-container.py:183-194` | `cargo_binary("airk",…)`, copy to `runtime/airk`; strip; imports on `airk` only [V] | P | Build and copy `airkctl`. Run the import check on both. | same |
| `build-service-container.py:199-203` | Metadata and receipt list one main (`main_features`) [V] | P | Pair metadata (schema 3). | same |
| `build-service-container.py:223-226` | Entrypoint `exec /usr/lib/airk/airk runtime container-host --source … --approved-sha256 …` [V] | S | §10.5.2 replaces the `runtime container-host` path with an approved **service role** of `airk`. Its name and arguments come from I1. Keep the fixed approval digest outside the payload; listener exposure does not change. | same, `test_container_check.py` (:304) |
| `build-service-container.py:227-229` | `runtime/airk runtime self-test` [V] | S/C | Use the private self-test role on `airk` per I1, plus `airkctl --version`. | same |
| `Dockerfile:2-17,24-47` | Toolchain stage has no Node; source stage copies no `packages/` [V] | A | Add a pinned Node build stage that runs the canonical admin build from locked sources. The cargo stage consumes only its sealed output (§10.5.2: keep Node in build stages). | `test_container_inputs.py`, `test_ci_plan.py` |
| `Dockerfile:159` `ln -s /usr/lib/airk/airk /usr/local/bin/airk` | `airk` on PATH [V] | C | Link `airkctl`. Whether `airk` stays on PATH in the container is a Phase 4 decision [I]. | `test_container_check.py` |
| `Dockerfile:164` `ENTRYPOINT ["airk-container"]` | [V] | U | Unchanged. | — |
| `.dockerignore:14-15,30-45,104-107` | `packages/**` excluded; only `packages/vscode/src/test/...` re-admitted [V] | A | Re-admit the admin/ui sources, `package.json`, the lock and the build config. Keep the private exclusions. | `test_container_inputs.py` |
| `container-inputs.json:3-101` | Trees and files roster [V] | A | Add the same inputs, kept in step with `.dockerignore` and the COPY lines. | same |

## A.10 Notices and dependency audit (A)

| File:line | Assumption | Required change | Tests |
|---|---|---|---|
| `license-notices.py:58,68-70,136-146` | Inventory from `cargo metadata` (so `airkctl` is covered automatically); the BOM appends only ONNX Runtime [V] | Add notices and BOM components for npm packages embedded in `airk` (Preact and any runtime dependency) from the admin lock. Build-only tools (esbuild) are excluded. Package notices per artifact (§10.5.2). | `test_license_declarations.py`, `test_sbom.py` |
| `dependency-audit.py:95-102,301-305` | Input snapshot and `npm audit` cover `packages/vscode` only [V] | Loop over every frontend lock. | `test_dependency_audit.py`, `test_supply_chain.py` |
| `.github/workflows/dependency-audit.yml:12-13,29-30` | Path triggers on the vscode manifest and lock only [V] | Add `packages/admin/**`, `packages/ui/**` manifests and locks. | `test_dependency_audit.py` |

## A.11 Workflows

| Workflow / job (lines) | Consumes | Class | Required change |
|---|---|---|---|
| `ci.yml` `plan` (:42-75) | `ci-plan.py plan` [V] | — | Add new outputs only with a new job (§A.17). |
| `ci.yml` `quick` (:76-102) | `release-version.py check`, `cargo fmt`, `test_ci_plan.py`, actionlint, repo/boundary checks; **no compilation** [V] | A | `release-version.py check` covers the frontend manifests and locks. No admin build is needed because nothing compiles here [I]. |
| `ci.yml` `contracts` (:103-120) | Python unittest discover per OS [V] | — | Runs the new and updated `scripts/tests` cases. |
| `ci.yml` `extension` (:121-137) | `npm ci`/`npm test` vscode; `test_package_extension.py` [V] | V/A | Build the universal VSIX here, once (it needs no native target). If `packages/ui` is shared, build and test it here. |
| `ci.yml` `native-build` (:138-262) | `ci-runtime resume` (:173); lightweight compile `cargo check -p airk-cli --no-default-features` (:207-209); nextest platform regressions (:214); workspace clippy (:217), tests (:220,:223), doctests (:226); `build-service-runtime.py` (:234); `ci-runtime seal` (:237); artifact `airk-runtime-<t>-<sha>-<run>` (:242) [V] | P/A | **Admin build and seal after :203 and before :206**, the first compile; every later compile embeds it. Retarget :207-209 to `-p airkctl` with a dependency proof. Keep the :214 selection identities, moving them deliberately with the crate rename (U34). |
| `ci.yml` `native` (:264-397) | `ci-runtime restore` (:298); `check-service-clients` (:302); `check-service-install` (:304); seven `--binary` drivers (:307-324); `AIRK_TEST_BINARY` (:328,:333); `package-native` (:351); **platform VSIX** (:352-356); artifacts `airk-native-…` (:360), `airk-vsix-<t>-…` (:379) [V] | P/V | Split the drivers and env (§A.14). Remove the per-target VSIX (:352-356, :376-383). Keep `npm ci` (:290) only for the native-backed extension tests. |
| `ci.yml` `container` (:399-467) | docker buildx `smoke` target, `check-container.py` [V] | P/A | Dockerfile Node stage (A.9). |
| `ci.yml` `required` (:468-484) | `needs: [plan, quick, contracts, extension, native, container]`; `ci-plan.py gate` [V] | — | Add a job here only together with the plan keys and `test_ci_plan.py:79`. |
| `github-release.yml` | Per-target sign job: `ci-windows-sign.py prepare/list/record --pair` (:216,:221,:254); Azure action ×3 (:232-251 pair, :261-269 uninstaller, :286-294 installer); `package-standalone-windows.py prepare/build/record` (:258,:283,:308); `ci-macos-sign.py --pair` (:323); `package-native` (:331,:346); `package-extension --target` + `promote-vsix` (:338,:348,:208-209,:428,:437-439); `airk-signed-vsix-<t>` (:376); Linux VSIX (:470); attest `*.vsix` (:534); path triggers (:6-15) [V] | P/V | Sign both images. Remove VSIX promotion and signing per target. Attest one universal VSIX. Include the admin identity. Add path triggers for new release-tool inputs (inno lock, signing-identity file). |
| `extension-release.yml:72,196-219` | "signed native and VSIX producer"; four platform receipts and downloads [V] | V | One VSIX. Keep the authenticated `signed_run_id` prerequisite. |
| `macos-signing.yml:46-54,57,67,72,74,83-86` | Build, then clippy/test; `AIRK_TEST_BINARY=…/MacOS/airk`; `--pair`; package-native; `package-extension --target`; `airk-signed-vsix-<t>` [V] | P/A/V | Admin build before :46. Split env. Delete the VSIX steps. |
| `release-tag.yml:20` | `ci-candidates.py tag` (asset count at `ci-candidates.py:476`) [V] | V | Follows the catalog. **No test — add a case in `test_ci_candidates.py`.** |
| `qualification.yml:36-41,55-56,63-73` | Matrix `executable: airk/airk.exe`; "single-executable native runtime"; `AIRK_BINARY`, `--binary` drivers [V] | P/A/C | Admin build; split drivers. Stays opt-in. |
| `downloads.yml:9-12,30` | Runs `test_downloads_site.py` [V] | U | Add shared release-catalog dependencies to the triggers (§10.5.2). |
| `dependency-audit.yml` | See A.10 [V] | A | — |
| `dco.yml` | [V] | U | — |

## A.12 `.config/nextest.toml`

| Line | Filter | Required change |
|---|---|---|
| :20 | `package(airk-process)` [V] | Unchanged; edit only if the crate moves. |
| :24 | Four test names (legacy review, PDF worker, guardian, manager deadline) [V] | Keep the identities. Move them deliberately if their tests move to `airkctl` or `airk-service`. |
| :44 | `package(airk-cli) & (binary(native_read_retry) \| legacy_native \| pdf_worker \| unix_guardian \| native_worker_storage \| combined_reads)` [V] | Retarget with U34. Note: `explorer_assistance` is in `ci.yml:214` but not in this group [V]; review it when the test moves. Covered by `test_skip_inventory.py:34`. |

## A.13 Golden and identity consumers

`scripts/dev/test_identities.py`, `boundary_check.py` and
`boundary-exceptions.json` take part in the T0 boundary gate (§12). The
expected crate edges must be updated with `airkctl` and `airk-ipc` (U27) as
checked-in values, not discovered ones [I]. Tests: `test_boundary_check.py`,
`packages/vscode/src/test/explorerBoundaries.test.ts`.

## A.14 Test-harness consumers of `--binary`, `AIRK_TEST_BINARY`, `AIRK_NATIVE_TEST_BINARY`, runtime root

Proposal [I]:

- Replace the ambiguous variables in one Phase 4 step with
  `AIRK_TEST_SERVICE_BINARY` (`airk`) and `AIRK_TEST_CLI_BINARY` (`airkctl`).
  Harnesses refuse to run when only the old variable is set.
- Python drivers take `--runtime-dir` and resolve both images through the pair
  roster (one owner) instead of `--binary`.
- Which binary owns `runtime stage/activate/inspect/lifecycle`, `model-catalog`,
  `explorer-assistance` and self-test is decided by the I1/I2 drafts. The
  classes below assume private roles stay on `airk` and user commands move to
  `airkctl`.

**Rust integration tests (`crates/airk-cli/tests`, `env!("CARGO_BIN_EXE_airk")`) [V]**

| File:line (count) | Use | Class |
|---|---|---|
| `common/mod.rs:122,128` (shared by configuration_native ×3, legacy_native ×3, usage_native ×2, extraction_policy_v2, vector_selection, tools) | `service requests`, `service legacy`, settings, `--version` | C |
| `common/reviewed.rs:162` (cli_commands ×3, unix_guardian ×3) | User arguments | C |
| `common/reviewed.rs:321` (+ fixture :608-1620) | Install payload hash; `runtime stage/activate/foreground/inspect/start/lifecycle` | P |
| `cli_commands.rs:21` (6 uses), :933, :1892-1941 | init/query/explorer/uninstall/help-tree parity | C (help tree splits) |
| `cli_commands.rs:1845` | `service session` hello reports the image hash | P (hash `airk`) |
| `service_runtime.rs:1198,1204` (10 uses) | `serve` ×2, `service <action>`, `service logs`, `runtime lifecycle` | P |
| `service_runtime.rs:246,4258,4353,4384` | `service logs/console/rpc` | C |
| `tools.rs:236` | Service image hash | S |
| `native_worker_storage.rs:41`, `integrity_restart.rs:384`, `helper_authority_roles.rs:67`, `unix_guardian.rs:44` | Worker roles (`project-worker`, `derived-worker`, guardian) | S |
| `bundled_model.rs:16,90`; `model_catalog.rs:12` | `--verify-model-smoke`, `model-catalog` | S |
| `native_read_retry.rs:699` | `--version` | C |
| `mcp.rs:1844` | `tools list --json` vs stdio `mcp` | C |
| `explorer_assistance.rs:88` | `explorer-assistance --descriptor` | C |
| `error_outcomes.rs:34,75,90` | `status`, `index`, `outline` | C |
| `catalog_upgrade.rs:253`; `combined_reads.rs:49` | `AIRK_CATALOG_CANDIDATE_BINARY` (staged runtime + reads) | P (needs a second override) |

**Rust unit test modules (`AIRK_TEST_BINARY`) [V]:**
`project_supervisor/guarded_qualification_tests.rs:714-715` and
`modern_checkpoint_tests.rs:244,247-248,603,625` (two `#[ignore]`) use sealed
payload and worker images. Class S.

**Python drivers (`scripts/`) [V]**

| Group | Files (lines) | Class |
|---|---|---|
| Through the `check-service-clients.py` `Fixture` (`serve`, `runtime *` on `airk`; user/`service *`/`mcp` on the CLI) | `benchmark-code.py:17,34`; `benchmark-scale.py:35,70,446`; `benchmark-usage.py:28,33,50,103`; `check-document-tables.py:25,45`; `check-storage-lifecycle.py:116,129`; `check-transformer-model.py:19,58,72`; `check-usage-inputs.py:48,328,337`; `check-retrieval-quality.py:450,553,616,643,685`; `check-publication-faults.py:67,69,84,106,112,155,269` (`samefile(fixture.binary)` must be `airk`) | P |
| Direct `[binary, --root …]` | `benchmark-indexing.py:251,277,330`; `benchmark-detail.py:48,129,144,159,211`; `benchmark-v2.py:339,383,419,433`; `check-local-providers.py:65-69,82`; `check-tokenizer-calibration.py:34,67` | C |
| `service plan/status/install/stop/start/uninstall` | `check-user-service.py:28,41-80` | P |
| `--runtime-dir` / `runtime_root` | `benchmark-service.py:415,712,721`; `benchmark-mcp-investigation.py:307,904,988,1504,1515`; `benchmark-source-verification.py:73`; `check-service-source-fairness.py:273,1055,1084` (`airk-runtime.json`) | P |
| Documentation string only | `report-test-skips.py:66` | U |

**`scripts/tests` [V]:**

- `test_service_clients.py:268,768` (stub `airk`; P)
- `test_retrieval_quality.py:284,322` (P)
- `test_service_source_fairness.py:50` (P)
- `test_ci_macos_signing.py:66-67`: asserts the workflow `AIRK_TEST_BINARY`
  line (P); change it with `macos-signing.yml:57` and `ci.yml:328,333`
- `test_benchmark_service_http.py:196` (mock; U)
- `test_native_read_contract.py:11` (marker scan; U)

**Extension tests (`packages/vscode/src/test`) [V]**

- `nativeServiceFixture.ts:15,95-117,129,165,176,193,196,331-346` drives
  `runtime stage/activate/managed/prepare-client/inspect/http-connection/lifecycle/uninstall`,
  `service console` and a `mcp` launch spec. Class P. `create()` needs both
  paths.
- Fixture consumers (P):
  - `nativeSmoke.test.ts` (6)
  - `explorerNative.test.ts` (3)
  - `native.test.ts:317,440`
  - `explorerAssistanceNative.test.ts:25`
  - `explorerReview.test.ts:227`
  - `localSearchScope.test.ts:97`
  - `universalLauncher.test.ts:47`
  - `diagnosticsNative.test.ts:9`
- CLI-only (C):
  - `native.test.ts:354,368-369` (`--version` upgrade)
  - `f09NativeQualification.test.ts:91,102` (`explorer-assistance --descriptor`)
- Under U8 the extension reaches the service only through `airkctl`. Phase 6
  therefore rewrites most of the fixture to `airkctl session`, and the
  `runtime *` staging calls retire with the install path [I].

## A.15 Test-file map (§10.5.5 plus files found here)

| Owner changed | Test files that must change |
|---|---|
| `build-service-runtime.py` | `test_service_runtime_build.py` |
| `package-native.py` | `test_package_native.py` |
| `ci-runtime.py` | `test_ci_runtime.py` |
| `ci-plan.py`, `ci.yml` | `test_ci_plan.py` |
| `ci-candidates.py`, `release-tag.yml` | `test_ci_candidates.py`, `test_release_promotion.py` |
| `package-extension.py`, `package.cjs` | `test_package_extension.py` |
| `promote-vsix.py` | `test_release_promotion.py` |
| `verify-extension-release.py`, `extension-release.yml` | `test_extension_release.py` |
| `sign-native.py` | `test_native_signing.py` |
| `ci-windows-sign.py` | `test_ci_windows_signing.py` |
| `ci-macos-sign.py`, `macos-signing.yml` | `test_ci_macos_signing.py` |
| `package-standalone-windows.py`, `airk.iss` | `test_standalone_windows.py` |
| `package-standalone-macos.py`, macOS templates | `test_standalone_macos.py` |
| `package-standalone-linux.py`, Linux templates | `test_package_standalone_linux.py` |
| `github-release.py`, `github-release.yml` | `test_github_release.py` |
| `release-version.py`, `publish.cjs` | `test_release_version.py` |
| `build-downloads.py`, `site/` | `test_downloads_site.py` |
| `build-service-container.py`, Dockerfile, inputs | `test_container_runtime_build.py`, `test_container_inputs.py`, `test_container_check.py` |
| `check-service-clients.py` and drivers | `test_service_clients.py`, `test_retrieval_quality.py`, `test_service_source_fairness.py` |
| `check-service-install.py` | `test_service_install.py` |
| `check-self-spawn.py` / `check-process-topology.py` | `test_check_self_spawn.py` / `test_check_process_topology.py` |
| `local-build.py`, `build-windows.ps1`, `ci-local.ps1`, new U37 wrappers | `test_local_build.py`, `test_windows_static_entrypoints.py` (+ wrapper cases) |
| `dev-check.py` | `test_dev_check.py` |
| `scripts/dev/receipt_runner.py`, `cli_golden.py` | `test_receipt_runner.py`, `test_cli_golden.py` |
| `license-notices.py` / `dependency-audit.py` | `test_license_declarations.py`, `test_sbom.py` / `test_dependency_audit.py`, `test_supply_chain.py` |
| `.config/nextest.toml` | `test_skip_inventory.py` |
| **No test today** | `verify-published-release.py`, `standalone-installer-contract.py`, `site/index.html` (only indirect), `build.ps1`/`build.sh` — add cases |

## A.16 Open items for the I1/I2/I4 drafts

1. Ownership of `runtime container-host`, `runtime self-test`, `model-catalog`,
   `explorer-assistance`, `runtime login-serve`, setup/uninstall arguments.
   This decides several S/C/P cells above.
2. Payload layout (flat or `bin/`+`service/`) and pair-metadata schema 3 key set.
3. `airkctl --version` output string (an I2 item).
4. Whether `airk` stays on the container's PATH.

## A.17 Build order and admin-bundle wiring

Order (§10.5.3; local scripts follow the same order):

| # | Step | Owner [I] | Consumes | Produces |
|---|---|---|---|---|
| 1 | Contracts and catalogs: generate and drift-check (U47 pilot) | contract generator + check | Rust contract owners, fixtures | Checked-in TS types; drift result |
| 2 | Shared UI: `npm ci` (locked), build, test | `packages/ui` | ui lock, Node pin | ui build |
| 3 | Admin bundle and seal | **one canonical `scripts/build-admin-assets.py`** called by every path below | ui build, admin source/lock | Bundle dir + `admin-assets.json` inventory (file digests, bundle digest, toolchain) |
| 4 | Native pair: `airk` embeds the sealed bundle and verifies the inventory at compile; `airkctl` lightweight | `build-service-runtime.py` | sealed bundle + `source_identity()` incl. frontend | Pair runtime, `airk-build.json` with `admin_assets` |
| 5 | Native verification | `package-native.py`, `check-service-*`, served-asset check | pair | Candidate receipt |
| 6 | Sign: pair → uninstaller → embed → installer (Windows); nested → bundle → notarize → staple (macOS) | `sign-native.py` (hosted: action + `ci-*-sign.py`) | candidate | Signed pair, installer |
| 7 | Service installer | `package-standalone-*.py` | signed pair | Installer + receipt |
| V | VSIX branch after step 2 (and step 1) | `package-extension.py` | ui build, vscode host | Universal VSIX + receipt; no native dependency |

Admin-bundle wiring must land **in the same step that makes the bundle
mandatory** for `airk` compilation. Otherwise any compile site left out of that
step fails (or, worse, silently embeds nothing):

| Site | Where to insert step 3 [I] |
|---|---|
| `ci.yml` `native-build` | After the static ORT prepare (:203), before the first cargo invocation (:206). Requires Node setup (already at :191-195 [V]). |
| `ci.yml` `native` | No compile; it restores the sealed pair. `ci-runtime restore` re-verifies the admin digest (A.2). |
| `ci.yml` `quick` | No compile [V]. Extend `release-version.py check` to the frontend manifests. |
| `ci.yml` `extension` | Shared UI build/test, and the universal VSIX. |
| `ci.yml` `container` / `Dockerfile` | Pinned Node stage before the cargo stage; `.dockerignore` and `container-inputs.json` admit exactly those inputs. |
| `macos-signing.yml` (:46), `qualification.yml` (:55) | Before clippy/test/runtime build. |
| `scripts/dev-check.py` | Before clippy when a selected package embeds the bundle. Cached by input digest; shown in `--dry-run`. |
| `build-windows.ps1`, `ci-local.ps1`, `local-build.py`, `build.ps1`/`build.sh` | Before cargo, through the same owner. |
| Neo batch (no committed script; the ledger S0.5 roster runs `dev-check.py` over SSH [V]) | Through `dev-check.py`. Neo needs pinned Node; its availability is not verified [I]. System `python3` 3.9.6 cannot run the 3.11+ packaging suites (ledger) [V]. |
| U37 wrappers | Steps 1-7 in order; `-ForceBuild` bypasses only artifact reuse. |

---

# Part B — Local Windows signing adapter (P0.9, U22/U70)

## B.1 Established facts

All entries below are [V], from ledger S0.3 and the source.

- This machine has no code-signing certificate in the Windows certificate
  stores. Today `sign-native.py` signs only by thumbprint
  (`/sha1`, :341-342, :376).
- The `Microsoft.ArtifactSigning.Client` 1.0.146 NuGet package is extracted
  outside the repository and every AIRK root, under the user profile
  (`~\.airk-dev-tools\artifact-signing\1.0.146\`):
  - nupkg SHA-256 `7761E5D8…6F70`;
  - `bin\x64\Azure.CodeSigning.Dlib.dll` is Microsoft-signed, SHA-256
    `123FA0C5…8A19`.
- Probe that succeeded: `signtool sign /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 /dlib <dll> /dmdf <metadata.json> <file>`.
  - Metadata: `Endpoint`, `CodeSigningAccountName`, `CertificateProfileName`
    from `AZURE_SIGNING`, and `ExcludeCredentials` listing every credential
    except `AzureCliCredential`.
  - The output passed `verify-authenticode.ps1 -Publisher … -IdentityEku …`.
- Hosted Windows signing uses `azure/artifact-signing-action` (pinned) with
  the same endpoint, account and profile and SHA-256 digests. Its credential
  inputs exclude everything except the Azure CLI credential
  (`github-release.yml:232-251`).
- `ci-windows-sign.py:18-25` is the hosted guard.
- `verify()` (`sign-native.py:42-58`) requires a 40-hex thumbprint, runs
  `signtool verify /pa /all /v`, then `verify-authenticode.ps1`.
- macOS (Neo, U60/U65/U69):
  - keychain `~/Library/Keychains/airk-signing.keychain-db`;
  - notary profile `airk-notary`;
  - unlock password stored in the login keychain as the generic-password item
    `airk-signing-keychain-password` (`docs/native-signing.md:51-73`);
  - over SSH, the login keychain is unlocked first, from
    `~/.airk-signing/keychain-pass` (U69);
  - identity `Developer ID Application: Ioka LLC (5PX7D99PXA)`, team
    `5PX7D99PXA`;
  - pinned leaf `CERTIFICATE_SHA256` (`ci-macos-sign.py:26-30`).

## B.2 Checked-in signing-identity file (C10) [I]

Proposed path: `scripts/signing-identity.json`, schema
`airk-signing-identity/1`. It contains only public identifiers; there are no
secrets, user names or absolute paths.

~~~json
{
  "schema": "airk-signing-identity/1",
  "windows": {
    "publisher": "CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US",
    "identity_eku": "1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669",
    "artifact_signing": {
      "endpoint": "https://wus2.codesigning.azure.net",
      "account": "ioka-llc-signing",
      "certificate_profile": "WritingForgePro",
      "file_digest": "SHA256",
      "timestamp_url": "http://timestamp.acs.microsoft.com",
      "timestamp_digest": "SHA256",
      "credential": "AzureCliCredential",
      "excluded_credentials": ["EnvironmentCredential", "WorkloadIdentityCredential",
        "ManagedIdentityCredential", "SharedTokenCacheCredential", "VisualStudioCredential",
        "VisualStudioCodeCredential", "AzurePowerShellCredential",
        "AzureDeveloperCliCredential", "InteractiveBrowserCredential"]
    },
    "local_client": {
      "package": "Microsoft.ArtifactSigning.Client", "version": "1.0.146",
      "nupkg_sha256": "<full value from ledger S0.3>",
      "dlib": "bin/x64/Azure.CodeSigning.Dlib.dll",
      "dlib_sha256": "<full value from ledger S0.3>",
      "dlib_publisher": "<reviewed Microsoft subject>"
    }
  },
  "macos": {
    "team_id": "5PX7D99PXA",
    "identity": "Developer ID Application: Ioka LLC (5PX7D99PXA)",
    "certificate_sha256": "<value of ci-macos-sign.py:30>",
    "local": {"keychain": "~/Library/Keychains/airk-signing.keychain-db",
              "notary_profile": "airk-notary",
              "unlock_item": "airk-signing-keychain-password"}
  }
}
~~~

The exclusion names must match the probe's metadata file exactly. Copy them
from the probe, not from this draft.

Readers (one owner: a small loader in `sign-native.py`; others import it):

| Reader | Use |
|---|---|
| `sign-native.py` | `AZURE_SIGNING` is built from it with **exactly** today's five keys and values, so hosted receipts and `github-release.py:203,335` equality checks stay byte-identical. Also the local-mode metadata, the DLL pin and macOS team checks. |
| `ci-macos-sign.py:26-30`, `package-standalone-macos.py:20`, `check-macos-direct-runtime.py:24`, `github-release.py:196-198,356` | Team, identity and certificate pin. Behavior is unchanged; only the source of the constants moves. Changing hosted files is a separate reviewed step: U70 keeps the hosted *path and guards* unchanged, and this only relocates constants. |
| `package-standalone-windows.py:143-150` | Installer and uninstaller verification policy. |
| Installers | Through their packagers. Inno `AppPublisher` and macOS/Linux wrappers get the publisher and team as defines (no hand copies). |
| Extension build (Phase 6, §8.2) | Reads the JSON at build time for `airkctl` signature verification (Windows publisher + EKU, macOS team). This consumes the checked-in file; it is not a hand-written TS mirror. |
| Workflows | Keep literal action inputs. A contract test asserts that `github-release.yml` endpoint/account/profile/timestamp/exclusions equal the file (the existing `test_standalone_windows.py:231` counts the profile 3 times). |

Unit test: the loaded `AZURE_SIGNING` equals a reviewed literal in the test.
This is an independent expectation and is not derived from the file.

## B.3 `sign-native.py` local Artifact Signing mode [I]

**Arguments** (Windows targets only):

| Argument | Rule |
|---|---|
| `--artifact-signing-dlib PATH` | New. Absolute path to a regular file (not a link), named as in `local_client.dlib`. It must not be under the repository root, any §9.5 install root or any pre-split AIRK root. Its SHA-256 must equal `local_client.dlib_sha256`, and Authenticode must be Valid with the pinned Microsoft subject. Mutually exclusive with a thumbprint `--identity`. |
| `--identity` | Becomes optional only when `--artifact-signing-dlib` is given. Otherwise it keeps today's 40-hex rule (:341). |
| `--signtool` | Full path required in local mode (U48). The file must be Microsoft-signed. Record its digest. |
| `--timestamp-url` | Must equal `artifact_signing.timestamp_url` in local mode; it defaults to that value. |

**Hosted separation:**

- Local mode refuses to run when `GITHUB_ACTIONS == "true"`, so hosted
  Windows signing stays on the pinned action.
- It reads no other `GITHUB_*` value and never sets one.
- `ci-windows-sign.py` (its guard, `prepare`, `record`, `list`) is untouched.

**Metadata:**

- Written per invocation into a fresh `tempfile.TemporaryDirectory(prefix="airk-artifact-signing-")`:
  `{"Endpoint","CodeSigningAccountName","CertificateProfileName","ExcludeCredentials"}`,
  built from the identity file. It holds no secret or token. The directory is
  deleted afterwards, and the file's SHA-256 is recorded.
- A preflight (`az account show` exit status only; no token output) fails
  early with a clear message when the Azure CLI login is missing.

**Command:**

```text
[signtool, "sign", "/v", "/fd", "SHA256", "/tr", ts, "/td", "SHA256",
 "/dlib", dlib, "/dmdf", metadata, binary]
```

- There is no `/sha1`.
- Then the observed identity: `verify-authenticode.ps1 -Binary -Publisher
  -IdentityEku` → thumbprint.
- Then the existing `verify(binary, receipt)` with
  `receipt.identity = observed thumbprint` and `receipt.azure = AZURE_SIGNING`.
- Any failure stops with no success receipt (today's rule at :396).

**Receipts (local and hosted kept distinct):**

| Receipt | Hosted (unchanged) | Local |
|---|---|---|
| Component | `airk-native-signing/1` | `airk-native-signing-local/1`: hosted fields plus `local_signing: {mode:"artifact-signing-dlib", client:{package,version,dlib_sha256}, signtool_sha256, metadata_sha256, credential:"AzureCliCredential"}`. **No paths** (they contain user names). |
| Pair | `airk-service-pair-signing/1` | `airk-service-pair-signing-local/1` |
| Standalone installer | `airk-standalone-installer/1` | `airk-standalone-installer-local/1` |

- `github-release.py:161,186,303` already require the hosted schemas exactly
  [V], so a local receipt cannot enter release assembly. A regression test pins
  this.
- macOS on the Neo uses the same Developer ID path as hosted
  `ci-macos-sign.py`, which calls `sign-native.py`. For it, the schema is
  chosen by a read-only `hosted_context()` predicate equal to the
  `ci-macos-sign.py:33-41` guard conditions. Outside that context the receipts
  are local, so the hosted path keeps its bytes and `ci-macos-sign.py` is
  unchanged.
- Spoofing the environment is forbidden by policy. It still could not produce
  hosted provenance, because release assembly also requires authenticated
  producer artifacts (§10.5.4).

**Pair functions:**

- `sign_pair` (:270-319) forwards `--artifact-signing-dlib` (and `--signtool`)
  to each child, for both `airk.exe` and `airkctl.exe`. On macOS it passes
  `--keychain`/`--keychain-profile` as today, signing nested `airkctl` before
  the bundle.
- `read_verified(…, accepted={hosted|local})` checks the schema the caller
  asked for.
- `finish_pair` (:217-267):
  - requires every component to have the same schema;
  - rejects a mixture of local and hosted components;
  - writes the matching pair schema;
  - keeps the existing inventory and unrelated-payload checks;
  - rewrites `airk_sha256` and `airkctl_sha256`.
- `prepare_pair` is unchanged.
- `read_verified_pair` accepts both schemas and returns which one it saw.
  `package-native.py --signed-dir` records the local signing receipt
  verbatim; the candidate is then rejected by `github-release.py`.

**macOS local batch (U69):**

- The Neo batch does the following:
  1. unlocks the login keychain using `~/.airk-signing/keychain-pass`;
  2. reads `airk-signing-keychain-password` into memory and unlocks the AIRK
     keychain;
  3. runs `sign-native.py --runtime-dir … --identity "Developer ID Application: Ioka LLC (5PX7D99PXA)" --team-id 5PX7D99PXA --notarize --keychain-profile airk-notary --keychain ~/Library/Keychains/airk-signing.keychain-db`;
  4. staples and runs Gatekeeper checks;
  5. locks both keychains in a `finally`.
- The password passes through stdin/memory only, as
  `ci-macos-sign.py::private_run` does. It never appears in argv, logs or
  receipts. Whether `security unlock-keychain` reads the password from stdin
  needs checking on the Neo [I].
- No `sign-native.py` change is needed for unlocking.

## B.4 Windows installer order with local signing [I]

Hosted (unchanged): `prepare` → action signs `uninstaller.exe` → `build` →
action signs the installer EXE → `record`.

Local (driven by `build-local-service-installer.ps1`):

1. `sign-native.py --target x86_64-pc-windows-msvc --runtime-dir <runtime> --output artifacts/signed/<t> --artifact-signing-dlib <dll> --signtool <full path>`.
   This signs both images and writes `airk-service-pair-signing-local/1`.
2. `package-standalone-windows.py prepare --runtime-dir … --signed-dir …`.
   `native_inputs` accepts the local pair receipt.
3. `package-standalone-windows.py sign-local --file uninstaller --artifact-signing-dlib … --signtool …` (new stage):
   - calls the shared `signer.sign_file_local(path)` in place;
   - the path must be exactly `<output>/uninstaller.exe`;
   - writes the sidecar `uninstaller.local-signing.json` with
     unsigned/signed digests and the `local_signing` block.
4. `build`: unchanged. It verifies the signature (`verified_signature`, the
   same publisher/EKU policy), embeds the signed bytes and checks the cache
   digest.
5. `sign-local --file installer`: signs `<output>/<artifact>` in place and
   writes the sidecar `installer.local-signing.json`.
6. `record`:
   - with both sidecars present and matching the observed digests, it writes
     `airk-standalone-installer-local/1`;
   - with no sidecars, it writes the hosted schema (unchanged);
   - with exactly one sidecar, or a sidecar whose digests differ, it refuses.

The order native pair → uninstaller → embed → installer is preserved
(§10.5.4). Every signing failure stops the build with a non-zero exit, and no
unsigned replacement is produced (§10.5.1). The VSIX uses ordinary Marketplace
signature handling and has no native signing (U22).

## B.5 Unit tests to add

All tests are mocked. They test construction and refusal, not real signing.

`scripts/tests/test_native_signing.py` (today 8 tests, :17-279 [V]):

1. The local command is exactly `signtool sign /v /fd SHA256 /tr <file ts> /td SHA256 /dlib <dll> /dmdf <tmp> <copy>`, with no `/sha1`.
2. Generated metadata equals the reviewed literal: endpoint, account, profile,
   exclusions = everything except `AzureCliCredential`. The temporary file is
   removed after success **and** after failure.
3. The DLL is refused when it is a link, under the repository root, under an
   AIRK root, has the wrong name or a digest mismatch, or has an untrusted
   signature. Each refusal happens before any signtool call.
4. The `--artifact-signing-dlib` + thumbprint `--identity` combination is
   refused. Without the DLL, the thumbprint rule is unchanged. A non-matching
   `--timestamp-url` is refused.
5. Local mode is refused under `GITHUB_ACTIONS=true`. The adapter never writes
   or reads other `GITHUB_*` values; the test asserts that the child
   environment is unmodified.
6. A signtool failure, a missing timestamp or a publisher/EKU failure leaves no
   `signing.json`.
7. A local component receipt has `airk-native-signing-local/1`, the observed
   thumbprint, `azure == AZURE_SIGNING`, and a `local_signing` block with
   digests and **no path strings**.
8. `finish_pair` rejects mixed local and hosted components. It writes
   `airk-service-pair-signing-local/1` for an all-local pair and rewrites
   `airkctl_sha256`.
9. `sign_pair` forwards the DLL and signtool to the child for both images.
   The pair requires exactly `{airk.exe, airkctl.exe}`.
10. `hosted_context()` false selects the macOS local schema. When true, the
    hosted schema is unchanged and byte-identical to a reviewed fixture.
11. The `AZURE_SIGNING` derived from the identity file equals the reviewed
    literal.

Companion tests:

- `test_ci_windows_signing.py`: the guard is still required (existing :42).
  The workflow action inputs (endpoint, account, profile, timestamp,
  exclude-* flags) equal the identity file.
- `test_github_release.py`: all three local schemas are rejected.
- `test_standalone_windows.py`:
  - the `sign-local` stage order;
  - sidecar presence, partial and mismatch cases;
  - a local installer receipt is never the hosted schema;
  - the A.5.1 compiler-identity cases.
- New U37 wrapper cases (§10.5.5): a missing DLL, Azure login or Inno fails
  before compilation; `-ForceBuild` bypasses reuse; build-only never
  installs.

## B.6 Documentation to update in the implementing step

- `docs/native-signing.md`: a new "Local Windows signing (Artifact Signing
  client)" section with the explicit inputs. It states that local signatures
  do not establish hosted provenance (U22/U60).
- CONTRIBUTING: prerequisites.
- The U37 script help.
- `docs/simplify/one-process-ledger.md`: record the full digests (the client
  nupkg and DLL are currently recorded only by prefix).
