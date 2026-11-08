# SPDX-License-Identifier: Apache-2.0
"""Actual saved-only adoption of authenticated, privately owned historical fixtures.

Never executes a predecessor. Original sources, helper and saved data stay intact;
only independent copies enter native migration. This does not qualify an agent UI.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import uuid


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inventory(root):
    root = Path(root)
    for ancestor in (root, *root.parents):
        info = ancestor.lstat()
        if ancestor.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
            raise AssertionError("Redirected historical fixture root")
    result = {}
    for path in sorted(root.rglob("*")):
        info = path.lstat()
        if path.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
            raise AssertionError("Redirected historical fixture entry")
        if path.is_file():
            result[path.relative_to(root).as_posix()] = {"bytes": info.st_size, "sha256": sha(path)}
        elif not path.is_dir():
            raise AssertionError("Non-file historical fixture entry")
    return result


def expected_inventory(rows):
    return {row["path"].replace("\\", "/"): {key: row[key] for key in ("bytes", "sha256")} for row in rows}


def replace_config(path, expected, content):
    """Guard our single-writer fixture file; production editors own their CAS."""
    path = Path(path)
    before = path.stat()
    assert sha(path) == expected, "Original fixture configuration changed"
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(content.encode("utf-8"))
        stream.flush()
        os.fsync(stream.fileno())
    try:
        after = path.stat()
        assert (before.st_dev, before.st_ino) == (after.st_dev, after.st_ino) and sha(path) == expected
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def comparable(value):
    value = copy.deepcopy(value)
    # These are observations of each transport request, not saved evidence.
    for field in ("request_id", "observed_unix_ms", "cache_age_ms", "cached"):
        value.pop(field, None)
    if isinstance(value.get("freshness"), dict):
        value["freshness"].pop("observed_unix_ms", None)
    if "source_verification" in value:
        receipt = value["source_verification"]
        assert isinstance(receipt, dict) and isinstance(receipt.get("sources"), list), "Invalid source verification receipt"
        # Each transport performs its own dated source observation. Keep every
        # source identity, hash, outcome, budget and unknown field in comparison.
        for observation in [receipt, *receipt["sources"]]:
            assert isinstance(observation, dict), "Invalid source verification observation"
            start = observation.get("observed_from_unix_ms")
            end = observation.get("observed_to_unix_ms")
            assert type(start) is int and type(end) is int and 0 < start <= end, "Invalid source verification interval"
            del observation["observed_from_unix_ms"]
            del observation["observed_to_unix_ms"]
    return value


def write_private_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    if os.name != "nt":
        path.chmod(0o600)


def write_native_input(control, purpose, value):
    """Stage one-shot metadata under the already verified native private root.

    A Python fixture directory can have a valid user-owned ACL that is broader
    than native runtime state. Native read_private still enforces its exact ACL;
    this helper neither changes that policy nor edits original configuration.
    Inputs stay in the preserved fixture on failure for exact replay/diagnosis.
    """
    if purpose not in ("locators", "registration"):
        raise ValueError("Unsupported standalone native input purpose")
    control = Path(control)
    if not control.is_absolute():
        raise ValueError("Native input control root must be absolute")
    private = control / "http-1"
    for component in (private, *private.parents):
        metadata = component.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Native input parent contains a redirected directory")
    metadata = private.stat()
    if os.name != "nt" and (metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077):
        raise ValueError("Native input parent is not private to its owner")
    payload = (json.dumps(value, indent=2) + "\n").encode("utf-8")
    if len(payload) > 1024 * 1024:
        raise ValueError("Standalone native input exceeds its bound")
    destination = private / ("fixture-standalone-" + purpose + "-" + uuid.uuid4().hex + ".json")
    descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(payload)
        stream.flush()
        os.fsync(stream.fileno())
    return destination


def verify_batch(batch, configs):
    assert batch["status"] == "ready" and len(batch["groups"]) == 1 and len(batch["claims"]) == 2
    group = batch["groups"][0]
    assert group["status"] == "ready" and group["batch_operation_id"] == batch["operation_id"]
    assert len(group["claims"]) == 2
    assert sorted(group["claims"], key=lambda row: row["origin_sha256"]) == sorted(batch["claims"], key=lambda row: row["origin_sha256"])
    claims = group["grant"]["provenance"]["adoption"]["configuration_claims"]
    assert group["grant"]["provenance"]["kind"] == "imported_standalone_adoption" and len(claims) == 2
    assert sorted(claims, key=lambda row: row["origin_sha256"]) == sorted([row["claim"] for row in batch["claims"]], key=lambda row: row["origin_sha256"])
    assert len({claim["origin_sha256"] for claim in claims}) == 2
    memberships = group["grant"]["grant"]["memberships"]
    assert len(memberships) == 1 and memberships[0]["membership"]["index_id"] == group["adoption_receipt"]["setup"]["index_id"]
    for item in configs:
        mapping = next(row for row in batch["claims"] if row["locator"] == item["locator"])
        assert mapping["config_sha256"] == item["sha256"] == mapping["claim"]["config_sha256"]
        assert mapping["group_operation_id"] == group["operation_id"]
        assert mapping["registration"] == group["registration"] and mapping["original_registration"] == group["original_registration"]
    return group


def qualify(runtime, historical, owner, model):
    clients = module("standalone_clients", "check-service-clients.py")
    transport = module("standalone_https", "http_fixture.py")
    report = {"tag": historical["tag"], "original_publication": historical["status"]["publication"],
              "original_helper_sha256": historical["helper"]["sha256"], "old_runtime_executed": False,
              "parity_observation_exclusions": ["request_id", "observed_unix_ms", "cache_age_ms", "cached", "freshness.observed_unix_ms",
                  "source_verification.observed_from_unix_ms", "source_verification.observed_to_unix_ms",
                  "source_verification.sources[].observed_from_unix_ms", "source_verification.sources[].observed_to_unix_ms"]}
    def stage(name, **values):
        report.update(values)
        report["stage"] = name
        write_private_json(owner / "acceptance-progress.json", report)
    stage("verify_original_fixture")
    original_data, original_source = Path(historical["data_root"]), Path(historical["source"])
    source_before, data_before = inventory(original_source), inventory(original_data)
    assert source_before == expected_inventory(historical["source_after"])
    assert data_before == expected_inventory(historical.get("saved_data_after_content_reads", historical["saved_data_inventory"]))
    archive = historical["saved_index_archive"]
    assert sha(archive["path"]) == archive["sha256"]
    helper = Path(historical["helper"]["path"])
    assert sha(helper) == historical["helper"]["sha256"]
    copied_data = owner / "old-private-data"
    shutil.copytree(original_data, copied_data, copy_function=shutil.copy2)
    assert inventory(copied_data) == data_before
    global_storage = owner / "User" / "globalStorage"
    copied_helper = global_storage / "iokaio.airk" / "helpers" / helper.parent.name / helper.name
    copied_helper.parent.mkdir(parents=True)
    shutil.copy2(helper, copied_helper)
    assert sha(copied_helper) == historical["helper"]["sha256"]
    config = owner / "User" / "config.toml"
    server = "airk-" + historical["collection"]
    argv = ["--root", str(original_source), "--data-dir", str(copied_data), "--collection", historical["collection"], "mcp"]
    config.write_text(f"[mcp_servers.{server}]\ncommand = {json.dumps(str(copied_helper))}\nargs = {json.dumps(argv,separators=(',', ':'))}\n", encoding="utf-8")
    original_config_sha = sha(config)
    locator = {"path": str(config), "format": "codex_toml", "server": server}
    claude_config = owner / "User" / "claude-mcp.json"
    write_private_json(claude_config, {"mcpServers":{server:{"command":str(copied_helper),"args":argv}}})
    configs = [
        {"client":"codex","native_client":"codex","path":config,"locator":locator,"sha256":original_config_sha},
        {"client":"claude","native_client":"claude-code","path":claude_config,
         "locator":{"path":str(claude_config),"format":"mcp_servers_json","server":server},"sha256":sha(claude_config)},
    ]
    stage("start_modern_runtime", original_config_sha256=[item["sha256"] for item in configs],
          original_source_inventory=source_before, original_data_inventory=data_before)
    with clients.Fixture(runtime, owner, command_timeout=240) as fixture:
        active = fixture.managed_health["runtime_sha256"]
        locators_file = write_native_input(fixture.control, "locators", [item["locator"] for item in configs])
        stage("prepare_legacy_caches", native_locators_file=str(locators_file))
        def native(command, *arguments, check=True):
            result = fixture.run("runtime", command, "--install-dir", fixture.install, *arguments, check=check)
            return json.loads(result.stdout) if check else result
        inspection = native("prepare-legacy-caches", "--expected-active", active, "--global-storage", global_storage,
                            "--origin-locators-file", locators_file)
        stage("adopt_saved_index_batch", inspection=inspection, modern_runtime_sha256=active)
        assert len(inspection["caches"]) == 1 and Path(inspection["caches"][0]["executable"]).samefile(copied_helper)
        assert len(inspection["origin_proofs"]) == 2
        operation = uuid.uuid4().hex
        adopt_args = ("--expected-active", active, "--operation-id", operation, "--locators-file", locators_file,
                      "--expected-cache-inspection-sha256", inspection["inspection_sha256"])
        batch = native("adopt-legacy-indexes", *adopt_args)
        stage("verify_adoption_batch", batch=batch)
        adoption = verify_batch(batch, configs)
        assert adoption["source_availability"] == "not_observed_saved_only"
        assert adoption["conversion"]["original"]["original_root_sha256"] == historical["status"]["publication"]["revision"]
        assert adoption["conversion"]["current"]["incarnation"] != historical["status"]["publication"]["incarnation"]
        saved_collection = copied_data / historical["layout"] / historical["collection"]
        archive_inventory = inventory(Path(adoption["archive"]))
        assert archive_inventory == inventory(saved_collection), "Native immutable archive differs from original saved collection"
        assert inventory(copied_data) == data_before and sha(copied_helper) == sha(helper)
        again = native("adopt-legacy-indexes", *adopt_args)
        assert again["retained_completed_operation"] is True
        assert again["groups"] == batch["groups"] and again["claims"] == batch["claims"]
        registration_file = write_native_input(fixture.control, "registration", adoption["original_registration"])
        connections, all_reads = [], {}
        registration = adoption["registration"]
        def cli_read(name, arguments):
            result = fixture.run("--service-runtime-dir", fixture.control, "--service-project", registration["binding"],
                                 "--root", registration["root"], "--data-dir", registration["data_root"],
                                 "--collection", registration["collection"], "tools", "call", "airk_" + name,
                                 "--args", json.dumps(arguments))
            return clients.tool_value(json.loads(result.stdout))
        for item in configs:
            mapping = next(claim for claim in batch["claims"] if claim["locator"] == item["locator"])
            stage("enroll_existing_authority", client=item["client"])
            connection = native("http-legacy-connection", "--registration-file", registration_file,
                                "--configuration-receipt", item["sha256"], "--origin-sha256", mapping["origin_sha256"],
                                "--connection-id", uuid.uuid4().hex, "--client", item["native_client"])
            selected = connection["selection"]["selection"]
            assert connection["selection"]["kind"] == "imported_legacy"
            assert selected["grant"] == adoption["grant"] and selected["registration"] == registration
            assert selected["configuration_receipt"] == item["sha256"]
            fixture.managed_health = fixture.rpc("health")["observation"]
            transport.validate_native_identity(connection, fixture.managed_health)
            network = transport.McpClient(connection, fixture.control)
            stage("compare_canonical_reads", client=item["client"])
            try:
                network.send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-standalone-"+item["client"],"version":"1"}}})
                network.receive(1)
                network.send({"jsonrpc":"2.0","method":"notifications/initialized"})
                reads = {}
                for identifier, (name, arguments) in enumerate([("status", {}), ("read_file", {"path":"README.md","start_line":1,"end_line":2}),
                                        ("search", {"query":"cobalt","mode":"lexical","full_chunks":True}), ("query_scope", {"view":"projects"})], start=10):
                    # query_scope is closed metadata input, with no display mode.
                    if name != "query_scope":
                        arguments = {**arguments, "response_mode":"full"}
                    api_envelope = network.api_read(name, arguments)
                    assert api_envelope.get("isError") is not True, api_envelope
                    api = clients.tool_value(api_envelope)
                    cli = cli_read(name, arguments)
                    network.send({"jsonrpc":"2.0","id":identifier,"method":"tools/call","params":{"name":"airk_"+name,"arguments":arguments}})
                    mcp_envelope = network.receive(identifier)["result"]
                    assert mcp_envelope.get("isError") is not True, mcp_envelope
                    mcp = clients.tool_value(mcp_envelope)
                    parity_path = owner / ("parity-" + item["client"] + "-" + name + ".json")
                    write_private_json(parity_path, {"name": name, "arguments": arguments,
                                                     "api": api, "cli": cli, "mcp": mcp})
                    stage("compare_canonical_reads", parity_observation=str(parity_path))
                    assert comparable(api) == comparable(cli), "Complete CLI/API parity differs: " + name
                    assert comparable(api) == comparable(mcp), "Complete MCP/API parity differs: " + name
                    reads[name] = api
                    all_reads[item["client"]] = reads
                    stage("compare_canonical_reads", reads=all_reads)
                assert "cobalt" in json.dumps(reads["read_file"]).lower() and "cobalt" in json.dumps(reads["search"]).lower()
            finally:
                network.close()
            connections.append(connection)
        assert len({value["connection_id"] for value in connections}) == 2
        assert len({value["credential_file"] for value in connections}) == 2
        assert comparable(all_reads["codex"]["read_file"]) == comparable(all_reads["claude"]["read_file"])
        refusal = native("retire-legacy-caches", "--expected-active", active,
                         "--expected-inspection-sha256", inspection["inspection_sha256"], check=False)
        assert refusal.returncode != 0 and copied_helper.is_file(), "Unmigrated configs were not preserved"
        stage("replace_fixture_configurations", unmigrated_cleanup_refused=True)
        migrated = []
        for item, connection in zip(configs, connections):
            rendered = subprocess.run(["node", "-e", "const fs=require('fs');const m=require(process.argv[1]);const v=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(m.serializeServers(v.client,[{key:v.key,http:v.info}]));", str(model)],
                                      input=json.dumps({"client":item["client"],"key":server,"info":connection}), capture_output=True, text=True, check=True).stdout
            replace_config(item["path"], item["sha256"], rendered)
            migrated.append({"client":item["client"],"sha256":sha(item["path"]),"configuration":rendered,"connection_id":connection["connection_id"]})
            if len(migrated) == 1:
                partial = native("retire-legacy-caches", "--expected-active", active,
                                 "--expected-inspection-sha256", inspection["inspection_sha256"], check=False)
                assert partial.returncode != 0 and copied_helper.is_file(), "Second unmigrated claim was lost"
        stage("retire_old_cache", migrated_configurations=migrated)
        retirement = native("retire-legacy-caches", "--expected-active", active,
                            "--expected-inspection-sha256", inspection["inspection_sha256"])
        assert retirement["caches_retired"] is True and retirement["remaining"] == 0 and not copied_helper.exists()
        assert not list(global_storage.rglob(".airk-withheld-*"))
        assert inventory(copied_data) == data_before
        assert inventory(Path(adoption["archive"])) == archive_inventory
        report.update(inspection=inspection, batch=batch, retirement=retirement,
                      immutable_native_archive_inventory=archive_inventory, migrated_configurations=migrated,
                      distinct_http_credential_ids=[value["connection_id"] for value in connections])
    assert inventory(original_data) == data_before and inventory(original_source) == source_before
    assert sha(helper) == historical["helper"]["sha256"] and sha(archive["path"]) == archive["sha256"]
    report["original_fixture_unchanged"] = True
    stage("passed")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--fixture-report", type=Path, required=True)
    parser.add_argument("--model-js", type=Path, required=True, help="Built production vscode/out/model.js for actual config rendering")
    parser.add_argument("--tag", action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    clients = module("standalone_owner", "check-service-clients.py")
    historical = json.loads(args.fixture_report.read_text(encoding="utf-8"))
    report = {"schema":1,"runtime":str(args.runtime.resolve()),
              "runtime_manifest_sha256":sha(args.runtime / "airk-install.json"),
              "fixture_report_sha256":sha(args.fixture_report), "harness_sha256":sha(__file__),
              "production_model_js_sha256":sha(args.model_js),
              "qualification":"actual native CLI/HTTPS; agent UI not exercised","cases":[]}
    with clients.owned_client_fixture() as temporary:
        for tag in args.tag:
            case = next(c for c in historical["candidates"] if c["tag"] == tag and c["outcome"] == "created")
            owner = Path(temporary) / tag
            owner.mkdir(mode=0o700)
            try:
                result = qualify(args.runtime.resolve(), case, owner, args.model_js.resolve())
            except BaseException as error:
                progress = owner / "acceptance-progress.json"
                result = json.loads(progress.read_text(encoding="utf-8")) if progress.exists() else {"tag":tag,"stage":"initialization"}
                result["error"] = {"type":type(error).__name__, "message":re.sub(r"airk1\.[0-9a-f]{32}\.[0-9a-f]{64}", "[redacted]", str(error))[:2048]}
                result["preserved_owner"] = str(owner)
                report["cases"].append(result)
                write_private_json(args.output, report)
                raise
            report["cases"].append(result)
            write_private_json(args.output, report)
    print(json.dumps({"passed":len(report["cases"]),"output":str(args.output)}))


if __name__ == "__main__":
    main()
