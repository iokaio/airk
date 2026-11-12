# SPDX-License-Identifier: Apache-2.0
"""Move the configuration draft type into airk-core (split step 3c.5a).

`airkctl configure` and the `usage` mutations must send the same draft
`airk` read or built, and `airkctl` depends on `airk-core` only. The draft
is a pure serde type over core types, so this script moves its definition
verbatim from `crates/airk-cli/src/service_configuration.rs` to
`crates/airk-core/src/service_configuration.rs` as `ConfigurationDraft`
(same attributes, variants and fields, so the same JSON). `airk-cli`
re-exports it as `Draft`, and its `prepare` method becomes the
`PrepareDraft` trait implemented in the same place, so callers and tests are
unchanged. No test moves. Each edit is asserted to apply exactly once; run it
once on the pre-move tree.

    python scripts/dev/split_3c5a_configuration_draft.py
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli" / "src" / "service_configuration.rs"
CORE = ROOT / "crates" / "airk-core" / "src" / "service_configuration.rs"

HEAD = '#[derive(Deserialize, Serialize)]\n#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]\npub enum Draft {\n'
END = "\nfn selected(path: &Path) -> Result<PathBuf> {"


def once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


cli = CLI.read_bytes().decode()
start = cli.index(HEAD)
assert cli.count(HEAD) == 1 and cli.count(END) == 1
block = cli[start:cli.index(END)]
cli = cli.replace(block, "")
cli = once(cli, "use std::path::{Path, PathBuf};\n",
           "use std::path::{Path, PathBuf};\n\n"
           "/// The draft `configure` reads and the `usage` commands build; owned by\n"
           "/// `airk_core` so every client sends the same JSON (split step 3c.5a).\n"
           "pub use airk_core::service_configuration::ConfigurationDraft as Draft;\n")
cli = once(cli, "impl Draft {\n    pub fn prepare(self) -> Result<ProjectConfiguration> {",
           "/// The draft's preparation into a project configuration (CLI-side checks).\n"
           "pub trait PrepareDraft {\n    fn prepare(self) -> Result<ProjectConfiguration>;\n}\n"
           "impl PrepareDraft for Draft {\n    fn prepare(self) -> Result<ProjectConfiguration> {")
# The moved type took its only uses of these imports with it.
cli = once(cli, """use airk_core::{
    extraction::IndexDetail,
    service::{ControlMethod, ControlResponse, ProjectBinding},
    service_configuration::{ConfigurationRequest, InputFile, ProjectConfiguration},
    usage::{ContentKind, RecordingMode},
    vector_selection::VectorSelectionPolicy,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
""", """use airk_core::{
    service::{ControlMethod, ControlResponse, ProjectBinding},
    service_configuration::{ConfigurationRequest, InputFile, ProjectConfiguration},
};
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use airk_core::usage::ContentKind;
""")
CLI.write_bytes(cli.encode())

moved = once(block, "pub enum Draft {\n", "pub enum ConfigurationDraft {\n")
moved = moved.replace("airk_core::", "crate::")
for bare, path in (("IndexDetail", "crate::extraction::IndexDetail"),
                   ("VectorSelectionPolicy", "crate::vector_selection::VectorSelectionPolicy"),
                   ("RecordingMode", "crate::usage::RecordingMode"),
                   ("ContentKind", "crate::usage::ContentKind"),
                   ("PathBuf", "std::path::PathBuf")):
    moved = moved.replace(f": {bare},", f": {path},")
core = CORE.read_bytes().decode()
core = once(core, "pub const MAX_CONFIGURATION_BYTES: usize = 16_384;\n",
            "pub const MAX_CONFIGURATION_BYTES: usize = 16_384;\n\n"
            "/// A project configuration draft as a client reads or builds it (the\n"
            "/// `configure --file` JSON); the service prepares it into a\n"
            "/// [`ProjectConfiguration`] with its pinned inputs (contracts M9).\n"
            + moved.rstrip("\n") + "\n")
CORE.write_bytes(core.encode())
print("ok")
