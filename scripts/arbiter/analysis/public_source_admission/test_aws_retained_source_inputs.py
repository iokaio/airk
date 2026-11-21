# SPDX-License-Identifier: Apache-2.0
"""Exercise the retained caller against the actual source-closure helper and one local control file."""
from pathlib import Path
import hashlib
import json
import runpy
import time
import unittest
from tempfile import TemporaryDirectory
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
probe = runpy.run_path(str(HERE / "probe_aws_canceled_worker.py"))


class RetainedInputs(unittest.TestCase):
    def test_verified_roster_and_rejected_changed_member(self):
        with TemporaryDirectory() as work:
            base = Path(work)
            source = base / "source"
            source.mkdir()
            (source / "control.txt").write_bytes(b"source\n")
            (base / "inventory").write_bytes(b"{}")
            members = {"control.txt": {"sha256": hashlib.sha256(b"source\n").hexdigest()}}
            cases = [{"members": members}]
            w = runpy.run_path(str(probe["n"]["WRAPPER"]))
            menu = {"native": {**probe["n"], "CAMPAIGN": base}, "INVENTORY": "inventory",
                    "INVENTORY_SHA": hashlib.sha256(b"{}").hexdigest(),
                    "inputs": lambda manifest: ("source", cases, members)}
            tree, verified_cases, verified_members = probe["retained_inputs"](w, w["owner_module"](), menu)
            self.assertEqual(tree, source)
            self.assertEqual(verified_cases, cases)
            self.assertEqual(verified_members, members)
            (source / "control.txt").write_bytes(b"changed\n")
            with self.assertRaisesRegex(ValueError, "pin differs"):
                probe["retained_inputs"](w, w["owner_module"](), menu)


class PublicRetry(unittest.TestCase):
    def exercise(self, codes, *, wrong_scope=False, previous=None):
        calls = []
        index = "idx_control"
        expected = {"members": [{"index_id": index}], "fingerprint": "control"}
        args = ("tools", "call", "airk_list_files", "--index", index, "--args", '{"limit":1}')
        with TemporaryDirectory() as scratch:
            directory = Path(scratch)
            deadline = time.monotonic() + 5

            def request(label, actual, cutoff):
                self.assertEqual((actual, cutoff), (args, deadline))
                calls.append(label)
                code = codes[min(len(calls) - 1, len(codes) - 1)]
                scope = expected if not (wrong_scope and len(calls) > 1) else dict(expected, fingerprint="changed")
                body = {"scope_observation": scope}
                if code == "success":
                    return {"isError": False, "structuredContent": body}
                body.update(code=code, retryable=True, action="repeat_same_request", status="retry", retry_after_ms=25)
                envelope = {"isError": True, "structuredContent": body, "content": [{"type": "text", "text": json.dumps(body)}]}
                (directory / (label + ".stdout")).write_text(json.dumps(envelope))
                (directory / (label + ".command.json")).write_text('{"exit_code":1}')
                raise ValueError("typed original CLI refusal")

            read_file = lambda path: json.loads(path.read_bytes())
            with patch.object(time, "sleep", return_value=None):
                self.current_calls = calls
                return probe["public_read"](request, read_file, directory, "source", args, index, deadline, previous or expected)

    def test_typed_busy_preserves_original_arguments_and_scope(self):
        self.assertFalse(self.exercise(["selection_busy", "publication_busy", "success"])["isError"])
        self.assertEqual(self.current_calls, ["source", "source-retry-1", "source-retry-2"])

    def test_new_lifetime_can_renew_fingerprint_only(self):
        previous = {"members": [{"index_id": "idx_control"}], "fingerprint": "prior lifetime"}
        self.assertFalse(self.exercise(["selection_busy", "success"], previous=previous)["isError"])
        with self.assertRaisesRegex(ValueError, "changed selection"):
            self.exercise(["selection_busy"], previous=dict(previous, selection={"catalog_revision": 99}))

    def test_changed_scope_and_untyped_errors_stop(self):
        with self.assertRaisesRegex(ValueError, "changed selection"):
            self.exercise(["selection_busy", "success"], wrong_scope=True)
        with self.assertRaisesRegex(ValueError, "typed original CLI refusal"):
            self.exercise(["quota_exceeded"])
        self.assertEqual(self.current_calls, ["source"])

    def test_original_native_client_ends_after_41_attempts(self):
        with self.assertRaisesRegex(AssertionError, "no further safe replay"):
            self.exercise(["selection_busy"])
        self.assertEqual(len(self.current_calls), 41)


if __name__ == "__main__":
    unittest.main()
