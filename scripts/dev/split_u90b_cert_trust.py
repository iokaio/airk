# SPDX-License-Identifier: Apache-2.0
"""Move the HTTPS certificate-trust code out of airk-cli (split U90, step 3e.5).

`airkctl service trust` must inspect, install and remove the AIRK issuer in the
user's OS trust store while the service is down (U75), and may link only
airk-core, airk-process and airk-ipc. airk-process links no AIRK crate and
airk-ipc links only airk-core (scripts/dev/boundary-exceptions.json), so the
code moves without new crate edges:

* `crates/airk-ipc/src/private_record.rs` (new) receives, verbatim, the
  owner-private bounded record helpers of `crates/airk-cli/src/http_endpoint.rs`
  (`private_file`, `read_private`, `write_private`, made `pub`, with
  `crate::local_ipc::` spelled `crate::endpoint::`) and the issuer-rotation
  lock of `http_endpoint/rotation.rs` (`MutationLock`/`lock`, as
  `PrivateLock`/`lock(root, name, busy)`). The regular-unlinked-file check
  `private_file` calls is copied from `install_files::safe_file` as
  `require_regular_unlinked`; `scripts/dev/split_u90b_forwarders.py` turns the
  airk-cli original into a forward (it edits files outside step 3e.5).
* `crates/airk-process/src/cert_trust.rs` (new) receives, verbatim, the OS
  trust adapter of `http_endpoint/trust.rs` (`Host`, the PowerShell script,
  `ps_string`, `Runner`, `command`, `observe`, `mutate`) and its receipt
  engine (`Action`, `Phase`, `Receipt`, `TrustStatus`, `public_ca`,
  `manage_with`). Because this crate links no AIRK crate, the binding and the
  digest become type parameters (`Receipt<B, D>`, `TrustStatus<D>`; serde
  output is byte-identical) and the engine's airk-cli calls become caller-
  supplied function pointers (`Ports`): the record reads and writes, the
  regular-file check, the PowerShell path, the binding check and the
  active-issuer guard (validate control root, take the rotation lock, refuse
  a pending rotation, in that order). Digests reach the OS adapter as their
  lowercase hex text, the `Display` spelling the script already used.
* `http_endpoint/trust.rs` keeps `saved_identity`, `remove_owned`,
  `remove_owned_with`, `manage` and `windows_program` verbatim and gains thin
  `public_ca` and `manage_with` wrappers, so `rotation_receipts.rs`, the trust
  tests and every caller are unchanged.
* `now_ms`, `saved_endpoint` and `output` (the `runtime http-issuer-rotation`
  reply) move verbatim from `runtime_install/http_rotation.rs` into
  `http_endpoint/rotation.rs`, so the service can answer M18 `Status` with the
  same document.

Each edit is asserted to apply exactly once (or the stated number of times);
run once on the pre-move tree. It refuses to run twice.

    python scripts/dev/split_u90b_cert_trust.py
"""
from pathlib import Path
import sys

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli" / "src"
HTTP = CLI / "http_endpoint.rs"
TRUST = CLI / "http_endpoint" / "trust.rs"
ROTATION = CLI / "http_endpoint" / "rotation.rs"
HTTP_ROTATION = CLI / "runtime_install" / "http_rotation.rs"
INSTALL_FILES = CLI / "install_files" / "mod.rs"
IPC_TARGET = ROOT / "crates" / "airk-ipc" / "src" / "private_record.rs"
IPC_LIB = ROOT / "crates" / "airk-ipc" / "src" / "lib.rs"
PROCESS_TARGET = ROOT / "crates" / "airk-process" / "src" / "cert_trust.rs"
PROCESS_LIB = ROOT / "crates" / "airk-process" / "src" / "lib.rs"
PROCESS_MANIFEST = ROOT / "crates" / "airk-process" / "Cargo.toml"


def read(path):
    return path.read_bytes().decode()


def write(path, text):
    path.write_bytes(text.encode())


def cut(text, start, end):
    """Remove and return text[start_marker .. end_marker) (end marker kept)."""
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + text[b:], text[a:b]


def take(text, start, end):
    a = text.index(start)
    return text[a:text.index(end, a)]


def replace_n(text, old, new, count=1):
    if text.count(old) != count:
        raise SystemExit(f"expected {count} occurrence(s) of: {old[:80]!r} (found {text.count(old)})")
    return text.replace(old, new)


def private_record(http, rotation, install_files):
    safe = take(install_files, "/// Refuse anything other than a regular, unlinked file",
                "/// Payload-relative name")
    safe = replace_n(safe, "pub(crate) fn safe_file(", "pub fn require_regular_unlinked(")
    helpers = take(http, "fn private_file(path: &Path) -> Result<()> {", "#[cfg(test)]\nmod tests;")
    helpers = replace_n(helpers, "fn private_file(", "pub fn private_file(")
    helpers = replace_n(helpers, "crate::install_files::safe_file(path)?;", "require_regular_unlinked(path)?;")
    helpers = replace_n(helpers, "crate::local_ipc::", "crate::endpoint::", 4)
    helpers = replace_n(helpers, "pub(crate) fn read_private(", "pub fn read_private(")
    helpers = replace_n(helpers, "\nfn write_private(", "\npub fn write_private(")
    lock = take(rotation, "pub struct MutationLock(File);", "fn stage_root(")
    lock = replace_n(lock, "MutationLock", "PrivateLock", 4)
    lock = replace_n(
        lock,
        "pub fn lock(control: &Path) -> Result<PrivateLock> {\n"
        "    let root = control.join(\"http-1\");\n"
        "    crate::local_ipc::require_existing_root(&root)?;\n"
        "    let path = root.join(\"issuer-rotation.lock\");\n",
        "pub fn lock(root: &Path, name: &str, busy: &'static str) -> Result<PrivateLock> {\n"
        "    crate::endpoint::require_existing_root(root)?;\n"
        "    let path = root.join(name);\n",
    )
    lock = replace_n(lock, 'file.try_lock().context("issuer rotation already active")?;',
                     "file.try_lock().context(busy)?;")
    return (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "//! Owner-private bounded record files (split U90): the HTTPS identity,\n"
        "//! credential and trust records and their locks, read and written with the\n"
        "//! same checks by the service and by `airkctl`'s offline recovery (U75).\n"
        "//! Moved from `airk-cli` (`http_endpoint.rs`, `http_endpoint/rotation.rs`;\n"
        "//! the regular-file check from `install_files`) by\n"
        "//! `scripts/dev/split_u90b_cert_trust.py`.\n"
        "use anyhow::{Context, Result, ensure};\n"
        "use std::{fs::File, io, path::Path};\n\n"
        + safe.rstrip() + "\n\n"
        + helpers.rstrip() + "\n\n"
        "/// An exclusive owner-private lock file, released on drop.\n"
        + lock.rstrip() + "\n"
    )


PORTS = '''/// Owner-private record access and the regular-file check, supplied by the
/// caller from `airk_ipc::private_record` (this crate links no AIRK crate).
#[derive(Clone, Copy)]
pub struct Files {
    pub read: fn(&Path, &str, usize) -> Result<Option<Vec<u8>>>,
    pub write: fn(&Path, &str, &[u8], usize, bool) -> Result<()>,
    pub regular: fn(&Path) -> Result<()>,
}

/// What the receipt engine needs from its caller: the record files, the
/// Windows PowerShell path, the check that the binding belongs to this user,
/// and the guard taken before mutating the active issuer (`<control>/http-1`):
/// validate the control root, take the issuer-rotation lock and refuse a
/// pending rotation, returning the held lock.
pub struct Ports<B> {
    pub files: Files,
    pub powershell: fn() -> Result<PathBuf>,
    pub validate_binding: fn(&B) -> Result<()>,
    pub active_guard: fn(&Path) -> Result<Box<dyn std::any::Any>>,
}

/// Lowercase hexadecimal SHA-256, the `Display` spelling of AIRK's digests.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

'''

ENGINE_SIGNATURE_OLD = '''fn manage_with(
    root: &Path,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    action: Action,
    host: Host,
    run: &mut Runner<'_>,
) -> Result<TrustStatus> {
    crate::http_endpoint::validate_binding(&binding)?;
'''
ENGINE_SIGNATURE_NEW = '''/// Inspect, install or remove the exact issuer at `root/ca.pem` in this
/// user's OS trust store, with the durable ownership receipt beside it.
pub fn manage_with<B, D>(
    ports: &Ports<B>,
    root: &Path,
    binding: B,
    ca_sha256: D,
    action: Action,
    host: Host,
    run: &mut Runner<'_>,
) -> Result<TrustStatus<D>>
where
    B: PartialEq + Serialize + DeserializeOwned,
    D: Copy + PartialEq + std::fmt::Display + Serialize + DeserializeOwned,
{
    (ports.validate_binding)(&binding)?;
'''
GUARD_OLD = '''            crate::install_layout::validate_control_root(control)?;
            let lock = super::rotation::lock(control)?;
            super::rotation::require_trust_change_allowed(control)?;
            Some(lock)
'''
GUARD_NEW = '''            Some((ports.active_guard)(control)?)
'''
OS_SIGNATURE_OLD = '''    host: Host,
    store: &Path,
    ca: &Path,
    expected: Sha256Digest,
'''
OS_SIGNATURE_NEW = '''    host: Host,
    powershell: fn() -> Result<PathBuf>,
    store: &Path,
    ca: &Path,
    expected: &str,
'''


def cert_trust(trust):
    action = take(trust, "#[derive(Clone, Copy, PartialEq, Eq)]\npub enum Action {",
                  "#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\nenum Phase {")
    types = take(trust, "#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\nenum Phase {",
                 "pub(crate) fn public_ca(")
    types = replace_n(types, "\nenum Phase {", "\npub enum Phase {")
    types = replace_n(
        types,
        "struct Receipt {\n    schema: u32,\n    binding: InstallationBinding,\n    ca_sha256: Sha256Digest,\n"
        "    store: PathBuf,\n    phase: Phase,\n}",
        "pub struct Receipt<B, D> {\n    pub schema: u32,\n    pub binding: B,\n    pub ca_sha256: D,\n"
        "    pub store: PathBuf,\n    pub phase: Phase,\n}",
    )
    types = replace_n(types, "pub struct TrustStatus {", "pub struct TrustStatus<D> {")
    types = replace_n(types, "    pub ca_sha256: Sha256Digest,\n", "    pub ca_sha256: D,\n")
    types = replace_n(types, "\nenum Host {", "\npub enum Host {")
    types = replace_n(types, "    fn current() -> Self {", "    pub fn current() -> Self {")
    types = replace_n(types, "    fn name(self) -> &'static str {", "    pub fn name(self) -> &'static str {")
    public_ca = take(trust, "pub(crate) fn public_ca(", "fn ps_string(")
    public_ca = replace_n(public_ca, "pub(crate) fn public_ca(root: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {",
                          "pub fn public_ca(files: &Files, root: &Path, expected: &str) -> Result<Vec<u8>> {")
    public_ca = replace_n(public_ca, "read_private(root, \"ca.pem\", 8192)?", "(files.read)(root, \"ca.pem\", 8192)?")
    public_ca = replace_n(public_ca, "digest(certs[0].as_ref()) == expected", "sha256_hex(certs[0].as_ref()) == expected")
    os = take(trust, "fn ps_string(", "/// Read the protected endpoint against")
    os, _ = cut(os, "fn windows_program() -> Result<PathBuf> {", "fn observe(")
    os = replace_n(os, "fn ps_string(", "pub fn ps_string(")
    os = replace_n(os, "type Runner<'a>", "pub type Runner<'a>")
    os = replace_n(os, "\nfn observe(", "\npub fn observe(")
    os = replace_n(os, "\nfn mutate(", "\npub fn mutate(")
    os = replace_n(os, OS_SIGNATURE_OLD, OS_SIGNATURE_NEW, 2)
    os = replace_n(os, "let program = windows_program()?;", "let program = powershell()?;", 2)
    os = replace_n(os, "ps_string(&expected.to_string())?", "ps_string(expected)?", 2)
    os = replace_n(os, ".filter(|der| digest(der.as_ref()) == expected)", ".filter(|der| sha256_hex(der.as_ref()) == expected)")
    engine = take(trust, "fn manage_with(", "#[cfg(test)]\nmod tests;")
    engine = replace_n(engine, ENGINE_SIGNATURE_OLD, ENGINE_SIGNATURE_NEW)
    engine = replace_n(engine, GUARD_OLD, GUARD_NEW)
    engine = replace_n(engine, "public_ca(root, ca_sha256)?", "public_ca(&ports.files, root, &ca_sha256.to_string())?", 3)
    engine = replace_n(engine, "crate::user_paths::home()?", "crate::user_folders::home()?")
    engine = replace_n(engine, "crate::install_files::safe_file(&store)?;", "(ports.files.regular)(&store)?;")
    engine = replace_n(engine, "read_private(", "(ports.files.read)(", 4)
    engine = replace_n(engine, "write_private(", "(ports.files.write)(", 3)
    engine = replace_n(engine, "let receipt: Receipt = serde_json::from_slice(bytes)?;",
                       "let receipt: Receipt<B, D> = serde_json::from_slice(bytes)?;")
    engine = replace_n(engine, "observe(host, &store, &ca, ca_sha256, run)?",
                       "observe(host, ports.powershell, &store, &ca, &ca_sha256.to_string(), run)?")
    engine = replace_n(engine, "observe(host, &next.store, &ca, ca_sha256, run)?",
                       "observe(\n        host,\n        ports.powershell,\n        &next.store,\n        &ca,\n"
                       "        &ca_sha256.to_string(),\n        run,\n    )?")
    engine = replace_n(engine, "    mutate(\n        host,\n        &next.store,\n        &ca,\n        ca_sha256,\n",
                       "    mutate(\n        host,\n        ports.powershell,\n        &next.store,\n        &ca,\n"
                       "        &ca_sha256.to_string(),\n")
    tests = '''#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_use_the_lowercase_hex_spelling_and_arguments_are_quoted() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(ps_string("a'b").unwrap(), "'a''b'");
        assert!(ps_string("a\\rb").is_err());
    }
}
'''
    return (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "//! Explicit per-user trust of AIRK's local HTTPS issuer (split U90): the OS\n"
        "//! trust-store adapter (Windows CurrentUser Root through PowerShell, the\n"
        "//! macOS login keychain through security(1)) and the durable ownership\n"
        "//! receipt engine, shared by `airk` and `airkctl`'s offline trust (U75).\n"
        "//! No request path invokes it. Moved from `airk-cli`\n"
        "//! (`http_endpoint/trust.rs`) by `scripts/dev/split_u90b_cert_trust.py`.\n"
        "use anyhow::{Context, Result, bail, ensure};\n"
        "use rustls::pki_types::{CertificateDer, pem::PemObject};\n"
        "use serde::{Deserialize, Serialize, de::DeserializeOwned};\n"
        "use std::{\n    fmt::Write as _,\n    fs::OpenOptions,\n    path::{Path, PathBuf},\n};\n\n"
        "/// The ownership receipt beside the issuer, and its byte bound.\n"
        "pub const RECEIPT: &str = \"user-trust-1.json\";\n"
        "pub const LIMIT: usize = 16_384;\n\n"
        + PORTS
        + action.rstrip() + "\n\n"
        + types.rstrip() + "\n\n"
        + public_ca.rstrip() + "\n\n"
        + os.rstrip() + "\n\n"
        + engine.rstrip() + "\n\n"
        + tests
    )


TRUST_HEAD = '''// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user trust enrollment. No request path invokes this adapter.
//! The OS trust adapter and receipt engine live in `airk_process::cert_trust`
//! (split U90, `scripts/dev/split_u90b_cert_trust.py`); these are `airk`'s
//! callers, with its record files, PowerShell path and rotation guard.
use super::{InstallationBinding, read_private, write_private};
use airk_core::identity::Sha256Digest;
use airk_process::cert_trust as engine;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

mod rotation_receipts;
pub(crate) use rotation_receipts::{
    abort_rotation_new, commit_rotation_receipt, finalize_rotation_old, snapshot_rotation_receipt,
};

pub use engine::Action;
#[cfg(test)]
use engine::ps_string;
use engine::{Host, LIMIT, Phase, RECEIPT, Runner};
type Receipt = engine::Receipt<InstallationBinding, Sha256Digest>;
pub type TrustStatus = engine::TrustStatus<Sha256Digest>;

const PORTS: engine::Ports<InstallationBinding> = engine::Ports {
    files: engine::Files {
        read: read_private,
        write: write_private,
        regular: crate::install_files::safe_file,
    },
    powershell: windows_program,
    validate_binding: crate::http_endpoint::validate_binding,
    active_guard,
};

/// Serialize ordinary active-issuer mutations with Prepare/Commit before the
/// receipt lock: the verified control root, the rotation lock and no pending
/// rotation, in that order.
fn active_guard(control: &Path) -> Result<Box<dyn std::any::Any>> {
    crate::install_layout::validate_control_root(control)?;
    let lock = super::rotation::lock(control)?;
    super::rotation::require_trust_change_allowed(control)?;
    Ok(Box::new(lock))
}

pub(crate) fn public_ca(root: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    engine::public_ca(&PORTS.files, root, &expected.to_string())
}

'''

TRUST_TAIL = '''
fn manage_with(
    root: &Path,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    action: Action,
    host: Host,
    run: &mut Runner<'_>,
) -> Result<TrustStatus> {
    engine::manage_with(&PORTS, root, binding, ca_sha256, action, host, run)
}

#[cfg(test)]
mod tests;
'''


def thin_trust(trust):
    windows = take(trust, "fn windows_program() -> Result<PathBuf> {", "fn observe(")
    callers = take(trust, "/// Read the protected endpoint against", "fn manage_with(")
    return TRUST_HEAD + windows.rstrip() + "\n\n" + callers.rstrip() + "\n" + TRUST_TAIL


def main():
    for target in (IPC_TARGET, PROCESS_TARGET):
        if target.exists():
            raise SystemExit(f"already moved: {target.relative_to(ROOT)} exists")
    http, trust, rotation = read(HTTP), read(TRUST), read(ROTATION)
    http_rotation, install_files = read(HTTP_ROTATION), read(INSTALL_FILES)

    ipc = private_record(http, rotation, install_files)
    process = cert_trust(trust)
    new_trust = thin_trust(trust)

    http, _ = cut(http, "fn private_file(path: &Path) -> Result<()> {", "#[cfg(test)]\nmod tests;")
    http = replace_n(http, "use anyhow::{Result, ensure};\n", "use anyhow::Result;\n")
    http = replace_n(
        http, "fn digest(bytes: &[u8]) -> Sha256Digest {",
        "/// Moved to `airk-ipc` (U90) so `airkctl` keeps the same owner-private records.\n"
        "pub(crate) use airk_ipc::private_record::read_private;\n"
        "use airk_ipc::private_record::{private_file, write_private};\n\n"
        "fn digest(bytes: &[u8]) -> Sha256Digest {")

    rotation, _ = cut(rotation, "pub struct MutationLock(File);", "fn stage_root(")
    rotation = replace_n(
        rotation, "fn stage_root(",
        "/// The issuer-rotation lock: an owner-private lock file (moved to `airk-ipc`, U90).\n"
        "pub type MutationLock = airk_ipc::private_record::PrivateLock;\n"
        "pub fn lock(control: &Path) -> Result<MutationLock> {\n"
        "    airk_ipc::private_record::lock(\n"
        "        &control.join(\"http-1\"),\n"
        "        \"issuer-rotation.lock\",\n"
        "        \"issuer rotation already active\",\n"
        "    )\n"
        "}\n"
        "fn stage_root(")
    rotation = replace_n(rotation, "use std::{fs::File, path::PathBuf};\n", "use std::path::PathBuf;\n")

    http_rotation, clock = cut(http_rotation, "fn now_ms() -> Result<u64> {", "async fn current_endpoint(")
    http_rotation, report = cut(http_rotation, "fn output(control: &Path", "pub(super) struct Input {")
    http_rotation = replace_n(http_rotation, "    rotation::{self, Phase, Record},\n",
                              "    rotation::{self, Phase, Record, now_ms, output, saved_endpoint},\n")
    clock = replace_n(clock, "fn now_ms(", "pub(crate) fn now_ms(")
    clock = replace_n(clock, "fn saved_endpoint(", "pub(crate) fn saved_endpoint(")
    clock = replace_n(clock, "http_endpoint::installation_binding(", "installation_binding(")
    clock = replace_n(clock, "http_endpoint::load_descriptor(", "load_descriptor(")
    report = replace_n(report, "fn output(", "pub(crate) fn output(")
    report = replace_n(report, "rotation::read(control)?", "read(control)?")
    rotation = replace_n(
        rotation, "#[cfg(test)]\nmod tests;",
        "/// The `runtime http-issuer-rotation` reply (moved from\n"
        "/// `runtime_install/http_rotation.rs`, U90), also the service's M18 `Status`.\n"
        + clock.rstrip() + "\n\n" + report.rstrip() + "\n\n#[cfg(test)]\nmod tests;")

    ipc_lib = replace_n(read(IPC_LIB), "pub mod frame;\n", "pub mod frame;\npub mod private_record;\n")
    process_lib = replace_n(read(PROCESS_LIB), "pub mod child_job;\n", "pub mod cert_trust;\npub mod child_job;\n")
    manifest = replace_n(
        read(PROCESS_MANIFEST), "[dependencies]\nanyhow.workspace = true\n",
        "[dependencies]\nanyhow.workspace = true\n"
        "# U90: issuer PEM parsing, receipt serde and DER digests; already in the\n"
        "# graph (airk-cli) at these versions and feature sets.\n"
        "rustls.workspace = true\nserde.workspace = true\nserde_json.workspace = true\nsha2.workspace = true\n")

    write(IPC_TARGET, ipc)
    write(PROCESS_TARGET, process)
    write(TRUST, new_trust)
    write(HTTP, http)
    write(ROTATION, rotation)
    write(HTTP_ROTATION, http_rotation)
    write(IPC_LIB, ipc_lib)
    write(PROCESS_LIB, process_lib)
    write(PROCESS_MANIFEST, manifest)
    return 0


if __name__ == "__main__":
    sys.exit(main())
