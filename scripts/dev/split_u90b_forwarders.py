# SPDX-License-Identifier: Apache-2.0
"""Give airkctl the bounded native command runner and finish the U90 moves.

Second half of split U90 (step 3e.5); run after
`scripts/dev/split_u90b_cert_trust.py`. `airkctl service trust` runs the same
PowerShell / security(1) commands as `airk runtime http-trust`, so it needs the
same bounded runner, and the airk-cli originals the first script copied must
become forwards so each primitive has one implementation (U84, U90). This
script moves, verbatim:

* from `crates/airk-cli/src/native_command/mod.rs`: `MAX_CONFIG`,
  `Invocation`, `Output`, both `windows_tool` variants, `execute`,
  `execute_native_program`, `execute_native_observation` and
  `execute_with_timeout`, made `pub`; the `test` cfg gates of the two
  `execute_native_*` helpers are dropped (they would name airk-process's own
  tests; airk-cli keeps them on its re-exports);
* from `crates/airk-cli/src/native_process.rs`: `attach_owned`, `spawn`,
  `ManagedChild` and `ContainedChild` with its impls (`attach` made `pub` for
  the Windows containment test);

into `crates/airk-process/src/bounded_command.rs`. `native_command` and
`native_process` re-export the names their callers and tests use, so call
sites are unchanged, and `install_files::safe_file` forwards to
`airk_ipc::private_record::require_regular_unlinked` (the verbatim copy the
first script made). Each edit is asserted to apply exactly once; run once.

    python scripts/dev/split_u90b_forwarders.py
"""
from pathlib import Path
import sys

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli" / "src"
COMMAND = CLI / "native_command" / "mod.rs"
COMMAND_CONTRACT = CLI / "native_command" / "CONTRACT.md"
PROCESS = CLI / "native_process.rs"
INSTALL_FILES = CLI / "install_files" / "mod.rs"
TARGET = ROOT / "crates" / "airk-process" / "src" / "bounded_command.rs"
LIB = ROOT / "crates" / "airk-process" / "src" / "lib.rs"
MANIFEST = ROOT / "crates" / "airk-process" / "Cargo.toml"


def read(path):
    return path.read_bytes().decode()


def write(path, text):
    path.write_bytes(text.encode())


def cut(text, start, end):
    a = text.index(start)
    b = text.index(end, a) if end is not None else len(text)
    return text[:a] + text[b:], text[a:b]


def replace_n(text, old, new, count=1):
    if text.count(old) != count:
        raise SystemExit(f"expected {count} occurrence(s) of: {old[:80]!r} (found {text.count(old)})")
    return text.replace(old, new)


def main():
    if TARGET.exists():
        raise SystemExit("already moved: crates/airk-process/src/bounded_command.rs exists")
    if not (ROOT / "crates" / "airk-ipc" / "src" / "private_record.rs").exists():
        raise SystemExit("run scripts/dev/split_u90b_cert_trust.py first")
    command, process, install_files = read(COMMAND), read(PROCESS), read(INSTALL_FILES)

    command, runner = cut(command, "/// Byte bound shared by service configuration", None)
    runner = replace_n(runner, "pub(crate) const MAX_CONFIG", "pub const MAX_CONFIG")
    runner = replace_n(runner, "pub(crate) struct Invocation", "pub struct Invocation")
    runner = replace_n(runner, "pub(crate) struct Output", "pub struct Output")
    runner = replace_n(runner, "    pub(crate) program:", "    pub program:")
    runner = replace_n(runner, "    pub(crate) args:", "    pub args:")
    runner = replace_n(runner, "    pub(crate) env:", "    pub env:")
    runner = replace_n(runner, "    pub(crate) input:", "    pub input:")
    runner = replace_n(runner, "    pub(crate) success:", "    pub success:")
    runner = replace_n(runner, "    pub(crate) text:", "    pub text:")
    runner = replace_n(runner, "    pub(crate) fn new(", "    pub fn new(")
    runner = replace_n(runner, "pub(crate) fn windows_tool(", "pub fn windows_tool(", 2)
    runner = replace_n(runner, "pub(crate) fn execute(", "pub fn execute(")
    runner = replace_n(runner, "#[cfg(any(target_os = \"macos\", test))]\npub(crate) fn execute_native_program(",
                       "pub fn execute_native_program(")
    runner = replace_n(runner, "#[cfg(any(windows, target_os = \"macos\", test))]\npub(crate) fn execute_native_observation(",
                       "pub fn execute_native_observation(")
    runner = replace_n(runner, "pub(crate) fn execute_with_timeout(", "pub fn execute_with_timeout(")
    runner = replace_n(runner, "crate::native_process::ContainedChild::spawn(", "ContainedChild::spawn(")

    process, attach = cut(process, "/// Contain this exact newly spawned child before Init",
                          "/// Replace this process with `command`")
    process, contained = cut(process, "pub(crate) fn spawn(command: &mut Command)", "#[cfg(windows)]\npub(crate) use airk_process::detached::spawn_detached;")
    attach = replace_n(attach, "#[cfg(any(windows, test))]\npub(crate) fn attach_owned(", "pub fn attach_owned(")
    contained = replace_n(contained, "pub(crate) fn spawn(", "pub fn spawn(")
    contained = replace_n(contained, "airk_process::child_group::require_waitable_children()",
                          "crate::child_group::require_waitable_children()")
    contained = replace_n(contained, "without_error_dialogs(|| command.spawn())",
                          "crate::detached::without_error_dialogs(|| command.spawn())")
    contained = replace_n(contained, "pub(crate) struct ContainedChild", "pub struct ContainedChild")
    contained = replace_n(contained, "airk_process::GroupChild", "crate::GroupChild", 2)
    contained = replace_n(contained, "    #[cfg(windows)]\n    fn attach(", "    #[cfg(windows)]\n    pub fn attach(")

    head = command[: command.index("use anyhow::")]
    command = (
        head
        + "//! Moved to `airk-process` (`bounded_command`, split U90) so `airkctl` runs\n"
        "//! the same bounded commands; re-exported for existing callers.\n"
        "pub(crate) use airk_process::bounded_command::{\n"
        "    Invocation, MAX_CONFIG, Output, execute, execute_with_timeout, windows_tool,\n"
        "};\n"
        "#[cfg(any(target_os = \"macos\", test))]\n"
        "pub(crate) use airk_process::bounded_command::execute_native_program;\n"
        "#[cfg(any(windows, target_os = \"macos\", test))]\n"
        "pub(crate) use airk_process::bounded_command::execute_native_observation;\n"
    )
    process = replace_n(process, "/// Replace this process with `command`",
                        "/// Moved to `airk-process` (`bounded_command`, U90); re-exported for callers.\n"
                        "#[cfg(any(windows, test))]\n"
                        "pub(crate) use airk_process::bounded_command::attach_owned;\n"
                        "pub(crate) use airk_process::bounded_command::{ContainedChild, spawn};\n\n"
                        "/// Replace this process with `command`")
    process = replace_n(process, "use std::{\n    io,\n    process::{Child, Command},\n};\n",
                        "use std::process::Command;\n")

    start = install_files.index("pub(crate) fn safe_file(path: &Path) -> Result<()> {")
    end = install_files.index("/// Payload-relative name", start)
    install_files = (install_files[:start]
                     + "pub(crate) fn safe_file(path: &Path) -> Result<()> {\n"
                     "    // One implementation (U90): moved to `airk-ipc` for `airkctl`'s records.\n"
                     "    airk_ipc::private_record::require_regular_unlinked(path)\n}\n\n"
                     + install_files[end:])

    module = (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "//! Bounded native command runner (split U90): one contained child, a fixed\n"
        "//! deadline and capped input and output; callers own each command's\n"
        "//! meaning. Shared by `airk` and `airkctl` (user certificate trust). Moved\n"
        "//! from `airk-cli` (`native_command`, `native_process`) by\n"
        "//! `scripts/dev/split_u90b_forwarders.py`.\n"
        "use anyhow::{Context, Result, bail, ensure};\n"
        "use std::{\n"
        "    collections::BTreeMap,\n"
        "    io::{self, Read, Write},\n"
        "    path::{Path, PathBuf},\n"
        "    process::{Child, Command, Stdio},\n"
        "    sync::mpsc,\n"
        "    thread,\n"
        "    time::{Duration, Instant},\n"
        "};\n\n"
        + runner.rstrip() + "\n\n"
        + attach.rstrip() + "\n\n"
        + contained.rstrip() + "\n"
    )
    contract = read(COMMAND_CONTRACT) + (
        "\n## Moved (split U90)\n\n"
        "The items above now live in `airk_process::bounded_command` (with\n"
        "`native_process::ContainedChild`, `spawn` and `attach_owned`), moved by\n"
        "`scripts/dev/split_u90b_forwarders.py` so `airkctl` runs the same bounded\n"
        "commands. This module re-exports them with the visibility and cfg gates\n"
        "listed here; their behavior and bounds are unchanged.\n"
    )
    lib = replace_n(read(LIB), "pub mod cert_trust;\n", "pub mod bounded_command;\npub mod cert_trust;\n")
    manifest = replace_n(read(MANIFEST), '"Win32_System_Diagnostics_ToolHelp", "Win32_System_Console"]',
                         '"Win32_System_Diagnostics_ToolHelp", "Win32_System_Console", "Win32_System_SystemInformation"]')
    write(TARGET, module)
    write(COMMAND, command)
    write(COMMAND_CONTRACT, contract)
    write(PROCESS, process)
    write(INSTALL_FILES, install_files)
    write(LIB, lib)
    write(MANIFEST, manifest)
    return 0


if __name__ == "__main__":
    sys.exit(main())
