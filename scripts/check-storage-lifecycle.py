# SPDX-License-Identifier: Apache-2.0
"""Native service smoke for aliases, quota refusal, maintenance and complete project removal."""
import argparse
from contextlib import ExitStack, closing
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import time

SPEC = importlib.util.spec_from_file_location("airk_storage_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(clients)
SOURCE_SPEC = importlib.util.spec_from_file_location("storage_reviewed_source_fixture", Path(__file__).with_name("reviewed-source-fixture.py"))
reviewed_source = importlib.util.module_from_spec(SOURCE_SPEC)
SOURCE_SPEC.loader.exec_module(reviewed_source)


def owner_arguments(fixture, name):
    return fixture.management_arguments(name)


def removal_complete(fixture, binding, collection, inactive, worker):
    """Retired catalog identities can survive completed physical cleanup."""
    bindings = [row["project"]["registration"]["binding"] for row in inactive["projects"]]
    assert bindings in ([binding], []) and inactive["after"] is None, "Unexpected inactive registrations"
    if collection.exists() or worker["identity"] is not None:
        return False
    if not bindings:
        return True
    if not fixture.catalog_authority:
        return False
    # This is fixture-only observation, never management authority. A retained
    # inactive row alone does not prove the final cleanup transaction committed.
    registry = (fixture.control / "service-1.4.sqlite3").resolve(strict=True)
    assert registry.is_relative_to(fixture.owner.resolve(strict=True)), "Registry escaped fixture owner"
    with closing(sqlite3.connect(registry.as_uri() + "?mode=ro", uri=True, timeout=1)) as connection:
        row = connection.execute(
            "SELECT registered,cleanup_guard,EXISTS(SELECT 1 FROM service_removed_projects WHERE binding=?1) "
            "FROM projects WHERE binding=?1", (binding,)
        ).fetchone()
    return row == (0, None, 1)


def settings_arguments(fixture, name, view):
    # Quota covers owned accounting outside a selected source subtree. Never
    # infer this owner path from an unknown view or widen an evidence grant.
    if view in ("quota_policy", "quota_status"):
        return fixture.management_arguments(name)
    assert view in ("extraction_policy", "vector_selection", "vector_status", "roots", "scan_limits"), \
        "Unsupported storage fixture settings view"
    return fixture.arguments(name)


def replacement_registration(fixture, name, root, parent):
    """A removed library identity is retained history, never fresh authority.

    Review an actual replacement Index/grant in a new owned storage leaf. The
    original request receipt, Index and selected grant are not rewritten.
    """
    previous = dict(fixture.projects[name])
    selected = dict(fixture.scopes[name]) if fixture.catalog_authority else None
    parent = parent.resolve(strict=True)
    assert parent.is_relative_to(fixture.owner.resolve(strict=True)), "Replacement storage escaped fixture owner"
    assert not any(parent.iterdir()), "Replacement storage is not fresh"
    fixture.register(name, root, parent)
    current = fixture.projects[name]
    assert Path(current["root"]).samefile(root)
    assert current["collection"] == previous["collection"]
    assert current["binding"] != previous["binding"], "Replacement reused the removed registration"
    actual = (parent / current["collection"]).resolve(strict=True) if fixture.catalog_authority else parent
    assert actual.is_relative_to(parent) and actual.samefile(Path(current["data_root"])), "Replacement registration changed its exact owned storage leaf"
    if selected is not None:
        replacement = fixture.scopes[name]
        assert replacement["namespace"] == selected["namespace"]
        assert replacement["index_id"] != selected["index_id"] and replacement["grant_id"] != selected["grant_id"], "Replacement reused retired Index/grant authority"
        assert replacement["project_ids"] != selected["project_ids"], "Replacement reused the original Project"
    return previous, selected


def assert_removed_selection(result, selected, retirement=None):
    assert result.returncode != 0, "Removed selection acquired replacement authority"
    if not result.stdout.strip():
        # resolve_scoped checks the native tombstone before worker brokerage.
        # This genuine CLI preflight refusal has no fabricated tool envelope.
        assert retirement is not None and retirement["status"] == "catalog_index_retirement_evidence"
        assert retirement["index_id"] == selected["index_id"] and retirement["evidence"]["index"]["tombstone"] is True
        assert retirement["evidence"]["catalog_clock"]["namespace"] == selected["namespace"]
        error = result.stderr.strip()
        expected = "selected catalog index metadata is not ready"
        if error.startswith('{'):
            value = json.loads(error)
            assert value == {"airk_error":1,"code":"operation_failed","retryable":False,"message":expected}
        else:
            assert error == "Error: " + expected, "Unexpected CLI failure is not removed-scope denial"
        return
    envelope = json.loads(result.stdout)
    assert envelope.get("isError") is True
    content = envelope.get("content", [])
    assert len(content) == 1 and content[0].get("type") == "text"
    body = json.loads(content[0]["text"])
    assert envelope.get("structuredContent") == body
    error, requested = body["error"], body["requested_scope"]
    assert error["code"] == "scoped_connection_unavailable" and error["query_submitted"] is False
    assert error["retryable"] is False, "Transient worker startup is not permanent removed-scope denial"
    assert requested["confirmed"] is False
    observed = dict(requested["selection"])
    expected = dict(selected)
    assert observed.pop("expected_catalog_revision") >= expected.pop("expected_catalog_revision")
    assert observed == expected, "Removed-scope denial changed the reviewed authority"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    with clients.owned_client_fixture() as temporary, ExitStack() as stack:
        base = Path(temporary).resolve(strict=True)
        root, specs, replacement, data = [base / name for name in ("workspace", "specs", "replacement", "index-data")]
        for directory in (root, specs, replacement, data):
            directory.mkdir()
        primary_text = "Original primary source.\n"
        original_text = "REQ-81: The service shall answer within 40 ms.\n"
        replacement_text = "Replacement requirement fixture.\n"
        (root / "main.md").write_bytes(primary_text.encode("utf-8"))
        (specs / "requirements.md").write_bytes(original_text.encode("utf-8"))
        (replacement / "requirements.md").write_bytes(replacement_text.encode("utf-8"))
        fixture = clients.Fixture(binary.parent, base)
        # Reviewed source mutation/retirement below uses the modern native
        # administrative owner. Historical catalog setup alone proves neither.
        if fixture.catalog_authority and not fixture.single_image:
            fixture.log.close()
            raise RuntimeError("Historical catalog source-mutation/retirement fixture is not qualified; no service started")
        fixture = stack.enter_context(fixture)
        assert hashlib.sha256(fixture.binary.read_bytes()).digest() == hashlib.sha256(binary.read_bytes()).digest()
        fixture.assert_alive()
        name = "storage-fixture"
        fixture.register(name, root, data)
        binding = fixture.projects[name]["binding"]
        fixture.configure(name, {"operation": "vectors", "enabled": False})
        fixture.configure(name, {"operation": "usage", "mode": "disabled"})

        def settings(view):
            return json.loads(fixture.run(*settings_arguments(fixture, name, view), "settings", view).stdout)

        def status():
            return fixture.published_status(name)

        def configure(draft, expected="succeeded"):
            if fixture.single_image and draft["operation"] in ("add_root", "remove_root"):
                assert expected == "succeeded"
                return reviewed_source.SourceReview(fixture, name).source_change(draft)
            job = fixture.configure(name, draft, expected)
            return fixture.rpc({"configuration_result": {"id": job["id"]}})["result"]

        def index(expected="succeeded"):
            return fixture.job(name, "index", expected)

        def pause(value):
            fixture.rpc({"pause": {"project": binding, "paused": value}})

        def retired(*command):
            result = fixture.run(*fixture.arguments(name), *command, check=False)
            assert result.returncode != 0, (command, result.stdout)
            assert "unrecognized subcommand" in result.stderr.lower(), result.stderr

        retired("collections", "retention", "set", "--json", "{}")
        index()
        initial_roots = settings("roots")["configured"]
        configure({"operation": "add_root", "alias": "requirements", "path": str(specs)})
        index()
        indexed_path = "@requirements/requirements.md"
        hit = fixture.query(name, "search", "--mode", "lexical", "REQ-81", "--path-prefix", "@requirements/")["hits"][0]
        assert hit["citation"]["path"] == indexed_path
        assert hit["citation"]["content_hash"] == hashlib.sha256(original_text.encode()).hexdigest()
        original_read = fixture.query(name, "read", indexed_path, "--start-line", "1", "--end-line", "1")
        assert original_read["hits"][0]["text"].strip() == original_text.strip()
        roots = settings("roots")
        assert "requirements" in roots["configured"], roots
        # Native root changes schedule source work themselves. Check the coherent
        # published evidence after completion, without assuming an editor scheduler.
        configure({"operation": "remove_root", "alias": "requirements"})
        configure({"operation": "add_root", "alias": "requirements", "path": str(replacement)})
        index()
        assert not fixture.query(name, "search", "--mode", "lexical", "REQ-81")["hits"]
        assert fixture.query(name, "search", "--mode", "lexical", "Replacement")["hits"]
        # The previously captured evidence keeps its original citation/hash.
        assert original_read["hits"][0]["citation"]["content_hash"] == hashlib.sha256(original_text.encode()).hexdigest()

        original_policy = settings("quota_policy")
        capacity = settings("quota_status")
        collection_capacity = capacity["collections"][name]
        policy_file = base / "quota.json"
        # Preserve allowances already admitted for the active WAL/catalog. The
        # next larger source must require new capacity instead of withdrawing
        # a live owner's reservation through an invalid policy update.
        constrained = {"root_bytes": capacity["used_bytes"] + capacity["reserved_bytes"] + (1 << 20),
                       "default_collection_bytes": collection_capacity["used_bytes"] + collection_capacity["reserved_bytes"] + (1 << 20), "collections": {}}
        policy_file.write_text(json.dumps(constrained), encoding="utf-8")
        configure({"operation": "quota_policy", "path": str(policy_file)})
        before = status()["publication"]["token"]
        changed_text = "Additional fictional storage payload.\n" * 100_000
        (root / "main.md").write_text(changed_text, encoding="utf-8")
        waiting_index = json.loads(fixture.run(*owner_arguments(fixture, name), "index").stdout)
        waiting_id = waiting_index["job"]["id"]
        deadline = time.monotonic() + 45
        while True:
            page = fixture.rpc({"console": {"after": None, "revision": None, "limit": 16}}, deadline=deadline)["page"]
            recovery = [item for row in page["projects"] if row["project"]["registration"]["binding"] == binding
                        for item in row.get("recovery", []) if item.get("job") == waiting_id]
            if any(item["state"] == "waiting" and item["reason"] in
                   ("storage_quota_root", "storage_quota_collection", "storage_quota_reservation") for item in recovery):
                break
            assert time.monotonic() < deadline, "Quota pressure did not produce a durable storage wait"
            time.sleep(0.05)
        print("Quota pressure retained the accepted index job in a durable wait.", flush=True)
        retained = fixture.query(name, "read", "main.md", "--start-line", "1", "--end-line", "1")
        assert retained["hits"][0]["text"].strip() == primary_text.strip()
        assert retained["hits"][0]["citation"]["content_hash"] == hashlib.sha256(primary_text.encode()).hexdigest()
        assert retained["publication"]["token"] == status()["publication"]["token"]
        policy_file.write_text(json.dumps(original_policy), encoding="utf-8")
        configure({"operation": "quota_policy", "path": str(policy_file)})
        completed = fixture.wait(waiting_index)
        assert completed["id"] == waiting_id and completed["attempts"] >= 2
        assert status()["publication"]["token"] != before
        print("Restored quota resumed the same accepted job without another index request.", flush=True)
        pause(True)
        published = status()["publication"]["token"]
        pause(False)
        maintenance = fixture.wait(fixture.enqueue(name, "maintain"))
        assert maintenance["state"] == "succeeded"
        assert status()["publication"]["token"] == published
        # A competing legacy relocation/removal writer is deliberately unavailable
        # in the service family. Refusal must preserve this publication and sources.
        destination = base / "unapproved-relocation"
        retired("storage", "relocate", destination)
        retired("collections", "remove", name)
        assert not destination.exists()
        assert status()["publication"]["token"] == published

        old_arguments = fixture.arguments(name)
        registered_data = (data / name).resolve(strict=True) if fixture.catalog_authority else data.resolve(strict=True)
        assert registered_data.is_relative_to(base) and registered_data.samefile(Path(fixture.projects[name]["data_root"])), "Registered storage escaped exact fixture layout"
        retirement = None
        if fixture.single_image:
            retirement = reviewed_source.SourceReview(fixture, name).retire()
            receipt_query = {"catalog_index_retirement_evidence": {"index_id": retirement["index_id"]}}
            assert retirement["evidence"]["index"]["backing"]["binding"] == binding
        else:
            receipt_query, removal = fixture.remove(name)
            assert removal["removed_project"] == binding
        collection = registered_data / "collections-1.4" / name
        deadline = time.monotonic() + 90
        while True:
            inactive = fixture.rpc({"inactive_console": {"after": None, "revision": None, "limit": 16}})["page"]
            worker = fixture.rpc({"worker": {"project": binding}})
            if removal_complete(fixture, binding, collection, inactive, worker):
                break
            assert time.monotonic() < deadline, (
                "Full removal did not finish: "
                f"collection_exists={collection.exists()}, inactive={inactive}, worker={worker}"
            )
            time.sleep(0.05)
        proof = fixture.rpc(receipt_query)
        if retirement is not None:
            assert proof["evidence"]["retirement"] == retirement["evidence"]["retirement"]
            assert proof["evidence"]["index"] == retirement["evidence"]["index"]
            assert proof["evidence"]["catalog_clock"]["revision"] >= retirement["evidence"]["catalog_clock"]["revision"]
        else:
            assert proof["removed_project"] == binding and proof["job"] is None
        assert fixture.rpc({"projects": {"after": None, "revision": None, "limit": 16}})["page"]["projects"] == []
        assert (root / "main.md").read_text(encoding="utf-8") == changed_text
        assert (specs / "requirements.md").read_text(encoding="utf-8") == original_text
        assert (replacement / "requirements.md").read_text(encoding="utf-8") == replacement_text
        if fixture.single_image:
            fresh = base / "replacement-state"
            fresh.mkdir()
            previous, selected = replacement_registration(fixture, name, root, fresh)
            assert previous["binding"] == binding and selected is not None
            stale = fixture.run(*old_arguments, "tools", "call", "airk_search", "--args", '{"query":"Additional","mode":"lexical"}', check=False)
            assert_removed_selection(stale, selected, retirement)
            assert fixture.rpc(receipt_query)["evidence"]["retirement"] == retirement["evidence"]["retirement"], "Original canonical retirement receipt changed during replacement"
        else:
            fixture.rpc({"register": {"project": fixture.projects[name]}})
        fixture.configure(name, {"operation": "vectors", "enabled": False})
        fixture.configure(name, {"operation": "usage", "mode": "disabled"})
        index()
        assert settings("roots")["configured"] == initial_roots
        assert not fixture.query(name, "search", "--mode", "lexical", "Replacement")["hits"]
        assert fixture.query(name, "search", "--mode", "lexical", "Additional")["hits"]
        assert (root / "main.md").read_text(encoding="utf-8") == changed_text
        assert (specs / "requirements.md").read_text(encoding="utf-8") == original_text
        assert (replacement / "requirements.md").read_text(encoding="utf-8") == replacement_text
    print("Native storage lifecycle passed: aliases, exact citations, quota refusal, maintenance, legacy-writer refusal and source-safe full removal and reviewed replacement registration.")


if __name__ == "__main__":
    main()
