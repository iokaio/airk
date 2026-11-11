# SPDX-License-Identifier: Apache-2.0
"""Share the log layout and ownership proof with airkctl (split U86/U89, step 3b.1f-2).

`airkctl service logs` reads the service's log rings itself (U88) and must
prove the same ownership the service proves. This script moves, verbatim:

* `canonical_location` from `crates/airk-cli/src/user_paths.rs` into
  `airk_process::user_folders` (beside `checked`, moved by U84);
* the log layout constants of `service_logs.rs` and `service_logs/raw.rs`
  (ring and raw folder and owner-record names, file names, file and retention
  bounds, the recent read size) and the owner digest of `expected_owner` into
  `airk_core::service_logs`.

`airk-cli` keeps its local names as aliases and forwards, so callers and tests
are unchanged. Each edit is asserted to apply exactly once; run once.

    python scripts/dev/split_u86b_log_layout.py
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
USER_PATHS = ROOT / "crates" / "airk-cli" / "src" / "user_paths.rs"
FOLDERS = ROOT / "crates" / "airk-process" / "src" / "user_folders.rs"
LOGS = ROOT / "crates" / "airk-cli" / "src" / "service_logs.rs"
RAW = ROOT / "crates" / "airk-cli" / "src" / "service_logs" / "raw.rs"
CORE = ROOT / "crates" / "airk-core" / "src" / "service_logs.rs"


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"expected one occurrence of: {old[:80]!r}")
    return text.replace(old, new)


def main():
    folders = FOLDERS.read_bytes().decode()
    if "pub fn canonical_location(" in folders:
        raise SystemExit("already moved: user_folders::canonical_location exists")
    paths = USER_PATHS.read_bytes().decode()
    start = paths.index("pub(crate) fn canonical_location(path: &Path) -> Result<PathBuf> {")
    end = paths.index("\n}\n", start) + 3
    function = paths[start:end]
    paths = paths[:start] + (
        "/// Moved to `airk-process` (U86); forwarded for existing callers.\n"
        "pub(crate) fn canonical_location(path: &Path) -> Result<PathBuf> {\n"
        "    airk_process::user_folders::canonical_location(path)\n}\n"
    ) + paths[end:]
    # `ensure!` is now used only by the test module.
    paths = replace_once(paths, "use anyhow::{Context, Result, ensure};\n", "use anyhow::{Context, Result};\n")
    paths = replace_once(paths, "#[cfg(test)]\nmod tests {\n", "#[cfg(test)]\nmod tests {\n    use anyhow::ensure;\n")
    USER_PATHS.write_bytes(paths.encode())
    folders = replace_once(folders, "use std::{ffi::OsString, path::PathBuf};",
                           "use std::{\n    ffi::OsString,\n    path::{Path, PathBuf},\n};")
    folders = folders.rstrip("\n") + "\n\n" + (
        "/// An absolute location whose existing ancestor is canonicalized and whose\n"
        "/// missing suffix is kept; parent traversal is refused (U86).\n"
        + function.replace("pub(crate) fn canonical_location(", "pub fn canonical_location(")
    )
    FOLDERS.write_bytes(folders.encode())

    logs = LOGS.read_bytes().decode()
    start = logs.index("const DIRECTORY: &str = \"logs-1.4\";")
    end = logs.index("const LOW_WATER_BYTES: u64 = 100_000_000;")
    constants = logs[start:end]
    files_start = constants.index("const FILES: [&str; 21] = [")
    files_end = constants.index("];\n", files_start) + 3
    files = constants[files_start:files_end]
    logs = logs[:start] + (
        "// The log layout is shared with `airkctl` (U86): `airk_core::service_logs`.\n"
        "const DIRECTORY: &str = airk_core::service_logs::LOG_DIRECTORY;\n"
        "const OWNER: &str = airk_core::service_logs::LOG_OWNER;\n"
        "const FILES: [&str; 21] = airk_core::service_logs::LOG_FILES;\n"
        "const FILE_BYTES: u64 = airk_core::service_logs::MAX_LOG_FILE_BYTES;\n"
        "const HIGH_WATER_BYTES: u64 = airk_core::service_logs::MAX_RETAINED_LOG_BYTES;\n"
    ) + logs[end:]
    logs = replace_once(logs, "const READ_BYTES: usize = 64 << 10;\n",
                        "const READ_BYTES: usize = airk_core::service_logs::RECENT_READ_BYTES;\n")
    logs = replace_once(logs, """        root: airk_core::identity::framed_digest(
            b"airk-diagnostic-log-owner/1.4",
            &[
                crate::user_paths::canonical_location(&std::path::absolute(root)?)?
                    .as_os_str()
                    .as_encoded_bytes(),
            ],
        ),""", """        root: airk_core::service_logs::owner_digest(&crate::user_paths::canonical_location(
            &std::path::absolute(root)?,
        )?),""")
    LOGS.write_bytes(logs.encode())
    raw = RAW.read_bytes().decode()
    raw = replace_once(raw, "const DIRECTORY: &str = \"job-errors-1\";\n",
                       "const DIRECTORY: &str = airk_core::service_logs::RAW_LOG_DIRECTORY;\n")
    raw = replace_once(raw, "pub(super) const OWNER: &str = \"job-errors-1.owner.json\";\n",
                       "pub(super) const OWNER: &str = airk_core::service_logs::RAW_LOG_OWNER;\n")
    RAW.write_bytes(raw.encode())

    core = CORE.read_bytes().decode()
    core = core.rstrip("\n") + "\n\n" + (
        "/// The summary ring's folder and its ownership record, beside the control\n"
        "/// root (U86/U89; moved from `airk-cli`).\n"
        "pub const LOG_DIRECTORY: &str = \"logs-1.4\";\n"
        "pub const LOG_OWNER: &str = \"logs-1.4.owner.json\";\n"
        "/// The raw job-error ring's folder and its ownership record.\n"
        "pub const RAW_LOG_DIRECTORY: &str = \"job-errors-1\";\n"
        "pub const RAW_LOG_OWNER: &str = \"job-errors-1.owner.json\";\n"
        "/// Ring files, newest first.\n"
        + files.replace("const FILES:", "pub const LOG_FILES:")
        + "/// The largest ring file.\n"
        "pub const MAX_LOG_FILE_BYTES: u64 = 10_000_000;\n"
        "/// The retention high-water mark of a ring.\n"
        "pub const MAX_RETAINED_LOG_BYTES: u64 = 200_000_000;\n"
        "/// How much of the newest files a recent page reads.\n"
        "pub const RECENT_READ_BYTES: usize = 64 << 10;\n\n"
        "/// The ownership digest a ring's owner record holds: the control root's\n"
        "/// canonical location (U86; moved from `airk-cli`).\n"
        "pub fn owner_digest(canonical_root: &std::path::Path) -> Sha256Digest {\n"
        "    crate::identity::framed_digest(\n"
        "        b\"airk-diagnostic-log-owner/1.4\",\n"
        "        &[canonical_root.as_os_str().as_encoded_bytes()],\n"
        "    )\n"
        "}\n"
    )
    CORE.write_bytes(core.encode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
