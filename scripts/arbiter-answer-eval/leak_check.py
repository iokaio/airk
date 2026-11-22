# SPDX-License-Identifier: Apache-2.0
"""Leak check of the airk index: frozen scenario phrases, the full listing and the credential probes.

1. **Phrases.** For every scenario in the freeze (development and sealed, both
   repositories), the scenario's ``leak_probe`` phrase is searched on airk
   (``airk_search``, limit 16, full mode). Every returned path is checked
   against ``fixture.py``'s airk exclusion rules.
2. **Listing.** The complete airk file listing (``airk_list_files`` paging) is
   checked against the same rules.
3. **Credentials.** The credential probe (``credential_probe.list_all`` and
   ``classify``) runs on each ``--credential-repos`` index.

The result goes to ``--out``. Any excluded path served or listed fails the
check (exit 1). Only paths are examined; no file is opened.

``python -I -B leak_check.py --out <R>/fixture/leak-probe-fix3.json [--credential-repos airk,bookbuilder]``
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import credential_probe  # noqa: E402
import evidence  # noqa: E402
import settled  # noqa: E402


def fixture_module():
    spec = importlib.util.spec_from_file_location("answer_eval_fixture_rules", HERE / "fixture.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def served_paths(body) -> list[str]:
    return sorted({s.path for s in evidence.extract(body)})


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--freeze", type=Path, default=common.DEFAULT_R / "scenario-freeze.json")
    parser.add_argument("--credential-repos", default="airk,bookbuilder")
    args = parser.parse_args(argv)
    fx = fixture_module()
    handle_path = common.resolve_handle(args.handle, common.DEFAULT_R / "fixture")
    handle = common.load_handle(handle_path)
    start = settled.snapshot(handle, "airk")
    frozen = common.read_json(args.freeze)["files"]
    probes, failures = [], []
    for rel, digest in sorted(frozen.items()):
        path = args.freeze.parent / rel
        common.require(common.sha256_file(path) == digest, f"{rel} differs from the freeze")
        scenario = common.read_json(path)
        outcome, _ = common.call_tool_retrying(handle, "airk_search", "airk", {"query": scenario["leak_probe"],
                                                                               "limit": 16, "response_mode": "full"},
                                               300.0)
        body = common.envelope_body(outcome.envelope) if outcome.ok else None
        paths = served_paths(body) if body is not None else []
        bad = [p for p in paths if fx.excluded(p, "airk")]
        probes.append({"scenario": rel, "ok": outcome.ok, "error": outcome.error,
                       "stdout_head": None if outcome.ok else outcome.stdout[:300].decode("utf-8", "replace"),
                       "paths": len(paths),
                       "excluded_hits": bad, "elapsed_ms": outcome.elapsed_ms})
        failures += [{"scenario": rel, "path": p} for p in bad]
        if not outcome.ok:
            failures.append({"scenario": rel, "error": outcome.error})
    listing = credential_probe.list_all(handle, "airk")
    listed_excluded = sorted(p for p in listing["paths"] if fx.excluded(p, "airk"))
    credentials = {}
    for repo in args.credential_repos.split(","):
        found = listing if repo == "airk" else credential_probe.list_all(handle, repo)
        matches = {k: sorted(set(v)) for k, v in credential_probe.classify(found["paths"]).items() if v}
        credentials[repo] = {"index_id": handle.index_id(repo), "listed": len(found["paths"]),
                             "complete": found["complete"], "matches": matches,
                             "denied_set_key_scripts_present": [p for p in found["paths"]
                                                               if fx.excluded(p, "bookbuilder")] if repo != "airk" else []}
    end = settled.snapshot(handle, "airk")
    result = {"schema": "airk.private.answer-eval-leak-check/1", "generated": common.now_iso(),
              "handle": str(handle_path), "readiness": handle.readiness(), "airk_index_id": handle.index_id("airk"),
              "airk_token_start": start.get("publication_token"), "airk_token_end": end.get("publication_token"),
              "probes": probes, "probe_count": len(probes), "listed_files": len(listing["paths"]),
              "listing_complete": listing["complete"], "listed_excluded_paths": listed_excluded,
              "exclusions": fx.EXCLUSIONS["airk"], "credentials": credentials, "failures": failures}
    result["passed"] = not failures and not listed_excluded and listing["complete"] and all(
        not c["denied_set_key_scripts_present"] for c in credentials.values())
    common.write_json(args.out, result)
    print(json.dumps({k: result[k] for k in ("passed", "probe_count", "listed_files", "listing_complete",
                                             "listed_excluded_paths", "failures")}))
    print(json.dumps({repo: {k: v for k, v in c.items() if k != "index_id"} for repo, c in credentials.items()}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
