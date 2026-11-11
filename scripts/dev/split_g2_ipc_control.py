# SPDX-License-Identifier: Apache-2.0
"""Split plan Phase 2 group G2: move the I1 control transport into `ipc::control`.

A structural move only: `registry_busy_retry` and `request_once` move verbatim
from `crates/airk-cli/src/service_runtime.rs` to `crates/airk-cli/src/ipc/control.rs`
(made `pub(crate)`). The baseline-gate policy (`prebaseline_metadata_method`,
`request_scoped`) and the public `request*` wrappers stay in `service_runtime`,
which imports the moved functions, so its tests (`use super::*`) and every caller
are unchanged. Test identities do not change.

Run from the repository root on a clean tree: python scripts/dev/split_g2_ipc_control.py
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = ROOT / "crates/airk-cli/src/service_runtime.rs"
CONTROL = ROOT / "crates/airk-cli/src/ipc/control.rs"
IPC_MOD = ROOT / "crates/airk-cli/src/ipc/mod.rs"
START = "async fn registry_busy_retry<F, Fut>(mut request: F) -> Result<ControlResponse>\n"
END = "#[cfg(test)]\nmod registry_retry_tests;\n"
HEADER = """// SPDX-License-Identifier: Apache-2.0
//! I1 control transport: one framed request per connection with bounded
//! deadlines, the `registry_busy` retry window, and the Health identity check.
//! Moved verbatim from `service_runtime` (split plan G2); the baseline-gate
//! policy stays with the service wrappers that call these functions.
use super::endpoint as local_ipc;
use airk_core::service::{ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION};
use anyhow::{Result, ensure};
use std::{path::Path, time::Duration};

"""


def main():
    text = RUNTIME.read_text(encoding="utf-8")
    if CONTROL.exists() or text.count(START) != 1 or text.count(END) != 1:
        raise SystemExit("G2 already applied or tree differs")
    start = text.index(START)
    end = text.index(END)
    block = text[start:end]
    if "async fn request_once(" not in block:
        raise SystemExit("request_once is not where G2 expects it")
    moved = (block.replace("async fn registry_busy_retry<", "pub(crate) async fn registry_busy_retry<", 1)
                  .replace("async fn request_once(", "pub(crate) async fn request_once(", 1)
                  .rstrip("\n") + "\n")
    CONTROL.write_text(HEADER + moved, encoding="utf-8", newline="\n")
    remaining = text[:start] + text[end:]
    anchor = "async fn request_scoped(\n"
    if remaining.count(anchor) != 1:
        raise SystemExit("request_scoped anchor missing")
    remaining = remaining.replace(
        anchor, "use crate::ipc::control::{registry_busy_retry, request_once};\n\n" + anchor, 1)
    RUNTIME.write_text(remaining, encoding="utf-8", newline="\n")
    module = IPC_MOD.read_text(encoding="utf-8")
    if "pub mod control;" in module:
        raise SystemExit("ipc already declares control")
    IPC_MOD.write_text(module.replace("pub mod endpoint;\n", "pub mod control;\npub mod endpoint;\n", 1),
                       encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
