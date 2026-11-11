# SPDX-License-Identifier: Apache-2.0
"""Move the native per-user folder lookup into airk-process (split U84, step 3b.1b).

`airkctl` must find the install pointer under the same per-user folders the
service uses, and may link only airk-core, airk-process and airk-ipc. This
script moves, verbatim, the folder primitives of
`crates/airk-cli/src/user_paths.rs` into `crates/airk-process/src/user_folders.rs`:

* `checked` (path validation) becomes `pub fn checked`;
* `known_folder` (Windows Known Folder through the process token) becomes
  `pub fn known_folder(KnownFolder)`; the three folder GUIDs used become a
  `KnownFolder` enum so callers need no windows-sys types;
* the three `home` variants (Windows profile, Unix passwd, unsupported) become
  `pub fn home`.

`user_paths.rs` keeps `checked` and `home` as thin forwards, so every caller and
test is unchanged. Each edit is asserted to apply exactly once; run once on the
pre-move tree (it refuses to run twice).

    python scripts/dev/split_u84b_user_folders.py
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
USER_PATHS = ROOT / "crates" / "airk-cli" / "src" / "user_paths.rs"
TARGET = ROOT / "crates" / "airk-process" / "src" / "user_folders.rs"
LIB = ROOT / "crates" / "airk-process" / "src" / "lib.rs"
MANIFEST = ROOT / "crates" / "airk-process" / "Cargo.toml"


def cut(text, start, end):
    """Remove and return text[start_marker .. end_marker) (end marker kept)."""
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + text[b:], text[a:b]


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"expected one occurrence of: {old[:80]!r}")
    return text.replace(old, new)


def main():
    if TARGET.exists():
        raise SystemExit("already moved: crates/airk-process/src/user_folders.rs exists")
    source = USER_PATHS.read_bytes().decode()
    source, checked = cut(source, "fn checked(path: PathBuf) -> Result<PathBuf> {",
                          "pub(crate) fn canonical_location(")
    source, folders = cut(source, "#[cfg(windows)]\nfn known_folder(",
                          "#[cfg(any(target_os = \"linux\", test))]\nfn xdg(")
    checked = replace_once(checked, "fn checked(", "pub fn checked(")
    folders = replace_once(folders, "#[cfg(windows)]\nfn known_folder(id: &windows_sys::core::GUID) -> Result<PathBuf> {",
                           "#[cfg(windows)]\npub fn known_folder(folder: KnownFolder) -> Result<PathBuf> {\n"
                           "    let id = match folder {\n"
                           "        KnownFolder::Profile => &windows_sys::Win32::UI::Shell::FOLDERID_Profile,\n"
                           "        KnownFolder::LocalAppData => &windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData,\n"
                           "        KnownFolder::RoamingAppData => &windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData,\n"
                           "    };")
    folders = replace_once(folders, "pub(crate) fn home() -> Result<PathBuf> {\n    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile)",
                           "pub fn home() -> Result<PathBuf> {\n    known_folder(KnownFolder::Profile)")
    folders = folders.replace("pub(crate) fn home() -> Result<PathBuf> {", "pub fn home() -> Result<PathBuf> {")
    module = (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "//! Native per-user folders (split U84): the Windows Known Folders through\n"
        "//! the process token and the Unix passwd home, never inherited environment\n"
        "//! overrides. Moved from `airk-cli` `user_paths.rs` by\n"
        "//! `scripts/dev/split_u84b_user_folders.py`; shared by the service and `airkctl`.\n"
        "use anyhow::{Context, Result, ensure};\n"
        "use std::{ffi::OsString, path::PathBuf};\n\n"
        "/// The Known Folders AIRK resolves on Windows.\n"
        "#[cfg(windows)]\n"
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n"
        "pub enum KnownFolder {\n    Profile,\n    LocalAppData,\n    RoamingAppData,\n}\n\n"
        "/// A native user directory: absolute, UTF-8, at most 4,096 bytes, no control characters.\n"
        + checked + folders.rstrip() + "\n"
    )
    TARGET.write_bytes(module.encode())
    source = replace_once(source, "pub(crate) fn canonical_location(",
                          "/// Path validation, shared with `airk-process` (U84).\n"
                          "fn checked(path: PathBuf) -> Result<PathBuf> {\n"
                          "    airk_process::user_folders::checked(path)\n}\n\n"
                          "/// The native user home (U84: `airk-process`).\n"
                          "pub(crate) fn home() -> Result<PathBuf> {\n"
                          "    airk_process::user_folders::home()\n}\n\n"
                          "pub(crate) fn canonical_location(")
    for folder in ("RoamingAppData", "LocalAppData"):
        source = replace_once(source, f"known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_{folder})",
                              f"airk_process::user_folders::known_folder(\n"
                              f"            airk_process::user_folders::KnownFolder::{folder},\n        )")
    # OsString is now used only by the Linux/test `xdg` helper.
    source = replace_once(source, "use std::{\n    ffi::OsString,\n    path::{Path, PathBuf},\n};\n",
                          "#[cfg(any(target_os = \"linux\", test))]\nuse std::ffi::OsString;\n"
                          "use std::path::{Path, PathBuf};\n")
    USER_PATHS.write_bytes(source.encode())
    lib = LIB.read_bytes().decode()
    lib = replace_once(lib, "pub mod monotonic;\n", "pub mod monotonic;\npub mod user_folders;\n")
    LIB.write_bytes(lib.encode())
    manifest = MANIFEST.read_bytes().decode()
    manifest = replace_once(manifest, '"Win32_System_Threading", "Win32_System_Performance"]',
                            '"Win32_System_Threading", "Win32_System_Performance", "Win32_System_Com", "Win32_UI_Shell"]')
    MANIFEST.write_bytes(manifest.encode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
