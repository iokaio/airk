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
            "file_info", "help", "imports", "list_files", "outline", "read_file", "references",
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
                        "collection": "project-a", "items": [1]})}]}},
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
                        smoke.remove_owned_container(name, "fixture-token")
                    else:
                        with self.assertRaises(subprocess.CalledProcessError) as caught:
                            smoke.remove_owned_container(name, "fixture-token")
                        self.assertEqual(caught.exception.stderr, error)

    def test_smoke_releases_container_before_deleting_owned_volume(self):
        status_calls = 0
        registered = False
        volume_removed = False
        token = "fixture-token"
        container_id = "owned-immutable-id"

        def docker(*args, check=True, timeout=60):
            self.assertGreater(timeout, 0)
            self.assertLessEqual(timeout, 60)
            nonlocal registered, volume_removed, status_calls
            output = ""
            if args[0] == "run":
                self.assertFalse(registered, "restart must retire the first container")
                registered = True
            elif args[0] == "stop":
                self.assertEqual(args[-1], container_id)
            elif args[0] == "rm":
                self.assertEqual(args[-1], container_id)
                registered = False
            elif args[:2] == ("volume", "inspect"):
                output = json.dumps([{"Labels": {"io.airk.fixture": token}}])
            elif args[:2] == ("volume", "rm"):
                if registered:
                    raise subprocess.CalledProcessError(
                        1, ["docker", *args], stderr="volume is in use")
                volume_removed = True
            elif args[0] == "inspect":
                if "--format" in args:
                    output = "none\n"
                else:
                    output = json.dumps([{"Id": container_id, "Config": {"Labels": {"io.airk.fixture": token}}}])
            elif args[0] == "exec" and args[2] == "airk":
                if args[3:] == ("runtime", "location"):
                    return subprocess.CompletedProcess(args, 0, json.dumps({"install_dir":"/data/install","user_paths": {"state": "/data/state"}}), "")
                if args[3:5] == ("runtime", "prepare-client"):
                    output = json.dumps({"launcher":"/data/install/airk-launcher","service_lifecycle_changed":False})
                    return subprocess.CompletedProcess(args,0,output,"")
                if args[3:5] == ("runtime", "inspect"):
                    output = json.dumps({"launcher":"/data/install/airk-launcher","managed_policy":{"lifecycle":"external"},"approval":{"version":"1.4.0"}})
                    return subprocess.CompletedProcess(args,0,output,"")
                if args[3:5] == ("service", "health"):
                    return subprocess.CompletedProcess(args, 0, "{}", "")
                collection = args[args.index("--collection") + 1]
                command = args[args.index("--collection") + 2:]
                if command[0] == "index":
                    output = json.dumps({"job": {"id": collection}})
                elif command[0] == "init":
                    output = json.dumps({"registration": {"collection": collection, "binding": "scope-" + collection}})
                elif command[:2] == ("tools", "call"):
                    result = {"collection": collection, "items": [1] if collection == "project-a" else []} if command[2] == "airk_symbols" else {"index_state": "available", "collection": collection, "publication": {"collection": collection, "token": "token-" + collection}, "files": 2}
                    if command[2] == "airk_status":
                        status_calls += 1
                        if status_calls == 1:
                            result = {"index_state": "unavailable", "project": "scope-" + collection,
                                "publication": None, "files": None, "chunks": None,
                                "index_error": {"code": "index_worker_unavailable", "message": "unknown cause"},
                                "runtime": {"service_error": None, "service": {"responsive": True, "accepting": True}, "index_worker": {"process": None}},
                                "work": {"basis": "instance_bound_service_metadata", "serving_worker_unavailable": True,
                                         "failed_observations": [], "worker": {"automatic_start_blocked": False, "identity": None}}}
                    output = json.dumps({"content": [{"type": "text", "text": json.dumps(result)}]})
                elif command == ("collections", "remove", "project-b"):
                    self.assertFalse(check)
                    return subprocess.CompletedProcess(args, 2, "", "retired writer")
                else:
                    self.fail(f"Unexpected public CLI command: {command}")
            elif args[:4] == ("exec", "airk-service-check-" + token[:12], "/data/install/airk-launcher", "--version"):
                output = "airk 1.4.0\n"
            return subprocess.CompletedProcess(args, 0, output, "")

        def completed_job(args, **kwargs):
            self.assertEqual(json.loads(kwargs["input"])["job"]["id"][:8], "project-")
            return subprocess.CompletedProcess(args, 0, json.dumps({"job": {"state": "succeeded"}}), "")

        with patch.object(smoke, "docker", side_effect=docker), \
                patch.object(smoke.subprocess, "run", side_effect=completed_job), \
                patch.object(smoke.uuid, "uuid4", return_value=type("Token", (), {"hex": token})()), \
                patch.object(smoke, "check_stdio"), \
                patch("sys.argv", ["check-container.py"]), \
                contextlib.redirect_stdout(io.StringIO()):
            smoke.main()
        self.assertEqual(status_calls, 4)
        self.assertTrue(volume_removed)
        self.assertFalse(registered)


if __name__ == "__main__":
    unittest.main()
