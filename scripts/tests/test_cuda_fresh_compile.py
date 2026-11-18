# SPDX-License-Identifier: Apache-2.0
"""Inert checks for fresh CUDA compile acceptance boundaries."""

import datetime as dt
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch


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



    def test_read_only_compile_verification_audits_all_retained_stage_files(self):
        compile_pin = {"path": str(self.root / "fresh-compile.json")}
        config_pin = {"path": str(self.root / "fresh-configure.json")}
        fields = {"recipe": {}, "toolchain": {}, "configured_graph": {}, "collector_source": {}}
        compiled = {
            **fields, "schema": "airk.private.cuda-fresh-compiled/1",
            "status": "COMPILED_UNQUALIFIED", "qualification": False,
            "configured": config_pin, "build_root": str(self.build),
        }
        guard_paths = Mock(side_effect=[None, ValueError("retained log owner differs")])
        with (
            patch.object(OWNER, "load", return_value=Mock()),
            patch.object(OWNER, "read_pinned", return_value=compiled),
            patch.object(OWNER, "load_chain", return_value=(fields, {}, self.root, {"guards": Mock()})),
            patch.object(OWNER, "output_guard", return_value=guard_paths),
            patch.object(OWNER, "verify_inputs") as verify_inputs,
            self.assertRaisesRegex(ValueError, "retained log owner differs"),
        ):
            OWNER.verify_receipt(compile_pin, 60)
        checked, created, cutoff = guard_paths.call_args.args
        self.assertIs(created, True)
        self.assertEqual(cutoff, 60)
        expected = {
            self.root / "logs" / (stage + suffix)
            for stage in ("configure", "core-session-build", "build")
            for suffix in (".request.json", ".stdout.log", ".stderr.log", ".outer.log",
                           ".started.json", ".tree.json", ".receipt.json")
        }
        self.assertEqual(set(checked), expected)
        self.assertEqual(len(checked), len(expected))
        verify_inputs.assert_not_called()



    def test_compile_audit_batches_cover_every_path_under_one_deadline(self):
        paths = [self.root / str(index) for index in range(408)]
        component = Mock()
        recipe = {"frozen": True}
        OWNER.output_guard(component, recipe)(paths, True, 60)
        calls = component.guard_many.call_args_list
        self.assertEqual(sorted(len(call.args[1]) for call in calls), [24, *([48] * 8)])
        self.assertEqual(sorted(path for call in calls for path in call.args[1]), sorted(paths))
        self.assertTrue(all(call.args[0] is recipe and call.args[2] is True
                            and call.kwargs == {"deadline": 60} for call in calls))

    def test_compile_audit_refusal_stops_before_dispatching_the_next_wave(self):
        paths = [self.root / str(index) for index in range(408)]
        component = Mock()

        def audit(_recipe, batch, _created, *, deadline):
            if paths[0] in batch:
                raise ValueError("guard refused first batch")

        component.guard_many.side_effect = audit
        with self.assertRaisesRegex(ValueError, "guard refused first batch"):
            OWNER.output_guard(component, {})(paths, True, 60)
        self.assertEqual(component.guard_many.call_count, 8)
        self.assertTrue(all(paths[-1] not in call.args[1] for call in component.guard_many.call_args_list))



    def test_timed_out_metadata_batch_splits_without_extending_original_limit(self):
        paths = [self.root / str(index) for index in range(4)]
        component = Mock()
        timeout = OWNER.subprocess.TimeoutExpired(["pwsh"], 10)
        component.guard_many.side_effect = [timeout, None, None]
        with patch.object(OWNER.time, "monotonic", return_value=59), patch.object(OWNER.sys, "stderr"):
            OWNER.output_guard(component, {}, batch_size=4, workers=1)(paths, True, 60)
        calls = component.guard_many.call_args_list
        self.assertEqual([call.args[1] for call in calls], [paths, paths[:2], paths[2:]])
        self.assertTrue(all(call.kwargs == {"deadline": 60} for call in calls))

        component.guard_many.reset_mock(side_effect=True)
        component.guard_many.side_effect = timeout
        with patch.object(OWNER.time, "monotonic", return_value=60), self.assertRaises(type(timeout)):
            OWNER.output_guard(component, {}, batch_size=4, workers=1)(paths, True, 60)
        component.guard_many.assert_called_once_with({}, paths, True, deadline=60)


    def test_fresh_recipe_pins_do_not_require_incremental_response_files(self):
        first = {"path": str(self.root / "input"), "bytes": 7, "sha256": "a" * 64}
        third = {"path": str(self.root / "second"), "bytes": 9, "sha256": "b" * 64}
        inputs = {"native_inputs": [first, first, third]}
        pins = Mock()
        pins.all_pins.side_effect = lambda recipe: iter(recipe["native_inputs"])
        OWNER.verify_frozen_inputs(pins, inputs, 60)
        self.assertEqual([call.args for call in pins.check_pin.call_args_list], [(first, 60), (third, 60)])
        conflict = {"native_inputs": [first, {**first, "sha256": "c" * 64}]}
        with self.assertRaisesRegex(ValueError, "conflicting fresh configure input pins"):
            OWNER.verify_frozen_inputs(pins, conflict, 60)


if __name__ == "__main__":
    unittest.main()
