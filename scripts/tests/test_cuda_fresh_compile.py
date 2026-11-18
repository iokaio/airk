# SPDX-License-Identifier: Apache-2.0
"""Inert checks for fresh CUDA compile acceptance boundaries."""

import datetime as dt
import importlib.util
from pathlib import Path
import sys
import unittest


sys.dont_write_bytecode = True
SCRIPT = Path(__file__).resolve().parents[1] / "cuda-static-runtime/build/fresh_compile.py"
SPEC = importlib.util.spec_from_file_location("test_cuda_fresh_compile_owner", SCRIPT)
OWNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNER)


class FreshCompileTests(unittest.TestCase):
    def setUp(self):
        self.root = Path("C:/test/fresh")
        self.build = self.root / "b/Release"
        self.libraries = [self.build / "onnxruntime_session.lib", self.build / "_deps/re2/re2.lib"]
        self.providers = [
            self.build / "onnxruntime_providers_shared.dll",
            self.build / "onnxruntime_providers_cuda.dll",
        ]
        self.configured = {
            "started_utc": "2026-11-18T22:00:00+00:00",
            "finished_utc": "2026-11-18T22:03:10+00:00",
            "elapsed_seconds": 190,
            "configure": {"original_deadline_monotonic": 9600},
        }

    def test_actual_commands_are_the_two_existing_target_stages(self):
        cmake = "C:/tools/bin/cmake.exe"
        common = [cmake, "--build", str(self.build), "--config", "Release", "--target"]
        self.assertEqual(OWNER.build_commands(self.root, cmake), [
            ("core-session-build", [*common, "onnxruntime_session", "--parallel", "2"]),
            ("build", [*common, "all", "re2", "--parallel", "2"]),
        ])

    def test_compile_deadline_intersects_utc_remainder_with_original_counter(self):
        now = dt.datetime.fromisoformat("2026-11-18T22:08:00+00:00")
        recipe = {"wall_seconds": 600}
        self.assertEqual(OWNER.build_deadline(self.configured, recipe, now_utc=now, now_monotonic=9400), 9520)
        self.assertEqual(OWNER.build_deadline(self.configured, recipe, now_utc=now, now_monotonic=9550), 9600)

    def test_compiler_cannot_restart_either_original_clock(self):
        now = dt.datetime.fromisoformat("2026-11-18T22:10:01+00:00")
        earlier = dt.datetime.fromisoformat("2026-11-18T22:05:00+00:00")
        with self.assertRaisesRegex(ValueError, "original fresh-build deadline exhausted"):
            OWNER.build_deadline(self.configured, {"wall_seconds": 600}, now_utc=now, now_monotonic=9200)
        with self.assertRaisesRegex(ValueError, "original fresh-build deadline exhausted"):
            OWNER.build_deadline(self.configured, {"wall_seconds": 600}, now_utc=earlier, now_monotonic=9600)

    def test_campaign_end_shortens_the_configure_origin_window(self):
        self.configured["started_utc"] = "2026-11-19T05:25:42+00:00"
        self.configured["finished_utc"] = "2026-11-19T05:28:52+00:00"
        now = dt.datetime.fromisoformat("2026-11-19T05:29:32+00:00")
        self.assertEqual(OWNER.build_deadline(self.configured, {"wall_seconds": 600},
                         now_utc=now, now_monotonic=9500), 9510)

    def test_same_build_output_roster(self):
        libraries, providers = OWNER.compilation_outputs(self.root, [*self.providers[::-1], *self.libraries])
        self.assertEqual(libraries, sorted(self.libraries))
        self.assertEqual(providers, self.providers)
        for duplicate in (self.build / "copies/ONNXRUNTIME_PROVIDERS_SHARED.DLL",):
            with self.assertRaisesRegex(ValueError, "same-build provider output"):
                OWNER.compilation_outputs(self.root, [*self.libraries, *self.providers, duplicate])
        with self.assertRaisesRegex(ValueError, "same-build provider output"):
            OWNER.compilation_outputs(self.root, [*self.libraries, self.providers[0]])
        with self.assertRaisesRegex(ValueError, "session archive missing"):
            OWNER.compilation_outputs(self.root, self.providers)

    def test_any_second_ort_core_is_rejected(self):
        for name in ("ONNXRUNTIME.DLL", "onnxruntime.lib"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "second ORT core"):
                OWNER.compilation_outputs(self.root, [*self.libraries, *self.providers, self.build / "copy" / name])

    def test_compilation_state_has_no_warm_ninja_or_objects(self):
        OWNER.require_clean_build(self.root, [
            self.build / "build.ninja",
            self.build / "CMakeFiles/3.31.6/CompilerIdCXX/CMakeCXXCompilerId.obj",
            self.build / "CMakeFiles/3.31.6/CompilerIdCUDA/tmp/a_dlink.obj",
            self.build / "CMakeFiles/ShowIncludes/main.obj",
            self.build / "_deps/re2-subbuild/.ninja_log",
        ])
        for path in (self.build / ".ninja_log", self.build / ".ninja_deps",
                     self.build / "_deps/re2-build/.ninja_log",
                     self.build / "_deps/re2-subbuild/.ninja_deps",
                     self.build / "CMakeFiles/onnxruntime_session.dir/onnxruntime_c_api.cc.obj"):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "previous compilation output"):
                OWNER.require_clean_build(self.root, [path])


if __name__ == "__main__":
    unittest.main()
