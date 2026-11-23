# SPDX-License-Identifier: Apache-2.0
"""Credential probe: list every indexed path and flag credential-like names, opening no file.

Each page of the listing is one fixture CLI call to ``airk_list_files``
(limit 100, paged by ``cursor``). Only the returned paths are examined, and
each basename is matched case-insensitively against the credential patterns:

* ``aikeys.json``
* ``appsettings*.json``. These are only *named* here: whether one holds keys
  cannot be known without opening it, so each is listed for a human decision.
* ``*secret*``
* ``.env`` and ``.env.*``
* ``*.pfx``
* ``set-*-key.ps1``

The publication token is read before and after (``settled.snapshot``), so a
listing taken across a publication change is marked. The result goes to
``--out`` (default ``R/fixture/credential-probe-<repo>.json``). A match is a
finding to report. The probe changes nothing: no exclusion is added here.

``python -I -B credential_probe.py --handle <root>/<lifetime>/handle.json --repo munarium --out <file>``

``--repo`` is a scenario repository (``bookbuilder`` resolves to the ``bookbuilder-v2`` index through the
root's ``index-aliases.json``) or an index name of the lifetime.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import settled  # noqa: E402

PATTERNS = {"aikeys.json": "aikeys.json", "appsettings*.json": "appsettings*.json", "*secret*": "*secret*",
            ".env": ".env", ".env.*": ".env.*", "*.pfx": "*.pfx", "set-*-key.ps1": "set-*-key.ps1"}


def classify(paths: list[str]) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {name: [] for name in PATTERNS}
    for path in paths:
        base = path.replace("\\", "/").rsplit("/", 1)[-1].lower()
        for name, pattern in PATTERNS.items():
            if fnmatch.fnmatchcase(base, pattern):
                found[name].append(path)
    return found


def entries_of(body: dict) -> list[str]:
    entries = body.get("paths") or body.get("files") or body.get("entries") or []
    return [e["path"] if isinstance(e, dict) else e for e in entries if isinstance(e, (dict, str))]


def _list_once(handle: common.Handle, repo: str, page_limit: int, max_pages: int) -> dict:
    paths, cursor, pages, errors = [], None, 0, []
    while True:
        args = {"limit": page_limit, "response_mode": "full"}
        if cursor:
            args["cursor"] = cursor
        outcome, _ = common.call_tool_retrying(handle, "airk_list_files", repo, args, 300.0)
        pages += 1
        body = common.envelope_body(outcome.envelope) if outcome.envelope else None
        if not outcome.ok or not isinstance(body, dict):
            errors.append({"page": pages, "error": outcome.error, "stdout_head": outcome.stdout[:400].decode(
                "utf-8", "replace")})
            break
        paths.extend(entries_of(body))
        cursor = body.get("cursor") or body.get("next_cursor")
        if not cursor or pages >= max_pages:
            break
    return {"paths": paths, "pages": pages, "errors": errors, "complete": not errors and not cursor}


def list_all(handle: common.Handle, repo: str, page_limit: int = 100, max_pages: int = 400, attempts: int = 8) -> dict:
    """The whole listing. A discovery cursor expires when a new publication lands (vectors still building on
    another index, 22 Nov 2026), so an expired listing starts again from the first page, up to ``attempts``."""
    restarts = 0
    while True:
        result = _list_once(handle, repo, page_limit, max_pages)
        expired = any("cursor expired" in e.get("stdout_head", "") for e in result["errors"])
        if not expired or restarts + 1 >= attempts:
            return {**result, "restarts": restarts}
        restarts += 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--repo", default="bookbuilder")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    handle_path = common.resolve_handle(args.handle, common.DEFAULT_R / "fixture")
    handle = common.load_handle(handle_path)
    start = settled.snapshot(handle, args.repo)
    listing = list_all(handle, args.repo)
    end = settled.snapshot(handle, args.repo)
    found = classify(listing["paths"])
    report = {"schema": "airk.private.answer-eval-credential-probe/1", "generated": common.now_iso(),
              "repo": args.repo, "handle": str(handle_path), "handle_sha256": handle.sha256,
              "index_id": handle.index_id(args.repo), "method": "airk_list_files paging (paths only; no file opened)",
              "patterns": list(PATTERNS), "listed_paths": len(listing["paths"]),
              "unique_paths": len(set(listing["paths"])), "pages": listing["pages"],
              "complete": listing["complete"], "errors": listing["errors"],
              "token_start": start.get("publication_token"), "token_end": end.get("publication_token"),
              "token_unchanged": settled.compare(start, end)["unchanged"],
              "matches": {k: sorted(set(v)) for k, v in found.items()},
              "match_count": sum(len(set(v)) for v in found.values()),
              "note": "appsettings*.json are listed by name only; whether one holds keys needs a human decision"}
    out = args.out or common.DEFAULT_R / "fixture" / f"credential-probe-{args.repo}.json"
    common.write_json(out, report)
    print(json.dumps({k: report[k] for k in ("repo", "listed_paths", "pages", "complete", "token_unchanged",
                                             "match_count", "matches")}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
