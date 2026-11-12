# SPDX-License-Identifier: Apache-2.0
"""Give the request journal its own module (split step 3c.3b, boundary repair).

Step 3c.3b let the service answer M5-M8 through `crate::client_requests`.
That module is also the CLI's client of the journal, so it reaches
`console_client` and `service_configuration`. The new edge
`service_runtime -> client_requests` therefore merged the baseline cycle
[client_requests, service_configuration] into the large service cycle, and the
boundary gate failed.

This script untangles the journal in place. It moves, verbatim, from
`crates/airk-cli/src/client_requests.rs` to a new
`crates/airk-cli/src/request_journal.rs`: the bounds, the saved-record type,
lock, directory, entries, field checks, read, persist, M5 `prepare_keyed`,
`CAPACITY`, `list`, `forget` and `forget_existed`, together with the test that
exercises them. The journal module reaches neither `console_client` nor
`service_configuration`.

`client_requests` keeps the CLI client half (epoch acquisition, `prepare`,
`prepare_configuration`, `refusal_guidance`, `submit_response`, `submit`) and
re-exports `Pending`, `list` and `forget`, so `main.rs` callers are unchanged.
The locked read at the start of `submit_response` becomes
`request_journal::saved`, and the crossed-receipt check becomes
`request_journal::check_receipt`. The service's M7 handler sends the saved
request through those same two functions and `console_client`, as it did
through `client_requests::submit_response`. The service handlers name only
`request_journal`.

Each edit is asserted to apply exactly once; run it once on the pre-move tree.
It prints the old -> new test identity map for `test_identities.py compare`.

    python scripts/dev/split_3c3b_request_journal.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates" / "airk-cli" / "src"
CLIENT = SRC / "client_requests.rs"
JOURNAL = SRC / "request_journal.rs"
HANDLERS = SRC / "service_runtime" / "journal.rs"
LIB = SRC / "lib.rs"


def read(path):
    return path.read_bytes().decode()


def write(path, text):
    path.write_bytes(text.encode())


def cut(text, start, end):
    """The text from the line beginning `start` up to the line beginning `end`."""
    assert text.count(start) == 1, start
    assert text.count(end) == 1, end
    first = text.index(start)
    last = text.index(end)
    assert first < last, (start, end)
    return text[first:last]


def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


source = read(CLIENT)
assert not JOURNAL.exists(), "already moved"

bounds = cut(source, "const LIMIT: usize = 64;", "/// Epoch acquisition precedes")
records = cut(source, "#[derive(Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Saved {",
              "pub async fn prepare(\n")
keyed = cut(source, "/// M5 (contracts §2.4): journal a keyed request",
            "pub fn refusal_guidance(")
forgetting = cut(source, "/// Explicit forgetting never cancels native work",
                 "#[cfg(test)]\nmod tests {")
TEST_START = "    #[test]\n    fn exact_requests_survive_reopen_and_capacity_refuses_before_more_work()"
assert source.count(TEST_START) == 1 and source.endswith("    }\n}\n")
# The last test runs to the end of the test module.
persist_test = source[source.index(TEST_START):-len("\n}\n")]

old_submit_head = """pub(crate) async fn submit_response(control: &Path, nonce: Incarnation) -> Result<ControlResponse> {
    let control = control.canonicalize()?;
    let control = control.as_path();
    let saved = {
        let _lock = lock(control)?;
        entries(control)?;
        read(control, nonce)?
    };
    let (_, project, kind) = fields(&saved)?;
    let response = crate::console_client::request(control, saved.request).await?;
    if let ControlResponse::Job { job } = &response {
        ensure!(
            job.project == project && job.kind == kind,
            "crossed native CLI job receipt"
        );
    }
    Ok(response)
}
"""
new_submit_head = """pub(crate) async fn submit_response(control: &Path, nonce: Incarnation) -> Result<ControlResponse> {
    let (control, request, project, kind) = crate::request_journal::saved(control, nonce)?;
    let response = crate::console_client::request(&control, request).await?;
    crate::request_journal::check_receipt(&response, project, kind)?;
    Ok(response)
}
"""

journal_helpers = """/// The saved request for M7 under the held lock: the canonical control
/// folder, the exact request with its saved key, and its project and kind.
pub(crate) fn saved(
    control: &Path,
    nonce: Incarnation,
) -> Result<(PathBuf, ControlMethod, ProjectBinding, JobKind)> {
    let control = control.canonicalize()?;
    let saved = {
        let _lock = lock(&control)?;
        entries(&control)?;
        read(&control, nonce)?
    };
    let (_, project, kind) = fields(&saved)?;
    Ok((control, saved.request, project, kind))
}
/// A job receipt must name the saved request's project and kind.
pub(crate) fn check_receipt(
    response: &ControlResponse,
    project: ProjectBinding,
    kind: JobKind,
) -> Result<()> {
    if let ControlResponse::Job { job } = response {
        ensure!(
            job.project == project && job.kind == kind,
            "crossed native CLI job receipt"
        );
    }
    Ok(())
}
"""

journal = (
    "// SPDX-License-Identifier: Apache-2.0\n"
    "//! The bounded durable request journal (contracts §2.4, M5-M8): its file\n"
    "//! format, lock and limits. It reaches neither the control client nor the\n"
    "//! configuration client; the `client_requests` module is the CLI's client of it.\n"
    "use airk_core::{\n"
    "    identity::Incarnation,\n"
    "    service::{ControlMethod, ControlResponse, JobKind, ProjectBinding, RequestKey},\n"
    "};\n"
    "use anyhow::{Context, Result, ensure};\n"
    "use serde::{Deserialize, Serialize};\n"
    "use std::{\n"
    "    fs::{self, File, OpenOptions},\n"
    "    io::{Read, Write},\n"
    "    path::{Path, PathBuf},\n"
    "};\n\n"
    + bounds
    + records.replace("fn persist(control: &Path, request: ControlMethod)",
                      "pub(crate) fn persist(control: &Path, request: ControlMethod)")
    + keyed
    + journal_helpers
    + forgetting
    + "#[cfg(test)]\nmod tests {\n    use super::*;\n\n"
    + persist_test
    + "\n}\n"
)
assert journal.count("pub(crate) fn persist(control: &Path, request: ControlMethod)") == 1

client = source
for block in (bounds, records, keyed, forgetting):
    client = replace_once(client, block, "")
client = replace_once(client, "\n" + persist_test, "")
client = replace_once(client, old_submit_head, new_submit_head)
client = replace_once(client, """    persist(
        &control,
        ControlMethod::Enqueue {""", """    crate::request_journal::persist(
        &control,
        ControlMethod::Enqueue {""")
client = replace_once(client, """    persist(
        &control,
        ControlMethod::ConfigureProject {""", """    crate::request_journal::persist(
        &control,
        ControlMethod::ConfigureProject {""")
client = replace_once(client, """//! Bounded durable CLI requests. Uncertain work is retried only with its saved key.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobKind, ProjectBinding, RequestKey},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
""", """//! Bounded durable CLI requests. Uncertain work is retried only with its saved key.
//! The journal itself (file format, lock and limits) is `crate::request_journal`.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobKind, ProjectBinding, RequestKey},
};
use anyhow::{Result, ensure};
use std::path::Path;

pub use crate::request_journal::{Pending, forget, list};
""")
write(CLIENT, client)
write(JOURNAL, journal)

handlers = read(HANDLERS)
handlers = replace_once(handlers, "//! service. The journal's owner, `crate::client_requests`, keeps its file\n",
                        "//! service. The journal's owner, `crate::request_journal`, keeps its file\n")
for old, new in (
    ("crate::client_requests::prepare_keyed(", "crate::request_journal::prepare_keyed("),
    ("crate::client_requests::list(control)", "crate::request_journal::list(control)"),
    ("crate::client_requests::CAPACITY", "crate::request_journal::CAPACITY"),
    ("crate::client_requests::forget_existed(", "crate::request_journal::forget_existed("),
):
    handlers = replace_once(handlers, old, new)
handlers = replace_once(handlers, """pub(super) async fn submit(control: &Path, nonce: Incarnation) -> ControlResponse {
    match crate::client_requests::submit_response(control, nonce).await {""", """/// M7: the saved request with its exact key, sent as `airk` sent it.
async fn send_saved(control: &Path, nonce: Incarnation) -> anyhow::Result<ControlResponse> {
    let (control, request, project, kind) = crate::request_journal::saved(control, nonce)?;
    let response = crate::console_client::request(&control, request).await?;
    crate::request_journal::check_receipt(&response, project, kind)?;
    Ok(response)
}

pub(super) async fn submit(control: &Path, nonce: Incarnation) -> ControlResponse {
    match send_saved(control, nonce).await {""")
assert "client_requests" not in handlers.replace("`crate::client_requests`", "")
write(HANDLERS, handlers)

lib = read(LIB)
lib = replace_once(lib, "pub mod client_requests;\n", "pub mod client_requests;\nmod request_journal;\n")
write(LIB, lib)

print(json.dumps({"identity_map": {
    "airk-cli::lib/airk_cli client_requests::tests::exact_requests_survive_reopen_and_capacity_refuses_before_more_work":
    "airk-cli::lib/airk_cli request_journal::tests::exact_requests_survive_reopen_and_capacity_refuses_before_more_work",
}}, indent=2))
