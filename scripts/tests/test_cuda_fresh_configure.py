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

    def test_requires_msvc_provider_definition_on_both_languages_and_keeps_wx(self):
        define = "-D_SILENCE_CXX20_CISO646_REMOVED_WARNING"
        edges = (
            "\nbuild dir/activations.cc.obj: CXX_COMPILER__onnxruntime_providers_cuda_unscanned_Release a.cc\n"
            f"  DEFINES = {define}\n"
            "  FLAGS = -std:c++20 -WX\n"
            "\nbuild dir/kernel.cu.obj: CUDA_COMPILER__onnxruntime_providers_cuda_llm_unscanned_Release kernel.cu\n"
            f"  DEFINES = -DOTHER {define}\n"
        )
        configure.verify_msvc_cuda_compat(edges)
        with self.assertRaisesRegex(ValueError, "compatibility definition"):
            configure.verify_msvc_cuda_compat(edges.replace(f"-DOTHER {define}", "-DOTHER"))
        with self.assertRaisesRegex(ValueError, "warnings-as-errors"):
            configure.verify_msvc_cuda_compat(edges.replace(" -WX", ""))
        with self.assertRaisesRegex(ValueError, r"C\+\+ or CUDA"):
            configure.verify_msvc_cuda_compat(edges.split("\nbuild dir/kernel", 1)[0])


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
