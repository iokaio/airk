# SPDX-License-Identifier: Apache-2.0
"""Qualify smoke-test cleanup without depending on Docker daemon timing."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location(
    "container_check", Path(__file__).parents[1] / "check-container.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ContainerCheckTests(unittest.TestCase):
    def test_stdio_rejects_retired_missing_and_duplicate_tools(self):
        names = ["airk_" + name for name in (
            "capabilities", "changes", "code_facts", "context", "document_facts",
            "file_info", "imports", "list_files", "outline", "read_file", "references",
            "relationships", "search", "status", "symbols")]
        for catalog, accepted in ((names, True), (names[:-1], False),
                                  (names + ["airk_history"], False),
                                  (names[:-1] + [names[0]], False)):
            with self.subTest(catalog=catalog):
                process = MagicMock()
                process.stdin = io.StringIO()
                process.stdout = io.StringIO("\n".join(json.dumps(response) for response in (
                    {"id": 1, "result": {"serverInfo": {"name": "airk"}}},
                    {"id": 2, "result": {"tools": [{"name": name} for name in catalog]}},
                    {"id": 3, "result": {"content": [{"text": json.dumps({
                        "collection": "project-a", "facts": [1]})}]}},
                )))
                with patch.object(smoke.subprocess, "Popen", return_value=process):
                    if accepted:
                        smoke.check_stdio("fixture")
                    else:
                        with self.assertRaises(AssertionError):
                            smoke.check_stdio("fixture")
                self.assertTrue(process.stdin.closed)
                process.wait.assert_called_once_with(timeout=10)

    def test_cleanup_accepts_only_missing_owned_container(self):
        name = "airk-scaffold-check-fixture"
        for error, absent in (
                (f"Error response from daemon: No such container: {name}\n", True),
                ("Error response from daemon: No such container: another\n", False),
                (f"Error response from daemon: No such container: {name}-other\n", False),
                ("Cannot connect to the Docker daemon", False),
                ("permission denied", False)):
            with self.subTest(error=error):
                result = subprocess.CompletedProcess(["docker", "rm", name], 1, "", error)
                with patch.object(smoke, "docker", return_value=result):
                    if absent:
                        smoke.remove_owned_container(name)
                    else:
                        with self.assertRaises(subprocess.CalledProcessError) as caught:
                            smoke.remove_owned_container(name)
                        self.assertEqual(caught.exception.stderr, error)

    def test_smoke_releases_container_before_deleting_owned_volume(self):
        registered = False
        auto_remove = False
        stops = 0
        removed_b = False
        volume_removed = False

        def docker(*args, check=True):
            nonlocal registered, auto_remove, stops, removed_b, volume_removed
            output = ""
            if args[0] == "run":
                self.assertFalse(registered, "restart must retire the first container")
                registered = True
                auto_remove = "--rm" in args
            elif args[0] == "stop":
                stops += 1
                # Docker may acknowledge stop before auto-removal releases the
                # volume. Make that timing deterministic on the final shutdown.
                if auto_remove and stops == 1:
                    registered = False
            elif args[0] == "rm":
                registered = False
            elif args[:2] == ("volume", "rm"):
                if registered:
                    raise subprocess.CalledProcessError(
                        1, ["docker", *args], stderr="volume is in use")
                volume_removed = True
            elif args[0] == "inspect":
                output = "none\n"
            elif args[0] == "exec" and args[2] == "airk":
                collection = args[args.index("--collection") + 1]
                command = args[args.index("--collection") + 2:]
                if command[0] == "index":
                    output = json.dumps({"collection": collection})
                elif command[0] == "symbol":
                    output = json.dumps({"collection": collection,
                                         "facts": [1] if collection == "project-a" else []})
                elif command == ("collections", "list"):
                    output = json.dumps([{"disk_bytes": 1}, {"disk_bytes": 1}])
                elif command == ("collections", "remove", "project-b"):
                    removed_b = True
                    output = json.dumps({"status": "removed-derived-index"})
                elif command == ("status",):
                    if collection == "project-b" and removed_b:
                        return subprocess.CompletedProcess(args, 1, "", "removed")
                    output = json.dumps({"files": 2})
                else:
                    self.fail(f"Unexpected public CLI command: {command}")
            return subprocess.CompletedProcess(args, 0, output, "")

        with patch.object(smoke, "docker", side_effect=docker), \
                patch.object(smoke, "check_stdio"), \
                patch("sys.argv", ["check-container.py"]), \
                contextlib.redirect_stdout(io.StringIO()):
            smoke.main()
        self.assertTrue(volume_removed)
        self.assertFalse(registered)


if __name__ == "__main__":
    unittest.main()
