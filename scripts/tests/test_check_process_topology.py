# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import sys
import unittest

SPEC = importlib.util.spec_from_file_location(
    "airk_check_process_topology", Path(__file__).parents[1] / "check-process-topology.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
# Dataclasses resolve their module while the module is being executed.
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def proc(pid, *args, image="C:/AIRK/runtime/airk.exe", parent=1):
    return MODULE.classify(MODULE.Process(pid, parent, image, tuple(args)))


class TopologyTests(unittest.TestCase):
    def test_roles_are_classified_from_their_command_words(self):
        cases = {
            ("serve", "--runtime-dir", "x"): "service",
            ("runtime", "managed-serve", "--install-dir", "x"): "service",
            ("--service-install-dir", "C:/AIRK", "runtime", "login-serve"): "service",
            ("service", "session"): "session",
            ("__airk_pdf_worker",): "pdf",
            ("runtime", "self-test"): "self-test",
            ("tools", "shell"): "tools-shell",
            ("runtime", "location"): "one-shot",
            ("service", "console"): "one-shot",
            ("project-worker",): "forbidden",
            ("derived-worker",): "forbidden",
            ("source-worker",): "forbidden",
            ("process-guardian",): "forbidden",
            ("mcp", "--root", "x"): "forbidden",
        }
        for args, role in cases.items():
            with self.subTest(args=args):
                self.assertEqual(proc(10, *args).role, role)
        alias = proc(11, "serve", image="C:/AIRK/runtime/airk-service.exe")
        self.assertEqual((alias.role, bool(alias.problem)), ("alias", True))

    def test_the_allowed_topology_passes(self):
        processes = [
            proc(1, "runtime", "managed-serve"),
            proc(2, "service", "session"),
            proc(3, "service", "session"),
            proc(4, "__airk_pdf_worker"),
        ]
        self.assertEqual(
            MODULE.evaluate(processes, services=1, max_sessions=2, allow_one_shots=False), [])

    def test_every_violation_is_reported(self):
        processes = [
            proc(1, "serve"),
            proc(2, "serve"),
            proc(3, "service", "session"),
            proc(4, "__airk_pdf_worker"),
            proc(5, "__airk_pdf_worker"),
            proc(6, "project-worker"),
            proc(7, "runtime", "location"),
        ]
        failures = MODULE.evaluate(processes, services=1, max_sessions=0, allow_one_shots=False)
        joined = "\n".join(failures)
        for expected in ("worker role process", "expected 1 service", "at most 0 window",
                         "more than one PDF", "one-shot(s) still running"):
            self.assertIn(expected, joined)
        self.assertEqual(
            MODULE.evaluate([proc(1, "serve")], services=1, max_sessions=0,
                            allow_one_shots=False), [])
        self.assertIn("found 0",
                      MODULE.evaluate([], services=1, max_sessions=0, allow_one_shots=True)[0])

    def test_a_plain_environment_drops_editor_and_airk_variables(self):
        import os
        saved = dict(os.environ)
        try:
            os.environ.update({"VSCODE_PID": "1", "AIRK_CLIENT": "x", "KEEP_ME": "1"})
            env = MODULE.plain_environment()
            self.assertNotIn("VSCODE_PID", env)
            self.assertNotIn("AIRK_CLIENT", env)
            self.assertEqual(env["KEEP_ME"], "1")
        finally:
            os.environ.clear()
            os.environ.update(saved)


if __name__ == "__main__":
    unittest.main()
