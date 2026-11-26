# SPDX-License-Identifier: Apache-2.0
"""Compare only the existing native path prior against the original Search policy."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import depth
from embedding_route_search_ab import configure, read_route, rpc
import evidence
import search_focus_ab as search
import settled


SCHEMA = "airk.private.path-prior-pilot/1"
CONTROL = "none"
TRIAL = "baseline_path_prior"
REPETITIONS = 3


def ranked(handle, request, arguments, index, timeout, limit):
    call = common.call_tool(handle, "airk_search", request["repo"], arguments, timeout,
                            strip_env=common.AGENT_STRIPPED_ENV)
    common.require(call.ok, "native pilot Search failed")
    body = common.envelope_body(call.envelope)
    hits = body["hits"]
    execution = body["execution"]
    common.require(body["publication"]["token"] == index["publication_token"]
                   and execution["served_mode"] == "hybrid" and not body.get("focus")
                   and len(hits) <= limit and (limit != 40 or len(hits) == 40 or not execution["incomplete"]),
                   "pilot Search did not admit comparable ranked evidence")
    spans = [evidence.extract(hit) for hit in hits]
    return {"hit_sha256": [search.hit_identity(hit) for hit in hits],
            "notes": [note["id"] for note in request["essential"]
                      if depth.ranks(spans, note)["rule2"] is not None],
            "result_sha256": common.sha256_bytes(call.stdout), "elapsed_ms": call.elapsed_ms,
            "incomplete": execution["incomplete"], "truncated": body["truncated"]}


def collect(args):
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    freeze = json.loads(search.pinned(args.freeze, args.freeze_sha256))
    common.require(freeze["schema"] == "airk.private.index-heldout-freeze/1"
                   and freeze["pilot_opened"] is False and search.roster(cohort["rows"]), "bad held-out freeze")
    reference = json.loads(search.pinned(args.reference, args.reference_sha256))
    raw = json.loads(search.pinned(args.handle, args.handle_sha256))
    handle = common.load_handle(args.handle)
    common.require(reference["state"] == "closed" and reference["final_stage_idle"]
                   and raw["state"] == handle.state == "ready" and handle.sha256 == args.handle_sha256
                   and raw["control"] == reference["control"], "pilot must reuse the pinned settled reference")
    repos = sorted({row["repo"] for row in cohort["rows"]})
    identity = search.live_identity(handle, raw, repos)
    common.require(identity["search_profile"] == args.profile, "native pilot policy differs")
    for repo, item in identity["indexes"].items():
        expected = reference["indexes"][item["physical"]]
        common.require(all(item[key] == expected[key] for key in
                           ("index_id", "binding", "source_commit", "pipeline_digest", "publication_token")),
                       f"{repo}: trial changed the scoped publication")
    common.require({item["name"]: item["source_commit"] for item in freeze["inputs"]}
                   == {repo: item["source_commit"] for repo, item in identity["indexes"].items()},
                   "held-out source revisions differ")
    timeout = args.timeout
    stage = search.stage(handle, None, timeout)
    saved = rpc(handle, "resources", "resources", timeout)["observation"]["configuration"]
    rows = []
    try:
        configure(handle, {**saved, "allow_embedding_gpu": False}, timeout)
        before = {repo: settled.snapshot(handle, repo, timeout) for repo in repos}
        common.require(all(item["settled"] for item in before.values())
                       and read_route(handle, timeout) == {"route": "cpu", "fallback": "disabled"},
                       "pilot needs the settled explicit CPU query path")
        for request in cohort["rows"]:
            repo = request["repo"]
            index = identity["indexes"][repo]
            arguments = request["arguments"]
            common.require(search.digest([repo, arguments, list(common.AGENT_STRIPPED_ENV)])
                           == request["request_sha256"], "pilot request changed")
            ordinary = [ranked(handle, request, arguments, index, timeout, request["limit"])
                        for _ in range(REPETITIONS)]
            first = ordinary[0]
            common.require(all(row["hit_sha256"] == first["hit_sha256"] for row in ordinary),
                           f"{request['key']}: ordinary ranked hits varied across repetitions")
            wide = ranked(handle, request, {**arguments, "limit": 40, "max_bytes": 65536}, index, timeout, 40)
            rows.append({"repo": repo, "key": request["key"], "request_sha256": request["request_sha256"],
                         "top10": first["notes"], "top40": wide["notes"], "ordinary": ordinary, "wide": wide})
        after = {repo: settled.snapshot(handle, repo, timeout) for repo in repos}
        common.require(read_route(handle, timeout) == {"route": "cpu", "fallback": "disabled"}
                       and all(after[repo]["settled"] and settled.compare(before[repo], after[repo])["unchanged"]
                               for repo in repos)
                       and search.stage(handle, None, timeout)["revision"] == stage["revision"]
                       and common.sha256_file(handle.path) == args.handle_sha256, "pilot custody changed")
    finally:
        configure(handle, saved, timeout)
    return {"schema": SCHEMA, "identity": identity, "rows": rows, "route": "cpu",
            "reference_sha256": args.reference_sha256, "cohort_sha256": args.cohort_sha256,
            "freeze_sha256": args.freeze_sha256}


def compare(control, trial):
    common.require(control["schema"] == trial["schema"] == SCHEMA
                   and control["route"] == trial["route"] == "cpu"
                   and control["identity"]["search_profile"] == CONTROL
                   and trial["identity"]["search_profile"] == TRIAL
                   and all(control[key] == trial[key] for key in
                           ("reference_sha256", "cohort_sha256", "freeze_sha256"))
                   and all(control["identity"][key] == trial["identity"][key] for key in
                           ("git_head", "service_sha256", "cli_sha256", "indexes")),
                   "pilot arms differ on more than the Search policy")
    pairs = list(zip(control["rows"], trial["rows"], strict=True))
    common.require(len(pairs) == 20 and all(
        all(a[key] == b[key] for key in ("repo", "key", "request_sha256")) for a, b in pairs),
        "pilot cases or requests differ")
    repositories = {}
    for repo in sorted({a["repo"] for a, _ in pairs}):
        group = [(a, b) for a, b in pairs if a["repo"] == repo]
        affected = {}
        for field in ("top10", "top40"):
            affected[f"{field}_gained"] = []
            affected[f"{field}_lost"] = []
            for original, candidate in group:
                first, second = set(original[field]), set(candidate[field])
                key = original["key"]
                affected[f"{field}_gained"].extend({"key": key, "id": note} for note in sorted(second - first))
                affected[f"{field}_lost"].extend({"key": key, "id": note} for note in sorted(first - second))
        timings = []
        for arm in (0, 1):
            times = sorted(item["elapsed_ms"] for pair in group for item in pair[arm]["ordinary"])
            common.require(len(times) == REPETITIONS * len(group)
                           and all(math.isfinite(t) and t > 0 for t in times), "invalid pilot latency")
            timings.append(times[math.ceil(0.95 * len(times)) - 1])
        repositories[repo] = {"changes": affected, "query_p95_ms": timings,
                              "query_p95_ratio": timings[1] / timings[0],
                              "counts": {f"{field}_{arm}": sum(len(pair[i][field]) for pair in group)
                                         for field in ("top10", "top40")
                                         for i, arm in enumerate(("original", "prior"))}}
    net = sum(len(item["changes"]["top40_gained"]) - len(item["changes"]["top40_lost"])
              for item in repositories.values())
    return {"schema": SCHEMA, "repositories": repositories, "net_top40": net,
            "qualifies": net >= 6 and all(not item["changes"]["top10_lost"]
                                         and item["query_p95_ratio"] <= 1.10 for item in repositories.values()),
            "scope": "Query-only pilot on identical settled publications and writer. Query p95 includes native CLI transport."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    collect_cmd = modes.add_parser("collect")
    compare_cmd = modes.add_parser("compare")
    names = {"collect": ("cohort", "freeze", "reference", "handle"), "compare": ("original", "prior")}
    for command, cmd in (("collect", collect_cmd), ("compare", compare_cmd)):
        for name in names[command]:
            cmd.add_argument(f"--{name}", type=Path, required=True)
            cmd.add_argument(f"--{name}-sha256", required=True)
        cmd.add_argument("--output", type=Path, required=True)
    collect_cmd.add_argument("--profile", choices=(CONTROL, TRIAL), required=True)
    collect_cmd.add_argument("--timeout", type=float, default=85)
    args = parser.parse_args()
    if args.mode == "collect":
        report = collect(args)
    else:
        report = compare(*(json.loads(search.pinned(getattr(args, name), getattr(args, f"{name}_sha256")))
                           for name in names["compare"]))
        report["input_sha256"] = {"original": args.original_sha256, "prior": args.prior_sha256}
    search.write_new(args.output, report)


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
