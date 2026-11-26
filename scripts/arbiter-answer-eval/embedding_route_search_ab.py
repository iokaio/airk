# SPDX-License-Identifier: Apache-2.0
"""Measure CPU and CUDA query rank agreement on one pinned settled publication."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search
import settled


def rpc(handle, request, status, timeout):
    command = [*handle.cli, "service", "rpc", "--runtime-dir", handle.control, "--stdin"]
    call = subprocess.run(command, input=json.dumps(request).encode(), capture_output=True,
                          env=common.tool_env(handle), timeout=timeout,
                          creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    common.require(call.returncode == 0, f"native {status} RPC failed")
    answer = json.loads(call.stdout)
    common.require(answer["status"] == status, f"native {status} RPC refused")
    return answer


def read_route(handle, timeout):
    observed = rpc(handle, "embedding_status", "embedding_status", timeout)
    common.require(observed["available"] and observed["last_embedding"], "no observed native embedding route")
    return observed["last_embedding"]


def configure(handle, configuration, timeout):
    args = {"configure_resources": {"configuration": configuration}}
    value = rpc(handle, args, "resources", timeout)["observation"]["configuration"]
    common.require(value == configuration, "resource policy differed after acknowledgement")


def collect(args):
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    search.roster(cohort["rows"])
    repo, slot = args.case.split("/", 1)
    matches = [row for row in cohort["rows"] if (row["repo"], row["slot"]) == (repo, slot)]
    common.require(len(matches) == 1, "expected one frozen development instruction")
    request = matches[0]
    reference = json.loads(search.pinned(args.reference, args.reference_sha256))
    raw = json.loads(search.pinned(args.handle, args.handle_sha256))
    handle = common.load_handle(args.handle)
    common.require(reference["state"] == "closed" and handle.sha256 == args.handle_sha256
                   and handle.state == "ready" and raw["control"] == reference["control"],
                   "matched case requires a reopened settled control")
    identity = search.live_identity(handle, raw, [repo])
    index = identity["indexes"][repo]
    original = reference["indexes"][index["physical"]]
    common.require(all(index[key] == original[key] for key in
                       ("index_id", "binding", "source_commit", "pipeline_digest", "publication_token")),
                   "the pinned publication or source identity changed")
    timeout = args.timeout
    before = settled.snapshot(handle, repo, timeout)
    stage = search.stage(handle, None, timeout)
    common.require(before["settled"] and before["publication_token"] == index["publication_token"],
                   "expected a settled publication with both optional stages off")
    saved = rpc(handle, "resources", "resources", timeout)["observation"]["configuration"]
    common.require(saved["allow_embedding_gpu"] is True and read_route(handle, timeout)["route"] == "cuda",
                   "the preflight Search did not prove CUDA embedding on this control")
    exact = {"query": request["arguments"]["query"], "mode": "vector", "limit": 40,
             "max_bytes": 65536, "response_mode": "full"}
    arms = {}
    try:
        for name, enabled in (("cpu", False), ("cuda", True)):
            configure(handle, {**saved, "allow_embedding_gpu": enabled}, timeout)
            old = read_route(handle, timeout)
            common.require(old["route"] != name, "expected the opposite route before the sole measured request")
            call = common.call_tool(handle, "airk_search", repo, exact, timeout, strip_env=common.AGENT_STRIPPED_ENV)
            common.require(call.ok, f"{name}: vector Search failed")
            body = common.envelope_body(call.envelope)
            actual = read_route(handle, timeout)
            common.require(actual["route"] == name and actual.get("fallback") == ("disabled" if not enabled else None),
                           f"{name}: the measured query did not take its requested route")
            common.require(body["execution"]["served_mode"] == "vector"
                           and body["publication"]["token"] == index["publication_token"]
                           and not body.get("focus"), f"{name}: Search lost its requested publication or mode")
            common.require(10 <= len(body["hits"]) <= 40, f"{name}: too few admitted hits for the comparison")
            arms[name] = {"route_before": old, "route_after": actual, "elapsed_ms": call.elapsed_ms,
                          "response_incomplete": body["execution"]["incomplete"],
                          "truncated": body["truncated"], "result_sha256": common.sha256_bytes(call.stdout),
                          "hits": [{"citation": item["citation"], "sha256": search.hit_identity(item)}
                                   for item in body["hits"]]}
    finally:
        configure(handle, saved, timeout)
    after = settled.snapshot(handle, repo, timeout)
    common.require(after["settled"] and settled.compare(before, after)["unchanged"]
                   and search.stage(handle, None, timeout)["revision"] == stage["revision"]
                   and common.sha256_file(handle.path) == args.handle_sha256,
                   "the settled source or native stage changed")
    cpu, cuda = ([item["sha256"] for item in arms[name]["hits"]] for name in ("cpu", "cuda"))
    return {"schema": "airk.private.embedding-query-routes/1", "case": args.case, "identity": identity,
            "input_sha256": {"cohort": args.cohort_sha256, "reference": args.reference_sha256,
                             "handle": args.handle_sha256, "request": search.digest(exact)},
            "arms": arms, "same_order": cpu == cuda,
            "shared_top10": len(set(cpu[:10]) & set(cuda[:10])),
            "shared_returned": len(set(cpu) & set(cuda)),
            "scope": "Canonical response rank comparison. No independent vector coordinates or per-row writer route."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cohort", "reference", "handle"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--case", required=True)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    search.write_new(args.output, collect(args))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
