# SPDX-License-Identifier: Apache-2.0
"""Owned P7 routing comparison through a real predecessor upgrade.

This independent campaign does not alter the frozen84-gate replay. Only this
new original fixture receives a native reviewed grant; it changes no user
installation, global configuration, ACL, source bytes or original import.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[2]


def module(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, default=ROOT / "artifacts/v190/p7-runtime-release-r3")
    parser.add_argument("--predecessor-runtime-dir", type=Path, default=ROOT / "artifacts/v190/p6f-resume-predecessor-shared-copy")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    bench = module("p7_routing_replay", "scripts/benchmark-mcp-investigation.py")
    clients = module("p7_routing_clients", "scripts/check-service-clients.py")
    old, new = args.predecessor_runtime_dir.resolve(strict=True), args.runtime_dir.resolve(strict=True)
    runtime = clients.runtime_root(new)
    predecessor = clients.runtime_root(old)
    if runtime != new or predecessor != old:
        raise RuntimeError("select exact staged runtime roots")
    suffix = ".exe" if os.name == "nt" else ""
    main_name = "airk" + suffix
    if not (new / main_name).is_file() or not (old / main_name).is_file():
        raise RuntimeError("explicit root main images required for this fixture")
    approvals = {path: sha(path / "airk-install.json") for path in (old, new)}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    base = Path(os.environ["AIRK_PRIVATE_CENSUS_BASE"]).resolve(strict=True)
    owner = Path(tempfile.mkdtemp(prefix="airk-p7-routing-", dir=base)).resolve()
    fixture, install, state, data = (owner / name for name in ("fixture", "install", "state", "data"))
    data.mkdir()
    manifest = bench.create_fixture(fixture)
    source, _ = bench.load_fixture(fixture)
    inventory = manifest["inventory"]
    target = next(row for row in inventory if row["path"] == "src/alpha.rs")
    # Oracle frozen from the original fixture before any candidate result.
    query = {"query": "Session", "mode": "lexical", "path_prefix": "src/alpha.rs",
             "limit": 1, "max_bytes": 2048, "verification": "off"}
    if target["sha256"] != sha(source / target["path"]) or "pub struct Session" not in (source / target["path"]).read_text(encoding="utf-8"):
        raise RuntimeError("original positive source oracle changed")
    write_json(output / "oracle-before-candidate.json", {"fixture_sha256": manifest["fixture_sha256"],
        "arguments": query, "expected": {"path": target["path"], "content_hash": target["sha256"]},
        "basis": "Predeclared literal positive source in the unchanged original E0 fixture; no candidate-derived relevance labels."})
    write_json(output / "fixture-manifest.json", manifest)
    write_json(output / "owner-location.json", {"root": str(owner)})
    write_json(output / "runtime-provenance.json", {"candidate": json.loads((new / "airk-build.json").read_bytes()),
        "predecessor": json.loads((old / "airk-build.json").read_bytes()),
        "candidate_manifest_sha256": approvals[new], "predecessor_manifest_sha256": approvals[old],
        "wrapper_sha256": sha(__file__), "runner_sha256": sha(ROOT / "scripts/benchmark-mcp-investigation.py")})
    actions = []

    def command(path, *words, value=None):
        number = len(actions)
        request = {"runtime": str(path), "arguments": list(map(str, words)), "stdin": value}
        # Journal the exact native request/key before every setup dispatch.
        write_json(output / f"command-{number:02d}.request.json", request)
        actions.append({"number": number, "submitted": False})
        write_json(output / "commands.json", actions)
        actions[-1]["submitted"] = True
        write_json(output / "commands.json", actions)
        result = subprocess.run([str(path / main_name), *map(str, words)],
            input=None if value is None else json.dumps(value), capture_output=True, text=True,
            encoding="utf-8", timeout=360, cwd=ROOT,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        (output / f"command-{number:02d}.stdout").write_text(result.stdout, encoding="utf-8")
        (output / f"command-{number:02d}.stderr").write_text(result.stderr, encoding="utf-8")
        actions[-1]["returncode"] = result.returncode
        write_json(output / "commands.json", actions)
        if result.returncode:
            raise RuntimeError(f"owned native command {number} failed; raw outcome retained")
        return json.loads(result.stdout)

    def rpc(method):
        response = command(new, "service", "rpc", "--runtime-dir", state / "control", "--stdin", value=method)
        if response.get("status") in ("failure", "incompatible", "catalog_outcome_unknown"):
            # No mutation or uncertain read is replayed under a replacement key.
            raise RuntimeError("owned native metadata refused or outcome unknown; exact raw receipt retained")
        return response

    service = clients.Fixture(old, owner)
    service.control = state / "control"
    started = False
    activated = False
    passed = False
    try:
        command(old, "runtime", "stage", "--source", old, "--install-dir", install, "--state-dir", state,
                "--approved-sha256", approvals[old], "--verification-basis", "organization-digest")
        command(old, "runtime", "activate", "--install-dir", install, "--runtime-sha256", approvals[old])
        started = True
        service.register("investigation", source, data)
        for configuration in ({"operation": "vectors", "enabled": False},
                              {"operation": "detail", "level": "agent_lean"},
                              {"operation": "usage", "mode": "disabled"}):
            service.configure("investigation", configuration)
        service.job("investigation", "index")
        service.rpc({"pause": {"project": service.projects["investigation"]["binding"], "paused": True}})
        command(new, "runtime", "stage", "--source", new, "--install-dir", install, "--state-dir", state,
                "--approved-sha256", approvals[new], "--verification-basis", "organization-digest")
        command(new, "runtime", "activate", "--install-dir", install, "--runtime-sha256", approvals[new])
        activated = True
        service.binary, service.runtime = new / main_name, new
        upgrade = json.loads((service.control / "index-catalog-upgrade.json").read_bytes())
        if upgrade["stage"] != "settled" or upgrade["authority_v1"]["phase"] != "settled":
            raise RuntimeError("actual native predecessor upgrade did not settle")
        write_json(output / "upgrade-receipt.json", upgrade)
        registration = service.projects["investigation"]
        imported = rpc({"catalog_legacy_binding": {"project": registration["binding"], "expected_catalog_revision": None}})["observation"]
        write_json(output / "imported-before.json", imported)
        choice = imported["eligibility"]
        if choice["kind"] != "imported_snapshot" or choice["registration"] != registration:
            raise RuntimeError("real imported compatibility snapshot required")
        persisted = choice["grant"]
        if persisted["provenance"]["kind"] != "imported_legacy_snapshot" or len(persisted["grant"]["memberships"]) != 1:
            raise RuntimeError("original single-index import identity required")
        member = persisted["grant"]["memberships"][0]["membership"]
        index_id, project_id = member["index_id"], member["project_id"]
        if len(member["scope"]) != 1 or member["scope"][0]["path"] != "" or member["scope"][0]["kind"] != "subtree":
            raise RuntimeError("this original primary-root fixture requires one explicit full-root membership")
        source_id = member["scope"][0]["source_root_id"]
        source_detail = rpc({"catalog_detail": {"entity": {"kind": "source", "id": source_id},
                                                 "expected_catalog_revision": imported["catalog_revision"]}})["detail"]
        native_source = source_detail["item"]["value"]
        if native_source["index_id"] != index_id or native_source["canonical_path"] != registration["root"] or not native_source["owner"]:
            raise RuntimeError("native source mapping changed")
        context_key = hashlib.sha256(os.urandom(32)).hexdigest()
        descriptor = rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": imported["catalog_revision"]}})["descriptor"]
        epoch = rpc("request_epoch")
        if epoch["namespace"] != descriptor["namespace"] or epoch["namespace"] != imported["namespace"]:
            raise RuntimeError("native authority changed before fixture grant review")
        selection = {"key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                     "expected_catalog_revision": descriptor["catalog_revision"], "project_ids": [project_id], "index_ids": [index_id]}
        draft = rpc({"catalog_grant_draft": {"context_key": context_key, "selection": selection}})["draft"]
        write_json(output / "fixture-reviewed-grant-draft.json", draft)
        grant = draft["request"]["operation"]["grant"]
        if (draft["request"]["operation"]["operation"] != "create_grant"
                or grant["memberships"] != persisted["grant"]["memberships"]
                or grant["host"] != descriptor["host"] or grant["id"] == persisted["grant"]["id"]):
            raise RuntimeError("native reviewed fixture snapshot did not preserve the selected original membership")
        apply = {"catalog_apply": {"request": draft["request"], "preview_digest": draft["preview_digest"]}}
        write_json(output / "fixture-reviewed-grant-apply.json", apply)
        applied = rpc(apply)
        write_json(output / "fixture-reviewed-grant-receipt.json", applied)
        if applied["status"] != "catalog_applied" or applied["receipt"]["request_digest"] != draft["preview"]["request_digest"]:
            raise RuntimeError("native fixture grant apply did not confirm its exact request")
        session = {"schema": 1, "context_key": context_key, "namespace": descriptor["namespace"],
                   "expected_catalog_revision": applied["receipt"]["catalog_revision"], "grant_id": grant["id"],
                   "expected_grant_revision": grant["revision"], "project_ids": [project_id], "index_ids": [index_id]}
        configuration = {"command": str(new / main_name),
            "args": ["--service-runtime-dir", str(service.control), "--combined-session", json.dumps(session), "mcp"],
            "launcher_sha256": sha(new / main_name), "expected_version": json.loads((new / "airk-runtime.json").read_bytes())["version"],
            "route": "isolated-approved-runtime", "cwd": str(new)}
        scenario = {"schema": bench.COMBINED_SCHEMA, "indexes": [{"index_id": index_id, "source_root": registration["root"], "inventory": inventory}],
            "cases": [{"project_ids": [project_id], "index_ids": [index_id], "arguments": query,
                       "expected": [{"index_id": index_id, "path": target["path"], "source_root_id": source_id, "project_ids": [project_id]}]}]}
        write_json(output / "configuration.json", configuration)
        write_json(output / "scenario.json", scenario)
        from types import SimpleNamespace
        previous_opt_in = os.environ.get(bench.LEGACY_COMPARE_ENV)
        os.environ[bench.LEGACY_COMPARE_ENV] = "1"
        try:
            passed = bench.combined(SimpleNamespace(configuration=output / "configuration.json", scenario=output / "scenario.json",
                output=output / "measurement", samples=30, fanouts=[1], timeout=30))
        finally:
            if previous_opt_in is None:
                os.environ.pop(bench.LEGACY_COMPARE_ENV, None)
            else:
                os.environ[bench.LEGACY_COMPARE_ENV] = previous_opt_in
        after = rpc({"catalog_legacy_binding": {"project": registration["binding"], "expected_catalog_revision": None}})["observation"]
        write_json(output / "imported-after.json", after)
        if after["eligibility"] != choice or bench.source_inventory(source) != inventory:
            raise RuntimeError("original compatibility snapshot or source bytes changed")
        if not passed:
            raise RuntimeError("paired comparison retained a failure; inspect measurement/report.json")
    finally:
        try:
            if started:
                active = new if activated else old
                service.binary, service.runtime = active / main_name, active
                health, epoch = service.rpc("health")["observation"], service.rpc("request_epoch")
                stop_request = {"caller": uuid.uuid4().hex, "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                    "action": "stop", "expected_instance": health["instance"], "runtime": approvals[active], "force": False}
                stop = command(active, "runtime", "lifecycle", "--install-dir", install, value=stop_request)
                write_json(output / "lifecycle.json", stop)
                if stop["status"] != "lifecycle" or stop["record"]["phase"] != "complete":
                    raise RuntimeError("owned native Stop did not complete")
        finally:
            service.log.close()
    print(json.dumps({"passed": passed, "output": str(output), "samples": 30}))


if __name__ == "__main__":
    main()
