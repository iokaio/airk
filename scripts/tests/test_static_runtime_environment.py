# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("airk_static_environment", Path(__file__).parents[1] / "static-runtime-environment.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StaticEnvironmentTests(unittest.TestCase):
    def fixture(self, root):
        owner = MODULE.provisioner_owner()
        destination = root / "persistent static cache"
        environment = {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1",
                       "ORT_PREFER_DYNAMIC_LINK": "0", "ORT_LIB_LOCATION": str(destination / "b"),
                       "ORT_LIB_PROFILE": "Release"}
        result = {"receipt": {"target": "fixture-native", "libraries": [{"sha256": "a" * 64}]},
                  "environment": environment, "static_link_closure": {"schema": 1, "future_field": ["retain exactly"]}}
        path = root / "environment.json"
        path.write_text(json.dumps(result), encoding="utf-8")
        return owner, destination, path, result

    def test_exact_handoff_reverifies_complete_owner_result_and_returns_only_five_vars(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, destination, path, result = self.fixture(Path(temporary))
            with patch.object(MODULE, "provisioner_owner", return_value=owner), \
                    patch.object(owner, "host_target", return_value="fixture-native"), \
                    patch.object(owner, "verify_build", return_value=result) as verify:
                actual = MODULE.load_environment_file(path, destination, "fixture-native")
                self.assertEqual(actual, result["environment"])
                self.assertIsNot(actual, result["environment"])
                verify.assert_called_once_with(destination, "fixture-native")

    def test_reviewed_neo_b2_is_accepted_only_when_complete_canonical_result_owns_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, destination, path, result = self.fixture(Path(temporary))
            result["receipt"]["build_mode"] = "adopted_reviewed_neo_arm64"
            result["environment"]["ORT_LIB_LOCATION"] = str(destination / "b2")
            path.write_text(json.dumps(result), encoding="utf-8")
            with patch.object(MODULE, "provisioner_owner", return_value=owner), \
                    patch.object(owner, "host_target", return_value="fixture-native"), \
                    patch.object(owner, "verify_build", return_value=result) as verify:
                self.assertEqual(MODULE.load_environment_file(path), result["environment"])
                verify.assert_called_once_with(destination, "fixture-native")
                # A sibling path cannot borrow the same otherwise valid receipt.
                for tree in ("b", "arbitrary-unowned-tree"):
                    changed = copy.deepcopy(result)
                    changed["environment"]["ORT_LIB_LOCATION"] = str(destination / tree)
                    path.write_text(json.dumps(changed), encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "complete build result"):
                        MODULE.load_environment_file(path, destination)

    def test_environment_injection_wrong_target_or_destination_never_reaches_verification(self):
        for change in ("extra", "dynamic", "newline", "relative", "wrong-destination", "wrong-target"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                owner, destination, path, result = self.fixture(Path(temporary))
                exported = copy.deepcopy(result)
                target = "fixture-native"
                if change == "extra":
                    exported["environment"]["PATH"] = "/injected"
                elif change == "dynamic":
                    exported["environment"]["ORT_PREFER_DYNAMIC_LINK"] = "1"
                elif change == "newline":
                    exported["environment"]["ORT_LIB_LOCATION"] += "\n"
                elif change == "relative":
                    exported["environment"]["ORT_LIB_LOCATION"] = "relative/b"
                elif change == "wrong-destination":
                    destination = destination / "foreign"
                else:
                    target = "cross-host"
                path.write_text(json.dumps(exported), encoding="utf-8")
                with patch.object(MODULE, "provisioner_owner", return_value=owner), \
                        patch.object(owner, "host_target", return_value="fixture-native"), \
                        patch.object(owner, "verify_build") as verify:
                    with self.assertRaises(ValueError):
                        MODULE.load_environment_file(path, destination, target)
                    verify.assert_not_called()

    def test_unchanged_environment_cannot_hide_different_library_receipt_or_link_closure(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, destination, path, result = self.fixture(Path(temporary))
            for key in ("receipt", "static_link_closure"):
                changed = copy.deepcopy(result)
                changed[key]["unverified"] = True
                path.write_text(json.dumps(changed), encoding="utf-8")
                with patch.object(MODULE, "provisioner_owner", return_value=owner), \
                        patch.object(owner, "host_target", return_value="fixture-native"), \
                        patch.object(owner, "verify_build", return_value=result):
                    with self.assertRaisesRegex(ValueError, "complete build result"):
                        MODULE.load_environment_file(path, destination)

    def test_child_argv_and_environment_are_local_and_failed_exit_is_preserved(self):
        environment = {name: "fixture" for name in MODULE.STATIC_KEYS}
        before = dict(os.environ)
        command = ["fictional tool with spaces", "literal $(argument)", "--flag"]
        with patch.object(MODULE, "load_environment_file", return_value=environment), \
                patch.object(MODULE.subprocess, "call", return_value=17) as invoke:
            self.assertEqual(MODULE.main(["--environment-file", "fixture.json", "--", *command]), 17)
            self.assertEqual(invoke.call_args.args[0], command)
            self.assertEqual(invoke.call_args.kwargs["env"], {**before, **environment})
            self.assertNotIn("shell", invoke.call_args.kwargs)
        self.assertEqual(dict(os.environ), before)

    def test_refused_handoff_never_executes_child(self):
        with patch.object(MODULE, "load_environment_file", side_effect=ValueError("owner refused")), \
                patch.object(MODULE.subprocess, "call") as invoke:
            with self.assertRaisesRegex(ValueError, "owner refused"):
                MODULE.main(["--environment-file", "fixture.json", "--", "never-run"])
            invoke.assert_not_called()


if __name__ == "__main__":
    unittest.main()
