# SPDX-License-Identifier: Apache-2.0
"""Move the owner observer and private session stdio into airk-process (split 3f.3, U84 pattern).

`airkctl session` must exit when its owner (the editor's extension host)
exits and must keep its frames on private duplicates of stdin/stdout, as
`airk service session` does. Both are thin OS-call modules in `airk-cli`,
which `airkctl` cannot depend on. This script moves them, verbatim, into
`airk-process` (which both crates already depend on):

* `crates/airk-cli/src/client_owner.rs` -> `crates/airk-process/src/client_owner.rs`
  (callers name `airk_process::client_owner`);
* the two `mod platform` blocks of `crates/airk-cli/src/session.rs`
  (`private_streams`) -> `crates/airk-process/src/private_stdio.rs`, whose
  function becomes `pub fn private_streams`; `session.rs` calls it there.

The Windows code needs two more `windows-sys` features in `airk-process`
(`Win32_System_Diagnostics_ToolHelp`, `Win32_System_Console`), which
`airk-cli` already enables. Each edit is asserted to apply exactly once; run
it once on the pre-move tree. It prints the old -> new test identity prefix map.

    python scripts/dev/split_3f3_owner_and_stdio.py
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli" / "src"
PROCESS = ROOT / "crates" / "airk-process"


def read(path):
    return path.read_bytes().decode()


def write(path, text):
    path.write_bytes(text.encode())


def once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


# 1. client_owner moves as a file.
subprocess.run(["git", "mv", str(CLI / "client_owner.rs"), str(PROCESS / "src" / "client_owner.rs")],
               check=True, cwd=ROOT)
# The observer gains callers in two crates, so its three items become `pub`.
owner = read(PROCESS / "src" / "client_owner.rs")
for item in ("struct OwnerObserver(Owner);", "    fn exited(&self) -> bool {", "fn observe() -> Result<OwnerObserver> {"):
    owner = once(owner, item.replace("fn ", "pub(crate) fn ").replace("struct ", "pub(crate) struct ") if not item.startswith("    ") else "    pub(crate) fn exited(&self) -> bool {",
                 item.replace("fn ", "pub fn ").replace("struct ", "pub struct ") if not item.startswith("    ") else "    pub fn exited(&self) -> bool {")
write(PROCESS / "src" / "client_owner.rs", owner)
lib = read(CLI / "lib.rs")
assert lib.count("mod client_owner;") == 1, "client_owner declaration"
line = [entry for entry in lib.split("\n") if entry.strip().endswith("mod client_owner;")][0]
lib = once(lib, line + "\n", "")
write(CLI / "lib.rs", lib)
for path in CLI.rglob("*.rs"):
    text = read(path)
    if "crate::client_owner::" in text:
        write(path, text.replace("crate::client_owner::", "airk_process::client_owner::"))

# 2. the private stdio blocks move out of session.rs.
session = read(CLI / "session.rs")
start = session.index("#[cfg(unix)]\nmod platform {")
end = session.index("#[cfg(test)]\nmod tests;")
block = session[start:end]
session = session[:start] + session[end:]
session = once(session, "platform::private_streams()?", "airk_process::private_stdio::private_streams()?")
write(CLI / "session.rs", session)
stdio = ("// SPDX-License-Identifier: Apache-2.0\n"
         "//! Private session streams: the frame channel moves to private,\n"
         "//! non-inheritable duplicates of stdin/stdout; the process's standard\n"
         "//! input then reads the null device and its standard output writes to\n"
         "//! stderr, so no stray print or child can corrupt a frame. Moved from\n"
         "//! `airk-cli`'s session (split step 3f.3) for `airkctl session`.\n"
         "pub use platform::private_streams;\n\n"
         + block.replace("    pub(super) fn private_streams()", "    pub fn private_streams()").rstrip("\n") + "\n")
assert stdio.count("pub fn private_streams()") == 2
write(PROCESS / "src" / "private_stdio.rs", stdio)

# 3. airk-process exports both and enables the Windows features.
plib = read(PROCESS / "src" / "lib.rs")
plib = once(plib, "pub mod child_job;\n", "pub mod child_job;\npub mod client_owner;\n")
plib = once(plib, "pub mod monotonic;\n", "pub mod monotonic;\npub mod private_stdio;\n")
write(PROCESS / "src" / "lib.rs", plib)
cargo = read(PROCESS / "Cargo.toml")
cargo = once(cargo, '"Win32_System_Diagnostics_Debug"] }',
             '"Win32_System_Diagnostics_Debug", "Win32_System_Diagnostics_ToolHelp", "Win32_System_Console"] }')
write(PROCESS / "Cargo.toml", cargo)

print(json.dumps({"identity_map": {
    "airk-cli::lib/airk_cli client_owner::": "airk-process::lib/airk_process client_owner::",
}}, indent=2))
