# SPDX-License-Identifier: Apache-2.0
"""Split plan Phase 2 group G1: give the local IPC transport an owned `ipc` module.

A structural move only (no behavior change):

    crates/airk-cli/src/local_ipc.rs                  -> src/ipc/endpoint.rs
    crates/airk-cli/src/local_ipc_windows.rs          -> src/ipc/endpoint/windows.rs
    crates/airk-cli/src/local_ipc_unix.rs             -> src/ipc/endpoint/unix.rs
    crates/airk-cli/src/local_ipc_windows_job.rs      -> src/ipc/endpoint/windows/job.rs
    crates/airk-cli/src/local_ipc_windows_path_tests.rs -> src/ipc/endpoint/windows/path_tests.rs
    crates/airk-cli/src/service_ipc.rs                -> src/ipc/frame.rs

The platform files become ordinary `cfg` submodules aliased to the existing
internal name `platform`, so no `#[path]` mount remains and the two
module-honesty exceptions for them are deleted. `lib.rs` keeps the old public
paths as re-exports (`pub use ipc::endpoint as local_ipc;`,
`pub use ipc::frame as service_ipc;`), so no caller or test changes.

Run from the repository root on a clean tree: python scripts/dev/split_g1_ipc_module.py
It prints the old -> new test identity prefix map for `test_identities.py compare`.
"""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SRC = "crates/airk-cli/src/"
MOVES = [
    ("local_ipc.rs", "ipc/endpoint.rs"),
    ("local_ipc_windows.rs", "ipc/endpoint/windows.rs"),
    ("local_ipc_unix.rs", "ipc/endpoint/unix.rs"),
    ("local_ipc_windows_job.rs", "ipc/endpoint/windows/job.rs"),
    ("local_ipc_windows_path_tests.rs", "ipc/endpoint/windows/path_tests.rs"),
    ("service_ipc.rs", "ipc/frame.rs"),
]
PLATFORM_MOUNT = ('#[cfg(windows)]\n#[path = "local_ipc_windows.rs"]\nmod platform;\n'
                  '#[cfg(unix)]\n#[path = "local_ipc_unix.rs"]\nmod platform;\n')
PLATFORM_MODULES = ("#[cfg(windows)]\nmod windows;\n#[cfg(windows)]\nuse self::windows as platform;\n"
                    "#[cfg(unix)]\nmod unix;\n#[cfg(unix)]\nuse self::unix as platform;\n")
IPC_MOD = """// SPDX-License-Identifier: Apache-2.0
//! Same-user local transport (split plan Phase 2): owner-only endpoints with
//! peer checks (`endpoint`) and the length-prefixed JSON frame codec (`frame`).
//! See `CONTRACT.md`. Transport only: no application policy lives here.
pub mod endpoint;
pub mod frame;
"""
HONESTY_VALUES = {"local_ipc_windows_job.rs", "local_ipc_windows_path_tests.rs"}


def replace_once(path, old, new):
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"{path}: expected exactly one occurrence of {old!r}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def main():
    src = ROOT / SRC
    if not (src / "local_ipc.rs").is_file() or (src / "ipc").exists():
        raise SystemExit("G1 already applied or tree differs")
    for old, new in MOVES:
        (src / new).parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "mv", SRC + old, SRC + new], cwd=ROOT, check=True)
    (src / "ipc/mod.rs").write_text(IPC_MOD, encoding="utf-8", newline="\n")
    replace_once(src / "ipc/endpoint.rs", PLATFORM_MOUNT, PLATFORM_MODULES)
    replace_once(src / "ipc/endpoint/windows.rs", '#[path = "local_ipc_windows_job.rs"]\nmod job;\n', "mod job;\n")
    replace_once(src / "ipc/endpoint/windows.rs",
                 '#[cfg(test)]\n#[path = "local_ipc_windows_path_tests.rs"]\nmod path_tests;\n',
                 "#[cfg(test)]\nmod path_tests;\n")
    replace_once(src / "lib.rs", "pub mod local_ipc;\n",
                 "pub mod ipc;\n/// Compatibility path (split G1): the transport moved to `ipc::endpoint`.\n"
                 "pub use ipc::endpoint as local_ipc;\n")
    replace_once(src / "lib.rs", "pub mod service_ipc;\n",
                 "/// Compatibility path (split G1): the frame codec moved to `ipc::frame`.\n"
                 "pub use ipc::frame as service_ipc;\n")
    exceptions = ROOT / "scripts/dev/boundary-exceptions.json"
    document = json.loads(exceptions.read_text(encoding="utf-8"))
    before = len(document["module_honesty"])
    document["module_honesty"] = [entry for entry in document["module_honesty"]
                                  if not (entry.get("file") == SRC + "local_ipc_windows.rs"
                                          and entry.get("value") in HONESTY_VALUES)]
    if before - len(document["module_honesty"]) != 2:
        raise SystemExit("expected to remove exactly two module-honesty rows")
    exceptions.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"identity_map": {
        "airk-cli::lib/airk_cli local_ipc::platform::path_tests::": "airk-cli::lib/airk_cli ipc::endpoint::windows::path_tests::",
        "airk-cli::lib/airk_cli local_ipc::platform::job::": "airk-cli::lib/airk_cli ipc::endpoint::windows::job::",
        "airk-cli::lib/airk_cli local_ipc::platform::": "airk-cli::lib/airk_cli ipc::endpoint::windows::",
        "airk-cli::lib/airk_cli local_ipc::": "airk-cli::lib/airk_cli ipc::endpoint::",
        "airk-cli::lib/airk_cli service_ipc::": "airk-cli::lib/airk_cli ipc::frame::",
    }}, indent=1))


if __name__ == "__main__":
    main()
