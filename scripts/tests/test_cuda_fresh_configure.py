# SPDX-License-Identifier: Apache-2.0
"""Inert tests for the fresh configure and process-stage contracts."""

import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

BUILD = Path(__file__).resolve().parents[1] / "cuda-static-runtime/build"


def load(name):
    spec = importlib.util.spec_from_file_location(name, BUILD / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


configure = load("fresh_configure")
process = load("fresh_process")
guards = load("fresh_guard")


class FreshGuard(unittest.TestCase):
    def test_partitions_every_path_with_the_original_cutoff(self):
        paths = list(range(515))
        calls = Mock()
        with patch.object(guards.time, "monotonic", return_value=1):
            guards.audit_batches(paths, True, 9, calls)
        self.assertEqual(sorted(value for call in calls.call_args_list for value in call.args[0]), paths)
        self.assertEqual(sorted(len(call.args[0]) for call in calls.call_args_list), [3, 128, 128, 128, 128])
        self.assertTrue(all(call.args[1:] == (True, 9) for call in calls.call_args_list))

    def test_failure_does_not_dispatch_the_next_group(self):
        def fail_first(paths, created, deadline):
            if paths[0] == 0:
                raise ValueError("metadata refusal")
        calls = Mock(side_effect=fail_first)
        with patch.object(guards.time, "monotonic", return_value=1):
            with self.assertRaisesRegex(ValueError, "metadata refusal"):
                guards.audit_batches(list(range(515)), False, 9, calls)
        self.assertEqual(calls.call_count, 4)


class FreshConfigure(unittest.TestCase):
    def test_pins_configure_to_the_reviewed_compiler_and_existing_cxx20(self):
        r8 = SimpleNamespace(gpu_command=Mock(return_value=["cmake", "-DRETENTION=ON"]))
        tools = {"compiler": {"path": "C:/reviewed/cl.exe"}}
        destination = Path("C:/fresh")
        frontend = destination / "frontend"
        result = configure.configure_command("cpu", r8, destination, tools, frontend)
        self.assertEqual(result, [
            "cmake", "-DRETENTION=ON",
            "-DCMAKE_C_COMPILER=C:/reviewed/cl.exe",
            "-DCMAKE_CXX_COMPILER=C:/reviewed/cl.exe",
            "-DCMAKE_CUDA_HOST_COMPILER=C:/reviewed/cl.exe",
            "-DCMAKE_CXX_STANDARD=20",
        ])
        r8.gpu_command.assert_called_once_with("cpu", destination, tools, frontend)

    def test_requires_observed_cxx20_and_preserves_old_placement_lock(self):
        placement = {"path": "capi.cc", "after_sha256": "old"}
        lock = {"final_placement": placement}
        r8 = SimpleNamespace(configured_build_preflight=Mock(return_value={"graph": "checked"}))
        with patch.object(Path, "read_text", return_value="FLAGS = /std:c++17"):
            with self.assertRaisesRegex(ValueError, "C\\+\\+20"):
                configure.configured_inputs("cpu", r8, Path("C:/fresh"), ["cmake"], lock, {"sha256": "new"})
        self.assertEqual(placement["after_sha256"], "old")
        self.assertEqual(r8.configured_build_preflight.call_args.args[-1]["final_placement"]["after_sha256"], "new")


class FreshProcess(unittest.TestCase):
    def test_guard_refuses_before_any_command_write_or_dispatch(self):
        root = Path(__file__).resolve().parent
        cpu = SimpleNamespace(plain=Mock(), verify_file=Mock(), run_owned=Mock())
        guard = Mock(side_effect=ValueError("guard refusal"))
        with patch.object(Path, "open") as opened, self.assertRaisesRegex(ValueError, "guard refusal"):
            process.run_stage(argv=[str(root / "tool.exe")], cwd=root, root=root,
                              stage="configure", environment={}, cpu=cpu, guard_paths=guard,
                              power_shell={"path": "ps"}, tree_waiter={"path": "wait"}, deadline=100)
        opened.assert_not_called()
        cpu.run_owned.assert_not_called()

    def test_refuses_historical_or_unapproved_working_directory(self):
        root = Path(__file__).resolve().parent / "fresh"
        with self.assertRaisesRegex(ValueError, "within the fresh build"):
            process.run_stage(argv=[str(root / "tool.exe")], cwd=root.parent, root=root,
                              stage="configure", environment={}, cpu=None, guard_paths=None,
                              power_shell={}, tree_waiter={}, deadline=100)
