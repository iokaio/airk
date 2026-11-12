# Split inventory: pre-split installations (P0.4)

**Status:** Phase 0 draft, 11 November 2026; source review only; authorizes no deletion.

This inventory serves [split-plan.md](../split-plan.md) §9.3 (roster and rules), §9.6
(transaction), U13–U15, U39 and U50. For every roster artifact class it records how
the item is found, which AIRK ownership receipt proves it, the shutdown proof it
needs, and the reviewed fixture cases and expected installer outcome that the Phase 7
**retirement** module (setup role, §10.2 last row) will be tested against. §8 gives
rows in the [simplify-plan.md](../../simplify-plan.md) Appendix D format, which
extend [compatibility-inventory.md](../../simplify/compatibility-inventory.md).

Method and limits:

- Read-only review on `feat/split` at `caab8ec9`. The AIRK MCP server failed to
  connect (connection closed). Evidence comes from `rg` and direct reads; three
  read-only sub-investigations covered login registrations, the extension
  footprint, HTTPS trust and the Inno script. No build or test ran. Test names are
  **proof candidates**, not passing evidence.
- **[V]** means verified by reading the cited code. **[I]** means inferred and
  still needs confirmation.
- Rust paths are relative to `crates/airk-cli/src/` unless prefixed. Extension
  paths are relative to `packages/vscode/src/`. "SM" means `service_manager.rs`.
- Paths use `%LOCALAPPDATA%`, `%APPDATA%`, `~` and `$XDG_*`. `<leaf>` is a channel
  leaf. `<root>` is a pre-split install root.

## 1. Pre-split path map and §9.5 collision check

`UserPaths::historical` (`user_paths.rs:344-403`) gives the historical
`service-1.4[-preview]` family. `select_layout` (`:405-436`) gives the newer
`runtime[-preview]` family. `owned_layout_locations` (`:288-320`) recognizes both
for removal. [V]

| Location | Windows | macOS | Linux |
|---|---|---|---|
| Install root, historical | `%LOCALAPPDATA%\AIRK\service-1.4[-preview]` | `~/Library/Application Support/AIRK/service-1.4[-preview]` | `~/.local/lib/airk/service-1.4[-preview]` |
| Install root, newer | `%LOCALAPPDATA%\AIRK\runtime[-preview]` | `…/AIRK/runtime[-preview]` | `~/.local/lib/airk/runtime[-preview]` |
| Settings | `%APPDATA%\AIRK\service-1.4…` or `settings[-preview]` | `…/AIRK/configuration/<leaf>` | `$XDG_CONFIG_HOME/airk/<leaf>` |
| Data | `%LOCALAPPDATA%\AIRK\data\service-1.4…` or `data\index-v1[-preview]` | `…/AIRK/data/<leaf>` | `$XDG_DATA_HOME/airk/<leaf>` |
| State (control is `state/control`) | `%LOCALAPPDATA%\AIRK\state\service-1.4…` or `state\service-v1[-preview]` | `…/AIRK/state/<leaf>` | `$XDG_STATE_HOME/airk/<leaf>` |
| Cache | `%LOCALAPPDATA%\AIRK\cache\…` (`cache-v1`) | `~/Library/Caches/AIRK/<leaf>` | `$XDG_CACHE_HOME/airk/<leaf>` |
| Logs | `<state>\logs` | `~/Library/Logs/AIRK/<leaf>` | `<state>/logs` |
| Universal launcher | `%LOCALAPPDATA%\AIRK\launcher[-preview]` | `…/AIRK/launcher[-preview]` | `~/.local/lib/airk/launcher[-preview]` |
| Inno `standalone` (Windows only) | `%LOCALAPPDATA%\AIRK\standalone` (`installers/windows/airk.iss:14`) | — | — |
| Login definitions (`configuration_base`, `user_paths.rs:236-254`) | `%APPDATA%\AIRK\services\<id>.xml` | `~/Library/LaunchAgents/<id>.plist`; SMAppService plist inside the bundle | `$XDG_CONFIG_HOME/systemd/user/<id>.service` |
| 1.3 scheduler receipts (SM:255) | `%APPDATA%\AIRK\airk\services\<id>.json` | `…/Application Support/AIRK/airk/services/<id>.json` | `$XDG_CONFIG_HOME/airk/services/<id>.json` |

**§9.5 collision check (U36):**

- **Windows.** `%LOCALAPPDATA%\Programs\AIRK\` is outside every pre-split root.
  No pre-split code writes under `Programs`. [V] (U96 later moved every Windows
  default to `%LOCALAPPDATA%\AIRK Service\`, a sibling of the pre-split
  `%LOCALAPPDATA%\AIRK` root and outside it.)
- **Linux.** `~/.local/lib/airk-service/` is a sibling of `~/.local/lib/airk/` and
  outside every pre-split root. [V]
- **macOS.** `AIRK.app` directly under `~/Library/Application Support/AIRK/` is
  outside every pre-split root. [V] However, that parent folder is also the macOS
  `configuration_base` and holds pre-split roots and 1.3 receipts. Retirement must
  therefore **never remove a shared parent** (`…/AIRK`, `%LOCALAPPDATA%\AIRK`,
  `data`, `state`, `cache`). [V]
- **New state, data and cache names** (§9.5: "new folder names under the existing
  per-user AIRK roots") must avoid every leaf above, `*.removed-*`,
  `.<leaf>.replacement-1`, `.<leaf>.replace-1.lock` and `standalone`. [V]
- **The U39 install pointer path** is not defined yet (I4, Phase 1). Its collision
  check is open (Q8).

## 2. Common fixture outcome cases

Every roster row below lists which of these cases apply and any row-specific
detail. **Expected installer outcome** is the U14 rule: install anyway and report.

| Case | Fixture | Expected outcome |
|---|---|---|
| **R** receipt proven | Valid receipt that names this exact item and digest | Removed; listed under "removed" |
| **U** unproven | Receipt absent, malformed, crossed (names another root, user or digest) or item changed after the receipt | Kept; listed with reason code; installation continues |
| **L** link or redirect | Item or any ancestor is a symlink, junction or reparse point (`0x400`), or is hard-linked | Not followed and kept; reason `redirected_location` |
| **K** locked | File open or mapped; directory rename or delete fails | Kept; reported; journaled for resume |
| **I** interrupted | Crash after each durable boundary (receipt frozen, registration removed, trust removed, images unlinked, folders aside) | Next run resumes the recorded operation. It never re-plans over an aside folder and never resets the new installation |
| **S** sources | A registered source root inside, equal to or containing a removal location; a sentinel source file | Source bytes unchanged. An overlapping location is kept and reported |
| **X** restart-capable leftover | Unproven login registration, or a definition that still starts an old image | Reported first, in plain words |

**Behavior gap against current code (Phase 7 behavior step, not structural).**
Today `replacement.rs` refuses the **whole** replacement in several cases:

- any unproven location (`ensure!` at `:625-634`, `require_owned` `:873-896`);
- trust residue (`:915-919`, `remove_installation_trust_incomplete`);
- a failed startup removal (`:986-988`);
- an unproven stop (`:1155-1159`).

U14 needs per-item "kept and reported" with continuation. §9.6 step 5 still forbids
running a half-installed successor. Tests that assert whole-plan refusal need
reviewed new expectations in that behavior step:

- `replacement_refuses_a_damaged_layout_naming_an_unrelated_folder_and_changes_nothing`
- `a_layout_folder_without_an_airk_ownership_record_is_refused_by_name`
- `replacement_proves_ownership_and_blocks_folder_removal_until_trust_is_retired`
- `a_process_stop_failure_blocks_every_move_and_cannot_be_reported_as_success`

Two cases must stay refusals:

- an executing image that cannot be stopped inside a location (K, with no removal
  of that location);
- a source overlap (S).

## 3. Roster

### A1. Stable and preview install roots, with state, data, cache, log and settings folders

| Aspect | Record |
|---|---|
| Discovery | Fixed paths: `UserPaths::historical` and `owned_layout_locations` (`user_paths.rs:288-320,344-403`) [V]. Plus `runtime-layout.json` `state` and `logs` fields (`replacement.rs:264-275`) [V], and each payload under `<root>/runtimes/<sha>/` (`airk-install.json`, `approval.json`; `runtime_install/retirement.rs:449-476`) [V]. Both channels and both families are pre-split (U17). Today's code scopes discovery to one install root (`owned_install_locations`, `user_paths.rs:274-286`). The roster needs all four roots. |
| Ownership receipts | Install root: `selected.json`, `runtime-layout.json` or `runtimes/` present (`replacement.rs:189-193`) [V]. **This is name/shape evidence, not a digest receipt.** State: the claimed `airk-state-1.4.owner.json` via `Layout::claimed` (`install_layout/mod.rs`; `replacement.rs:197-199`) [V]. Control: `control/airk-control-1.4.owner.json` plus `installation_for_control == root` (`replacement.rs:181-187`) [V]. Logs: `logs-1.4.owner.json` via `service_logs::has_saved_owner` (`service_logs.rs:81`; `replacement.rs:201-203`) [V]. Data: `service-data-1.4.owner.json` plus `data_scope::has_saved_binding` (`airk-store/src/service/data_scope.rs:196`; `replacement.rs:204-211`) [V]. Stable launcher: `launcher.json` `{schema:1, sha256}` (`replacement.rs:510-523`) [V]. Settings folder: **none found**. `owned_by_airk` has no branch for `configuration`, so a settings folder outside the install root is refused today [I: traced; no test names it]. |
| Shutdown proof | Images: every `runtimes/*` image from the verified superseded inventory (`retirement/superseded.rs:153`), the stable launcher by `launcher.json` digest, process-role aliases (`upgrade_stop.rs:177-206`), and the pair helper `airk-embed` for 1.8.11-era payloads (`:179-181`). In replacement, every executable under the owned locations (`replacement.rs:693-742`, symlinks and reparse points skipped). Stop: `upgrade_stop::stop_paths` (`:214-251`, 5 s budget). Windows: `OpenProcess` → `QueryFullProcessImageNameW` must equal the expected path and the same user → `TerminateProcess` and wait on the held handle (`:266-360`). Linux: `/proc` (`:429-496`). macOS: `runtime_upgrade_stop_macos.rs`. Absent images need a complete zero-match `legacy_processes::audit` (`replacement.rs:1026-1034`). Unlink happens under `image_exclusion` (`legacy_launchers.rs:375`; `replacement.rs:1076-1109`) [V]. |
| Cleanup and resume | Operation receipt `<parent>/.<leaf>.replacement-1/receipt.json` (`replacement/operation.rs:38-48`): schema 1, locations, index roster, clients, directory identities, `frozen`, image roster, `completed`. Lock `<parent>/.<leaf>.replace-1.lock` (`replacement.rs:748-781`). Folders are renamed to `<name>.removed-<op>` and then deleted (`:1167-1227`). Resume runs through `operation::resume` (`:830-832`) [V]. |
| Fixture cases | R, U, L, K, I, S, X. Specific fixtures: **(a)** historical and newer families both present in one channel; **(b)** stable and preview both present, so both are removed (U17), which changes today's channel-scoped rule; **(c)** a state folder claimed by another root (`replacement_refuses_default_named_locations_owned_by_another_installation`) → kept and reported; **(d)** a layout without claim → kept; **(e)** a registry present but no `runtime-layout.json` → kept with `remove_installation_legacy_state_unproven` (`:635-648`); **(f)** a managed root (`managed-runtime.json`, `managed-detach.json`, `managed_catalog_origin`) → kept and reported (`replacement.rs:219-255`; Q5); **(g)** a settings folder → kept and reported unless Q2 adds a proof; **(h)** an interrupted pre-split `.removed-<op>` folder with its receipt → resumed; **(i)** the setup image inside a removal location → refused (`require_outside`, `:850-870`). |
| Platforms; Windows unit test | All three. Ownership decisions and location rosters are filesystem-only (tempdir fixtures, as in `replacement/tests.rs`), so they are buildable on Windows. `decide`/`decide_install` are pure (`:106-161`). Process stop needs an injected observer for non-host platforms; today `stop` is injected only in tests (`apply_plan_with_stop`, `:1112-1121`). |

### A2. Custom install roots (universal-launcher pointer and login registrations)

| Aspect | Record |
|---|---|
| Discovery | **(i) Pointer.** `<launcher dir>/airk-launcher.json` `Pointer {schema:1, install, launcher_sha256}` (`universal_launcher.rs:22,64-74`) names the install root [V]. **(ii) Login registrations.** No current code enumerates registrations; every operation computes one expected name from a known root (SM:231-236) [V]. The root can be recovered from the saved definition: Windows XML `<Arguments>` (`--install-dir <root>`), the unit `ExecStart=`, or the macOS job `program` (two levels above `runtimes/<sha>`, `runtime_macos_login.rs:427-442`) [V]. Schema 1 points to `<root>/airk-launcher(.exe) runtime foreground --install-dir <root>` (SM:194-208). Schema 2 points to `<root>/runtimes/<sha>/<main> runtime managed-serve --install-dir <root>` (SM:355-381) [V]. **New code:** enumerate `airksvc-*` definitions and parse them. The candidate is verifiable because `id = "airksvc-" + sha256("<principal>\0<canonical root>")[..32]` must equal the file stem (SM:225-236) [V]. |
| Ownership receipts | Pointer: valid only when the binary digest equals `launcher_sha256` (`universal_launcher.rs:430-436`). The root itself then needs the A1 receipts. Registration: `<root>/startup-owner.json` (A3). Discovery evidence is never removal authority. |
| Shutdown proof | As A1, for the discovered root. |
| Fixture cases | R, U, L, X. Specific fixtures: **(a)** a pointer to a root with valid A1 receipts → root removed; **(b)** a pointer whose binary digest differs → pointer and root kept (`changed universal launcher preserved`); **(c)** an XML whose `--install-dir` hash does not match its file stem → kept and reported X; **(d)** a definition naming a root that has no receipts → kept, reported X first; **(e)** a definition naming a source folder → kept (S). |
| Platforms; Windows unit test | All. Parsing saved XML, unit and plist text and checking the hash are pure. Test on Windows with text fixtures. The Windows XML is UTF-16LE with BOM (SM:893-899). |

### A3. Login registrations

| Kind | Discovery | Receipt (writer / reader) | OS-side proof | Removal today |
|---|---|---|---|---|
| Windows `airksvc-<32hex>` task | Computed id; saved XML `%APPDATA%\AIRK\services\<id>.xml` (SM:243-244) [V] | `<root>/startup-owner.json` `{schema, id, data_root, definition_sha256, runtime?}` (SM:60-79, 250-253; written SM:547-557, read SM:383-398) [V] | Registered task XML equals the saved XML after normalization (`service_manager_windows.ps1:26-59`) [V] | `check_owned` (SM:868-885) → `schtasks /Delete /F` → delete the definition and the receipt (SM:630-658) [V] |
| Linux `airksvc-<32hex>.service` | `$XDG_CONFIG_HOME/systemd/user/<id>.service` (SM:245) [V] | Same receipt | systemd bus checks: FragmentPath, no drop-ins, exact ExecStartEx and properties (`service_manager_linux.rs:356-504`) [V] | `systemctl --user disable`, delete, `daemon-reload` (SM:646-658) [V] |
| macOS `io.iokaio.airk.service.<stable\|preview>` SMAppService agent | Fixed label (`runtime_macos_login.rs:207`); plist inside `<root>/runtimes/<sha>/AIRK.app/Contents/Library/LaunchAgents/` (`:220-222`) [V] | `<root>/mac-login-1.4.json`, framed digest `mac-login-ownership/1.4` (`service_manager_macos.rs:8,29-58,98-142`) [V] | `launchctl print gui/<uid>/<label>` job scope: path, program, args (`runtime_macos_login.rs:205-224`) [V] | **Unregister runs the old installed binary** (`service_manager_macos.rs:143-164,300-337`) [V]. This conflicts with §9.3 "No old binary is executed" (Q1) |
| macOS legacy `airk-<hash>` LaunchAgent | `~/Library/LaunchAgents/<id>.plist` (SM:246-248) [V] | 1.3 receipt (next row) | `launchctl print`, exact program and args (SM:698-723) [V] | Through `legacy_startup` [V]. An `airksvc-` LaunchAgent is refused outright (SM:737-743) |
| 1.3 `airk-<32hex>` schedulers (all OSes) | Computed from `sha256(principal\0canonical data_dir)` (SM:155,231-236). Inputs come from the operator (`legacy_startup.rs:15-29`) [V]. **New:** enumerate the receipt directory | `<configuration_base>/airk/services/<id>.json` (SM:255) [V] | As for the platform | Explicit `legacy_startup` flow. Approval digest covers file sha256, size and mtime (`legacy_startup.rs:125-164`). Journal `<control>/legacy-retirement-1.4/<approval>.json` (`legacy_startup/journal.rs:118,319`). Refusals: changed files, changed registration, running worker, reappearing files (`:337-456`) [V] |

- **Shutdown proof.** Remove or unregister the registration **before** stopping
  images, so login cannot restart them (`replacement.rs:973-989`, §10.3 order) [V].
  Then stop A1 images.
- **Fixture cases.** R, U, X, I. Specific fixtures:
  - **(a)** receipt plus matching definition plus matching OS state → removed;
  - **(b)** receipt missing → definition and task kept, **reported X first**;
  - **(c)** definition edited (sha differs from `definition_sha256`) → kept, X;
  - **(d)** OS task differs from the saved XML → kept, X;
  - **(e)** OS reports not loaded → definition and receipt removed without an OS
    delete (SM:652);
  - **(f)** macOS: the bundle is gone but the label is still registered → kept, X
    (Q1);
  - **(g)** crash between the OS delete and the receipt delete → resume sees "not
    loaded" plus receipt → finishes.
- **Platforms; Windows unit test.**
  - Ungated pure tests exist for receipt and definition logic and for parsing
    `launchctl` text (§6).
  - Windows Task Scheduler comparison is `cfg(windows)` through a PowerShell
    adapter.
  - The Linux bus module is Linux-only (SM:20).
  - A retirement decision table over injected observations (receipt, definition
    bytes, OS state) can run on Windows.

### A4. Universal launcher folder

| Aspect | Record |
|---|---|
| Discovery | Fixed `<parent of install>/launcher[-preview]` (`user_paths.rs:30-36,388-393`) [V] |
| Ownership receipts | Pointer `airk-launcher.json` naming this root, with the binary digest; or `<root>/universal-uninstall.json` (copy of the pointer) for interrupted removal (`universal_launcher.rs:391-437,446-510`) [V]. The folder also holds `airk-launcher-publish.lock` (`:24`) and the extension's `extension-uninstall/` receipts (A8). Neither has a removal rule today. |
| Shutdown proof | Stop the universal image by exact path. Its digest comes from the pointer or `universal-uninstall.json` (`upgrade_stop.rs:86-96,156-172`; `universal_launcher.rs:365-389`) [V] |
| Fixture cases | R, U, L, K, I. Specific fixtures: **(a)** pointer to a removed root plus matching binary → image, pointer and receipt removed; **(b)** binary only, or pointer only → kept, `partial universal launcher publication preserved` (`:430-433`); **(c)** pointer to another root (custom installation) → that root follows A2; **(d)** crash after the image unlink → resume through `universal-uninstall.json`; **(e)** an unrecognized publication lock → kept (`unrecognized_publication_lock_is_preserved_without_touching_launchers`); **(f)** the empty folder after A8 → removed only if empty (Q3). |
| Platforms; Windows unit test | All; tempdir fixtures already run on Windows. Unix adds owner-only mode checks (`:105-119`). |

### A5. Inno `standalone` folder, its PATH entry and uninstall registration (Windows only)

| Aspect | Record |
|---|---|
| Discovery | Fixed `%LOCALAPPDATA%\AIRK\standalone` (`installers/windows/airk.iss:14`, `DisableDirPage=yes`) [V]. Uninstall key `HKCU\…\Uninstall\{87B12FF2-FF1A-4E65-AD6C-53CB538E6303}_is1` [I: Inno default for `AppId` at `:9`, `PrivilegesRequired=lowest`]. Start-menu shortcut `{userprograms}\AIRK Setup` (`:39`) [V]. **No native code reads any of these today** [V by `rg`]. |
| Ownership receipts | `standalone\user-path-owner.txt`, 6 UTF-8 lines: `airk-user-path/1`, `pending\|owned`, the entry (`{app}\runtime`), whether PATH existed before, PATH before, PATH after (written `airk.iss:164-194`, read `:122-138`) [V]. **It proves only the PATH edit.** The folder, `unins000.exe/.dat`, the shortcut and the uninstall key carry no AIRK receipt (Q4). The packaged `runtime\airk.exe` is pinned by `AirkBinarySHA` (`:231-237`) and `airk-install.json` [I]. |
| PATH removal rule (port to native) | Only an `owned` receipt; remove the single exact entry, case-sensitive (`:213`); a duplicate entry is ambiguous → keep; delete the value only if it did not exist before and nothing remains; then delete the receipt (`:196-229`) [V]. `pending` → keep and report. |
| Shutdown proof | `standalone\runtime\airk.exe` and any role alias, stopped by exact image path. Agents may have started `airk` from PATH. |
| Fixture cases | R (owned receipt and exact entry), U (no receipt; entry present without receipt → kept, "no ownership claimed" `:177-179`), `pending` receipt → kept, duplicate entry → kept, K (runtime image held), I (PATH rewritten but receipt not deleted → resume sees the entry absent and deletes the receipt), L (`standalone` is a junction → kept). Old `unins000.exe` is **never run** (§9.3). |
| Platforms; Windows unit test | Windows only. Receipt parsing and PATH rewriting are pure string functions with an injected registry read and write. |

### A6. VS Code global-storage helper caches and CLI copies (Code, Insiders, VSCodium)

| Aspect | Record |
|---|---|
| Discovery | `native_global_stores`: `<base>/{Code, Code - Insiders, VSCodium}/User/globalStorage`. The base is `%APPDATA%` on Windows, `~/Library/Application Support` on macOS and `$XDG_CONFIG_HOME` on Linux (`runtime_legacy_origin.rs:698-714`) [V]. Plus `User/profiles/<id>/globalStorage`, at most 128 (`:737-770`) [V]. Helper caches: `iokaio.airk/helpers/<version>-<platform>-<digest16>/airk[.exe]` (`:221,959-997`) [V]. CLI copies: `iokaio.airk/cli/bin/airk[.exe]` (retired), `iokaio.airk/service-cli/bin/airk.exe` (Windows), and wrappers `service-cli/bin/airk` / `airk.cmd` (`cliInstall.ts:20-41,216-259`) [V]. |
| Ownership receipts | **Helper cache: self-describing; no separate receipt.** The name's 16-hex prefix must match the full image sha256, the version must be obsolete (≥0.2.1 and below the running version), the only entry must be the image, and the directories must be user-owned (`runtime_legacy_origin.rs:326-453,998-1014`) [V]. Retirement journal `legacy-cache-retirement.json` (`runtime_install/legacy_caches.rs:9`) [V]. **CLI copies: no receipt.** The extension removes them by fixed location and regular-file check (`cliInstall.ts:216-240`). Wrappers are removed by exact shape regex (`cliRemove.ts:60-72`) [V]. Indirect evidence: an `extension-uninstall/<sha>.json` receipt naming `storage` (A8). Whether that suffices is Q6. |
| Shutdown proof | Each helper image by exact path under `image_exclusion` (`legacy_caches.rs:3`, `legacy_launchers.rs:360-425`) [V]. CLI copies by exact path. Old stdio MCP clients may hold them. |
| Fixture cases | R (well-formed obsolete cache), U (current or future version, `current, future or unknown helper cache version preserved for review`; wrong prefix; extra entries), L (redirected storage, `redirected editor global storage layout`, `:722-735`; hard-linked image), K (image held by a stdio client), I (empty leaf with a journal → resume, `:414-441`). **Never remove `globalStorage/iokaio.airk` itself.** The split extension has the same id and owns that folder (U12) [I]. Only the enumerated sub-items are removed. |
| Platforms; Windows unit test | All. Name, version and prefix parsing is pure. Directory-identity fixtures already run on Windows (`runtime_legacy_origin/tests.rs`). Note: `cache_name` accepts only the host OS platform (`:971-996`). |

### A7. PATH and shell edits written by the old extension

| Edit | Discovery | Receipt | Removal rule today |
|---|---|---|---|
| POSIX rc block `# AIRK CLI PATH` … `# End AIRK CLI PATH` in `~/.profile`, `~/.bashrc`, an existing `~/.bash_profile`/`~/.bash_login`, `$ZDOTDIR/.zprofile`/`.zshrc` (`cliInstall.ts:10,71-74,136-160`) [V] | Receipt `profiles[]` plus default candidates (`cliRemove.ts:16-22`) | `<launcher dir>/extension-uninstall/shell-<sha256(storage)>.json` `{schema:1, storage, profiles:[{file, created}]}` (written `extensionUninstall.ts:112-134`, read `:136-139,190-196`) [V] | Exact byte match of the block for the two owned directories, in three framings. Delete the file only if `created` and now empty. Leftover marker text is preserved and reported (`cliRemove.ts:30-59,127-138`) [V] |
| fish `$XDG_CONFIG_HOME/fish/conf.d/airk-path.fish` (`cliInstall.ts:75-79,161-165`) [V] | Same | Same | Same |
| Windows `HKCU\Environment\Path` entry `<storage>\service-cli\bin` or `<storage>\cli\bin` (`cliInstall.ts:170-196`) [V] | Owned directories derived from `storage` | **No receipt for the entry itself.** `storage` comes from the generic receipt `extension-uninstall/<sha>.json` `{schema:1, installDir, storage, spec?}` (`extensionUninstall.ts:54-69`) [V] | Case-insensitive match after expansion and trimming; keeps the value kind; broadcasts `WM_SETTINGCHANGE` (`cliRemove.ts:77-110`) [V] |
| `environmentVariableCollection` PATH prepend (VS Code terminals) (`cliInstall.ts:261-268`) | — | none | VS Code-owned state; native setup cannot reach it [I]. Out of installer scope; the split extension clears it (Q7) |

- **Note.** Receipts exist only when the extension had `uninstallDirectory`
  (`serviceExtension.ts:2817-2821`) [V]. A missing receipt means U, and the native
  installer then reports any `# AIRK CLI PATH` marker it sees.
- **Fixture cases.**
  - R: exact block plus receipt → block removed; created and empty → file deleted.
  - U: block edited by the user → kept with the marker reported; no receipt →
    kept and reported.
  - L: profile is a symlink → kept (`cliInstall.test.ts` refusal cases).
  - I: block removed but receipt kept → resume finds nothing to remove and retires
    the receipt.
  - Windows: an entry present without any generic receipt → kept (Q6).
- **Platforms; Windows unit test.** All. The block matcher is pure text. **The
  native port is new code**, because today only the TypeScript removes these. It
  needs Rust fixtures that reproduce the existing TS cases (§6).

### A8. Extension uninstall receipts (found during review; not named in §9.3)

`<launcher dir>/extension-uninstall/` holds `<sha>.json` targets, `shell-<sha>.json`
and `last-result.json` (`extensionUninstall.ts:38,113`; `uninstallHook.ts:36-46`) [V].
These are the receipts for A6 and A7, and they sit inside the A4 folder.

- **Outcome.** Consumed and then deleted only after the A6 and A7 items they prove
  are settled. If anything was preserved, the receipts are kept with the report
  (mirrors `extensionUninstall.ts:223-232`).
- **Fixture.** A tampered receipt → kept and reported (`extensionUninstall.test.ts:92`).

### A9. Custom index data folders (old service registry)

| Aspect | Record |
|---|---|
| Discovery | `<state>/control/service-1.4.sqlite3`, opened read-only with `ServiceRegistry::open_saved` (accepts `user_version` 2..=13; `airk-store/src/service/service_registry.rs:521`) [V per compatibility inventory D2]. Each registration's `data_root` (`replacement.rs:366-423`) [V]. Source roots come from `clean_break_retained_source_roots` (`airk-store/src/service/collection/split.rs:21`) [V]. |
| Ownership receipts | `service-data-1.4.owner.json` in the folder plus `data_scope::has_saved_binding(data, control, namespace)` and `validate_clean_break_storage` (`replacement.rs:396-409`; `airk-store/src/service/service_data_scope_clean_break.rs:410`) [V]. |
| Shutdown proof | The A1 roster. Data folders hold no images. Writers are excluded by stopping the A1 images first. |
| Fixture cases | R. U: binding missing → kept and reported. Today this is a whole-plan refusal, `registered storage has no owned binding` (`:398-401`). S: a source root inside or containing a data root → kept (`IndexLocations::protect`, `:425-436`; test `replacement_includes_registered_custom_storage_and_preserves_all_source_roots`). Missing data root → nothing to do (`explicit_reinstall_cleans_existing_storage_when_another_registered_root_is_missing`). Registry `user_version` 1 or a pending P5 catalog upgrade (compatibility inventory P3/P8) → registry unreadable, custom folders **not enumerated**, reported. I: data folder aside → resume (`original_control_is_reauthenticated_even_when_custom_storage_is_already_aside`). |
| Platforms; Windows unit test | All. Registry fixtures are built with current code (no authentic historical fixtures except schema 8; compatibility inventory Q5). |

### A10. Per-user certificate trust added by AIRK

| Aspect | Record |
|---|---|
| Discovery | `<state>/control/http-1/user-trust-1.json`. Without a readable layout, fallback probes `control/` and `state/control/` (`replacement.rs:933-952`) [V]. The OS store alone is never discovery. |
| Ownership receipt | `Receipt {schema:1, binding{installation, user, namespace}, ca_sha256 (DER SHA-256), store, phase}`. `store` is `CurrentUser/Root` or the login keychain path; `phase` is prepared, installed, remove_prepared or removed (`http_endpoint/trust.rs:18,28-45`) [V]. Written before and after each OS call (`:536-566`); read `:336,460-473` [V]. |
| Removal | `remove_owned` → `remove_owned_with` (`trust.rs:318-357`). Requires binding and CA equal to the saved endpoint and phase installed or removed. Deletes by exact DER SHA-256. Windows: `X509Store('Root','CurrentUser')` (`:110-130`). macOS: `security delete-certificate -t -Z` (`:282-292`). Re-checks absence [V]. No receipt → **no OS call**. An issuer that is present but unowned is never removed (`:505-511`) [V]. |
| Shutdown proof | None needed. Runs before folders move (`replacement.rs:914`) because the receipt lives inside a folder being removed. |
| Fixture cases | R. U: no receipt → no OS call; preexisting issuer → kept. Interrupted `prepared` or `remove_prepared` → `outcome_unknown`, never replayed (`:481-488`) → kept and reported with the CA digest. Crossed binding → kept. Unreadable layout with a receipt → reported, manual removal (`replacement.rs:943-949`). Windows removal shows an OS consent dialog; automated runs decline trust (U56) [I]. |
| Platforms; Windows unit test | Windows and macOS. Linux has no trust and no receipt (`trust.rs:67-75,418-431`) [V]. `manage_with` takes an injected runner, so it is testable on Windows (`crossed_issuer_and_unsupported_platform_never_invoke_os_commands`). `retire_owned_trust` has no test [V per sub-investigation]. |

### A11. Other artifacts found during review (outside the §9.3 roster)

| Artifact | Proof today | Proposed outcome |
|---|---|---|
| Aside folders `<name>.removed-<op>` and `.<leaf>.replacement-1/receipt.json`, `.<leaf>.replace-1.lock` under the AIRK parent | Operation receipt (A1) [V] | Resume and finish, then remove the lock (I) |
| In-flight pre-split journals in a root: `rebuild.json`, `clean-break-cleanup-<sha>.json`, `uninstall.json`, `retire-<id>.json`, `previous-retirement.json`, `legacy-launcher-retirement.json`, `superseded-retirement.json` (`runtime_install/retirement.rs:14,427-451,682`; compatibility inventory R2/R5) [V] | Their own schemas | Q9: supersede or resume |
| 1.3 per-collection derived files under the operator's `--data-dir` | `store/current/legacy_retirement.rs` receipts through `legacy_startup` [V per compatibility inventory P1] | Q10 |
| `{userprograms}\AIRK Setup` shortcut (`airk.iss:39`) | none | Kept and reported unless Q4 |
| Extension global-storage data: `http-trust/` (CA bundle copies for VS Code terminals, `serviceExtension.ts:925`), `install-captures/`, `service-intents`, `interface-cache/`, `diagnostic-captures` | Extension-owned | Not installer scope (U12; the same extension id survives). Listed for §8.5 |
| Uninstall-hook temp folders `airk-uninstall-*` (`uninstallHook.ts:61-81`) | `approval.json` | Not installer scope |

## 4. Order and shutdown proof (setup role, §9.6 steps 1–2)

Proposed order, reproducing §10.3 as implemented in `apply_ordered`
(`replacement.rs:898-927`). Steps that change are noted.

1. Take the operation lock.
2. Discover the roster: A1–A11 across both channels and both families, plus custom
   roots from A2.
3. Prove every item and classify it R, U, L or S. Record **X items first** in the
   report.
4. Freeze the operation record.
5. Remove proven login registrations (A3).
6. Retire receipted trust (A10).
7. Stop images. Each image must be proven by an inventory or receipt and stopped
   on a verified handle (A1, A4, A5, A6).
8. Unlink images under exclusion.
9. Move proven folders aside, then delete them (A1, A9, A4).
10. Settle PATH and shell edits (A5, A7).
11. Retire the consumed receipts (A8).
12. Write the retirement receipt with executables, endpoint, CA SHA-256 and
    connection ids (`ClientReferences`, `replacement.rs:476-541`).
13. Continue the install with the leftovers reported.

Step 7 for an image that cannot be proven or stopped keeps its containing folder
(K/U). Whether that alone blocks starting the new service is Q11.

## 5. Platform coverage summary

| Row | Windows | macOS | Linux | Pure decision testable on Windows |
|---|---|---|---|---|
| A1 roots | ✓ | ✓ | ✓ | Yes (tempdir trees; inject process observation) |
| A2 custom roots | ✓ | ✓ | ✓ | Yes (definition text and hash) |
| A3 login | Task Scheduler | SMAppService, LaunchAgent | systemd user | Yes with injected OS state; OS adapters are platform-gated |
| A4 universal launcher | ✓ | ✓ (plus owner-only mode) | ✓ | Yes |
| A5 Inno | ✓ | — | — | Yes (receipt and PATH strings; injected registry) |
| A6 editor caches | ✓ | ✓ | ✓ | Host-platform names only (`cache_name`); make the platform injectable |
| A7 shell and PATH | Registry PATH | rc and fish | rc and fish | Yes (pure text) |
| A9 custom data | ✓ | ✓ | ✓ | Yes |
| A10 trust | CurrentUser Root | login keychain | none | Yes (injected runner) |

## 6. Existing tests covering pieces (proof candidates)

| Area | File: tests |
|---|---|
| A1 plan, ownership, resume | `runtime_install/replacement/tests.rs`: 43 tests, including `replacement_removes_only_airk_locations_of_a_pre_1_9_installation`, `replacement_refuses_default_named_locations_owned_by_another_installation`, `a_failed_directory_removal_stays_pending_and_exact_retry_finishes_the_retired_roster`, `interrupted_image_unlink_resumes_only_the_recorded_unchanged_surviving_images`, `owned_image_names_are_removed_under_exclusion_before_parent_directory_moves`, `replacement_refuses_to_remove_the_image_it_runs_from`, `a_concurrent_replacement_is_refused_before_any_side_effect`, `replacement_decision_covers_every_installed_state`, `raw_only_native_logs_have_read_only_retirement_proof`, `a_partial_move_is_rolled_back_newest_first`, `a_failed_rollback_names_every_folder_left_aside`. `replacement/operation.rs:773,827,836,940` (budget, inode reuse, forged journal, recovery-authority scope) |
| A1 paths | `user_paths.rs`: `replacement_locations_preserve_other_channels_and_independent_default_layouts`, `fresh_layout_separates_formats_from_runtime_and_preserves_existing_ownership_paths`, `selected_new_layout_is_not_shadowed_by_old_install_leftovers`, `mac_defaults_keep_collection_data_outside_installer_and_control_ownership`, `xdg_absolute_policy_and_separate_channels_do_not_create_directories` |
| Shutdown | `upgrade_stop.rs`: `old_unresponsive_process_is_stopped_by_verified_handle_only`, `mac_predecessor_roster_uses_payload_root_for_legacy_helper_and_main_for_current_roles` (macOS). `runtime_upgrade_stop_roster_tests.rs` (4). `legacy_processes.rs`: `deleted_owned_executable_remains_a_read_only_process_candidate`, `same_user_exact_executable_audit_finds_self_without_a_stop_capability`. `runtime_install/legacy_launchers/tests.rs` |
| A3 login | SM: `service_receipt_refuses_modified_or_foreign_configuration`, `shared_service_mutation_requires_registered_definition_proof`, `legacy_mac_job_requires_exact_user_label_origin_program_and_all_arguments`, `task_scheduler_schema_compares_actions_principals_and_generated_metadata` (`cfg(windows)`), `reviewed_legacy_task_removal_preserves_sources_and_replays_receipt` (`cfg(windows)`, `#[ignore]`). `service_manager/direct_tests.rs` (4). `service_manager_macos.rs`: `mac_login_receipt_rejects_cross_root_channel_target_and_changed_metadata`. `runtime_macos_login.rs` (3, `cfg(any(macos,test))`). `legacy_startup/tests.rs` (8, fake backend). `replacement/tests.rs`: `replacement_alters_no_startup_registration_without_an_ownership_receipt` |
| A4 | `universal_launcher.rs`: `retirement_discovery_refuses_partial_or_changed_publication_and_preserves_foreign_owner`, `uninstall_removes_only_the_owned_entry_point_and_resumes_partial_removal`, `unrelated_uninstall_never_creates_or_claims_a_universal_publication_lock`, `unrecognized_publication_lock_is_preserved_without_touching_launchers` |
| A5 | `scripts/tests/test_standalone_windows.py:71-138` (static checks of `airk.iss`) only |
| A6 | `runtime_legacy_origin/tests.rs`: `cache_only_inventory_preserves_unknown_entries_links_and_changed_images`, `cleanup_refuses_current_future_and_unknown_cache_versions`, `cache_retirement_resume_requires_original_directory_and_image_identities_even_after_unlink`, `refuses_wrong_cache_prefix_and_noncanonical_global_storage`, `rejects_hardlinked_original_image`, `accepts_exact_owned_named_profile_storage`. TS `cliInstall.test.ts:34-83` (retirement) |
| A7, A8 | TS `cliRemove.test.ts:17,42,62,80,97,103,123`; `extensionUninstall.test.ts:33,52,79,92,102`; `cliInstall.test.ts:116-221` |
| A9 | `replacement/tests.rs`: `replacement_includes_registered_custom_storage_and_preserves_all_source_roots`, `explicit_reinstall_cleans_existing_storage_when_another_registered_root_is_missing`, `original_control_is_reauthenticated_even_when_custom_storage_is_already_aside` |
| A10 | `http_endpoint/trust/tests.rs`: `stopped_installation_cleanup_removes_only_receipted_issuer_and_preserves_unknown_outcomes`, `inspect_is_read_only_and_preexisting_issuer_is_never_owned_or_removed`, `interrupted_mutation_remains_unknown_and_never_replays_or_adopts`, `crossed_issuer_and_unsupported_platform_never_invoke_os_commands`. `replacement/tests.rs`: `replacement_proves_ownership_and_blocks_folder_removal_until_trust_is_retired` |

**Gaps.** None of these exist yet:

- registration enumeration and definition parsing (A2, A3);
- native Inno receipt and PATH handling (A5);
- a native port of the shell and PATH block removal (A7);
- a test of `retire_owned_trust`;
- a both-channel, both-family roster;
- per-item continue-and-report.

## 7. Keep and move vs. staged engine (§10.2 last row; proposal, needs the D2 review under U61)

**Retirement (keep, move, widen):**

- `runtime_install/replacement.rs` and `replacement/operation.rs`;
- `upgrade_stop.rs` and `runtime_upgrade_stop_macos.rs`;
- `legacy_processes.rs`;
- `legacy_launchers.rs` (`Image`, `image_exclusion`, `observe`, `stop_legacy`,
  intent journal);
- `retirement/superseded.rs` (inventory readers) and
  `retirement::absent_optional_process_aliases`;
- `universal_launcher.rs`: `Pointer`, `resolve`, `retirement_image_at`,
  `retirement_digest_for`, `retire_at`, `publication_lock`;
- `user_paths.rs`: `historical`, `owned_layout_locations`, `canonical_location`,
  `configuration_base`, `home`;
- `install_layout/` readers: `Layout::read`, `claimed`, `saved_control`,
  `retained_parameters`, `installation_for_control`, `validate_catalog_control`;
- `install_files/`;
- `service_manager*` ownership readers and removers: `for_runtime`, `for_command`
  id derivation, `read_receipt`, `check_owned`, `uninstall`, the Windows `.ps1`
  adapter, the Linux bus checks and the macOS receipt;
- `runtime_macos_login.rs` (`job_scope`, root derivation);
- `legacy_startup.rs`, `legacy_startup/journal.rs` and
  `airk-store` `current/legacy_retirement.rs` (pending Q10);
- `runtime_legacy_origin.rs` cache and storage readers, and
  `runtime_install/legacy_caches.rs` retirement and journal. The config-locator and
  adoption parts are not retirement; see the compatibility inventory P2;
- `http_endpoint/trust.rs` `remove_owned` (shared with setup's trust install);
- `service_logs::has_saved_owner`;
- the store readers: `open_saved`, `retained_namespace`, `has_saved_binding`,
  `validate_clean_break_storage`, `clean_break_retained_source_roots`.

**Setup (new fixed-layout role):**

- reuses `trust.rs` `manage` (install and remove with receipt);
- reuses `service_manager` registration of exactly one task, unit or SMAppService
  agent, under a new receipt;
- reuses `stop_paths` for its own drain.

The `airksvc-`/`for_runtime` naming and the schema 1/2 runtime commands are
pre-split forms. Setup's own form is an I4 contract question [I].

**Staged engine (delete with evidence once no caller remains) [I, candidates]:**

- `runtime_install/staging.rs`, selection and `activation.rs`, the `selected.json`
  writers;
- `managed.rs` and `managed_service.rs` (organization-managed payloads);
- `payload_repair.rs`, `launcher_repair.rs`;
- `retirement.rs` `retire_previous`, `remove`, `uninstall` slot logic (keep only
  readers);
- `layout.rs` successor writer, `clean_break_cleanup.rs`,
  `clean_break_background.rs`, `runtime_rebuild*`;
- `legacy_maintenance.rs`, `http_legacy.rs`, `data_migration.rs`;
- the `replacement.rs` `decide` version gate (`REPLACEMENT_THROUGH`), which the
  split replaces with "any pre-split → remove".

Each candidate needs its compatibility-inventory row and a surviving owner first.

## 8. Appendix D rows (pre-split, for the split)

"Surviving owner" is the split **retirement** module in the setup role of the
signed `airk` image (U33). Status is a proposal until reviewed (U61).

| Layout | Discovery | Captured selections | Shutdown proof | Resumable cleanup | Refusal / keep cases | Code serving it today | Surviving owner after split | Status |
|---|---|---|---|---|---|---|---|---|
| A1 Stable and preview roots, both families, with state, data, cache, logs, settings | `user_paths.rs:288-320,344-403`; `runtime-layout.json` | None carried (U30). `ClientReferences` → retirement receipt | `stop_paths` on inventory, launcher and alias images; verified handle | `.<leaf>.replacement-1/receipt.json`, `.removed-<op>` | Unclaimed or crossed state; no layout plus registry; managed; settings unproven; self-image inside | `replacement.rs`, `operation.rs`, `upgrade_stop.rs`, `legacy_processes.rs` | Retirement | Live; widen to both channels and per-item continue |
| A2 Custom roots | Pointer (`universal_launcher.rs:64-74`); **new** definition enumeration with id-hash check (SM:225-236) | None | As A1 | As A1 | Pointer digest mismatch; id/root mismatch; no receipts | `universal_launcher.rs`; none for enumeration | Retirement | Partly new code |
| A3 Login registrations (`airksvc-*`, SMAppService, systemd, 1.3 `airk-*`) | Computed id; definitions under `configuration_base`; label `io.iokaio.airk.service.<ch>` | None | Removed before stop (§10.3) | Receipt then definition order; 1.3 journal `legacy-retirement-1.4/` | No receipt, edited definition, OS mismatch (all X); macOS needs old binary (Q1) | `service_manager*`, `runtime_macos_login.rs`, `legacy_startup*` | Retirement | Live; enumeration new; Q1 open |
| A4 Universal launcher folder | Fixed sibling `launcher[-preview]` | Pointer recorded in `universal-uninstall.json` | Image digest from pointer | `universal-uninstall.json` | Partial publication; changed binary or pointer; unknown lock | `universal_launcher.rs` | Retirement | Live |
| A5 Inno `standalone`, PATH entry, uninstall key | Fixed `%LOCALAPPDATA%\AIRK\standalone`; AppId key [I] | None | `standalone\runtime\airk.exe` by exact path | `user-path-owner.txt` phases | `pending` receipt; duplicate entry; no receipt for folder or key (Q4) | `installers/windows/airk.iss` only (Pascal) | Retirement (native port) | New code |
| A6 Editor helper caches and CLI copies | `runtime_legacy_origin.rs:698-770` | None | `image_exclusion` per image | `legacy-cache-retirement.json` | Current or future version; prefix mismatch; extra entries; redirected storage; CLI copies without receipt (Q6) | `runtime_legacy_origin.rs`, `legacy_caches.rs`; TS `cliInstall.ts` | Retirement | Caches live; CLI copies need Q6 |
| A7 Extension PATH and shell edits | `extension-uninstall/shell-*.json`, `<sha>.json` | None | None | Receipt retained until settled | Edited block; no receipt; Windows entry without generic receipt | TS `cliRemove.ts`, `extensionUninstall.ts` | Retirement (native port) | New native code |
| A9 Custom index data | Registry `open_saved` → `data_root` | None (U30) | Via A1 | A1 operation receipt | No binding; source overlap; unreadable registry (P3, P8) | `replacement.rs:331-437`; store data scope | Retirement | Live |
| A10 Per-user trust | `control/http-1/user-trust-1.json` | CA SHA-256 → retirement receipt | n/a | Receipt phases | No receipt; `prepared`/`remove_prepared` unknown; crossed binding; preexisting issuer | `http_endpoint/trust.rs`, `replacement.rs:933-971` | Retirement (removal); setup (install) | Live; untested wrapper |
| A11 Interrupted pre-split operations | Sibling journals and aside folders | — | As A1 | Own journals | Q9 | `operation.rs`, rebuild and cleanup journals | Retirement | Open (Q9) |

## 9. Open questions

- **Q1. macOS SMAppService removal runs the old binary.** Unregister invokes
  `runtimes/<sha>/AIRK.app/Contents/MacOS/airk runtime mac-login --action unregister`
  (`service_manager_macos.rs:143-164`). §9.3 says no old binary is executed.
  Options:
  - allow this one verified invocation (signed, digest-matched);
  - `launchctl bootout` plus leaving the Background Task Management record and
    reporting it;
  - keep and report X.
- **Q2. Settings folders.** `%APPDATA%\AIRK\service-1.4` or `settings` and the
  macOS and Linux `configuration/<leaf>` folders have no ownership receipt branch
  in `owned_by_airk`. Leave and report under U14, or define a proof?
- **Q3. Universal launcher folder remainder.** `airk-launcher-publish.lock` and
  `extension-uninstall/` have no removal rule. May the folder be removed when only
  these AIRK-named, proven-empty remnants remain?
- **Q4. Inno `standalone` folder, `unins000.*`, uninstall key and shortcut.** Only
  the PATH edit carries a receipt. Is the fixed path plus the exact AppId key
  (`InstallLocation` equal to the fixed path) plus the pinned `runtime\airk.exe`
  digest acceptable ownership proof? If not, they stay and are reported.
  Related: does the split Inno installer use a **new** AppId? Reusing it would make
  Inno treat the old entry as the same application.
- **Q5. Organization-managed pre-split roots** (`managed-runtime.json`). Today they
  are refused. Remove them under U13, or keep and report as managed?
- **Q6. Extension CLI copies and the Windows PATH entry.** These have no direct
  receipt; the extension-uninstall target receipt names `storage`. Does that count
  as the U14 ownership receipt?
- **Q7. `environmentVariableCollection` PATH and CA entries** are VS Code-owned
  state that the native installer cannot clear. Confirm that the split extension
  (§8.5) removes them.
- **Q8. U39 pointer path.** It is not defined yet. The collision check with §1
  waits for I4 (Phase 1).
- **Q9. In-flight pre-split journals** (`rebuild.json`, `clean-break-cleanup-*`,
  `uninstall.json`, `retire-*`, `legacy-launcher-retirement.json`,
  `.replacement-1`). Does retirement resume each journal's own semantics, or
  supersede them with its own operation record and use them only as ownership
  evidence?
- **Q10. 1.3 per-collection derived data** under `--data-dir`. Removal today is
  explicit-only through `legacy_startup`. Do U13 "old indexes" include it? The
  saved 1.3 definition carries `--root/--data-dir/--collection`, so the inputs are
  recoverable.
- **Q11. An old service left running** (unproven image, K/U). Does it block
  starting the new service: same user pipe name, HTTPS port, the I1 identity? If
  so, §9.6 step 4 must report "installed, not started" rather than run beside it.
