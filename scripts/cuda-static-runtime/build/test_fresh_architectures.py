# SPDX-License-Identifier: Apache-2.0
"""Pure controls for the exact CUDA architecture producer candidate."""

import datetime as dt
import importlib.util
import json
from pathlib import Path
import unittest

source = Path(__file__).with_name("fresh_architectures.py")
spec = importlib.util.spec_from_file_location("cuda_fresh_architecture_controls", source)
arch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(arch)


def edge(name, *codes):
    flags = " ".join(f"--generate-code=arch=compute_{code},code=[{output}_{code}]"
                     for code, output in codes)
    return f"\nbuild kernel.obj: CUDA_COMPILER__onnxruntime_providers_cuda{name}_unscanned_Release source.cu\n  FLAGS = {flags}\n"


class Candidate(unittest.TestCase):
    def test_current_guarded_owners_are_expected_exact_sources(self):
        config, compiler, recovery = arch.modules()
        self.assertEqual(config.SCHEMA, arch.CANDIDATE_SCHEMA)
        self.assertIs(compiler.load(Path(config.__file__), "configured"), config)
        self.assertIs(recovery.load(Path(compiler.__file__), "recovery"), compiler)

    def test_candidate_recipe_and_failed_audit_argv_are_exact(self):
        _, _, recovery = arch.modules()
        reviewed = {"path": "candidate.py", "sha256": "approved"}
        original = {"schema": arch.CANDIDATE_SCHEMA, "cuda_architectures": "86;89;120",
                    "cuda_architecture_owner": reviewed, "cuda_code_inspector": {"path": str(arch.INSPECTOR)},
                    "tools": {"python": {"path": "python.exe"}}, "wall_seconds": 600}
        self.assertEqual(arch.reviewed_recipe(json.dumps(original), reviewed), original)
        with self.assertRaisesRegex(ValueError, "exact reviewed"):
            arch.reviewed_recipe(json.dumps(dict(original, cuda_architectures="120;89;86")), reviewed)
        started = dt.datetime(2026, 11, 19, tzinfo=dt.timezone.utc)
        origin = {"path": "recipe.json", "sha256": "recipe"}
        arguments = ["-B", "-S", "scripts/cuda-static-runtime/build/fresh_architectures.py",
                     "configure", "--input", "recipe.json", "--input-sha256", "recipe",
                     "--approved-script-sha256", "approved"]
        receipt = {"exit_code": 1, "raw_receipt": None, "input_hash": "recipe", "arguments": arguments,
                   "command": "python.exe", "qualification": False, "human_sid": "human",
                   "started_utc": started.isoformat(), "elapsed_seconds": 3}
        log = (b'guard_paths(sorted((root / "b").rglob("*")), True, deadline) '
               b'subprocess.TimeoutExpired: timed out after 10.0 seconds')
        self.assertEqual(recovery.check_failed_audit(receipt, log, origin, original, "human",
                         dt.datetime.fromisoformat, started + dt.timedelta(seconds=4))[0], started)
        receipt["arguments"] = [*arguments[:-1], "other"]
        with self.assertRaisesRegex(ValueError, "same-launch"):
            recovery.check_failed_audit(receipt, log, origin, original, "human",
                                        dt.datetime.fromisoformat, started + dt.timedelta(seconds=4))

    def test_command_replaces_only_the_exact_original_single_argument(self):
        original = ["cmake.exe", "-DCMAKE_CUDA_ARCHITECTURES=86", "-DOTHER=ON"]
        self.assertEqual(arch.command_with_architectures(original),
                         ["cmake.exe", "-DCMAKE_CUDA_ARCHITECTURES=86;89;120", "-DOTHER=ON"])
        self.assertEqual(original[1], "-DCMAKE_CUDA_ARCHITECTURES=86")
        for wrong in (original + original[1:2], ["-DCMAKE_CUDA_ARCHITECTURES=native"]):
            with self.assertRaisesRegex(ValueError, "single original"):
                arch.command_with_architectures(wrong)

    def test_ninja_checks_general_sass_and_windows_llm_ptx(self):
        common = (("86", "sm"), ("89", "sm"))
        generic = edge("", *common, ("120a", "sm"))
        llm = edge("_llm", *common, ("120", "compute"))
        facts = arch.ninja_architectures(generic + llm)
        self.assertEqual(facts["generic_sass"], ["120a", "86", "89"])
        self.assertEqual(facts["windows_llm_ptx"], ["120"])
        with self.assertRaisesRegex(ValueError, "general CUDA provider"):
            arch.ninja_architectures(edge("", *common) + llm)
        with self.assertRaisesRegex(ValueError, "Windows LLM"):
            arch.ninja_architectures(generic + edge("_llm", *common, ("120a", "sm")))

    def test_actual_code_inventory_rejects_missing_or_extra_targets(self):
        elf = "\n".join(f"ELF file  {index}: provider.{index}.sm_{code}.cubin"
                        for index, code in enumerate(("86", "89", "120a", "90a"), 1))
        ptx = "PTX file    1: provider.1.sm_120.ptx"
        self.assertEqual(arch.code_inventory(elf, ptx)["ptx_objects"], {"120": 1})
        with self.assertRaisesRegex(ValueError, "missing required"):
            arch.code_inventory(elf.replace("120a", "100a"), ptx)
        with self.assertRaisesRegex(ValueError, "missing required"):
            arch.code_inventory(elf, "No PTX file found")
        with self.assertRaisesRegex(ValueError, "unrecognized"):
            arch.code_inventory(elf, "PTX file    1: provider.1.special.ptx")


if __name__ == "__main__":
    unittest.main()
