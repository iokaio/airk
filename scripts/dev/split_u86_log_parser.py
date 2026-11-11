# SPDX-License-Identifier: Apache-2.0
"""Move the pure log parsing and rendering into airk-core (split U86, step 3b.1f-1).

`airkctl service logs` must print what `airk service logs` printed, from the
running service (M13) and from the files while the service is down. One
parser and renderer serve both. This script moves, verbatim, from
`crates/airk-cli/src/service_logs.rs` into `crates/airk-core/src/service_logs.rs`:

* the per-file parsing block of `read_retained` (newline split, the 768-byte
  record bound, validation, malformed counts, truncation flags, newest-first
  order, the record limit) as `read_tail(page, bytes, started_mid_file, limit)`;
* its final reverse and encoded-size bound as `finish(page, output_bytes)`;
* `render` and the record bound `MAX_RECORD_BYTES`.

`airk-cli` keeps its file I/O loop, which now calls `read_tail` and `finish`,
and re-exports `render`, so callers and tests are unchanged. Each edit is
asserted to apply exactly once; run once on the pre-move tree.

    python scripts/dev/split_u86_log_parser.py
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli" / "src" / "service_logs.rs"
CORE = ROOT / "crates" / "airk-core" / "src" / "service_logs.rs"

LOOP_BODY = """        remaining -= bytes.len();
        let complete_tail = bytes.last() == Some(&b'\\n');
        page.tail_truncated |= start > 0 || !complete_tail;
        let mut lines: Vec<_> = bytes.split(|byte| *byte == b'\\n').collect();
        if start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        if !lines.is_empty() {
            lines.pop();
        } // empty terminator or unfinished last record
        for line in lines.into_iter().rev() {
            if line.len() > RECORD_BYTES {
                page.malformed_records += 1;
                continue;
            }
            match serde_json::from_slice::<LogRecord>(line) {
                Ok(record) if record.validate().is_ok() => page.records.push(record),
                _ => page.malformed_records += 1,
            }
            if page.records.len() == limit {
                // The recent page cannot certify that older records were absent.
                page.tail_truncated = true;
                break;
            }
        }
        if page.records.len() == limit || remaining == 0 {
            break;
        }
    }
    page.records.reverse();
    ensure!(
        serde_json::to_vec(&page)?.len() <= output_bytes,
        "diagnostic log page exceeds bound"
    );
    Ok(page)
}
"""

LOOP_CALL = """        remaining -= bytes.len();
        let full = airk_core::service_logs::read_tail(&mut page, &bytes, start > 0, limit);
        if full || remaining == 0 {
            break;
        }
    }
    airk_core::service_logs::finish(page, output_bytes)
}
"""


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"expected one occurrence of: {old[:80]!r}")
    return text.replace(old, new)


def main():
    core = CORE.read_bytes().decode()
    if "pub fn read_tail(" in core:
        raise SystemExit("already moved: airk_core::service_logs::read_tail exists")
    cli = CLI.read_bytes().decode()
    cli = replace_once(cli, LOOP_BODY, LOOP_CALL)
    start = cli.index("pub fn render(page: &LogPage) -> Result<String> {")
    end = cli.index("#[cfg(test)]\nmod tests;", start)
    render = cli[start:end]
    cli = cli[:start] + (
        "/// Moved to `airk-core` (U86); re-exported for existing callers.\n"
        "pub use airk_core::service_logs::render;\n\n"
    ) + cli[end:]
    cli = replace_once(cli, "const RECORD_BYTES: usize = 768;\n",
                       "const RECORD_BYTES: usize = airk_core::service_logs::MAX_RECORD_BYTES;\n")
    CLI.write_bytes(cli.encode())

    parse = LOOP_BODY.split("        if page.records.len() == limit || remaining == 0 {")[0]
    parse = parse.replace("        remaining -= bytes.len();\n", "")
    parse = parse.replace("RECORD_BYTES", "MAX_RECORD_BYTES")
    parse = "\n".join(line[4:] if line.startswith("    ") else line for line in parse.split("\n"))
    core = replace_once(core, "use crate::identity::{Incarnation, Sha256Digest};\n",
                        "use crate::identity::{Incarnation, Sha256Digest};\nuse anyhow::{Result, ensure};\n")
    core = core.rstrip("\n") + "\n\n" + (
        "/// The largest retained log record (U86; moved from `airk-cli`).\n"
        "pub const MAX_RECORD_BYTES: usize = 768;\n\n"
        "/// Add one retained file's tail to `page`, newest record first. Files are\n"
        "/// read newest first; `started_mid_file` is true when the read began after\n"
        "/// the file's start. Returns `true` once `limit` records are held (U86).\n"
        "pub fn read_tail(page: &mut LogPage, bytes: &[u8], started_mid_file: bool, limit: usize) -> bool {\n"
        "    let start = u64::from(started_mid_file);\n"
        + parse.rstrip() + "\n"
        "    page.records.len() == limit\n"
        "}\n\n"
        "/// Put the records oldest first and hold the page to its encoded bound (U86).\n"
        "pub fn finish(mut page: LogPage, output_bytes: usize) -> Result<LogPage> {\n"
        "    page.records.reverse();\n"
        "    ensure!(\n"
        "        serde_json::to_vec(&page)?.len() <= output_bytes,\n"
        "        \"diagnostic log page exceeds bound\"\n"
        "    );\n"
        "    Ok(page)\n"
        "}\n\n"
        "/// The text form of a recent page (U86; moved from `airk-cli`).\n"
        + render.rstrip() + "\n"
    )
    CORE.write_bytes(core.encode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
