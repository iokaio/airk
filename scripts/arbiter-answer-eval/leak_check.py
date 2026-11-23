# SPDX-License-Identifier: Apache-2.0
"""Leak check of a lifetime's indexes: scenario phrases, the full listings, credentials and no airk index.

The check is the same for every repository of the pass (default: the loop
pair, munarium and bookbuilder). A scenario repository is served by an index
of the lifetime, through the fixture root's ``index-aliases.json`` when the
names differ (``bookbuilder`` by ``bookbuilder-v2``); the exclusion list is
``fixture.py``'s list for that index.

1. **No airk index.** The lifetime must hold no airk index: the index under
   test holds nothing about the experiment (user direction, 22 November
   2026). The handle, the fixture root's state files and the service's own
   index catalog are read (``fixture.forbidden_index_findings``).
   ``--allow-airk-index`` is for the one-turn history root only.
2. **Phrases.** Every scenario's ``leak_probe`` phrase is searched on its own
   repository's index (``airk_search``, limit 16, full mode) and every
   returned path is checked against that index's exclusion rules. The
   scenarios come from the freeze when it holds that repository (each file
   re-pinned), else from the scenario folders.
3. **Listing.** The complete file listing of each index (``airk_list_files``
   paging) is checked against the same rules.
4. **Credentials.** The credential probe (``credential_probe.classify``) names
   the credential-like paths of each listing.

A repository without exclusions (munarium) is checked the same way: its
phrases are searched, its listing is read, and the result records the empty
list it was compared against. The result goes to ``--out``. An excluded path
served or listed, an incomplete listing, a failed call or an airk index fails
the check (exit 1). Only paths are examined; no file is opened.

``python -I -B leak_check.py --handle <root>/<lifetime>/handle.json --out <root>/leak-check.json [--repos munarium,bookbuilder]``
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import credential_probe  # noqa: E402
import evidence  # noqa: E402
import settled  # noqa: E402

SCHEMA = "airk.private.answer-eval-leak-check/2"


def fixture_module():
    spec = importlib.util.spec_from_file_location("answer_eval_fixture_rules", HERE / "fixture.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def served_paths(body) -> list[str]:
    return sorted({s.path for s in evidence.extract(body)})


SCENARIO_SCHEMA = "airk.private.answer-eval-scenario/1"


def scenario_probes(evaluation: Path, repo: str) -> list[dict]:
    """One repository's ``leak_probe`` phrases as ``{source, phrase[, sha256]}`` rows.

    From the scenario freeze when it holds scenarios of that repository (every frozen file is re-pinned),
    else from ``scenarios/<repo>/*.json`` and ``sealed/<repo>/*.json`` (rows without a pin). Only scenario
    objects of that repository count: any other JSON file in or beside the scenario folders is left alone
    (on 22 November 2026 a review's ``findings.json``, a list, lay in ``scenarios/munarium-review``)."""
    evaluation = Path(evaluation)
    freeze = evaluation / "scenario-freeze.json"
    rows, frozen_repositories = [], set()
    if freeze.is_file():
        for relative, digest in sorted(common.read_json(freeze)["files"].items()):
            path = evaluation.joinpath(*relative.split("/"))
            common.require(common.sha256_file(path) == digest, f"frozen scenario changed: {relative}")
            scenario = common.read_json(path)
            frozen_repositories.add(scenario.get("repository"))
            if scenario.get("repository") == repo and scenario.get("leak_probe"):
                rows.append({"source": relative, "phrase": scenario["leak_probe"], "sha256": digest})
        if repo in frozen_repositories:
            return rows
    for folder in ("scenarios", "sealed"):
        for path in sorted((evaluation / folder / repo).glob("*.json")):
            scenario = common.read_json(path)
            if (isinstance(scenario, dict) and scenario.get("schema") == SCENARIO_SCHEMA
                    and scenario.get("repository") == repo and scenario.get("leak_probe")):
                rows.append({"source": path.relative_to(evaluation).as_posix(), "phrase": scenario["leak_probe"]})
    return rows


def index_name(handle: common.Handle, repo: str) -> str:
    """The index of the lifetime that serves a scenario repository."""
    return (handle.aliases or {}).get(repo, repo)


def search_phrase(handle: common.Handle, repo: str, phrase: str) -> dict:
    outcome, _ = common.call_tool_retrying(handle, "airk_search", repo, {"query": phrase, "limit": 16,
                                                                         "response_mode": "full"}, 300.0)
    body = common.envelope_body(outcome.envelope) if outcome.ok else None
    return {"ok": outcome.ok, "error": outcome.error, "elapsed_ms": outcome.elapsed_ms,
            "stdout_head": None if outcome.ok else outcome.stdout[:300].decode("utf-8", "replace"),
            "paths": served_paths(body) if body is not None else []}


def check_repository(repo: str, index: str, probes: list[dict], listing: dict, fx, search) -> dict:
    """One repository's result from its scenario probes and its index's listing (pure but for ``search``).

    ``probes`` are ``scenario_probes`` rows; ``search(phrase)`` returns ``{ok, error, paths, ...}``;
    ``listing`` is ``credential_probe.list_all``'s result. The exclusion list is the index's own."""
    common.require(index in fx.EXCLUSIONS, f"fixture.py has no exclusion list for index {index!r}")
    rows, failures = [], []
    for probe in probes:
        found = search(probe["phrase"])
        bad = [p for p in found["paths"] if fx.excluded(p, index)]
        rows.append({"scenario": probe["source"], "frozen": "sha256" in probe, "ok": found["ok"],
                     "error": found.get("error"), "stdout_head": found.get("stdout_head"),
                     "paths": len(found["paths"]), "excluded_hits": bad, "elapsed_ms": found.get("elapsed_ms")})
        failures += [{"repository": repo, "scenario": probe["source"], "path": p} for p in bad]
        if not found["ok"]:
            failures.append({"repository": repo, "scenario": probe["source"], "error": found.get("error")})
    listed_excluded = sorted(p for p in listing["paths"] if fx.excluded(p, index))
    matches = {k: sorted(set(v)) for k, v in credential_probe.classify(listing["paths"]).items() if v}
    passed = not failures and not listed_excluded and listing["complete"]
    return {"repository": repo, "index": index, "exclusions": [list(atom) for atom in fx.EXCLUSIONS[index]],
            "probes": rows, "probe_count": len(rows),
            "probe_source": "none: no scenario names this repository yet" if not rows else
                            "scenario freeze" if all(r["frozen"] for r in rows) else "scenario folders (not frozen)",
            "listed_files": len(listing["paths"]), "unique_listed_files": len(set(listing["paths"])),
            "listing_complete": listing["complete"], "listing_restarts": listing.get("restarts"),
            "listing_errors": listing.get("errors") or [], "listed_excluded_paths": listed_excluded,
            "credentials": {"patterns": list(credential_probe.PATTERNS), "matches": matches,
                            "match_count": sum(len(v) for v in matches.values())},
            "failures": failures, "passed": passed}


def catalog_indexes(handle: common.Handle, timeout_s: float = 120.0) -> list:
    """The service's own index catalog (``library list --kind indexes``); raises when it cannot be read."""
    command = [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control, "library", "list",
               "--kind", "indexes"]
    proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, env=common.tool_env(handle),
                          timeout=timeout_s, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    common.require(proc.returncode == 0, f"index catalog listing failed (exit {proc.returncode})")
    page = json.loads(proc.stdout)
    inner = page.get("page") if isinstance(page.get("page"), dict) else page
    common.require(inner.get("continuation") is None, "the index catalog did not fit one page")
    return inner.get("items") or []


def forbidden_index(handle_path: Path, catalog: list, fx) -> list[dict]:
    """Why the lifetime holds an airk index (handle, state files, catalog); empty when it holds none."""
    raw = common.read_json(handle_path)
    states = sorted(path.name.split(".")[0] for path in (handle_path.parent.parent / "indexes").glob("*.state.json")
                    if path.name != "minilm.state.json")
    return fx.forbidden_index_findings(raw.get("indexes") or {}, states, catalog, common.FORBIDDEN_LOOP_INDEX)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--evaluation", type=Path, default=common.DEFAULT_R,
                        help="the evaluation root holding scenario-freeze.json, scenarios/ and sealed/")
    parser.add_argument("--repos", default=",".join(common.REPOS), help="scenario repositories to check")
    parser.add_argument("--allow-airk-index", action="store_true",
                        help="history only: the one-turn campaign root holds an airk index")
    args = parser.parse_args(argv)
    fx = fixture_module()
    handle_path = common.resolve_handle(args.handle, common.DEFAULT_R / "fixture")
    handle = common.load_handle(handle_path)
    repos = [r for r in args.repos.split(",") if r]
    forbidden = forbidden_index(handle_path, catalog_indexes(handle), fx)
    repositories = {}
    for repo in repos:
        index = index_name(handle, repo)
        start = settled.snapshot(handle, repo)
        probes = scenario_probes(args.evaluation, repo)
        listing = credential_probe.list_all(handle, repo)
        row = check_repository(repo, index, probes, listing, fx, lambda phrase: search_phrase(handle, repo, phrase))
        end = settled.snapshot(handle, repo)
        row.update(index_id=handle.index_id(repo), token_start=start.get("publication_token"),
                   token_end=end.get("publication_token"), token_unchanged=settled.compare(start, end)["unchanged"])
        repositories[repo] = row
    result = {"schema": SCHEMA, "generated": common.now_iso(), "handle": str(handle_path),
              "readiness": handle.readiness(), "aliases": handle.aliases or {},
              "forbidden_index": common.FORBIDDEN_LOOP_INDEX, "forbidden_index_findings": forbidden,
              "forbidden_index_allowed": bool(args.allow_airk_index), "repositories": repositories,
              "failures": [f for row in repositories.values() for f in row["failures"]]}
    result["passed"] = (all(row["passed"] for row in repositories.values())
                        and (not forbidden or bool(args.allow_airk_index)))
    common.write_json(args.out, result)
    print(json.dumps({"passed": result["passed"], "forbidden_index_findings": forbidden,
                      "repositories": {repo: {k: row[k] for k in ("index", "probe_count", "probe_source",
                                                                  "listed_files", "listing_complete",
                                                                  "listed_excluded_paths", "exclusions", "passed")}
                                       for repo, row in repositories.items()}, "failures": result["failures"]}))
    print(json.dumps({repo: row["credentials"]["matches"] for repo, row in repositories.items()}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
