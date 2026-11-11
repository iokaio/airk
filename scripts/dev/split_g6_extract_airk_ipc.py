# SPDX-License-Identifier: Apache-2.0
"""Split plan Phase 2 group G6: extract `crates/airk-ipc` from `airk-cli` (U27).

Moves `crates/airk-cli/src/ipc/{endpoint.rs, endpoint/, frame.rs, control.rs}`
into the new library crate `crates/airk-ipc` with `git mv`, and makes these
mechanical edits:

- `airk-ipc/src/lib.rs` declares `control`, `endpoint` and `frame`;
  `crates/airk-cli/src/ipc/CONTRACT.md` moves to `crates/airk-ipc/CONTRACT.md`.
- The one in-crate path `crate::service_ipc::` becomes `crate::frame::`.
- `pub(crate)` becomes `pub` for the items `airk-cli` calls (every `pub(crate)`
  in these files is such an item); visibility is widened only for those callers.
- Gates on `airk-cli`'s `local-inference` feature, which does not exist in the
  new crate, are removed; the gated item is a Windows job-security helper with
  no caller in that configuration.
- `airk-cli` re-exports the crate as `ipc` (`pub use airk_ipc as ipc;`), so every
  existing path (`crate::ipc::…`, `crate::local_ipc`, `crate::service_ipc`)
  still resolves; no caller changes.
- Workspace dependency, `airk-cli` dependency and the reviewed crate edge
  `airk-ipc → airk-core` are added.

Run from the repository root on a clean tree: python scripts/dev/split_g6_extract_airk_ipc.py
"""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OLD = "crates/airk-cli/src/ipc/"
NEW = "crates/airk-ipc/src/"
MOVES = [("endpoint.rs", "endpoint.rs"), ("endpoint", "endpoint"), ("frame.rs", "frame.rs"),
         ("control.rs", "control.rs")]
MANIFEST = """# SPDX-License-Identifier: Apache-2.0
[package]
name = "airk-ipc"
description = "Same-user local transport for AIRK: owner-only endpoints, peer checks and the framed control protocol"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
publish.workspace = true

# Split plan U27: airk-ipc depends only on airk-core (and airk-process if a
# transport needs it). The edge gate (scripts/dev/boundary-exceptions.json)
# enforces the list.
[dependencies]
airk-core.workspace = true
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
tempfile.workspace = true
tokio = { workspace = true, features = ["net", "io-util", "sync"] }

[target.'cfg(unix)'.dependencies]
libc.workspace = true

[target.'cfg(windows)'.dependencies]
windows-sys = { workspace = true, features = ["Win32_Foundation", "Win32_Security", "Win32_Security_Authorization", "Win32_Storage_FileSystem", "Win32_System_JobObjects", "Win32_System_Pipes", "Win32_System_SystemServices", "Win32_System_Threading"] }
"""
LIB = """// SPDX-License-Identifier: Apache-2.0
//! Same-user local transport for AIRK (split plan §10.1, U27): owner-only
//! endpoints with peer checks (`endpoint`), the length-prefixed JSON frame codec
//! (`frame`) and the I1 control request (`control`). See `CONTRACT.md`.
//! Transport only: no registry, store, kernel or MCP dependency.
pub mod control;
pub mod endpoint;
pub mod frame;
"""
FEATURE_GATE = '#[cfg(any(not(feature = "local-inference"), test))]\n'


def replace(path, old, new, count=1):
    text = path.read_text(encoding="utf-8")
    if text.count(old) != count:
        raise SystemExit(f"{path}: expected {count} of {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def main():
    if (ROOT / "crates/airk-ipc").exists() or not (ROOT / OLD / "mod.rs").is_file():
        raise SystemExit("G6 already applied or tree differs")
    (ROOT / NEW).mkdir(parents=True)
    for old, new in MOVES:
        subprocess.run(["git", "mv", OLD + old, NEW + new], cwd=ROOT, check=True)
    subprocess.run(["git", "mv", OLD + "CONTRACT.md", "crates/airk-ipc/CONTRACT.md"], cwd=ROOT, check=True)
    subprocess.run(["git", "rm", "-q", OLD + "mod.rs"], cwd=ROOT, check=True)
    (ROOT / "crates/airk-ipc/Cargo.toml").write_text(MANIFEST, encoding="utf-8", newline="\n")
    (ROOT / NEW / "lib.rs").write_text(LIB, encoding="utf-8", newline="\n")
    source = ROOT / NEW
    replace(source / "endpoint.rs", "crate::service_ipc::write_frame_limited", "crate::frame::write_frame_limited")
    replace(source / "endpoint.rs", '#[cfg(all(windows, any(not(feature = "local-inference"), test)))]\n', "#[cfg(windows)]\n")
    replace(source / "endpoint/windows.rs", FEATURE_GATE, "")
    replace(source / "endpoint/windows/job.rs", FEATURE_GATE, "", count=3)
    for path in sorted(source.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "pub(crate)" in text:
            path.write_text(text.replace("pub(crate)", "pub"), encoding="utf-8", newline="\n")
    replace(ROOT / "crates/airk-cli/src/lib.rs", "pub mod ipc;\n",
            "/// The local transport lives in `airk-ipc` (split G6); `ipc` keeps the old path.\npub use airk_ipc as ipc;\n")
    replace(ROOT / "Cargo.toml", 'airk-core = { path = "crates/airk-core" }\n',
            'airk-core = { path = "crates/airk-core" }\nairk-ipc = { path = "crates/airk-ipc" }\n')
    replace(ROOT / "crates/airk-cli/Cargo.toml", "airk-core.workspace = true\n",
            "airk-core.workspace = true\nairk-ipc.workspace = true\n")
    exceptions = ROOT / "scripts/dev/boundary-exceptions.json"
    document = json.loads(exceptions.read_text(encoding="utf-8"))
    allowed = document["crate_edges"]["allowed"]
    allowed["airk-cli"] = sorted(set(allowed["airk-cli"]) | {"airk-ipc"})
    allowed["airk-ipc"] = ["airk-core"]
    document["crate_edges"]["allowed"] = dict(sorted(allowed.items()))
    exceptions.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"identity_map": {
        "airk-cli::lib/airk_cli ipc::endpoint::": "airk-ipc::lib/airk_ipc endpoint::",
        "airk-cli::lib/airk_cli ipc::frame::": "airk-ipc::lib/airk_ipc frame::",
        "airk-cli::lib/airk_cli ipc::control::": "airk-ipc::lib/airk_ipc control::",
    }}, indent=1))


if __name__ == "__main__":
    main()
