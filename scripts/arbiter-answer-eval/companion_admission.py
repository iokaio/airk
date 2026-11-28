# SPDX-License-Identifier: Apache-2.0
"""Require the admitted native companion lifetime and observe its real legacy-stage state."""
import hashlib
import json
from pathlib import Path
import subprocess

import common
import product_skill_http


def observe(handle, admission_path):
    admission = common.read_json(admission_path)
    common.require(admission["schema"] == "airk.eval.companion-admission/1" and admission["search"] == "none",
                   "Expected the model-free companion admission")
    parent_path = Path(admission["handle_path"]).resolve(strict=True)
    parent = common.read_json(parent_path)
    current = common.read_json(handle.path)
    endpoint = product_skill_http.http_fixture.read_endpoint(handle.control)
    common.require(common.sha256_file(parent_path).lower() == admission["handle_sha256"].lower() and
                   common.sha256_file(handle.path) == handle.sha256 and parent["state"] == current["state"] == "ready" and
                   parent["instance"] == current["instance"] == admission["service_instance"] == endpoint["service_instance"] and
                   parent["image"]["service"]["sha256"] == current["image"]["service"]["sha256"] ==
                   endpoint["executable_sha256"] == admission["service_sha256"] and
                   parent["service_pid"] == admission["service_pid"] and
                   Path(parent["control"]).resolve() == Path(handle.control).resolve() and parent["search"] == "none",
                   "Companion instance, binary, process, policy or control binding changed")
    command = [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control, "arbiter", "status-legacy"]
    status = json.loads(subprocess.run(command, check=True, timeout=30, capture_output=True).stdout)
    stages = {}
    for name in ("focus", "context"):
        stage = {"enabled": status["configuration"][name]["enabled"], **{
            k: status[name][k] for k in ("queued", "running")}}
        expected = {"enabled": False, "queued": 0, "running": 0}
        common.require(stage == expected and all(admission["stage_status"][name][k] == v for k, v in expected.items()),
                       f"Legacy native {name} stage is enabled or has pending work")
        stages[name] = stage
    observers = {}
    for name, filename in (("dispatch", "composed-observer.jsonl"), ("warm", "warm-completions.jsonl")):
        declared = admission["observers"][name]
        path = Path(declared["path"]).resolve(strict=True)
        common.require(path == (parent_path.parent / "fixture" / filename).resolve(strict=True),
                       "Unexpected dispatch or warm observer path")
        data = path.read_bytes()
        measured = {"bytes": len(data), "rows": len(data.splitlines()), "sha256": hashlib.sha256(data).hexdigest()}
        common.require(not data and all(str(measured[k]).lower() == str(declared[k]).lower() for k in measured),
                       f"The native model {name} observer contains activity")
        observers[name] = measured
    embedding = admission["embedding"]
    common.require(embedding.get("model") and embedding.get("route") and embedding.get("pipeline_digest") and
                   embedding.get("asset_sha256") and embedding.get("graph_sha256"), "Missing native embedding identity")
    return {"admission_sha256": common.sha256_file(admission_path), "handle_sha256": handle.sha256,
            "service_instance": admission["service_instance"], "service_sha256": admission["service_sha256"],
            "embedding": embedding, "legacy_stages": stages, "observers": observers}