# SPDX-License-Identifier: Apache-2.0
"""Stage combined read retry wiring without changing frozen native sources."""
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent / "combined-retry-copies"

def stage(name, replacements):
    path = base / name
    if path.exists():
        return
    text = (root / name).read_text(encoding="utf-8")
    for before, after in replacements:
        assert text.count(before) == 1, (name, before)
        text = text.replace(before, after, 1)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")

stage("crates/airk-core/src/combined_reads.rs", [
    ("pub mod query_scope;", "pub mod query_scope;\npub mod read_retry;")])
stage("crates/airk-kernel/src/combined_reads.rs", [
    ("    pub error: Option<CombinedReadFailure>,", "    pub error: Option<CombinedReadFailure>,\n    /// Safe only for the exact sole failed required member; all native detail remains inline.\n    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n    pub read_retry: Option<airk_core::combined_reads::read_retry::SingleRequiredMemberRetry>,"),
    ("        error: (partial && request.selection.member_availability", "        read_retry: None,\n        error: (partial && request.selection.member_availability"),
    ("    result.publication_vector.validate()?;", "    result.publication_vector.validate()?;\n    result.read_retry = airk_core::combined_reads::read_retry::proposed_retry(\n        &serde_json::to_value(&result)?,\n    );")])
stage("crates/airk-cli/tests/common/read_retry.rs", [
    ("    let (delay, fresh_brokerage, publication) = if hint.get(\"status\")", "    if let Some(retry) = airk_core::combined_reads::read_retry::validated_retry(&hint) {\n        return Some(Retry {\n            delay: Duration::from_millis(retry.retry_after_ms),\n            fresh_brokerage: retry.fresh_brokerage_required,\n            publication: matches!(retry.code.as_str(), \"publication_busy\" | \"selection_busy\"),\n        });\n    }\n    let (delay, fresh_brokerage, publication) = if hint.get(\"status\")")])
stage("crates/airk-kernel/src/combined_reads_tests.rs", [])
stage("crates/airk-cli/tests/native_read_retry.rs", [])
stage("crates/airk-cli/tests/combined_reads.rs", [])

patch = []
for path in sorted(base.rglob("*.rs")):
    name = path.relative_to(base).as_posix()
    original = root / name
    before = original.read_text(encoding="utf-8") if original.exists() else ""
    after = path.read_text(encoding="utf-8")
    patch.extend(difflib.unified_diff(before.splitlines(keepends=True), after.splitlines(keepends=True),
        fromfile="a/" + name if before else "/dev/null", tofile="b/" + name))
destination = base.parent / "combined-retry.patch"
destination.write_text("".join(patch), encoding="utf-8", newline="\n")
print(destination)
