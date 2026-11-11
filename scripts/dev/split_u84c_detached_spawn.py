# SPDX-License-Identifier: Apache-2.0
"""Move the detached-spawn primitives into airk-process (split U84, step 3b.1b).

`airkctl service start` must start `airk` detached exactly as the service's
own on-demand start does, and may link only airk-core, airk-process and
airk-ipc. This script moves, verbatim:

* from `crates/airk-cli/src/native_process.rs`: `without_error_dialogs`,
  `DetachedChild` with its methods, `windows_argument` and `spawn_detached`
  (one contiguous block), made `pub`;
* from `crates/airk-cli/src/runtime_install.rs`: the inline Unix `setsid`
  pre-exec block of the on-demand service start, as `new_session(&mut Command)`;

into `crates/airk-process/src/detached.rs`. `native_process` re-exports the
names its callers and tests use, so call sites are unchanged. Each edit is
asserted to apply exactly once; run once on the pre-move tree.

    python scripts/dev/split_u84c_detached_spawn.py
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / "crates" / "airk-cli" / "src" / "native_process.rs"
INSTALL = ROOT / "crates" / "airk-cli" / "src" / "runtime_install.rs"
TARGET = ROOT / "crates" / "airk-process" / "src" / "detached.rs"
LIB = ROOT / "crates" / "airk-process" / "src" / "lib.rs"
MANIFEST = ROOT / "crates" / "airk-process" / "Cargo.toml"

SETSID = """    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid has no pointer arguments and runs before exec in the child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
"""


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"expected one occurrence of: {old[:80]!r}")
    return text.replace(old, new)


def main():
    if TARGET.exists():
        raise SystemExit("already moved: crates/airk-process/src/detached.rs exists")
    native = NATIVE.read_bytes().decode()
    start = native.index("pub(crate) fn without_error_dialogs<T>(")
    end = native.index("#[cfg(all(test, windows))]\nmod tests {")
    block = native[start:end]
    native = native[:start] + (
        "/// Moved to `airk-process` (U84); re-exported for existing callers.\n"
        "pub(crate) use airk_process::detached::{windows_argument, without_error_dialogs};\n"
        "#[cfg(windows)]\n"
        "pub(crate) use airk_process::detached::spawn_detached;\n\n"
    ) + native[end:]
    block = block.replace("pub(crate) fn ", "pub fn ").replace("pub(crate) struct ", "pub struct ")
    install = INSTALL.read_bytes().decode()
    install = replace_once(install, SETSID, "    #[cfg(unix)]\n    airk_process::detached::new_session(&mut command);\n")
    body = SETSID.replace("    #[cfg(unix)]\n    {\n", "", 1)
    body = body[: body.rindex("    }\n")]
    body = "\n".join(line[4:] if line.startswith("    ") else line for line in body.split("\n"))
    module = (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "//! Detached process start (split U84): the primitives the service's on-demand\n"
        "//! start uses, shared with `airkctl service start`. Moved from `airk-cli`\n"
        "//! (`native_process.rs`, `runtime_install.rs`) by\n"
        "//! `scripts/dev/split_u84c_detached_spawn.py`.\n"
        "use std::io;\n\n"
        + block.rstrip() + "\n\n"
        "/// Start the child in a new session (`setsid` before exec), as the service's\n"
        "/// on-demand start does on Unix.\n"
        "#[cfg(unix)]\n"
        "pub fn new_session(command: &mut std::process::Command) {\n"
        + body.rstrip() + "\n}\n"
    )
    TARGET.write_bytes(module.encode())
    NATIVE.write_bytes(native.encode())
    INSTALL.write_bytes(install.encode())
    lib = LIB.read_bytes().decode()
    lib = replace_once(lib, "pub mod child_job;\n", "pub mod child_job;\npub mod detached;\n")
    LIB.write_bytes(lib.encode())
    manifest = MANIFEST.read_bytes().decode()
    manifest = replace_once(manifest, '"Win32_System_Com", "Win32_UI_Shell"]',
                            '"Win32_System_Com", "Win32_UI_Shell", "Win32_System_Diagnostics_Debug"]')
    MANIFEST.write_bytes(manifest.encode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
