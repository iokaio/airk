# SPDX-License-Identifier: Apache-2.0
"""Qualify one genuinely registered writable benchmark checkout against its frozen eligible bytes."""
import argparse
from contextlib import closing
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import subprocess

import common
import fixture
import product_skill_http
import workspace_scope


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("handle", "setup", "work", "frozen", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--repo", required=True, choices=("bookbuilder-v2", "munarium"))
    args = parser.parse_args()
    setup = json.loads(args.setup.read_bytes().splitlines()[-1])["receipt"]
    binding = setup["request"]["registration"]["binding"]
    index = setup["index_id"]
    work = args.work.resolve(strict=True)
    common.require(work.samefile(setup["request"]["registration"]["root"]), "registration has a different working root")
    saved = common.read_json(args.handle)
    frozen = common.read_json(args.frozen)[args.repo]
    origin = saved["indexes"][args.repo]
    common.require(frozen["index"] == origin["index_id"] and frozen["publication"] == origin["publication_token"],
                   "frozen eligible manifest belongs to a different corpus publication")
    common.require(origin["index_id"] != index and not work.samefile(origin["root"]), "editing needs a separate native index")
    canonical = common.load_handle(args.handle)
    bound = replace(canonical, indexes={args.repo: index}, bindings={args.repo: binding})
    base = [*bound.cli, "--output", "json", "--service-runtime-dir", bound.control, "--service-project", binding]

    def cli(*arguments, request=None):
        options = {"stdin": subprocess.DEVNULL} if request is None else {"input": json.dumps(request).encode()}
        result = subprocess.run([*base, *arguments], check=True, **options,
                                capture_output=True, env=common.tool_env(bound), timeout=120)
        return json.loads(result.stdout)

    watch = cli("watch", "status")["observation"]
    common.require(watch["enabled"] and watch["configuration_observed"] and watch["native_roots"] == 1
                   and not watch.get("failure"), "watch has not confirmed the editable root")
    status = cli("settings", "vector_status")
    sem = status["semantic_readiness"]
    disco = status["discovery_readiness"]
    common.require(sem["state"] == "ready" and disco["state"] == "ready"
                   and sem["pipeline_digest"] == origin["pipeline_digest"]
                   and disco["committed_artifacts"] == frozen["file_count"],
                   "native semantic or discovery state differs from the frozen benchmark")
    policy = fixture.unwrap(cli("service", "rpc", "--stdin", request={"catalog_policy_status": {"index_id": index}}),
                            "catalog_policy_status")
    common.require(policy["index_id"] == index, "source policy returned another native index")
    observed, _ = fixture.denied_atoms(policy)
    denies = sorted([list(x) for x in observed])
    common.require(denies == sorted(frozen["denies"]), "current native source policy differs from the frozen scope")
    metrics = {}
    with closing(product_skill_http.ScopedClient(bound, args.repo)) as client:
        client.exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "airk-editable-qualification", "version": "1"}}})
        client.exchange({"jsonrpc": "2.0", "method": "notifications/initialized"})
        paths = workspace_scope.published_paths(client, index, metrics)
    workspace_scope.check_denies(paths, denies)
    digest = hashlib.sha256()
    for rel in sorted(paths):
        path = (work / rel).resolve(strict=True)
        common.require(path.is_relative_to(work), "native file outside working checkout")
        digest.update(rel.encode() + b"\0" + hashlib.sha256(path.read_bytes()).digest())
    common.require(len(paths) == frozen["file_count"] and digest.hexdigest() == frozen["digest"],
                   "editable native eligible bytes differ from the frozen workspace")
    out = common.call_tool(bound, "airk_search", args.repo, {"query": "source", "limit": 1, "max_bytes": 4096,
                           "response_mode": "full", "include_status": True}, 120)
    common.require(out.ok, "editable native search failed")
    search = common.envelope_body(out.envelope)
    common.require(search["publication"]["token"] == metrics["publication"]
                   and search["execution"]["served_mode"] in ("hybrid", "vector"),
                   "editable native search differs from inventory publication or is not hybrid/vector")
    names = ("cli", "control", "control_owner", "deadline_utc", "environment", "image", "instance",
             "lifetime", "ready", "schema", "search", "service_pid", "started_utc", "state", "work")
    derived = {key: saved[key] for key in names}
    derived.update(queryable=True, semantic={args.repo: "ready"}, indexes={args.repo: {
        "repository": args.repo, "root": str(work), "binding": binding, "index_id": index,
        "data_dir": setup["request"]["registration"]["data_root"], "source_commit": origin["source_commit"],
        "denies": denies, "files": len(paths), "publication_token": metrics["publication"],
        "served_mode": search["execution"]["served_mode"], "pipeline_digest": sem["pipeline_digest"]}},
        qualification={"parent_handle": str(args.handle.resolve()), "parent_sha256": canonical.sha256,
                       "watch": watch, "semantic_readiness": sem, "discovery_readiness": disco,
                       "eligible_digest": digest.hexdigest(), "inventory": metrics})
    with args.output.open("x", encoding="utf-8") as fp:
        json.dump(derived, fp, indent=2)
    print(index, len(paths), digest.hexdigest(), search["execution"]["served_mode"])


if __name__ == "__main__":
    main()
