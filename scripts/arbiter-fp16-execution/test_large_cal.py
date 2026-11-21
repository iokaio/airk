# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the Large CAL build launcher, image stager, phase launcher and panel assembler."""
import copy
import hashlib
import json
from pathlib import Path
import re
import runpy
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
BUILD = runpy.run_path(str(HERE / "large_cal_build.py"))
STAGE = runpy.run_path(str(HERE / "stage_large_cal_image.py"))
PROBE = runpy.run_path(str(HERE / "probe_large_cal.py"))
PANEL = runpy.run_path(str(HERE / "large_cal_panel.py"))

# Reviewed identities, written out here so the checks do not read them back from the code under test.
TYPED = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"
GRAPH = "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4"
WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
AUDIT = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
SERVICE = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
PLAN = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c"
BATCH = "fac3dd246c91ede7fef576baea1e65ff2971e26f229159878e1993ac068fe6bf"
PROJECTION = "e1fd8a2b0f2f064a0c32ef361f8bd085c58120f7eaf5ab1758a80f9bea2ea845"
COMPILER = "e990a1b8346b5cd0fcd68ffd97b0c1eee776094641bd41ef864af547971690cb"
R7_COMPILER = "6987a9beaa1656b40c18b58bc1f320a6de112bfd6b69da893eb4f498eb014f8e"
R7_SEAL = "e64c46617bc4a5ee83271a9adc4e85b4f391568fb79d2a43e118c01325ccb7ec"
LARGE_MODEL = "arbiter-context-qwen3-4b"
LARGE_MANIFEST = "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
COMPACT_MODEL = "arbiter-context-qwen3-17b"
COMPACT_MANIFEST = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
            "fp16_large::cal::large_direct_original_cal_phase")
COMPILER_SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
                     "fp16_compact::cal::chronological_original_context_compiler_batch")
GIB22 = 23_622_320_128
GIB44 = 47_244_640_256
QUALIFICATION = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification"
CUTLASS = "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass"
CAL_SOURCES = (QUALIFICATION + "/fp16_large/cal.rs", CUTLASS + "/large/cal.rs", QUALIFICATION + "/fp16_compact/cal.rs")
SCRATCH = ("flash=0, lean=0, efficient=1, fused_runner=0, fused_cross=0, bias=0, "
           "attn_bias=0, mask_dims=0, has_qkv_workspace=0, workspace=0, past=0, present=0")
SETTLED = {"exit_code": 0, "child_assigned_before_resume": True, "timed_out": False, "log_oversized": False,
           "owned_job_forced_termination": False, "owned_descendants_settled": True,
           "owned_job_final_active_processes": 0, "owned_job_final_member_pids": []}


def sha(label):
    return hashlib.sha256(label.encode()).hexdigest()


def population(capture_path=lambda n: f"record-{n:04}.json"):
    ids = [f"q{n}" for n in range(100)]
    rows = [{"ordinal": n, "id": key, "status": "eligible",
             "captures": [{"path": str(capture_path(n)), "bytes": 10, "sha256": sha(f"capture-{n}")}],
             "compiled": {"input_ids": [n, n + 1, n + 2], "option_mask": [True, True, True],
                          "input_digest": sha(f"large-input-{n}")},
             "roster": {"cards": ["read", "sufficient", "clarify"]},
             "remaining_ms": 1000 + n, "remaining_input_tokens": 4096} for n, key in enumerate(ids)]
    batch = {"model": LARGE_MODEL, "original_manifest": LARGE_MANIFEST, "phase_plan_sha256": PLAN,
             "launch_audit_sha256": AUDIT, "NN_invocations": 0, "original_requests": 100, "rows": rows,
             "phases": [{"path": f"phase-{n:02}", "sha256": sha(f"phase-{n}")} for n in range(13)],
             "statuses": {"eligible": 100}}
    return ids, rows, batch


class BuildLauncher(unittest.TestCase):
    def test_disk_guard_uses_the_approved_budget_and_floor(self):
        # 1,233,246,265,344 baseline - 195,421,011,968 budget; the 107,374,182,400 floor is lower.
        self.assertEqual(BUILD["disk_floor"](), 1_037_825_253_376)
        BUILD["require_disk"](1_037_825_253_376 + 536_870_912)
        BUILD["require_disk"](1_097_085_763_584)
        for free in (1_037_825_253_376 + 536_870_911, 107_374_182_400 + 536_870_912, 1_097_085_763_584.0):
            with self.subTest(free=free), self.assertRaisesRegex(ValueError, "approved campaign budget floor"):
                BUILD["require_disk"](free)

    def test_no_cutoff_keeps_the_phase_limit_and_a_run_cutoff_only_shortens_it(self):
        parse, wall = BUILD["parse_not_after"], BUILD["wall_seconds"]
        self.assertIsNone(parse(None))
        self.assertEqual(parse("2026-11-20T17:10:00Z"), 1_795_194_600)
        self.assertEqual(wall(None, 1_795_194_600 + 86_400), 1800)
        self.assertEqual(wall(1_795_194_600, 1_795_194_600 - 7200), 1800)
        self.assertEqual(wall(1_795_194_600, 1_795_194_600 - 600), 600)
        for now in (1_795_194_600 - 119, 1_795_194_600 + 1):
            with self.subTest(now=now), self.assertRaisesRegex(ValueError, "insufficient native time"):
                wall(1_795_194_600, now)
        for text in ("2026-11-20 17:10:00", "1795194600", "2026-11-20T17:10:00+00:00", ""):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "--not-after takes UTC"):
                parse(text)

    def test_nextest_roster_gains_the_cal_regressions_and_other_phases_are_untouched(self):
        original = {"phase_argv": lambda phase, directory: ["runner", "--", "cargo", phase, "-E", "test(base)"]}
        self.assertEqual(
            BUILD["phase_argv"](original, "nextest", Path("checks"))[-1],
            "test(base)"
            " | test(large_direct_cal_keeps_original_final_phase_and_refuses_foreign_rows)"
            " | test(large_cal_refuses_more_than_one_phase_even_if_collector_ignores_error)"
            " | test(chronological_cal_rejects_foreign_audit_before_opening_file)"
            " | test(compact_cal_population_rejects_reordered_and_duplicate_ordinals)")
        for phase in ("clippy", "image"):
            self.assertEqual(BUILD["phase_argv"](original, phase, Path("checks")),
                             ["runner", "--", "cargo", phase, "-E", "test(base)"])
        with self.assertRaisesRegex(ValueError, "already in the original roster"):
            BUILD["cal_filter"]("test(large_cal_refuses_more_than_one_phase_even_if_collector_ignores_error)")

    def test_junit_must_show_every_required_regression_passing_once(self):
        prefix = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_large::cal::"
        case = '<testcase name="{}" classname="airk-inference" time="0.01"/>'
        names = ("large_direct_cal_keeps_original_final_phase_and_refuses_foreign_rows", "other_regression")
        def report(cases, failures="0"):
            return (f'<?xml version="1.0" encoding="UTF-8"?><testsuites name="nextest-run" tests="{len(cases)}" '
                    f'failures="{failures}" errors="0"><testsuite name="airk-inference">' + "".join(cases)
                    + "</testsuite></testsuites>")
        passing = [case.format(prefix + names[0]), case.format("arbiter::tests::other_regression"),
                   case.format("arbiter::tests::unrelated")]
        self.assertEqual(BUILD["confirm_regressions"](report(passing), names), 3)
        failed = ('<testcase name="arbiter::tests::other_regression" classname="airk-retrieval">'
                  '<failure type="test failure">boom</failure></testcase>')
        for cases, failures in ((passing[:1] + passing[2:], "0"), (passing + passing[:1], "0"),
                                ([passing[0], failed], "0"), (passing, "1")):
            with self.subTest(cases=len(cases), failures=failures), self.assertRaises(ValueError):
                BUILD["confirm_regressions"](report(cases, failures), names)
        # A name that merely ends with the required name is a different test.
        with self.assertRaisesRegex(ValueError, "did not run and pass exactly once: other_regression"):
            BUILD["confirm_regressions"](report([passing[0], case.format("arbiter::tests::an_other_regression")]), names)

    def test_successor_pins_the_live_original_and_its_nested_job_owner(self):
        self.assertEqual(hashlib.sha256((HERE / "large_scientific_build.py").read_bytes()).hexdigest(),
                         BUILD["ORIGINAL_SHA"])
        cap = HERE.parent / "arbiter/analysis/public_source_admission/aws_normal_service.py"
        self.assertEqual(hashlib.sha256(cap.read_bytes()).hexdigest(), BUILD["CAP_OWNER_SHA"])
        original = BUILD["load_original"]()
        self.assertEqual(original["HOST"], 4_294_967_296)
        self.assertEqual(sorted(original["TEMPLATES"]), ["clippy", "image", "nextest"])
        self.assertEqual(BUILD["PREFIX"], "windows-large-direct-cal-checks-")
        self.assertEqual((BUILD["PHASE_SECONDS"], BUILD["MINIMUM_SECONDS"]), (1800, 120))


class Stager(unittest.TestCase):
    def receipt(self):
        files = {key: sha(key) for key in STAGE["SNAPSHOT_KEYS"]}
        return {"schema": "airk-readiness-command/1", "passed": True, "inputs_unchanged": True, "exit_code": 0,
                "command": ["cargo.exe", "test", "-p", "airk-retrieval", "-p", "airk-inference", "--all-features", "--lib",
                            "--no-run", "--locked", "--offline", "--timings", "--message-format=json"],
                "target_directory": str(STAGE["ROOT"] / "target"), "groups": {"native": {"files": files}}}

    def test_roster_is_the_pressure_snapshot_plus_the_three_cal_sources(self):
        keys = STAGE["SNAPSHOT_KEYS"]
        self.assertEqual((len(keys), len(set(keys))), (24, 24))
        self.assertEqual(keys[21:], CAL_SOURCES)
        consumers = {
            # probe_compact.linked_image "metadata" roster
            QUALIFICATION + ".rs", QUALIFICATION + "/fp16_compact.rs", QUALIFICATION + "/CONTRACT.md",
            "crates/airk-retrieval/src/arbiter/runtime.rs", "crates/airk-retrieval/src/arbiter/runtime/cuda.rs",
            "crates/airk-retrieval/src/arbiter/runtime/cuda/fp16.rs",
            "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic.rs",
            # static seal, operand and graph probes
            CUTLASS + ".rs", QUALIFICATION + "/fp16_compact/full.rs", CUTLASS + "/large.rs", CUTLASS + "/model.rs",
            CUTLASS + "/large_routes.rs", QUALIFICATION + "/fp16_large.rs", QUALIFICATION + "/fp16_large/compensated.rs",
            # pressure and timing probes that may move to this image
            CUTLASS + "/model/pressure.rs", QUALIFICATION + "/fp16_compact/timing.rs"}
        self.assertLessEqual(consumers, set(keys))
        self.assertLessEqual({*PROBE["NATIVE_KEYS"], *PROBE["SOURCE_KEYS"]}, set(keys))
        self.assertEqual(PROBE["SOURCE_KEYS"], CAL_SOURCES)

    def test_only_a_passed_locked_image_build_with_every_roster_source_is_staged(self):
        receipt = self.receipt()
        self.assertEqual(STAGE["image_build"](receipt), {key: sha(key) for key in STAGE["SNAPSHOT_KEYS"]})
        changes = [{"inputs_unchanged": False}, {"passed": False}, {"exit_code": 1},
                   {"command": [word for word in receipt["command"] if word != "--locked"]},
                   {"command": [word for word in receipt["command"] if word != "--no-run"]},
                   {"target_directory": str(STAGE["ROOT"] / "artifacts/other-target")}]
        for change in changes:
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "locked Cargo JSON image build"):
                STAGE["image_build"](receipt | change)
        older = copy.deepcopy(receipt)
        del older["groups"]["native"]["files"][CAL_SOURCES[0]]
        with self.assertRaisesRegex(ValueError, "lacks retained source digests: .*fp16_large/cal.rs"):
            STAGE["image_build"](older)

    def test_cargo_log_must_name_exactly_one_inference_test_image(self):
        deps = STAGE["ROOT"] / "target/debug/deps"
        def artifact(name, test, executable):
            return json.dumps({"reason": "compiler-artifact", "target": {"name": name}, "profile": {"test": test},
                               "executable": executable})
        image = artifact("airk_inference", True, str(deps / "airk_inference-0123456789abcdef.exe"))
        noise = ["   Compiling airk-inference", artifact("airk_retrieval", True, str(deps / "airk_retrieval-1.exe")),
                 artifact("airk_inference", False, None), json.dumps({"reason": "build-finished", "success": True})]
        self.assertEqual(STAGE["cargo_test_image"]([*noise, image + "\n"]),
                         deps / "airk_inference-0123456789abcdef.exe")
        elsewhere = artifact("airk_inference", True, str(STAGE["ROOT"] / "artifacts/p10-target/debug/deps/airk_inference-1.exe"))
        for lines in (noise, [image, image], [elsewhere]):
            with self.subTest(lines=len(lines)), self.assertRaises(ValueError):
                STAGE["cargo_test_image"](lines)

    def test_snapshot_writes_built_bytes_under_digest_names_and_refuses_drift(self):
        digest = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory() as scratch:
            root, directory = Path(scratch) / "repo", Path(scratch) / "retained-native-sources"
            bodies = {"crates/a/src/one.rs": b"fn one() {}\n", "crates/b/src/two.rs": b"fn two() {}\n"}
            for key, body in bodies.items():
                (root / key).parent.mkdir(parents=True)
                (root / key).write_bytes(body)
            directory.mkdir()
            digests = {key: hashlib.sha256(body).hexdigest() for key, body in bodies.items()}
            audits = []
            retained = STAGE["snapshot"](digests, root, directory, digest, lambda paths, created=True: audits.append((list(paths), created)))
            self.assertEqual({key: path.name for key, path in retained.items()},
                             {key: value + ".source" for key, value in digests.items()})
            self.assertEqual({path.read_bytes() for path in retained.values()}, set(bodies.values()))
            self.assertEqual([created for _, created in audits], [False, True])
            self.assertEqual(sorted(audits[0][0]), sorted(retained.values()))
            # A second pass keeps equal copies and has nothing new to create.
            audits.clear()
            STAGE["snapshot"](digests, root, directory, digest, lambda paths, created=True: audits.append((list(paths), created)))
            self.assertEqual([created for _, created in audits], [True])
            first = retained["crates/a/src/one.rs"]
            first.write_bytes(b"tampered\n")
            with self.assertRaisesRegex(ValueError, "existing retained copy differs"):
                STAGE["snapshot"](digests, root, directory, digest, lambda paths, created=True: None)
            first.unlink()
            (root / "crates/a/src/one.rs").write_bytes(b"fn one() { edited_after_the_build(); }\n")
            with self.assertRaisesRegex(ValueError, "live source differs from the built source.*one.rs"):
                STAGE["snapshot"](digests, root, directory, digest, lambda paths, created=True: None)
            self.assertFalse(first.exists())

    def test_fresh_names_for_the_staged_image_and_stage_receipt(self):
        self.assertIsNotNone(re.fullmatch(STAGE["STAGED_NAME"], "fresh-cu15-large-direct-cal-r1-inference.exe"))
        self.assertIsNotNone(re.fullmatch(STAGE["OUTPUT_NAME"], "windows-large-direct-cal-stage-r12"))
        for name in ("fresh-cu15-large-direct-pressure-r1-inference.exe", "fresh-cu15-large-direct-cal-r0-inference.exe",
                     "fresh-cu15-large-direct-cal-r1-inference.exe.bak"):
            self.assertIsNone(re.fullmatch(STAGE["STAGED_NAME"], name))


class PhaseFixture:
    """One synthetic phase 12: ordinals 96, 98 and 99 are eligible and 97 is a retained refusal."""
    IMAGE = "ab" * 32
    EXPIRY = 900_000_000_000
    PID = 4242

    def __init__(self):
        _, rows, _ = population()
        self.rows = rows[96:]
        self.rows[1] = {"ordinal": 97, "id": "q97", "status": "no_native_assessment_request", "captures": []}
        self.env = {"AIRK_CUDA_CAL_PHASE": "12", "AIRK_CUDA_CAL_LAUNCH_AUDIT": "D:/c/audit.json",
                    "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT, "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": "D:/c/large.json",
                    "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": BATCH,
                    "AIRK_LARGE_CUTLASS_EXPIRES_NS": str(self.EXPIRY), "AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA": self.IMAGE}
        observed, self.events = [], []
        clock = 100_000_000_000
        for row in self.rows:
            if row["status"] != "eligible":
                observed.append({"ordinal": row["ordinal"], "id": row["id"], "status": row["status"]})
                continue
            observed.append({"ordinal": row["ordinal"], "id": row["id"], "status": "raw_scores_only",
                             "input_digest": row["compiled"]["input_digest"], "captures": row["captures"],
                             "compiled": row["compiled"], "roster": row["roster"], "scores": [1.5, -0.25, 0.125],
                             "remaining_ms": row["remaining_ms"], "remaining_input_tokens": 4096,
                             "compile_us": 900, "inference_us": 410_500, "native_call_elapsed_us": 410_000})
            common = {"schema": "airk.arbiter.native-operation/1", "kind": "numeric", "ordinal": 1 + row["ordinal"] % 8,
                      "identity": {"input_sha256": BATCH, "profile": TYPED, "process_id": self.PID,
                                   "native_binary_sha256": self.IMAGE},
                      "operation_input_digest": row["compiled"]["input_digest"],
                      "clock": "windows_performance_counter", "started_monotonic_ns": clock,
                      "expires_monotonic_ns": self.EXPIRY}
            self.events.append(common | {"sequence": len(self.events) + 1, "phase": "started",
                                         "observed_monotonic_ns": clock, "native_call_elapsed_us": None,
                                         "native_success": None})
            self.events.append(common | {"sequence": len(self.events) + 1, "phase": "returned",
                                         "observed_monotonic_ns": clock + 410_000_000,
                                         "native_call_elapsed_us": 410_000, "native_success": True})
            clock += 500_000_000
        self.payload = {
            "job": {"process_memory_limit_bytes": GIB22, "job_memory_limit_bytes": GIB22, "flags": 8968},
            "native": {"result": {"typed_graph_sha256": TYPED, "graph": None, "score": None, "timing": None,
                                  "route_refusal": None, "cpu_refusal": None, "model_sha256": GRAPH,
                                  "weight_sha256": WEIGHTS,
                                  "cal": {"native_forwards": 3, "observation": {
                                      "phase": 12, "qualification": False, "calibration_fitted": False,
                                      "original_projection": {"path": "D:/c/large.json", "sha256": BATCH},
                                      "launch_audit": {"path": "D:/c/audit.json", "sha256": AUDIT},
                                      "rows": observed}}},
                       "adapter": {"ordinal": 0}, "placement": {"complete": True},
                       "before": {"free_bytes": 49_886_003_200}, "live": {}, "released": {},
                       "pressure": None, "qualification": False}}

    @staticmethod
    def stdout(payload, forwards, *, layers=36, markers=1, verdict="test result: ok. 1 passed; 0 failed; 0 ignored"):
        lines = ["", "running 1 test", "AttentionKernelOptions: FLASH_ATTENTION=0 EFFICIENT_ATTENTION=1"]
        for _ in range(forwards):
            for layer in range(layers):
                lines.append(f"\x1b[36mOperator=MultiHeadAttention Node=/airk_large_cutlass/{layer}/efficient "
                             "DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION\x1b[0m")
                lines.append(SCRATCH)
        lines += ["AIRK_LARGE_COMPENSATED=" + json.dumps(payload)] * markers
        return "\n".join([*lines, verdict, ""]).encode()


class PhaseLauncher(unittest.TestCase):
    def test_phase_intervals_selector_and_output_name(self):
        span = PROBE["phase_span"]
        self.assertEqual([span(0), span(11), span(12)], [(0, 8), (88, 96), (96, 100)])
        for phase in (13, -1, True, "12"):
            with self.subTest(phase=phase), self.assertRaisesRegex(ValueError, "original R8 CAL phase required"):
                span(phase)
        image = Path("D:/stage/fresh-cu15-large-direct-cal-r1-inference.exe")
        self.assertEqual(PROBE["command"](image), [str(image), "--exact", SELECTOR, "--ignored", "--nocapture"])
        name = re.fullmatch(PROBE["OUTPUT_NAME"], "large-direct-cal-r8-full100-phase07-r2")
        self.assertEqual(name.group(1), "07")
        self.assertIsNone(re.fullmatch(PROBE["OUTPUT_NAME"], "compact-cutlass-cal-r8-full100-phase07-r1"))
        self.assertEqual((PROBE["HOST"], PROBE["SECONDS"], PROBE["DEVICE_FREE"], PROBE["LAYERS"]), (GIB22, 120, GIB44, 36))

    def test_native_environment_is_exact_and_drops_inherited_grants(self):
        inherited = {"SystemRoot": r"C:\Windows", "USERNAME": "tester", "PATH": "X", "TEMP": "old",
                     "AIRK_LARGE_PRESSURE_TIER_GIB": "24", "airk_large_pressure_nonce": "n",
                     "AIRK_LARGE_CUTLASS_TYPED_GRAPH": TYPED, "ORT_CUDA_GEMM_OPTIONS": "0",
                     "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "0", "PYTHONPATH": "p", "CUDA_VISIBLE_DEVICES": "",
                     "CUDNN_LOGINFO_DBG": "1"}
        settings = {"AIRK_CUDA_CAL_PHASE": "12", "AIRK_CUDA_CAL_LAUNCH_AUDIT": "D:/c/audit.json",
                    "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT, "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": "D:/c/large.json",
                    "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": BATCH}
        env = PROBE["environment"](inherited, Path("T"), "ab" * 32, 123_456_789, settings)
        self.assertEqual(env, {
            "SystemRoot": r"C:\Windows", "USERNAME": "tester", "PATH": r"C:\Windows\System32;C:\Windows",
            "TEMP": "T", "TMP": "T", "TMPDIR": "T", "AIRK_PRIVATE_CENSUS_BASE": "T", "CUDA_CACHE_PATH": "T",
            "ORT_CUDA_GEMM_OPTIONS": "2", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "1",
            "AIRK_LARGE_CUTLASS_EXPIRES_NS": "123456789", "AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA": "ab" * 32,
            "AIRK_CUDA_CAL_PHASE": "12", "AIRK_CUDA_CAL_LAUNCH_AUDIT": "D:/c/audit.json",
            "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT, "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": "D:/c/large.json",
            "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": BATCH})
        refused = [settings | {"AIRK_LARGE_PRESSURE_TIER_GIB": "24"}, settings | {"AIRK_LARGE_PRESSURE_NONCE": "n"},
                   settings | {"AIRK_LARGE_CUTLASS_TYPED_GRAPH": TYPED}, settings | {"AIRK_LARGE_CUTLASS_WIDTH": "8192"},
                   settings | {"AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": PROJECTION},
                   settings | {"AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": BATCH}, settings | {"AIRK_CUDA_CAL_PHASE": "13"},
                   {key: value for key, value in settings.items() if key != "AIRK_CUDA_CAL_PHASE"}]
        for changed in refused:
            with self.subTest(changed=sorted(changed)), self.assertRaisesRegex(ValueError, "native environment differs"):
                PROBE["environment"](inherited, Path("T"), "ab" * 32, 123_456_789, changed)
        with self.assertRaisesRegex(ValueError, "native environment differs"):
            PROBE["environment"](inherited, Path("T"), "not-a-digest", 123_456_789, settings)

    def test_device_needs_44_gib_free_before_the_job_exists(self):
        check = PROBE["device_precondition"]
        self.assertEqual(check({"nvidia_smi_mib": {"memory.free": 45_056}})["observed_free_bytes"], GIB44)
        self.assertEqual(check({"nvidia_smi_mib": {"memory.free": 47_450}}),
                         {"required_free_bytes": GIB44, "observed_free_bytes": 49_754_931_200,
                          "observation": "nvidia-smi memory.free before load"})
        for free in (45_055, 24_576, "47450"):
            with self.subTest(free=free), self.assertRaisesRegex(ValueError, "less than 44 GiB free"):
                check({"nvidia_smi_mib": {"memory.free": free}})

    def test_scorer_image_must_carry_the_large_cal_selector_sources(self):
        files = {key: sha(key) for key in (*PROBE["NATIVE_KEYS"], *CAL_SOURCES)}
        PROBE["scorer_sources"](files)
        for absent in CAL_SOURCES:
            older = {key: value for key, value in files.items() if key != absent}
            with self.subTest(absent=absent), self.assertRaisesRegex(ValueError, "lacks Large CAL selector sources"):
                PROBE["scorer_sources"](older)

    def test_population_must_be_the_complete_chronological_large_batch(self):
        ids, rows, batch = population()
        checked, phases = PROBE["validate_population"](batch, ids, PLAN)
        self.assertEqual((len(checked), len(phases), checked[99]["ordinal"]), (100, 13, 99))
        swapped = rows[:2][::-1] + rows[2:]
        unbacked = copy.deepcopy(rows)
        unbacked[7]["captures"] = []
        changes = [{"model": COMPACT_MODEL}, {"original_manifest": COMPACT_MANIFEST}, {"phase_plan_sha256": AUDIT},
                   {"launch_audit_sha256": PLAN}, {"NN_invocations": 1}, {"rows": swapped}, {"rows": rows[:99]},
                   {"phases": batch["phases"][:12]}, {"statuses": {"eligible": 99}}]
        for change in changes:
            with self.subTest(change=sorted(change)), self.assertRaisesRegex(ValueError, "chronological R8 Large population"):
                PROBE["validate_population"](batch | change, ids, PLAN)
        with self.assertRaisesRegex(ValueError, "chronological R8 Large population"):
            PROBE["validate_population"](batch | {"phase_plan_sha256": AUDIT}, ids, AUDIT)
        with self.assertRaisesRegex(ValueError, "invalid R8 request or refusal shapes"):
            PROBE["validate_population"](batch | {"rows": unbacked}, ids, PLAN)

    def capture(self, row):
        return {"schema": "airk.private-selected-Context-native-admission-observation/1", "Context_NN_dispatch": False,
                "Focus_configuration": "ON", "actual_Focus_profile": "focus", "MiniLM_pipeline": "minilm",
                "actual_Context_profile": {"manifest": COMPACT_MANIFEST},
                "request": {"scope": {"publication": {"token": "r8"}}, "roster": row["roster"],
                            "remaining_ms": row["remaining_ms"] + 50, "remaining_input_tokens": 4096},
                "remaining_ns_at_observation": row["remaining_ms"] * 1_000_000 + 999_999,
                "compiler_census": [
                    {"model": LARGE_MODEL, "profile": LARGE_MANIFEST, "compiled": row["compiled"],
                     "fits_remaining_allowance": True, "NN_invocations": 0},
                    {"model": COMPACT_MODEL, "profile": COMPACT_MANIFEST, "compiled": {"input_digest": sha("compact")},
                     "fits_remaining_allowance": True, "NN_invocations": 0}]}

    def test_capture_binds_the_large_census_of_a_compact_profile_capture(self):
        _, rows, _ = population()
        row, check = rows[96], PROBE["validate_capture"]
        cap = self.capture(row)
        check(cap, row, {"token": "r8"}, "focus", "minilm")
        compact_input = row | {"compiled": {"input_digest": sha("compact")}}
        with self.assertRaisesRegex(ValueError, "Large compiler budget or card roster changed"):
            check(cap, compact_input, {"token": "r8"}, "focus", "minilm")
        with self.assertRaisesRegex(ValueError, "Large compiler budget or card roster changed"):
            check(cap, row | {"remaining_ms": row["remaining_ms"] + 50}, {"token": "r8"}, "focus", "minilm")
        twice = copy.deepcopy(cap)
        twice["compiler_census"].append(twice["compiler_census"][0])
        with self.assertRaisesRegex(ValueError, "one original Large compiler census required"):
            check(twice, row, {"token": "r8"}, "focus", "minilm")
        dispatched = cap | {"Context_NN_dispatch": True}
        other_profile = cap | {"actual_Context_profile": {"manifest": LARGE_MANIFEST}}
        for changed, publication in ((dispatched, {"token": "r8"}), (other_profile, {"token": "r8"}), (cap, {"token": "r7"})):
            with self.assertRaisesRegex(ValueError, "foreign R8 native admission profile or publication"):
                check(changed, row, publication, "focus", "minilm")

    def test_settled_phase_returns_scores_for_eligible_rows_only(self):
        fixture = PhaseFixture()
        payload, records, scratch = PROBE["observation"](fixture.stdout(fixture.payload, 3), fixture.events,
                                                         fixture.rows, fixture.env, fixture.PID)
        self.assertEqual(payload, fixture.payload)
        self.assertEqual((len(records), len(scratch)), (108, 108))
        self.assertEqual(records[0], "Operator=MultiHeadAttention Node=/airk_large_cutlass/0/efficient DataType=fp32 "
                                     "SdpaKernel=EFFICIENT_ATTENTION")
        observed = payload["native"]["result"]["cal"]["observation"]["rows"]
        self.assertEqual([row["status"] for row in observed],
                         ["raw_scores_only", "no_native_assessment_request", "raw_scores_only", "raw_scores_only"])
        self.assertEqual([event["ordinal"] for event in fixture.events], [1, 1, 3, 3, 4, 4])

    def test_stdout_refusals(self):
        fixture = PhaseFixture()
        run = lambda raw: PROBE["observation"](raw, fixture.events, fixture.rows, fixture.env, fixture.PID)
        with self.assertRaisesRegex(ValueError, "one successful original Large CAL selector"):
            run(fixture.stdout(fixture.payload, 3, markers=2))
        with self.assertRaisesRegex(ValueError, "one successful original Large CAL selector"):
            run(fixture.stdout(fixture.payload, 3, markers=0))
        with self.assertRaisesRegex(ValueError, "one successful original Large CAL selector"):
            run(fixture.stdout(fixture.payload, 3, verdict="test result: FAILED. 0 passed; 1 failed"))
        for raw in (fixture.stdout(fixture.payload, 2), fixture.stdout(fixture.payload, 4),
                    fixture.stdout(fixture.payload, 3, layers=35),
                    fixture.stdout(fixture.payload, 3).replace(b"/airk_large_cutlass/35/", b"/airk_large_cutlass/34/"),
                    fixture.stdout(fixture.payload, 3).replace(b"DataType=fp32", b"DataType=fp16", 1),
                    fixture.stdout(fixture.payload, 3).replace(b"workspace=0, past", b"workspace=1, past", 1),
                    fixture.stdout(fixture.payload, 3).replace(b"/airk_large_cutlass/", b"/airk_compact_cutlass/")):
            with self.assertRaisesRegex(ValueError, "36 efficient FP32 attention kernels per original forward"):
                run(raw)

    def test_native_result_refusals(self):
        fixture = PhaseFixture()
        def refused(message, change, *, rows=None, env=None):
            payload = copy.deepcopy(fixture.payload)
            change(payload)
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                PROBE["observation"](fixture.stdout(payload, 3), fixture.events, rows or fixture.rows,
                                     env or fixture.env, fixture.PID)
        result = lambda payload: payload["native"]["result"]
        seen = lambda payload: result(payload)["cal"]["observation"]
        refused("not 22 GiB", lambda p: p["job"].update(process_memory_limit_bytes=48 << 30))
        refused("not 22 GiB", lambda p: p["job"].update(job_memory_limit_bytes=24 << 30))
        refused("CAL role differs", lambda p: result(p).update(typed_graph_sha256=sha("another typed graph")))
        refused("CAL role differs", lambda p: result(p).update(model_sha256=sha("rounded graph")))
        refused("CAL role differs", lambda p: result(p).update(weight_sha256=sha("other weights")))
        refused("CAL role differs", lambda p: result(p).update(cpu_refusal="unbounded CPU value"))
        refused("CAL role differs", lambda p: result(p).update(route_refusal="missing projection"))
        refused("CAL role differs", lambda p: p["native"]["placement"].update(complete=False))
        refused("CAL role differs", lambda p: p["native"].update(pressure={"tier_gib": 24}))
        refused("CAL role differs", lambda p: p["native"]["before"].update(free_bytes=GIB44 - 1))
        refused("CAL role differs", lambda p: seen(p).update(phase=11))
        refused("CAL role differs", lambda p: seen(p).update(calibration_fitted=True))
        refused("native CAL source pin differs", lambda p: seen(p)["original_projection"].update(sha256=PROJECTION))
        refused("native CAL source pin differs", lambda p: seen(p)["launch_audit"].update(path="D:/c/other.json"))
        refused("different number of requests", lambda p: seen(p)["rows"].pop())
        refused("changed native CAL chronology", lambda p: seen(p)["rows"][0].update(id="q98"))
        refused("scores invented for original refusal", lambda p: seen(p)["rows"][1].update(scores=[0.0, 0.0, 0.0]))
        refused("incomplete vector", lambda p: seen(p)["rows"][0].update(scores=[1.5, -0.25]))
        refused("incomplete vector", lambda p: seen(p)["rows"][2].update(scores=[1.5, float("inf"), 0.0]))
        refused("incomplete vector", lambda p: seen(p)["rows"][3].update(input_digest=sha("another input")))
        refused("incomplete vector", lambda p: seen(p)["rows"][3].update(remaining_input_tokens=8192))
        refused("incomplete vector", lambda p: seen(p)["rows"][0].update(status="eligible"))
        refused("native forward count differs", lambda p: result(p)["cal"].update(native_forwards=4))
        refused("late or mismatched original observation",
                lambda p: seen(p)["rows"][0].update(native_call_elapsed_us=410_001, inference_us=410_500))
        refused("late or mismatched original observation",
                lambda p: seen(p)["rows"][0].update(compile_us=1_096_000_000))

    def test_journal_refusals(self):
        fixture = PhaseFixture()
        raw = fixture.stdout(fixture.payload, 3)
        def refused(message, change):
            events = copy.deepcopy(fixture.events)
            change(events)
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                PROBE["observation"](raw, events, fixture.rows, fixture.env, fixture.PID)
        compact_typed = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
        refused("missing or extra original Large CAL operation", lambda e: e.pop())
        refused("graph or owned PID differs", lambda e: e[0]["identity"].update(profile=compact_typed))
        refused("graph or owned PID differs", lambda e: e[1]["identity"].update(input_sha256=PROJECTION))
        refused("graph or owned PID differs", lambda e: e[2]["identity"].update(process_id=7))
        refused("graph or owned PID differs", lambda e: e[3]["identity"].update(native_binary_sha256="cd" * 32))
        refused("graph or owned PID differs", lambda e: e[2].update(ordinal=2))
        refused("graph or owned PID differs", lambda e: e[4].update(expires_monotonic_ns=fixture.EXPIRY + 1))
        refused("graph or owned PID differs", lambda e: e[5].update(operation_input_digest=sha("another input")))
        refused("graph or owned PID differs", lambda e: e[0].update(kind="warmup"))
        refused("late or mismatched original observation", lambda e: e[5].update(observed_monotonic_ns=fixture.EXPIRY))
        refused("late or mismatched original observation", lambda e: e[1].update(native_success=False))
        refused("late or mismatched original observation", lambda e: e[2].update(started_monotonic_ns=1, observed_monotonic_ns=1))
        with self.assertRaisesRegex(ValueError, "graph or owned PID differs"):
            PROBE["observation"](raw, fixture.events, fixture.rows, fixture.env, fixture.PID + 1)


class CampaignFixture:
    """A synthetic closed R8 capture whose files exist only as pins and JSON bodies."""

    def __init__(self):
        self.campaign = HERE / "synthetic-campaign"
        self.index, self.audited = {}, []
        campaign = self.campaign
        capture = campaign / "cal100-focus1024-capture-r8"
        self.life = life = capture / "lifetime-01"
        root = campaign / "r8-chronological-full100-r2"
        compiled = campaign / "r8-chronological-native-full100-r1"
        stage = campaign / "windows-cu15-scientific-provider-stage-r1"
        self.publication, self.focus, self.pipeline = {"token": "r8"}, "focus-profile", "minilm-pipeline"
        self.ids, self.rows, batch = population(
            lambda n: capture / f"lifetime-{n // 56:02}/fixture/context-cal-admissions/record-{n:04}.json")
        for n in range(96, 100):
            row = self.rows[n]
            row["captures"] = [self.add(Path(row["captures"][0]["path"]), PhaseLauncher.capture(None, row)
                                        | {"actual_Focus_profile": self.focus, "MiniLM_pipeline": self.pipeline})]
        census = self.add(campaign / "cal100-retained-census-r1.json", {"question_ids": self.ids})
        self.audit = self.add(campaign / "cal100-fused-candidate-launch-r1/cal100-launch-audit.json",
                              {"census": census, "profiles": {"arbiter-focus-qwen3-reranker-06b": self.focus},
                               "pipeline": self.pipeline}, AUDIT)
        plan = {"path": str(capture / "lifetime-plan.json"), "bytes": 10, "sha256": PLAN}
        transcript = self.add(life / "phase-12.jsonl", {})
        owned = self.add(life / "phase-12.owned.json", SETTLED | {"phase": 12, "transcript": transcript})
        closures = [{"path": f"phase-{n:02}.owned.json", "bytes": 10, "sha256": sha(f"owner-{n}")} for n in range(7, 12)]
        self.closure = self.add(life / "lifetime-closure.json", {
            "schema": "airk.private.cal100-group-lifetime/1", "group": 1, "plan": plan, "phases": list(range(7, 13)),
            "phase_closures": [*closures, owned], "publication": self.publication})
        report = self.add(root / "phase-12-independent-inputs.json", {
            "phase": 12, "plan_sha256": PLAN, "input_audit_sha256": AUDIT, "closed_questions": 4, "certificate_count": 0,
            "lifetime_sha256": self.closure["sha256"], "phase_owner": owned, "source_transcript": transcript,
            "rows": [{"ordinal": row["ordinal"], "question_id": row["id"], "captures": row["captures"]}
                     for row in self.rows[96:]]})
        batch["phases"] = [{"path": str(root / f"phase-{n:02}-independent-inputs.json"), "sha256": sha(f"report-{n}")}
                           for n in range(12)] + [{"path": report["path"], "sha256": report["sha256"]}]
        self.large = self.add(root / "large.json", batch, BATCH)
        preflight = self.add(root / "reference-preflight.json", {
            "complete": True, "lifetime_plan_sha256": PLAN, "capture_sha256": sha("fixture-closure"),
            "rows": [{"id": key} for key in self.ids]})
        cargo = str(campaign / "target/debug/deps/airk_inference-8ef8bcfbb6d4d40e.exe")
        image = {"path": cargo, "sha256": sha("r7 compact image")}
        build = {"path": str(campaign / "windows-r6-chronological-checks-r4/large-image-r1.json"), "bytes": 10,
                 "sha256": sha("r7 build")}
        self.staged = self.add(stage / "fresh-cu15-compact-r7-chronological-r4-inference.exe", {}, image["sha256"])
        self.seal = self.add(campaign / "compact-cutlass-cal-r7-seal-r1/execution.json", {
            "schema": "airk.private.compact-static-cargo-custody/1", "complete": True, "native_invocations": 0,
            "build": build, "image": self.staged, "linked_image": image | {"bytes": 10}}, R7_SEAL)
        self.add(campaign / "r7-chronological-native-first56-r1/native.json", {"image": image, "build": build}, R7_COMPILER)
        log = self.add(compiled / "native.log", {})
        runner = self.add(PROBE["RAW"] / "run_compiler_batch.py", {})
        producer = self.add(PROBE["RAW"] / "r8_chronological_inputs.py", {})
        self.summary = {"compact": "ContextQwenCompact original_requests=100 native_recompiled=100 retained_refusals=0",
                        "large": "ContextQwenLarge original_requests=100 native_recompiled=100 retained_refusals=0"}
        self.native = {
            "passed": True, "NN_invocations": 0, "projection_sha256": PROJECTION, "previous_compiler_sha256": R7_COMPILER,
            "reference_preflight": preflight, "launch_audit_sha256": AUDIT, "image": image,
            "job": SETTLED | {"suspended_assignment": {"pid": 9, "image": image}},
            "command": [cargo, "--exact", COMPILER_SELECTOR, "--ignored", "--nocapture", "--test-threads=1"],
            "inputs": {"large": {"path": self.large["path"], "sha256": BATCH}}, "summary": self.summary,
            "log_sha256": log["sha256"], "source_sha256": runner["sha256"]}
        self.compiler = self.add(compiled / "native.json", self.native, COMPILER)
        self.saved = {
            "schema": "airk.private.cal100-r8-chronological-projection/1", "service_sha256": SERVICE,
            "original_requests": 100, "lifetimes": [sha("lifetime-00"), self.closure["sha256"]],
            "terminal": [sha("outer"), sha("fixture-closure"), sha("release")], "NN_invocations": 0,
            "gain_certificates": 0, "plan": plan, "plan_sha256": PLAN, "launch_audit": self.audit,
            "reference_preflight": preflight, "models": {"large": self.large}, "source_sha256": producer["sha256"]}
        self.add(root / "projection-receipt.json", self.saved, PROJECTION)

    def add(self, path, body, digest=None):
        pin = {"path": str(path), "bytes": 10, "sha256": digest or sha(str(path))}
        self.index[Path(path)] = (pin, body)
        return pin

    def exact(self, path, digest):
        pin, _ = self.index[Path(path)]
        if pin["sha256"] != digest:
            raise ValueError(f"reviewed digest differs: {path}")
        return pin

    def text(self, path):
        return "\n".join(["running 1 test", "test compiler ... " + self.summary["compact"], self.summary["large"],
                          "ok", "test result: ok. 1 passed; 0 failed; 0 ignored"])

    def inputs(self, phase=12):
        return PROBE["inputs"](phase, self.campaign, lambda path, limit: self.index[Path(path)][1], self.exact,
                               lambda paths: self.audited.extend(paths), text=self.text)


class PhaseCustody(unittest.TestCase):
    def test_final_phase_binds_capture_projection_and_the_sealed_compiler_image(self):
        fixture = CampaignFixture()
        pins, rows, settings, facts = fixture.inputs()
        self.assertEqual([row["ordinal"] for row in rows], [96, 97, 98, 99])
        self.assertEqual(settings, {
            "AIRK_CUDA_CAL_PHASE": "12", "AIRK_CUDA_CAL_LAUNCH_AUDIT": fixture.audit["path"],
            "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT, "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": fixture.large["path"],
            "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": BATCH})
        self.assertEqual(facts["compiler_image"], fixture.staged)
        self.assertEqual(facts["compiler_receipt"], fixture.compiler)
        self.assertEqual(facts["compiler_image_seal"], fixture.seal)
        self.assertEqual(facts["rows_recompiled_by_compiler_image"], 100)
        for expected in (fixture.closure, fixture.large, fixture.audit, fixture.seal, fixture.staged,
                         *[row["captures"][0] for row in rows]):
            self.assertIn(expected, pins)
        # Every input was guarded once before its bytes were read, and nothing unbound was read.
        self.assertEqual(len(fixture.audited), len(set(fixture.audited)))
        self.assertEqual(set(fixture.audited), {Path(pin["path"]) for pin in pins})
        self.assertEqual(len(pins), 20)

    def test_custody_refusals(self):
        def refused(message, change, phase=12):
            fixture = CampaignFixture()
            change(fixture)
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                fixture.inputs(phase)
        refused("original R8 CAL phase required", lambda f: None, phase=13)
        refused("foreign R8 evidence, Large batch or native compiler receipt",
                lambda f: f.native["inputs"]["large"].update(sha256=sha("compact.json")))
        refused("foreign R8 evidence, Large batch or native compiler receipt", lambda f: f.native.update(NN_invocations=1))
        refused("foreign R8 evidence, Large batch or native compiler receipt",
                lambda f: f.native["job"].update(owned_job_forced_termination=True))
        refused("foreign R8 evidence, Large batch or native compiler receipt",
                lambda f: f.native["command"].__setitem__(2, SELECTOR))
        refused("foreign R8 evidence, Large batch or native compiler receipt", lambda f: f.saved.update(gain_certificates=1))
        refused("foreign R8 evidence, Large batch or native compiler receipt", lambda f: f.saved.update(plan_sha256=AUDIT))
        refused("not the sealed R7 Compact image",
                lambda f: f.index[Path(f.seal["path"])][1]["linked_image"].update(sha256=sha("a later build")))
        refused("not the sealed R7 Compact image",
                lambda f: f.index[Path(f.seal["path"])][1].update(complete=False))
        refused("disagrees with the Large population", lambda f: f.summary.update(
            large="ContextQwenLarge original_requests=100 native_recompiled=99 retained_refusals=1"))
        refused("incomplete original R8 native compiler stdout",
                lambda f: setattr(f, "text", lambda path: "test result: ok. 1 passed; 0 failed;"))
        refused("independent R8 certificate is incomplete",
                lambda f: f.index[Path(f.saved["reference_preflight"]["path"])][1].update(complete=False))
        refused("Large compiler budget or card roster changed",
                lambda f: f.index[Path(f.rows[98]["captures"][0]["path"])][1]["compiler_census"][0].update(
                    compiled={"input_digest": sha("compact")}))
        report = lambda f: f.index[f.campaign / "r8-chronological-full100-r2/phase-12-independent-inputs.json"][1]
        refused("foreign R7 phase report or Job", lambda f: report(f).update(phase=11))
        refused("foreign R7 phase report or Job", lambda f: report(f).update(input_audit_sha256=PLAN))
        refused("repurposed a chronological row or capture", lambda f: report(f)["rows"][2].update(question_id="q0"))
        refused("R8 original phase has not settled",
                lambda f: f.index[f.life / "phase-12.owned.json"][1].update(timed_out=True))
        refused("reviewed digest differs",
                lambda f: f.index[f.life / "lifetime-closure.json"][0].update(sha256=sha("edited closure")))


class Panel(unittest.TestCase):
    SCORES = {5: [2.0, 1.990234375, 0.0], 40: [1.984375, 2.0, 0.0], 77: [2.0, 2.0, 0.0], 99: [2.0, 1.96875, 0.0]}

    def setUp(self):
        self.campaign = HERE / "synthetic-campaign"
        self.index = {}
        root = self.campaign / "r8-chronological-full100-r2"
        self.ids, self.rows, batch = population()
        self.large = self.add(root / "large.json", batch, BATCH)
        self.add(root / "projection-receipt.json", {"models": {"large": self.large}}, PROJECTION)
        self.add(self.campaign / "r8-chronological-native-full100-r1/native.json",
                 {"projection_sha256": PROJECTION, "passed": True, "NN_invocations": 0,
                  "inputs": {"large": {"path": self.large["path"], "sha256": BATCH}}}, COMPILER)
        self.image = {"path": "image.exe", "bytes": 10, "sha256": sha("large cal image")}
        self.build = {"path": "large-image-r1.json", "bytes": 10, "sha256": sha("large cal build")}
        self.manifest = {"path": "airk-cuda-runtime.json", "bytes": 10, "sha256": MANIFEST}
        self.graph = self.add(self.campaign / "windows-large-direct-cal-graph-r1/execution.json", {
            "mode": "graph", "complete": True, "direct": True, "owned": {"settled": True}, "image": self.image,
            "build": self.build, "runtime_manifest": self.manifest,
            "result": {"result": {"typed_graph_sha256": TYPED, "model_sha256": GRAPH, "weight_sha256": WEIGHTS}}})
        self.receipts = []
        for phase in range(13):
            revision = 2 if phase == 12 else 1
            path = self.campaign / f"large-direct-cal-r8-full100-phase{phase:02}-r{revision}/execution.json"
            pin = self.add(path, self.report(phase))
            self.receipts.append((path, pin["sha256"]))

    def add(self, path, body, digest=None):
        pin = {"path": str(path), "bytes": 10, "sha256": digest or sha(str(path))}
        self.index[Path(path)] = (pin, body)
        return pin

    def report(self, phase):
        chosen = self.rows[phase * 8:phase * 8 + 8]
        observed = [{"ordinal": row["ordinal"], "id": row["id"], "status": "raw_scores_only",
                     "input_digest": row["compiled"]["input_digest"], "captures": row["captures"],
                     "compiled": row["compiled"], "roster": row["roster"],
                     "scores": self.SCORES.get(row["ordinal"], [3.0, 1.0, 2.0]), "remaining_ms": row["remaining_ms"],
                     "remaining_input_tokens": 4096, "compile_us": 1, "inference_us": 2, "native_call_elapsed_us": 2}
                    for row in chosen]
        return {"schema": "airk.private.large-direct-cal-cuda/1", "complete": True, "mode": "cal", "direct": True,
                "phase": phase, "owned": {"settled": True}, "qualification": False, "calibration_fitted": False,
                "graph_receipt": self.graph, "image": self.image, "build": self.build, "runtime_manifest": self.manifest,
                "host_bytes": GIB22, "seconds": 120, "kernel_records": ["kernel"] * (36 * len(chosen)),
                "result": {"result": {"typed_graph_sha256": TYPED, "model_sha256": GRAPH, "weight_sha256": WEIGHTS,
                                      "cal": {"native_forwards": len(chosen), "observation": {
                                          "phase": phase, "qualification": False, "calibration_fitted": False,
                                          "original_projection": {"path": self.large["path"], "sha256": BATCH},
                                          "launch_audit": {"path": "audit.json", "sha256": AUDIT}, "rows": observed}}}}}

    def exact(self, path, digest):
        pin, _ = self.index[Path(path)]
        if pin["sha256"] != digest:
            raise ValueError(f"reviewed digest differs: {path}")
        return pin

    def assemble(self, receipts=None):
        return PANEL["panel"](self.receipts if receipts is None else receipts, self.graph, self.campaign,
                              lambda path, limit: self.index[Path(path)][1], self.exact,
                              lambda owned: owned == {"settled": True})

    def body(self, phase):
        return self.index[self.receipts[phase][0]][1]

    def test_rows_have_the_compact_panel_shape_and_cover_all_100_requests(self):
        value = self.assemble()
        self.assertEqual(value["schema"], "airk.private.r8-large-scientific-raw-panel/1")
        self.assertEqual(len(value["rows"]), 100)
        self.assertEqual(value["rows"][0], {"ordinal": 0, "id": "q0", "source_phase": 0,
                                            "input_digest": sha("large-input-0"), "scores": [3.0, 1.0, 2.0]})
        self.assertEqual(list(value["rows"][99]), ["ordinal", "id", "source_phase", "input_digest", "scores"])
        self.assertEqual(value["rows"][99], {"ordinal": 99, "id": "q99", "source_phase": 12,
                                             "input_digest": sha("large-input-99"), "scores": [2.0, 1.96875, 0.0]})
        self.assertEqual([group["raw_vectors"] for group in value["phases"]], [8] * 12 + [4])
        self.assertEqual(value["phases"][12]["source"]["path"],
                         str(self.campaign / "large-direct-cal-r8-full100-phase12-r2/execution.json"))
        self.assertEqual(value["artifact"], {"model_sha256": GRAPH, "weight_sha256": WEIGHTS,
                                             "typed_graph_sha256": TYPED, "runtime_manifest_sha256": MANIFEST})
        self.assertEqual((value["input"], value["graph"], value["image"]), (self.large, self.graph, self.image))
        self.assertIs(value["calibration_fitted"], False)
        self.assertIs(value["qualification"], False)

    def test_margin_screen_counts_rows_at_or_below_each_near_tie_limit(self):
        margins = self.assemble()["top_two_margins"]
        self.assertEqual((margins["rows"], margins["at_or_below_0_014"], margins["at_or_below_0_020"]), (100, 2, 3))
        self.assertEqual(margins["near_tie_rows"], [
            {"ordinal": 77, "id": "q77", "winner": 0, "runner_up": 1, "margin": 0.0},
            {"ordinal": 5, "id": "q5", "winner": 0, "runner_up": 1, "margin": 0.009765625},
            {"ordinal": 40, "id": "q40", "winner": 1, "runner_up": 0, "margin": 0.015625}])
        self.assertEqual(margins["minimum"], {"ordinal": 77, "id": "q77", "winner": 0, "runner_up": 1, "margin": 0.0})
        masked = PANEL["top_two_margins"]([{"ordinal": 0, "id": "a", "scores": [9.0, 1.0, 0.5]}], [[False, True, True]])
        self.assertEqual(masked["minimum"], {"ordinal": 0, "id": "a", "winner": 1, "runner_up": 2, "margin": 0.5})
        self.assertEqual((masked["at_or_below_0_014"], masked["at_or_below_0_020"], masked["near_tie_rows"]), (0, 0, []))
        for scores, mask in (([9.0, 1.0, 0.5], [True, False, False]), ([9.0, 1.0], [True, True, True])):
            with self.assertRaisesRegex(ValueError, "two legal options"):
                PANEL["top_two_margins"]([{"ordinal": 0, "id": "a", "scores": scores}], [mask])

    def test_phase_roster_refusals(self):
        with self.assertRaisesRegex(ValueError, "thirteen distinct Large phase receipts"):
            self.assemble(self.receipts[:12])
        with self.assertRaisesRegex(ValueError, "thirteen distinct Large phase receipts"):
            self.assemble(self.receipts[:12] + self.receipts[:1])
        again = self.campaign / "large-direct-cal-r8-full100-phase03-r2/execution.json"
        self.add(again, self.report(3))
        with self.assertRaisesRegex(ValueError, "duplicate or unknown Large phase receipt"):
            self.assemble(self.receipts[:4] + [(again, sha(str(again)))] + self.receipts[5:])
        compact = self.campaign / "compact-cutlass-cal-r8-full100-phase12-r1/execution.json"
        self.add(compact, self.report(12))
        with self.assertRaisesRegex(ValueError, "outside the campaign or misnamed"):
            self.assemble(self.receipts[:12] + [(compact, sha(str(compact)))])
        with self.assertRaisesRegex(ValueError, "reviewed digest differs"):
            self.assemble(self.receipts[:12] + [(self.receipts[12][0], sha("edited receipt"))])

    def test_phase_receipt_refusals(self):
        def refused(message, phase, change):
            original = copy.deepcopy(self.body(phase))
            change(self.body(phase))
            try:
                with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                    self.assemble()
            finally:
                self.index[self.receipts[phase][0]] = (self.index[self.receipts[phase][0]][0], original)
        seen = lambda body: body["result"]["result"]["cal"]["observation"]
        incomplete = "incomplete or used another image, graph receipt or limit"
        refused(incomplete, 0, lambda b: b.update(complete=False))
        refused(incomplete, 1, lambda b: b.update(owned={"settled": False}))
        refused(incomplete, 2, lambda b: b.update(image=self.image | {"sha256": sha("pressure image")}))
        refused(incomplete, 3, lambda b: b.update(graph_receipt=self.graph | {"sha256": sha("older graph receipt")}))
        refused(incomplete, 4, lambda b: b.update(host_bytes=48 << 30))
        refused(incomplete, 5, lambda b: b.update(phase=6))
        exact = "does not bind the exact graph, phase or original inputs"
        refused(exact, 6, lambda b: b["result"]["result"].update(typed_graph_sha256=sha("another graph")))
        refused(exact, 7, lambda b: seen(b).update(phase=8))
        refused(exact, 8, lambda b: seen(b)["original_projection"].update(sha256=sha("compact.json")))
        refused(exact, 9, lambda b: b.update(kernel_records=["kernel"] * 252))
        refused(exact, 10, lambda b: seen(b)["rows"].reverse())
        refused(exact, 12, lambda b: seen(b)["rows"].pop())
        changed = "changed the exact original input, card roster or score vector"
        refused(changed, 11, lambda b: seen(b)["rows"][3].update(input_digest=sha("another input")))
        refused(changed, 11, lambda b: seen(b)["rows"][0].update(scores=[3.0, 1.0]))
        refused(changed, 11, lambda b: seen(b)["rows"][7].update(scores=[3.0, float("nan"), 2.0]))
        refused(changed, 11, lambda b: seen(b)["rows"][5].update(compiled={"input_digest": sha("large-input-93")}))
        self.assertEqual(len(self.assemble()["rows"]), 100)

    def test_population_and_graph_receipt_refusals(self):
        graph = self.index[Path(self.graph["path"])][1]
        graph["result"]["result"]["typed_graph_sha256"] = sha("another graph")
        with self.assertRaisesRegex(ValueError, "wrong same-image Large direct graph receipt"):
            self.assemble()
        graph["result"]["result"]["typed_graph_sha256"] = TYPED
        self.rows[50]["status"] = "compiler_refused_remaining_allowance"
        with self.assertRaisesRegex(ValueError, "incomplete Large chronology"):
            self.assemble()


if __name__ == "__main__":
    unittest.main()
