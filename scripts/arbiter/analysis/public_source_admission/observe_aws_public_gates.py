# SPDX-License-Identifier: Apache-2.0
"""Record native AWS publication, status, positive-file, answer-range and cursor outcomes independently."""
from pathlib import Path
import json
import runpy

HERE = Path(__file__).resolve().parent
durable_scope = runpy.run_path(str(HERE / "aws_grant_lifetime_control.py"))["durable_scope"]
native = runpy.run_path(str(HERE / "native_cohort_source_reads.py"))
require = native["require"]


def original_scope(response, scope, revision):
    require(durable_scope(response["scope_observation"]) == durable_scope(scope)
            and response["publication"]["token"] == revision, "native publication or public scope changed")
    return response["scope_observation"]["fingerprint"]


def pages(first, tool, members, limit, prefix=""):
    """The native cursor and terminal flag bound pagination even if independent full status is unavailable."""
    scope, revision = first["scope_observation"], first["publication"]["token"]
    response = first
    seen = set()
    cursors = set()
    while True:
        require(original_scope(response, scope, revision) == scope["fingerprint"], "cursor grant changed")
        rows = response["paths"]
        require(isinstance(rows, list) and 0 < len(rows) <= limit and len(set(rows)) == len(rows)
                and all(p in members and p.startswith(prefix) and p not in seen for p in rows), "repeated, empty or foreign native page")
        seen.update(rows)
        yield rows
        cursor = response.get("cursor")
        if not response["truncated"]:
            require(not cursor, "terminal native page includes a continuation")
            return
        require(isinstance(cursor, str) and cursor and cursor not in cursors and len(seen) < len(members),
                "missing, repeated or excessive native continuation")
        cursors.add(cursor)
        response = tool(f"menu-page-{len(cursors):04d}", "airk_list_files",
                        {"prefix": prefix, "limit": limit, "max_bytes": 65536, "response_mode": "full",
                         "include_status": False, "cursor": cursor})


def source_gates(w, output, first, cases, tree, tool):
    scope, revision = first["scope_observation"], first["publication"]["token"]
    case, = cases
    members = case["members"]
    span_file = native["CAMPAIGN"] / "aws-unique-source-corpus-r1/manifest.private.json"
    native["checked"](span_file, "ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882")
    originals = json.loads(span_file.read_bytes())["cases"]
    stable = {row["question_sha256"]: row for row in case["cases"]}
    require(len(originals) == len(stable) == 41, "original positive cohort differs")
    normalized = []
    for row in originals:
        canon = stable[row["question_sha256"]]
        path = canon["source_path"].removeprefix("documents/")
        require(row["selector"] == canon["selector"] and members[path]["sha256"] == row["document"]["sha256"],
                "original positive member or selector differs")
        normalized.append({**row, "document": {**row["document"], "path": path}})
    positive = sorted({row["document"]["path"] for row in normalized})
    require(len(positive) == 27, "original positive path count differs")
    files, ranges = [], []

    def gate(label, action):
        try:
            return {"ok": True, "observation": action()}
        except (AssertionError, ValueError, TimeoutError, KeyError, TypeError) as error:
            # Exact native arguments and outputs are separately retained by the real CLI runner.
            return {"ok": False, "request_label": label, "error_type": type(error).__name__, "error": str(error)}

    def status():
        body = tool("source-status", "airk_status", {"response_mode": "full", "include_status": True})
        require(durable_scope(body["scope_observation"]) == durable_scope(scope), "full status returned foreign scope")
        count = body.get("files")
        if type(count) is not int or not 0 < count <= len(members):
            return {"available": False, "files": count, "index_state": body.get("index_state"),
                    "index_error": body.get("index_error"), "cache_age_ms": body.get("cache_age_ms")}
        original_scope(body, scope, revision)
        return {"available": True, "files": count}

    status_result = gate("source-status", status)
    w["write_json"](output / "full-status.private.json", status_result)

    def file_observation(label, path):
        body = tool(label, "airk_file_info", {"path": path, "response_mode": "full", "include_status": False})
        fingerprint = original_scope(body, scope, revision)
        require(body["path"] == path and body["content_hash"] == members[path]["sha256"], "positive file differs")
        return {"path": path, "sha256": body["content_hash"], "scope_fingerprint": fingerprint}

    for number, path in enumerate(positive):
        label = f"positive-info-{number:02}"
        files.append({"path": path, **gate(label, lambda: file_observation(label, path))})
    w["write_json"](output / "positive-files.private.json", files)

    def range_observation(label, row):
        span = row["source_span"]
        path = row["document"]["path"]
        body = tool(label, "airk_read_file", {"path": path, "start_line": span["line_start"],
                    "end_line": span["line_end_inclusive"], "max_bytes": 65536,
                    "response_mode": "full", "include_status": False})
        fingerprint = original_scope(body, scope, revision)
        return {"scope_fingerprint": fingerprint, **native["verified_lines"](row, (tree / path).read_bytes(), body)}

    for number, row in enumerate(normalized):
        label = f"source-range-{number:02}"
        ranges.append({"selector": row["selector"], **gate(label, lambda: range_observation(label, row))})
    w["write_json"](output / "source-ranges.private.json", ranges)

    inventory = []
    def inventory_observation():
        for rows in pages(first, tool, members, 100):
            inventory.append(rows)
        missing = sorted(members.keys() - {path for page in inventory for path in page})
        return {"missing": missing, "original_reported_visible_count": 35400,
                "difference_from_original_count": sum(map(len, inventory)) - 35400}

    listing = gate("public-menu-pages", inventory_observation)
    w["write_json"](output / "native-menu.private.json", {"result": listing, "pages": inventory})
    w["write_json"](output / "public-native-census.private.json", {"revision": revision, "first_scope": scope,
        "source_files": len(members), "status": status_result, "terminal_page_verified": listing["ok"],
        "native_pages_observed": len(inventory), "native_files_observed": sum(map(len, inventory)),
        "positive_files_verified": sum(row["ok"] for row in files), "positive_files_total": len(files),
        "source_ranges_verified": sum(row["ok"] for row in ranges), "source_ranges_total": len(ranges),
        "excluded_ambiguous": 24, "scored_requests": 0, "source_only_config_applied": False,
        "original_quota_refusal_preserved": True, "ranker_observed": False})
