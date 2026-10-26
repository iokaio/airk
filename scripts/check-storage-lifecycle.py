# SPDX-License-Identifier: Apache-2.0
"""Native service smoke for aliases, quota refusal, maintenance and guarded cleanup."""
import argparse
from contextlib import ExitStack
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time

SPEC = importlib.util.spec_from_file_location("airk_storage_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(clients)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-storage-smoke-") as temporary, ExitStack() as stack:
        base = Path(temporary).resolve(strict=True)
        root, specs, replacement, data = [base / name for name in ("workspace", "specs", "replacement", "state")]
        for directory in (root, specs, replacement, data):
            directory.mkdir()
        primary_text = "Original primary source.\n"
        original_text = "REQ-81: The service shall answer within 40 ms.\n"
        replacement_text = "Replacement requirement fixture.\n"
        (root / "main.md").write_bytes(primary_text.encode("utf-8"))
        (specs / "requirements.md").write_bytes(original_text.encode("utf-8"))
        (replacement / "requirements.md").write_bytes(replacement_text.encode("utf-8"))
        fixture = stack.enter_context(clients.Fixture(binary.parent, base))
        assert fixture.binary == binary
        name = "storage-fixture"
        fixture.register(name, root, data)
        binding = fixture.projects[name]["binding"]
        fixture.configure(name, {"operation": "vectors", "enabled": False})
        fixture.configure(name, {"operation": "usage", "mode": "disabled"})

        def settings(view):
            return json.loads(fixture.run(*fixture.arguments(name), "settings", view).stdout)

        def status():
            return fixture.published_status(name)

        def configure(draft, expected="succeeded"):
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
        rebound_roots = settings("roots")
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
        index("failed")
        retained = fixture.query(name, "read", "main.md", "--start-line", "1", "--end-line", "1")
        assert retained["hits"][0]["text"].strip() == primary_text.strip()
        assert retained["hits"][0]["citation"]["content_hash"] == hashlib.sha256(primary_text.encode()).hexdigest()
        assert retained["publication"]["token"] == status()["publication"]["token"]
        policy_file.write_text(json.dumps(original_policy), encoding="utf-8")
        configure({"operation": "quota_policy", "path": str(policy_file)})
        index()
        assert status()["publication"]["token"] != before
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

        configure({"operation": "unregister"})
        inactive = fixture.rpc({"inactive_console": {"after": None, "revision": None, "limit": 16}})["page"]
        assert [row["project"]["registration"]["binding"] for row in inactive["projects"]] == [binding]
        preview_job = fixture.configure_inactive(name, {"operation": "prepare_derived_cleanup"})
        preview = fixture.rpc({"configuration_result": {"id": preview_job["id"]}})["result"]
        assert preview["owned_files"] > 0 and preview["preserved_settings_files"] > 0
        cleanup_job = fixture.configure_inactive(name, {"operation": "delete_derived", "preview": preview["guard"]})
        report = fixture.rpc({"configuration_result": {"id": cleanup_job["id"]}})["result"]
        assert report["complete"] and report["pending_incarnations"] == 0
        assert report["removed_known_files"] > 0 and report["guard"] == preview["guard"]
        deadline = time.monotonic() + 30
        while fixture.rpc({"worker": {"project": binding}})["identity"] is not None:
            assert time.monotonic() < deadline, "Cleanup owner did not drain"
            time.sleep(0.05)
        fixture.rpc({"register": {"project": fixture.projects[name]}})
        index()
        assert fixture.query(name, "search", "--mode", "lexical", "Replacement")["hits"]
        assert settings("roots")["configured"] == rebound_roots["configured"]
        assert (root / "main.md").read_text(encoding="utf-8") == changed_text
        assert (specs / "requirements.md").read_text(encoding="utf-8") == original_text
        assert (replacement / "requirements.md").read_text(encoding="utf-8") == replacement_text
    print("Native storage lifecycle passed: aliases, exact citations, quota refusal, maintenance, legacy-writer refusal and source-safe cleanup/re-registration.")


if __name__ == "__main__":
    main()
