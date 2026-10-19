# SPDX-License-Identifier: Apache-2.0
"""Public CLI smoke for aliases, revision binding, quotas, relocation and retention."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-storage-smoke-") as temporary:
        base = Path(temporary)
        root, specs, replacement, data = [base / name for name in ("workspace", "specs", "replacement", "state")]
        for directory in (root, specs, replacement, data):
            directory.mkdir()
        primary_text = "Original primary source.\n"
        original_text = "REQ-81: The service shall answer within 40 ms.\n"
        replacement_text = "Replacement requirement fixture.\n"
        (root / "main.md").write_bytes(primary_text.encode())
        (specs / "requirements.md").write_bytes(original_text.encode())
        (replacement / "requirements.md").write_bytes(replacement_text.encode())
        environment = {**os.environ, "AIRK_USAGE_MODE": "disabled"}

        def call(*command, succeeds=True, error_terms=("quota", "reservation", "capacity")):
            result = subprocess.run([str(binary), "--root", str(root), "--data-dir", str(data),
                                     "--collection", "storage-fixture", *map(str, command)],
                                    env=environment, capture_output=True, text=True, encoding="utf-8", timeout=120)
            if not succeeds:
                assert result.returncode != 0, command
                assert any(word in result.stderr.lower() for word in error_terms), result.stderr
                return result
            assert result.returncode == 0, (command, result.stderr)
            return json.loads(result.stdout)

        # This scenario verifies explicit prune preview/apply, so preserve its
        # old generations instead of relying on the automatic new-user default.
        call("collections", "retention", "set", "--json", json.dumps({
            "version": 1, "mode": "manual", "searchable_predecessors": 0}))
        call("index")
        assert call("collections", "roots", "add", "requirements", specs)["pending_index"]
        assert call("coverage", "--refresh")["percent"] is None
        call("index")
        status = call("status")
        assert status["files"] == 2
        assert call("coverage", "--refresh")["percent"] == 100
        indexed_path = "@requirements/requirements.md"
        hit = call("search", "--mode", "lexical", "REQ-81", "--path-prefix", "@requirements/")["hits"][0]
        assert hit["citation"]["path"] == indexed_path
        assert hit["citation"]["content_hash"] == hashlib.sha256(original_text.encode()).hexdigest()
        original_location = call("collections", "roots", "locate", indexed_path)
        assert original_location["relative_path"] == "requirements.md"
        call("collections", "roots", "remove", "requirements")
        call("collections", "roots", "add", "requirements", replacement)
        assert call("collections", "roots", "locate", indexed_path) == original_location
        assert call("search", "--mode", "lexical", "REQ-81")["generation"] == status["generation"]
        call("index")
        assert not call("search", "--mode", "lexical", "REQ-81")["hits"]
        assert call("collections", "roots", "locate", indexed_path)["root"] != original_location["root"]

        original_policy = call("collections", "quota", "policy")
        capacity = call("collections", "quota", "status")
        collection_used = capacity["collections"]["storage-fixture"]["used_bytes"]
        constrained = {"root_bytes": capacity["used_bytes"] + 65_536,
                       "default_collection_bytes": collection_used + 65_536, "collections": {}}
        policy_file = base / "quota.json"
        policy_file.write_text(json.dumps(constrained), encoding="utf-8")
        call("collections", "quota", "set", policy_file)
        generation = call("status")["generation"]
        changed_text = "Additional fictional storage payload.\n" * 20_000
        (root / "main.md").write_bytes(changed_text.encode())
        call("index", succeeds=False)
        assert call("status")["generation"] == generation
        assert call("read", "main.md", "--start-line", 1, "--end-line", 1)["hits"][0]["text"] == primary_text
        policy_file.write_text(json.dumps(original_policy), encoding="utf-8")
        call("collections", "quota", "set", policy_file)
        call("index")
        assert call("status")["generation"] > generation
        preview = call("collections", "prune")
        assert preview["dry_run"] and preview["candidates"]
        applied = call("collections", "prune", "--apply")
        assert not applied["dry_run"] and applied["removed_bytes"] > 0
        published = call("status")
        destination = base / "relocated-state"
        plan = call("storage", "plan", destination)
        assert plan["files"] > 0 and plan["old_state_retained"]
        moved = call("storage", "relocate", destination)
        assert moved["files"] == plan["files"] and moved["old_state_retained"]
        call("status", succeeds=False, error_terms=("relocated", "relocation"))
        assert (data / "collections" / "storage-fixture").is_dir()
        data = destination
        assert call("status") == published
        assert call("coverage", "--refresh")["percent"] == 100
        assert call("search", "--mode", "lexical", "Replacement")["hits"]
        call("collections", "remove", "storage-fixture")
        assert (root / "main.md").read_text(encoding="utf-8") == changed_text
        assert (specs / "requirements.md").read_text(encoding="utf-8") == original_text
        assert (replacement / "requirements.md").read_text(encoding="utf-8") == replacement_text
    print("Storage CLI smoke passed: aliases, pinned origins, quota rollback, relocation, pruning and source-safe removal.")


if __name__ == "__main__":
    main()
