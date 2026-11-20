# SPDX-License-Identifier: Apache-2.0
"""Pure conversion custody controls; no fixture writes or third-party imports."""
import copy
import importlib.util
from pathlib import Path
import runpy
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('fp16_execution', ROOT / 'scripts/arbiter-fp16-execution.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def row(name='literal', size=1, sha='a' * 64):
    return {'path': str(ROOT / name), 'bytes': size, 'sha256': sha}


def fixture():
    plan = {'schema': M.SCHEMA, 'human_sid': M.SID, 'limits': dict(M.LIMITS), 'model': 'compact',
            'expected_weights_bytes': 3441397760, 'expected_attention_count': 28,
            'repository_root': str(ROOT), 'output_root': str(ROOT / 'artifacts/absent-literal-output'),
            'cache_prefix': str(ROOT / 'absent-literal-cache'),
            'prerequisite_evidence': {'first_fp32': row('fp32-proof'), 'native_g5': row('g5-proof')},
            'helpers': {name: row(name, sha=sha) for name, sha in M.HELPERS.items()}}
    for key in ('python', 'powershell', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest', 'source_plan', 'runtime_baseline', 'prerequisites_manifest'):
        plan[key] = row(key)
    plan['wrapper'] = row('scripts/arbiter-fp16-execution.py')
    plan['input_files'] = [plan[key] for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest', 'source_plan', 'runtime_baseline', 'prerequisites_manifest')] + list(plan['helpers'].values()) + list(plan['prerequisite_evidence'].values())
    return plan


class PlanControls(unittest.TestCase):
    def validate(self, plan):
        with patch.object(M.Path, 'exists', return_value=False), patch.object(M.Path, 'is_dir', return_value=True):
            return M.validate_plan(plan)

    def test_exact_original_conversion_limits(self):
        self.assertEqual(M.LIMITS, {'wall_seconds': 600, 'host_committed_bytes': 8589934592, 'log_bytes': 67108864, 'cleanup_seconds': 5})
        self.validate(fixture())

    def test_bounds_cannot_be_extended(self):
        for key in M.LIMITS:
            plan = fixture(); plan['limits'][key] += 1
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(plan)

    def test_identity_or_schema_cannot_be_reinterpreted(self):
        for key in ('human_sid', 'schema', 'model'):
            plan = fixture(); plan[key] = 'other'
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(plan)

    def test_prerequisite_pins_are_required_not_generated(self):
        plan = fixture(); del plan['prerequisite_evidence']['first_fp32']
        with self.assertRaises(ValueError): self.validate(plan)

    def test_missing_helper_or_changed_helper_refuses(self):
        plan = fixture(); del plan['helpers']['reference_worker.py']
        with self.assertRaises(ValueError): self.validate(plan)
        plan = fixture(); plan['helpers']['reference_worker.py']['sha256'] = 'b' * 64
        with self.assertRaises(ValueError): self.validate(plan)

    def test_omitted_or_duplicate_input_refuses(self):
        plan = fixture(); plan['input_files'].pop()
        with self.assertRaises(ValueError): self.validate(plan)
        plan = fixture(); plan['input_files'].append(plan['input_files'][0])
        with self.assertRaises(ValueError): self.validate(plan)

    def test_wrong_attention_roster_refuses(self):
        plan = fixture(); plan['expected_attention_count'] = 36
        with self.assertRaises(ValueError): self.validate(plan)

    def test_sibling_output_escape_refuses(self):
        plan = fixture(); plan['output_root'] = str(ROOT.parent / 'airk-other/artifacts/output')
        with self.assertRaises(ValueError): self.validate(plan)

    def test_lexical_dotdot_output_escape_refuses(self):
        plan = fixture(); plan['output_root'] = str(ROOT / 'artifacts/../scripts/new-output')
        with self.assertRaises(ValueError): self.validate(plan)

    def test_existing_output_refuses(self):
        with patch.object(M.Path, 'exists', return_value=True), self.assertRaises(ValueError):
            M.validate_plan(fixture())

    def test_complete_runtime_roster_required(self):
        plan = fixture(); manifest = {'roots': [str(ROOT)], 'files': {}, 'import_paths': []}
        with patch.object(M, 'validate_plan', return_value={}), patch.object(M, 'pinned'), patch.object(M, 'read', return_value=manifest), patch.object(M, 'file_roster', return_value={str(ROOT / 'extra.pyd')}), self.assertRaisesRegex(ValueError, 'roster'):
            M.authenticate(plan)

    def test_runtime_member_must_be_in_input_seal(self):
        plan = fixture(); item = row('module.py'); manifest = {'roots': [str(ROOT)], 'files': {item['path']: item}, 'import_paths': []}
        with patch.object(M, 'validate_plan', return_value={}), patch.object(M, 'pinned'), patch.object(M, 'read', return_value=manifest), patch.object(M, 'file_roster', return_value={item['path']}), self.assertRaisesRegex(ValueError, 'omitted'):
            M.authenticate(plan)

    def test_loaded_module_outside_seal_refuses(self):
        module = types.SimpleNamespace(__file__=__file__)
        with patch.dict(M.sys.modules, {'literal': module}, clear=True), self.assertRaisesRegex(ValueError, 'outside sealed'):
            M.loaded_runtime({})


class IndependentReferenceImportControls(unittest.TestCase):
    def test_only_the_exact_existing_audited_cache_directory_may_be_checked(self):
        path = ROOT / 'scripts/arbiter-fp16-execution/verify_compact.py'
        spec = importlib.util.spec_from_file_location('fp16_reference_worker_cache', path)
        reference = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reference)
        reference.read_only('os.mkdir', (str(ROOT), 0o700, -1), ROOT)
        for proposed, fd in ((ROOT / 'absent-cache-child', -1), (ROOT, 1)):
            with self.subTest(proposed=proposed, fd=fd), self.assertRaises(ValueError):
                reference.read_only('os.mkdir', (str(proposed), 0o700, fd), ROOT)
        with self.assertRaises(ValueError):
            reference.read_only('open', (str(ROOT / 'cache-data'), 'w', reference.os.O_WRONLY), ROOT)

    def test_generated_names_cannot_invent_a_file_or_hide_real_modules(self):
        path = ROOT / 'scripts/arbiter-fp16-execution/verify_compact.py'
        spec = importlib.util.spec_from_file_location('fp16_reference_worker', path)
        reference = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reference)

        class GeneratedNamespace(types.ModuleType):
            def __getattr__(self, key):
                return '_ops.py' if key == '__file__' else None

        synthetic = GeneratedNamespace('torch.ops')
        real = types.SimpleNamespace(__file__=str(path))
        real_pin = row('scripts/arbiter-fp16-execution/verify_compact.py')
        result = reference.loaded_runtime(
            {str(path): real_pin}, lambda file, check: real_pin, lambda: None, [synthetic, real])
        self.assertEqual(result, {str(path): real_pin['sha256']})
        with self.assertRaisesRegex(ValueError, 'outside sealed closure'):
            reference.loaded_runtime({}, lambda file, check: real_pin, lambda: None, [real])


class WiderCompactReferenceControls(unittest.TestCase):
    CASES = (
        (128, "all-live-1", "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a"),
        (1024, "native-all-live-1024", "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357"),
        (8192, "native-all-live-8192", "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0"),
    )

    @classmethod
    def setUpClass(cls):
        path = ROOT / "scripts/arbiter-fp16-execution/verify_compact.py"
        spec = importlib.util.spec_from_file_location("compact_wider_reference", path)
        cls.reference = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.reference)

    def source_row(self, width, name, digest):
        return {
            "name": name,
            "input": {"prompt": "", "input_ids": [1] * width, "attention_mask": [1] * width,
                      "position_ids": list(range(width)), "decision_position": width - 1,
                      "candidate_positions": list(range(8)), "option_mask": [True] * 8, "input_digest": digest},
            "reference_original_fp32": [-1.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
        }

    def pairs(self):
        short, middle, longest = [self.source_row(*case) for case in self.CASES]
        fixture = {"compiler": "literal", "resource_policy": "120", "original_reference_assets": {"graph": "fp32"}}
        retained = dict(fixture, rows=[self.source_row(17, "retained-t17", "d" * 64), short, longest])
        supplemental = dict(fixture, rows=[self.source_row(32, "existing-t32", "e" * 64), middle])
        publisher = {"weights.bin": row("publisher")}
        return [
            (retained, row("retained-fixture"), {"original_source_files": publisher}, row("retained-producer")),
            (supplemental, row("supplemental-fixture"), {"original_source_files": publisher}, row("supplemental-producer")),
        ]

    def select(self, pairs):
        with patch.object(self.reference, "reference_pair", side_effect=pairs):
            return self.reference.wider_references(lambda: None)

    def test_exact_wider_roster_reuses_the_two_original_sources(self):
        references, publisher = self.select(self.pairs())
        self.assertEqual([(len(item["input"]["input_ids"]), item["name"], item["input"]["input_digest"])
                          for item in references], list(self.CASES))
        self.assertEqual([item["fixture"] for item in references],
                         [row("retained-fixture"), row("supplemental-fixture"), row("retained-fixture")])
        self.assertEqual(publisher, {"weights.bin": row("publisher")})

    def test_fixture_lineage_and_native_input_drift_refuse(self):
        def remove(pairs):
            pairs[0][0]["rows"].pop()
        def swap(pairs):
            pairs[0][0]["rows"][1:] = reversed(pairs[0][0]["rows"][1:])
        def move_digest(pairs):
            pairs[1][0]["rows"][1]["input"]["input_digest"] = self.CASES[2][2]
        def change_geometry(pairs):
            pairs[1][0]["rows"][1]["input"]["option_mask"][0] = False
        def change_publisher(pairs):
            pairs[1][2]["original_source_files"] = {"weights.bin": row("foreign")}

        for edit in (remove, swap, move_digest, change_geometry, change_publisher):
            pairs = self.pairs()
            edit(pairs)
            with self.subTest(edit=edit.__name__), self.assertRaises(ValueError):
                self.select(pairs)

    def test_owned_result_rejects_missing_misbound_and_invalid_score_rows(self):
        expected, _ = self.select(self.pairs())
        actual = [dict(item, matching_fp16=[0.25] * 8) for item in expected]
        self.reference.validate_wider_observation(actual, expected)
        for bad in (actual[1:], actual[::-1], [actual[0]] * 3,
                    [dict(actual[0], fixture=row("wrong")), *actual[1:]],
                    [dict(actual[0], matching_fp16=[0.25] * 7), *actual[1:]],
                    [dict(actual[0], matching_fp16=[float("nan")] * 8), *actual[1:]],
                    [dict(actual[0], matching_fp16=[True] * 8), *actual[1:]]):
            with self.assertRaises(ValueError):
                self.reference.validate_wider_observation(bad, expected)

    def test_exact_half_projection_witness_distinguishes_midpoint_and_closer_result(self):
        weights = [0x3c00, 0x3c00]
        distances = self.reference.exact_dot_distances([0x3c00, 0x1000], weights, 0x3c00, 0x3c01)
        self.assertEqual(distances, {"cpu_distance_2^-48": 1 << 37, "cuda_distance_2^-48": 1 << 37})
        distances = self.reference.exact_dot_distances([0x3c00, 0x0c00], weights, 0x3c00, 0x3c01)
        self.assertEqual(distances, {"cpu_distance_2^-48": 1 << 36, "cuda_distance_2^-48": 3 << 36})

    def test_cli_preserves_default_and_uses_distinct_recipe_markers(self):
        cases = (
            ([], "t32", "AIRK_FP16_COMPACT_REFERENCE "),
            (["--wider"], "wider", "AIRK_FP16_COMPACT_WIDER_REFERENCE "),
            (["--flash-t32"], "flash-t32", "AIRK_FP16_COMPACT_FLASH_T32_REFERENCE "),
            (["--layer0-operands"], "layer0-operands", "AIRK_FP16_COMPACT_LAYER0_OPERANDS "),
            (["--layer0-projections", "--projection-capture", "capture.json", "--projection-capture-sha256", "a" * 64],
             "layer0-projections", "AIRK_FP16_COMPACT_LAYER0_PROJECTIONS "),
        )
        for args, kind, marker in cases:
            argv = ["verify_compact.py", "--conversion-plan", "plan", "--conversion-plan-sha256", "a",
                    "--conversion-receipt", "receipt", "--conversion-receipt-sha256", "b",
                    "--deadline-monotonic", str(self.reference.time.monotonic() + 600), *args]
            with self.subTest(kind=kind), patch.object(self.reference.sys, "argv", argv), \
                    patch.object(self.reference, "verify", return_value={"complete": True}) as verify, \
                    patch("builtins.print") as emit:
                self.reference.main()
                self.assertEqual(verify.call_args.args[0].reference_kind, kind)
                emit.assert_called_once_with(marker + '{"complete":true}', flush=True)


class ProjectionGemmRoundingControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = ROOT / "scripts/arbiter-fp16-execution/probe_projection_gemm.py"
        spec = importlib.util.spec_from_file_location("projection_gemm_rounding", path)
        cls.gemm = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.gemm)

    def test_exact_binary16_rounding_uses_bits_below_float64_at_halfway(self):
        one = 1 << 48
        halfway = 1 << 37
        controls = (
            (one, 0x3c00),
            (one + halfway, 0x3c00),
            (one + halfway + 1, 0x3c01),
            (one + 3 * halfway, 0x3c02),
            (-(one + halfway + 1), 0xbc01),
            (1 << 24, 0x0001),
            (1 << 23, 0x0000),
            (-(1 << 23), 0x8000),
        )
        for dot, word in controls:
            with self.subTest(dot=dot):
                self.assertEqual(self.gemm.rounded_half_word(dot), word)

    def test_exact_binary32_rounding_and_half_double_rounding(self):
        one = 1 << 48
        halfway_float = 1 << 24
        self.assertEqual(self.gemm.rounded_float_word(one + halfway_float), 0x3f800000)
        self.assertEqual(self.gemm.rounded_float_word(one + halfway_float + 1), 0x3f800001)
        self.assertEqual(self.gemm.rounded_float_word(-(one + 3 * halfway_float)), 0xbf800002)
        above_half_midpoint = one + (1 << 37) + 1
        self.assertEqual(self.gemm.rounded_float_word(above_half_midpoint), 0x3f801000)
        self.assertEqual(self.gemm.rounded_half_word(above_half_midpoint), 0x3c01)
        self.assertEqual(self.gemm.rounded_half_word(above_half_midpoint - 1), 0x3c00)


class CompactT32ComparisonControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"
        spec = importlib.util.spec_from_file_location("compact_t32_probe", path)
        cls.probe = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.probe)

    def comparison_row(self):
        return {
            "matching_fp16_raw_error": .001,
            "matching_fp16_mass_error": .05,
            "matching_fp16_argmax_matches": True,
            "original_fp32_raw_error": .012271881,
            "original_fp32_mass_error": .05,
            "original_fp32_argmax_matches": False,
        }

    def test_original_score_drift_and_winner_changes_are_diagnostic(self):
        self.probe.validate_t32_comparisons(self.comparison_row())

    def test_matching_parity_original_mass_and_diagnostic_types_remain_required(self):
        for name, value in {
            "matching_fp16_raw_error": .0011,
            "matching_fp16_mass_error": .051,
            "matching_fp16_argmax_matches": False,
            "original_fp32_raw_error": float("inf"),
            "original_fp32_mass_error": .051,
            "original_fp32_argmax_matches": 0,
        }.items():
            invalid = self.comparison_row()
            invalid[name] = value
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.probe.validate_t32_comparisons(invalid)


class CompactBoundedT32Controls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.probe = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"))

    def bounded_evidence(self):
        reviewed = {"path": str(ROOT / "reviewed-graph.json"), "sha256": "11" * 32}
        source = {"sha256": "33" * 32}
        reviewed_placement = {
            "nodes": [
                {"subgraph_index": 0, "name": "dims", "domain": "", "op": "Shape", "ep": "CPUExecutionProvider"},
                {"subgraph_index": 0, "name": "matmul", "domain": "", "op": "MatMul", "ep": "CUDAExecutionProvider"},
            ],
        }
        value = {
            "profile": "22" * 32,
            "reviewed_graph": dict(reviewed),
            "bounded_policy_identity": "72f357ec0b22800ab6e79f3fba70578a25c62a76743bc47af3c91ad1850dbc0b",
            "final_placement": {"nodes": list(reversed(copy.deepcopy(reviewed_placement["nodes"])))},
            "rows": [{"native_call_elapsed_us": 21}],
            "bounded_profile": {"tokens": 32, "node_count": 1746, "cpu_nodes": 374, "model_run_us": 20,
                                "cpu_input_bytes": 200, "cpu_output_bytes": 300, "cpu_max_tensor_bytes": 32},
        }
        return value, reviewed, reviewed_placement, source

    def test_bounded_policy_provenance_and_complete_placement_accept_reordered_rows(self):
        self.probe["validate_bounded_t32"](*self.bounded_evidence())

    def test_bounded_refuses_different_policy_graph_roster_and_profile(self):
        changes = [
            ("policy identity", lambda value: value.update(bounded_policy_identity="44" * 32)),
            ("policy identity", lambda value: value["reviewed_graph"].update(sha256="44" * 32)),
            ("complete reviewed graph", lambda value: value["final_placement"]["nodes"][0].update(name="changed")),
            ("complete reviewed graph", lambda value: value["final_placement"]["nodes"][0].update(ep="CPUExecutionProvider")),
            ("profile timing", lambda value: value["bounded_profile"].update(model_run_us=22)),
            ("CPU tensor bounds", lambda value: value["bounded_profile"].update(cpu_max_tensor_bytes=33)),
            ("complete node coverage", lambda value: value["bounded_profile"].update(cpu_nodes=373)),
            ("complete node coverage", lambda value: value["bounded_profile"].update(node_count=1745)),
        ]
        for message, change in changes:
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                value, reviewed, placement, source = self.bounded_evidence()
                change(value)
                self.probe["validate_bounded_t32"](value, reviewed, placement, source)

    def test_each_layer0_policy_has_its_own_framed_identity(self):
        cases = (
            ("layer0-t32", "airk.private.compact-fp16-layer0-taps/1",
             "9656e47e1818def4d810b8e50b6624cb452cec5545333a10c59c77d87662725e"),
            ("layer0-projection-t32", "airk.private.compact-fp16-layer0-projection-taps/1",
             "0545fddc40411323e14d98f898f03ffc6494755b3bcd11253f2e8743e1c4a6d1"),
        )
        for mode, expected_schema, digest in cases:
            with self.subTest(mode=mode):
                value, reviewed, placement, source = self.bounded_evidence()
                schema, policy, _ = self.probe["TAP_SCOPES"][mode]
                self.assertEqual(schema, expected_schema)
                with self.assertRaisesRegex(ValueError, "policy identity"):
                    self.probe["validate_bounded_t32"](value, reviewed, placement, source, policy=policy)
                value["bounded_policy_identity"] = digest
                self.probe["validate_bounded_t32"](value, reviewed, placement, source, policy=policy)

    def test_flash_backend_log_accepts_each_exact_selector_and_bare_historical_rows(self):
        rows = [
            f"Operator=GroupQueryAttention Node=/airk_g5/attention/{index}/efficient "
            "DataType=fp16 SdpaKernel=FLASH_ATTENTION"
            for index in range(28)
        ]
        prefix = ("test inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
                  "fp16_compact::")
        for mode, selector in [("t32", "cuda_compact_fp16_t32_component"),
                               ("bounded-t32", "cuda_compact_fp16_bounded_t32_component"),
                               ("layer0-t32", "cuda_compact_fp16_layer0_taps"),
                               ("layer0-projection-t32", "cuda_compact_fp16_layer0_projection_taps")]:
            with self.subTest(mode=mode):
                namespaced = [prefix + selector + " ... \x1b[36m" + rows[0] + "\x1b[0m", *rows[1:]]
                self.assertEqual(self.probe["attention_logs"]("\n".join(namespaced).encode(), mode), rows)
        self.assertEqual(self.probe["attention_logs"]("\n".join(rows).encode()), rows)
        refused = [rows[0], *rows[:-1]]
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.probe["attention_logs"]("\n".join(refused).encode(), "bounded-t32")


    def test_development_logs_bind_every_flash_roster_to_its_frozen_input(self):
        widths = [128, 1024, 8192]
        cases = [{"tokens": width, "input_digest": f"{width:064x}"} for width in widths]
        flash = [f"Operator=GroupQueryAttention Node=/airk_g5/attention/{layer}/efficient "
                 "DataType=fp16 SdpaKernel=FLASH_ATTENTION" for layer in range(28)]
        lines = []
        for case in cases:
            lines.append(f"AIRK_CUDA_QKV_DEVELOPMENT_FORWARD {case['tokens']} {case['input_digest']}")
            lines.extend(flash)
        check = self.probe["development_attention_logs"]
        self.assertEqual(check("\n".join(lines).encode(), cases), [flash] * 3)
        bad_digest = lines.copy()
        bad_digest[29] = bad_digest[29].replace(cases[1]["input_digest"], cases[0]["input_digest"])
        with self.assertRaisesRegex(ValueError, "forward marker differs"):
            check("\n".join(bad_digest).encode(), cases)
        moved = lines.copy()
        moved.insert(29, moved.pop(30))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            check("\n".join(moved).encode(), cases)

    def test_observed_seven_card_vectors_keep_slot_order_and_reject_a_missing_card(self):
        compare = self.probe["development_metrics"]
        reference = [0, 1, 2, 3, 8, 6, 7]
        actual = [0, 1, 2, 3, 7, 6, 8]
        self.assertEqual(compare(actual, reference)["winner"], 4)
        self.assertEqual(compare(actual, reference)["native_winner"], 6)
        with self.assertRaisesRegex(ValueError, "cardinality"):
            compare(actual[:-1], reference)

    def test_development_comparison_retains_the_signed_near_tie_margin(self):
        compare = self.probe["development_metrics"]
        lower = 1 - 1 / 128
        metrics = compare([lower, 1, 0, 0, 0, 0, 0, 0], [1, lower, 0, 0, 0, 0, 0, 0])
        self.assertEqual(metrics["raw_error"], 1 / 128)
        self.assertEqual(metrics["reference_margin"], 1 / 128)
        self.assertEqual(metrics["native_margin_for_reference_winner"], -1 / 128)
        self.assertEqual((metrics["native_winner"], metrics["winner"]), (1, 0))
        self.assertFalse(metrics["argmax_matches"])
        self.assertFalse(self.probe["provisional_development_pass"](metrics, metrics))


class CompactLayer0Controls(unittest.TestCase):
    TAPS = (
        ("/airk_g5/attention/0/q16", 2048),
        ("/airk_g5/attention/0/k16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
        ("/airk_g5/attention/0/raw16", 2048),
    )
    PROJECTIONS = (
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16", 2048),
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16", 2048),
    )

    @classmethod
    def setUpClass(cls):
        cls.probe = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"))
        cls.component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))

    def layer0_evidence(self, roster=None):
        if roster is None:
            roster = self.TAPS
        node = {"name": "literal producer", "domain": "", "op": "Identity", "ep": "CUDAExecutionProvider"}
        rows = [
            {"kind": "graph", "graph": 0, "parent_graph": None, "parent_node": None,
             "attribute": "", "outer_bindings": "not_applicable"},
            {"kind": "node", "graph": 0, "node": 0, "version": 19, **node},
        ]
        outputs = [("scores", [1, 8], 1), *((name, [1, 32, width], 10) for name, width in roster)]
        for value, (name, shape, elem) in enumerate(outputs):
            rows.append({"kind": "value", "graph": 0, "value": value, "name": name, "element_type": elem,
                         "rank": len(shape), "type_state": "tensor", "provenance": "node_output", "producer": 0})
            rows.extend({"kind": "dimension", "graph": 0, "value": value, "index": index, "number": number,
                         "symbol": None} for index, number in enumerate(shape))
            rows.append({"kind": "edge", "graph": 0, "slot": value, "node": 0, "value": value, "role": 2})
            rows.append({"kind": "edge", "graph": 0, "slot": value, "node": None, "value": value, "role": 4})
        rows.append({"kind": "end", "graph": 0})
        names = sum(len(name) + len("node_output") for name, _, _ in outputs)
        spans = sum(len(text) for text in node.values()) + names
        return {
            "layer0_taps": [{"name": name, "width": width, "bits": [0x8000, 0x7BFF, 0x0001, 0xFBFF] * (8 * width)}
                            for name, width in roster],
            "final_placement": {"nodes": [dict(node, subgraph_index=0)]},
            "layer0_final_graph": {"schema": "airk.private.ort-final-graph-metadata/1",
                                   "complete": True, "failure": None, "shim_status": 0, "ort_error_code": 0,
                                   "encoded_bytes": 4096, "reported_counts": [1, 5, 10, spans, 0, len(rows)],
                                   "rows": rows},
        }

    def test_observation_requires_exact_finite_half_taps_and_the_actual_typed_graph(self):
        self.probe["validate_layer0_evidence"](self.layer0_evidence(), self.component)
        for bad_bits in (0x7C00, 0xFC00, 0x7E00, -1, 0x10000, False):
            with self.subTest(bits=bad_bits), self.assertRaisesRegex(ValueError, "finite raw binary16"):
                wrong = self.layer0_evidence()
                wrong["layer0_taps"][0]["bits"][0] = bad_bits
                self.probe["validate_layer0_evidence"](wrong, self.component)
        for edit, message in (
            (lambda item: item["layer0_taps"].reverse(), "tap name, order or width"),
            (lambda item: item["layer0_taps"][2].update(width=2048), "tap name, order or width"),
            (lambda item: item["layer0_taps"][3]["bits"].pop(), "finite raw binary16"),
            (lambda item: item["layer0_final_graph"].update(complete=False), "complete native graph"),
            (lambda item: item["layer0_final_graph"]["rows"][1].update(version=0), "graph node identity"),
        ):
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                wrong = self.layer0_evidence()
                edit(wrong)
                self.probe["validate_layer0_evidence"](wrong, self.component)

    def test_tap_must_be_a_cuda_half_graph_output_with_exact_width(self):
        for field, changed in (("element_type", 1), ("rank", 2)):
            with self.subTest(field=field):
                wrong = self.layer0_evidence()
                graph = wrong["layer0_final_graph"]
                tap = next(row for row in graph["rows"] if row["kind"] == "value" and row["value"] == 1)
                tap[field] = changed
                if field == "rank":
                    width = next(row for row in graph["rows"]
                                 if row["kind"] == "dimension" and row["value"] == 1 and row["index"] == 2)
                    graph["rows"].remove(width)
                    graph["reported_counts"][-1] -= 1
                with self.assertRaisesRegex(ValueError, "type, width or CUDA origin"):
                    self.probe["validate_layer0_evidence"](wrong, self.component)

    def test_projection_taps_bind_their_own_exact_order_and_typed_boundary(self):
        _, _, roster = self.probe["TAP_SCOPES"]["layer0-projection-t32"]
        validate = self.probe["validate_layer0_evidence"]
        value = self.layer0_evidence(self.PROJECTIONS)
        validate(value, self.component, roster)
        with self.assertRaisesRegex(ValueError, "tap name, order or width"):
            validate(value, self.component)
        value["layer0_taps"][1], value["layer0_taps"][2] = value["layer0_taps"][2], value["layer0_taps"][1]
        with self.assertRaisesRegex(ValueError, "tap name, order or width"):
            validate(value, self.component, roster)
        wrong = self.layer0_evidence()
        wrong["layer0_taps"] = self.layer0_evidence(self.PROJECTIONS)["layer0_taps"]
        with self.assertRaisesRegex(ValueError, "graph output boundary"):
            validate(wrong, self.component, roster)

    def test_only_layer0_observations_accept_a_comparison_failure_with_finite_scores(self):
        row = {
            "name": "diagnostic-all-live-32", "tokens": 32,
            "input_digest": "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335",
            "raw_scores": [0.125] * 8, "elapsed_us": 21, "native_call_elapsed_us": 20,
            "matching_fp16_raw_error": .025, "matching_fp16_mass_error": .0001,
            "matching_fp16_argmax_matches": True, "original_fp32_raw_error": .02,
            "original_fp32_mass_error": .0001, "original_fp32_argmax_matches": True,
        }
        for mode in ("layer0-t32", "layer0-projection-t32"):
            with self.subTest(mode=mode):
                self.assertEqual(self.probe["validate_t32_row"]([row], mode), row)
        for mode in ("t32", "bounded-t32"):
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                self.probe["validate_t32_row"]([row], mode)
        row["raw_scores"][0] = float("nan")
        with self.assertRaisesRegex(ValueError, "finite T32 scores"):
            self.probe["validate_t32_row"]([row], "layer0-t32")


class CompactLayer0WiderReplayControls(unittest.TestCase):
    def test_replay_requires_exact_native_t1024_scores_and_finite_complete_taps(self):
        replay = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/replay_layer0.py"))
        validate = replay["validated_t1024_taps"]
        common = {"complete": True, "error": None, "native_forward_returned": True, "forward_synchronized": True}
        score = {
            "name": "native-all-live-1024", "tokens": 1024,
            "input_digest": "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357",
            "raw_scores": [1.0, 0.0, 2, 3, 4, 5, 6, 7],
        }
        capture = dict(common, schema="airk.private.compact-fp32-qkv-layer0-t1024/1",
                       numeric_forward_attempts=1, rows=[copy.deepcopy(score)])
        sealed_row = dict(score, forward_synchronized=True)
        sealed = dict(common, schema="airk.private.compact-fp32-qkv-development-batch/1",
                      numeric_forward_attempts=3, rows=[{}, sealed_row, {}])
        taps = [{"name": name, "width": width, "bits": [0] * (1024 * width)}
                for name, width in CompactLayer0Controls.TAPS]
        capture["layer0_taps"] = taps
        capture["rows"][0]["raw_scores"][0] += 2 ** -30
        self.assertIs(validate(capture, sealed), taps)

        capture["rows"][0]["raw_scores"][1] = -0.0
        with self.assertRaisesRegex(ValueError, "captured raw scores differ"):
            validate(capture, sealed)
        capture["rows"][0]["raw_scores"][1] = 0.0

        sealed_row["forward_synchronized"] = False
        with self.assertRaisesRegex(ValueError, "sealed native T1024 row did not synchronize"):
            validate(capture, sealed)
        sealed_row["forward_synchronized"] = True

        taps[0]["bits"][0] = 0x7C00
        with self.assertRaisesRegex(ValueError, "complete finite binary16"):
            validate(capture, sealed)
        taps[0]["bits"][0] = 0
        taps[0]["bits"].pop()
        with self.assertRaisesRegex(ValueError, "complete finite binary16"):
            validate(capture, sealed)


class CompactOwnedExitControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.probe = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"))

    def owned_exit(self, status):
        return {
            "exit_code": status, "timed_out": False, "log_oversized": False, "owned_leader_pid": 42,
            "owned_descendants_settled": True, "owned_job_forced_termination": False,
            "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
            "handle_close_proof": {
                "forced": False, "settled": True, "final_active": 0, "final_member_pids": [],
                "retained_exits": [{"pid": 42, "exit_observed": True, "exit_code": status}],
            },
        }

    def test_natural_access_violation_preserves_the_actual_windows_status(self):
        with self.assertRaisesRegex(ValueError, r"Windows access violation \(0xC0000005\)"):
            self.probe["observed_native_exit"](self.owned_exit(3221225477))

    def test_termination_and_unsettled_job_take_priority_over_status(self):
        for changes, error, message in [
            ({"timed_out": True, "owned_job_forced_termination": True}, TimeoutError, "original Compact phase deadline"),
            ({"log_oversized": True, "owned_job_forced_termination": True}, ValueError, "stdout exceeded"),
            ({"owned_job_forced_termination": True}, ValueError, "Job did not settle naturally"),
            ({"owned_job_final_active_processes": 1}, ValueError, "Job did not settle naturally"),
        ]:
            with self.subTest(changes=changes), self.assertRaisesRegex(error, message):
                owned = self.owned_exit(0xC0000005)
                owned.update(changes)
                self.probe["observed_native_exit"](owned)

    def test_success_and_ordinary_native_refusal_statuses_pass_through_for_validation(self):
        for code in (0, 101):
            with self.subTest(status=code):
                self.assertEqual(self.probe["observed_native_exit"](self.owned_exit(code)), code)

    def test_missing_leader_exit_observation_cannot_attest_access_violation(self):
        owned = self.owned_exit(0xC0000005)
        owned["handle_close_proof"]["retained_exits"][0]["exit_observed"] = False
        with self.assertRaisesRegex(ValueError, "was not observed at handle close"):
            self.probe["observed_native_exit"](owned)


class CompactGraphControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.probe = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"))
        cls.component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))

    def graph_evidence(self):
        identity = {"input_sha256": "a" * 64, "profile": "b" * 64, "process_id": 23,
                    "native_binary_sha256": "c" * 64}
        events = [
            {"schema": "airk.arbiter.native-operation/1", "sequence": sequence,
             "identity": dict(identity), "kind": "load", "ordinal": 0, "phase": phase,
             "clock": "windows_performance_counter", "operation_input_digest": None,
             "started_monotonic_ns": 1, "observed_monotonic_ns": observed, "expires_monotonic_ns": 10,
             "native_success": success, "native_call_elapsed_us": duration}
            for sequence, phase, observed, success, duration in
            [(1, "started", 1, None, None), (2, "returned", 2, True, 5)]
        ]
        placement = {"scope": "onnxruntime-final-session-state-ep-placement", "complete": True,
                     "shim_status": 0, "ort_error_code": 0, "reported_nodes": 1, "reported_string_bytes": 30,
                     "nodes": [{"subgraph_index": 0, "name": "shape", "domain": "", "op": "Shape",
                                "ep": "CPUExecutionProvider"}]}
        graph = {
            "schema": "airk.private.ort-final-graph-metadata/1", "complete": True, "failure": None,
            "shim_status": 0, "ort_error_code": 0, "encoded_bytes": 435, "reported_counts": [1, 0, 0, 30, 0, 3],
            "rows": [
                {"kind": "graph", "graph": 0, "parent_graph": None, "parent_node": None,
                 "attribute": "", "outer_bindings": "not_applicable"},
                {"kind": "node", "graph": 0, "node": 0, "name": "shape", "domain": "", "op": "Shape",
                 "ep": "CPUExecutionProvider", "version": 19},
                {"kind": "end", "graph": 0},
            ],
        }
        value = {
            "numeric_forward_attempts": 0, "native_forward_returned": False, "forward_synchronized": False,
            "rows": [], "input_refusals": [], "runtime_profile": None, "load_us": 6, "load_native_call_us": 5,
            "elapsed_us_to_final_report_start": 11, "original_expires_monotonic_ns": 10,
            "request": {"sha256": identity["input_sha256"]}, "profile": identity["profile"],
            "pid": identity["process_id"], "native_binary_sha256": identity["native_binary_sha256"],
            "graph_observation": {
                "session_created": True, "placement": placement, "graph_metadata": graph,
                "logs": {"seen": 0, "selected": 0, "retained": 0, "omitted": 0,
                         "bytes": 0, "truncated": False, "rows": []},
                "teardown": {"pre_sync_observed": True, "session_drop_attempted": True,
                             "session_drop_returned": True, "post_sync_observed": True,
                             "pending_claims": 0, "poisoned": False, "errors": []},
            },
        }
        return value, events

    def test_load_only_graph_accepts_observed_cpu_placement_without_forward(self):
        value, events = self.graph_evidence()
        self.probe["validate_graph"](value, events, self.component)
        with self.assertRaisesRegex(ValueError, "load/numeric"):
            self.component["validate_operations"](events, value)

    def test_load_only_graph_requires_zero_forward_complete_graph_and_settled_load(self):
        changes = [
            ("scorer work", lambda value, events: value.update(numeric_forward_attempts=1)),
            ("did not settle", lambda value, events: value["graph_observation"]["teardown"].update(pending_claims=1)),
            ("placement required", lambda value, events: value["graph_observation"]["placement"].update(complete=False)),
            ("full node cross-map", lambda value, events:
                value["graph_observation"]["placement"]["nodes"][0].update(ep="CUDAExecutionProvider")),
            ("bounded graph Load", lambda value, events:
                value.update(final_report_retention_exceeded_original_deadline=True)),
            ("Load-only start-return", lambda value, events: events.pop()),
            ("native return proof", lambda value, events: events[1].update(native_success=False)),
            ("independent returned marker", lambda value, events: events[1].update(native_call_elapsed_us=4)),
        ]
        for message, change in changes:
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                value, events = self.graph_evidence()
                change(value, events)
                self.probe["validate_graph"](value, events, self.component)


class FinalProofControls(unittest.TestCase):
    def callbacks(self, events):
        return {name: lambda name=name: (events.append(name), row(name))[1] for name in M.PROOFS}

    def test_last_clock_follows_all_six_callbacks(self):
        events = []; receipt = {'complete': True, 'failure': None}
        M.finish(receipt, 0, 600, self.callbacks(events), clock=lambda: (events.append('clock'), 599)[1], utc_clock=lambda: (events.append('utc'), M.datetime(2026, 11, 17, tzinfo=M.timezone.utc))[1])
        self.assertTrue(receipt['complete'])
        self.assertEqual(events, ['graph', 'weights', 'manifest', 'stdout', 'worker', 'inputs', 'utc', 'clock'])

    def test_slow_final_inputs_cannot_pass(self):
        events = []; receipt = {'complete': True, 'failure': None}
        M.finish(receipt, 0, 600, self.callbacks(events), clock=lambda: 600)
        self.assertFalse(receipt['complete']); self.assertTrue(receipt['whole_phase_expired'])

    def test_missing_callback_does_not_skip_remaining_proofs(self):
        events = []; callbacks = self.callbacks(events); del callbacks['manifest']
        receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertIn('inputs', events)
        self.assertEqual(receipt['final_errors'][0]['proof'], 'manifest')

    def test_original_failure_survives_successful_callbacks(self):
        original = {'type': 'ChildFailure'}
        receipt = M.finish({'complete': True, 'failure': original}, 0, 600, self.callbacks([]), clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertIs(receipt['failure'], original)

    def test_malformed_callback_rows_refuse(self):
        for bad in (None, {}, row(sha='z' * 64), row(size=-1)):
            callbacks = self.callbacks([]); callbacks['graph'] = lambda: bad
            with self.subTest(bad=bad):
                receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
                self.assertFalse(receipt['complete'])

    def test_hash_exception_keeps_all_later_proofs(self):
        events = []; callbacks = self.callbacks(events)
        def failed(): raise OSError('retained hash denied')
        callbacks['weights'] = failed
        receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertEqual(events[-1], 'inputs')
        self.assertEqual(receipt['final_errors'][0]['type'], 'OSError')

    def test_post_validation_drift_refuses(self):
        receipt = {'complete': True, 'failure': None, 'observed_proofs': {'graph': row('graph', sha='b' * 64)}}
        M.finish(receipt, 0, 600, self.callbacks([]), clock=lambda: 1)
        self.assertFalse(receipt['complete'])

    def test_nonfinite_or_backwards_clock_refuses(self):
        for now in (-1, float('nan'), float('inf')):
            receipt = M.finish({'complete': True, 'failure': None}, 0, 600, self.callbacks([]), clock=lambda: now)
            self.assertFalse(receipt['complete'])


class GuardDeadlineControls(unittest.TestCase):
    def test_guard_uses_only_remaining_original_deadline(self):
        calls = []
        def runner(command, **kwargs):
            calls.append(kwargs['timeout'])
            return types.SimpleNamespace(stdout='{"identity":"literal"}')
        with patch.object(M, 'pinned'):
            value = M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 599.75, runner=runner)
        self.assertEqual(calls, [.25]); self.assertEqual(value, {'identity': 'literal'})

    def test_guard_timeout_preserves_original_exception(self):
        failure = M.subprocess.TimeoutExpired('literal guard', .25)
        def runner(command, **kwargs):
            self.assertEqual(kwargs['timeout'], .25)
            raise failure
        with patch.object(M, 'pinned'):
            with self.assertRaises(M.subprocess.TimeoutExpired) as caught:
                M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 599.75, runner=runner)
        self.assertIs(caught.exception, failure)

    def test_expired_guard_is_not_launched(self):
        def runner(*args, **kwargs): self.fail('expired guard launched')
        with patch.object(M, 'pinned'), self.assertRaises(TimeoutError):
            M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 600, runner=runner)

    def test_guard_postprocessing_cannot_pass_after_original_deadline(self):
        values = iter([599, 600])
        with patch.object(M, 'pinned'), self.assertRaises(TimeoutError):
            M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: next(values), runner=lambda *a, **kw: types.SimpleNamespace(stdout='{}'))

class OutputControls(unittest.TestCase):
    def test_worker_cannot_spawn_or_connect(self):
        for event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'socket.connect'):
            with self.subTest(event=event), self.assertRaises(ValueError): M.output_access(event, (), ROOT)

    def test_dotdot_write_escape_refuses(self):
        target = ROOT / 'artifacts/absent-output'
        with self.assertRaisesRegex(ValueError, 'outside'):
            M.output_access('open', (str(target / '../escape'), 'w', M.os.O_CREAT | M.os.O_WRONLY), target, owner=lambda p: None)

    def test_new_write_parent_is_checked(self):
        target = ROOT / 'artifacts/absent-output'; seen = []
        M.output_access('open', (str(target / 'new'), 'w', M.os.O_CREAT | M.os.O_WRONLY), target, owner=seen.append)
        self.assertEqual(seen, [target])

    def test_foreign_output_parent_refuses(self):
        def foreign(path): raise ValueError('foreign owner')
        with self.assertRaisesRegex(ValueError, 'foreign'):
            M.output_access('os.mkdir', (str(ROOT / 'artifacts/new'),), ROOT / 'artifacts', owner=foreign)

    def test_manifest_binds_actual_graph_size_and_roster(self):
        plan = fixture(); graph = row('model.onnx'); weights = row('weights.bin', size=3441397760)
        manifest = {'qualification': False, 'native_execution': False, 'graph_sha256': graph['sha256'], 'attention_nodes': [{'name': f'/airk_g5/attention/{i}/efficient', 'operator': 'GroupQueryAttention', 'domain': 'com.microsoft'} for i in range(28)]}
        M.validate_manifest(manifest, plan, graph, weights)
        for field in ('qualification', 'native_execution', 'graph_sha256', 'attention_nodes'):
            bad = copy.deepcopy(manifest); bad[field] = True if field in ('qualification', 'native_execution') else 'changed'
            with self.subTest(field=field), self.assertRaises(ValueError): M.validate_manifest(bad, plan, graph, weights)
        with self.assertRaises(ValueError): M.validate_manifest(manifest, plan, graph, row('weights.bin', size=1))


class PreparationControls(unittest.TestCase):
    def fp32(self):
        return {'schema': 'airk.private.owned-compact-bounded-hybrid-profile/2', 'complete': True,
                'failure': None, 'execution_user_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
                'whole_phase_deadline_not_refreshed': True, 'whole_phase_expired': False,
                'qualification': False, 'product_admission': False, 'elapsed_seconds': 119.5,
                'graph_custody': {'opened_handles': 15, 'closed_handles': 15, 'close_errors': []},
                'final_guard_errors': {}, 'final_proof_errors': [],
                'final_proof_success': ['native_result', 'operations', 'stdout', 'runtime_profile']}


    def readiness(self, kind='tests'):
        return {'schema': 'airk-readiness-command/1', 'status': 'completed', 'passed': True,
                'exit_code': 0, 'runner_exit_code': 0, 'interrupted': False,
                'inputs_unchanged': True, 'runner_unchanged': True, 'runtime_unchanged': True,
                'command_inputs_unchanged': True, 'selected_tests': 25,
                'command': ['cargo', 'nextest', 'run', '--profile', 'ci', '--locked', '-p', 'airk-retrieval', '--all-features']
                if kind == 'tests' else ['python', 'scripts/dev-check.py', '-p', 'airk-retrieval', '--all-features']}

    def test_original_source_identity_cannot_be_replaced(self):
        with self.assertRaisesRegex(ValueError, 'source plan identity'):
            M.source_facts(row(), 'compact')

    def test_focus_has_no_qualified_fp32_prerequisite(self):
        with self.assertRaisesRegex(ValueError, 'only Compact and Large'):
            M.source_facts(row(), 'focus')

    def test_wrong_model_receipt_refuses_before_open(self):
        evidence = {'model': 'large',
                    'first_fp32': row('compact-proof', sha='1abbd515e534f4056e3c48380fffd290654e0b92dea67088ae291512f02cf2a2'),
                    'resource_fp32': row('large-timing', sha='8ff0d3ed5152ea533a46ab1dc258b96b536aa7df2948b49400f275aabda8fc7b')}
        with self.assertRaisesRegex(ValueError, 'exact selected model'):
            M.fp32_facts(evidence, {}, lambda proof: self.fail('wrong-model proof opened'), None, {})

    def test_original_source_graph_and_closure_are_reopened(self):
        original = {'schema': 'airk.private.g5-original3-export-run-plan/1', 'models': [
            {'model': 'compact', 'original_graph': row('graph'), 'source_closure': row('closure'), 'source_closure_authority': row('authority')}]}
        source = row('source', sha='cdf57c6c86b490cf2b6656fa8c48fbb4ff3b6ccb4d5c5be9705a71a2866b735c')
        observed = []
        with patch.object(M, 'read', return_value=original), patch.object(M, 'pinned', side_effect=lambda value, check: observed.append(value['path'])):
            M.source_facts(source, 'compact')
        self.assertEqual(observed, [source['path'], str(ROOT / 'graph'), str(ROOT / 'closure'), str(ROOT / 'authority')])

    def test_missing_or_changed_source_member_refuses(self):
        original = {'schema': 'airk.private.g5-original3-export-run-plan/1', 'models': [
            {'model': 'compact', 'original_graph': row('graph'), 'source_closure': row('closure'), 'source_closure_authority': row('authority')}]}
        source = row('source', sha='cdf57c6c86b490cf2b6656fa8c48fbb4ff3b6ccb4d5c5be9705a71a2866b735c')
        for name in ('graph', 'closure', 'authority'):
            def verify(value, check):
                if Path(value['path']).name == name: raise ValueError('changed source')
            with self.subTest(name=name), patch.object(M, 'read', return_value=original), patch.object(M, 'pinned', side_effect=verify), self.assertRaisesRegex(ValueError, 'changed source'):
                M.source_facts(source, 'compact')

    def test_runtime_baseline_cannot_be_replaced(self):
        with self.assertRaisesRegex(ValueError, 'baseline identity'):
            M.runtime_facts(row(), [])

    def test_runtime_baseline_plus_added_files_are_explicit(self):
        baseline = row('baseline', sha='23f9fbfa770d18ee2549dbd6c1b553e204ab8e387ba51b92b620dd24c10189cb')
        one, added, outside = row('one'), row('added'), row('outside')
        pins = {x['path']: x for x in (one, added, outside)}
        value = {'schema': 'airk.private.exact-python-runtime-files/1', 'files': {one['path']: one['sha256'], outside['path']: outside['sha256']}}
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value={one['path'], added['path']}), patch.object(M, 'pin', side_effect=lambda path, check: pins[str(path)]):
            manifest, rest = M.runtime_facts(baseline, [ROOT])
        self.assertEqual(manifest['files'], {one['path']: one, added['path']: added})
        self.assertEqual(rest, {outside['path']: outside})

    def test_changed_or_missing_runtime_baseline_refuses(self):
        baseline = row('baseline', sha='23f9fbfa770d18ee2549dbd6c1b553e204ab8e387ba51b92b620dd24c10189cb')
        value = {'schema': 'airk.private.exact-python-runtime-files/1', 'files': {str(ROOT / 'missing'): 'b' * 64}}
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value=set()), patch.object(M, 'pin', return_value=row('missing')), self.assertRaisesRegex(ValueError, 'changed/missing'):
            M.runtime_facts(baseline, [ROOT])
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value=set()), patch.object(M, 'pin', side_effect=FileNotFoundError), self.assertRaises(FileNotFoundError):
            M.runtime_facts(baseline, [ROOT])

    def test_runtime_interpreter_pin_may_also_be_retained_process_evidence(self):
        shared = row('python.exe')
        pins = {shared['path']: shared}
        self.assertEqual(M.merge_input_pins({}, pins, pins), pins)
        changed = dict(shared, sha256='b' * 64)
        with self.assertRaisesRegex(ValueError, 'conflicting input pin'):
            M.merge_input_pins(pins, {shared['path']: changed})

    def test_actual_scoring_success_shape(self):
        M.successful_fp32(self.fp32(), M.FP32['compact'])

    def test_fp32_failed_late_missing_or_retagged_refuses(self):
        for key, value in [('complete', False), ('failure', {}), ('whole_phase_expired', True), ('elapsed_seconds', 120),
                           ('qualification', True), ('product_admission', True), ('execution_user_sid', 'other'),
                           ('final_proof_success', ['native_result']), ('final_proof_errors', [{}]),
                           ('final_guard_errors', {'output': 'changed'}),
                           ('graph_custody', {'opened_handles': 15, 'closed_handles': 14, 'close_errors': []})]:
            receipt = self.fp32(); receipt[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): M.successful_fp32(receipt, M.FP32['compact'])
        for key in self.fp32():
            receipt = self.fp32(); del receipt[key]
            with self.subTest(missing=key), self.assertRaises(ValueError): M.successful_fp32(receipt, M.FP32['compact'])

    def test_large_scoring_requires_the_equivalent_predecessor_basis(self):
        receipt = {'schema': 'airk.private.owned-large-cuda-batch-view-scoring/1', 'complete': True,
                   'failure': None, 'execution_user_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
                   'qualification': False, 'elapsed_seconds': 84, 'reference_basis': 'equivalent_predecessor',
                   'model_requests_dispatched': True}
        M.successful_fp32(receipt, M.FP32['large'])
        for wrong in ('same_artifact', None):
            with self.subTest(wrong=wrong), self.assertRaisesRegex(ValueError, 'reference basis'):
                M.successful_fp32(dict(receipt, reference_basis=wrong), M.FP32['large'])

    def test_actual_readiness_shapes(self):
        M.successful_readiness(self.readiness(), 'tests')
        M.successful_readiness(self.readiness('clippy'), 'clippy')

    def test_readiness_failures_and_missing_predicates_refuse(self):
        for key, value in [('passed', False), ('exit_code', 1), ('runner_exit_code', 1), ('interrupted', True),
                           ('inputs_unchanged', False), ('runner_unchanged', False), ('runtime_unchanged', False),
                           ('command_inputs_unchanged', False), ('selected_tests', 0), ('command', ['cargo', 'check'])]:
            receipt = self.readiness(); receipt[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): M.successful_readiness(receipt, 'tests')

    def test_fp32_cannot_substitute_for_g5(self):
        for kind in ('clippy', 'tests'):
            with self.assertRaises(ValueError): M.successful_readiness(self.fp32(), kind)

    def test_junit_missing_required_tests_refuses(self):
        with self.assertRaisesRegex(ValueError, 'identities absent'):
            M.validate_junit(b'<testsuites><testcase classname="airk-retrieval" name="unrelated"/></testsuites>', 1)

    def test_junit_skipped_failed_and_duplicate_refuse(self):
        for tag in ('skipped', 'failure', 'error'):
            raw = ('<testsuites><testcase name="literal"><' + tag + '/></testcase></testsuites>').encode()
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, 'failure/skipped/error'):
                M.validate_junit(raw, 1)
        raw = b'<testsuites><testcase name="literal"/><testcase name="literal"/></testsuites>'
        with self.assertRaisesRegex(ValueError, 'unique identities'): M.validate_junit(raw, 2)

    def test_junit_entity_and_count_refuse(self):
        with self.assertRaisesRegex(ValueError, 'literal JUnit'):
            M.validate_junit(b'<!DOCTYPE testsuites><testsuites/>', 1)
        with self.assertRaisesRegex(ValueError, 'selected count'):
            M.validate_junit(b'<testsuites/>', 1)

    def test_g5_supplement_is_mandatory_and_independent_of_fp32(self):
        self.assertEqual(len(M.G5_TESTS), 27)
        self.assertIn('arbiter::runtime::cpu_source_transition::tests::g5_supplement_binds_exact_current_source_without_relabelling_old_row', M.G5_TESTS)
        self.assertIn('arbiter::runtime::cpu_source_transition::tests::cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source', M.G5_TESTS)
        self.assertIn('crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs', M.G5_SOURCES)
        self.assertIn('crates/airk-retrieval/src/arbiter/runtime/cuda/fp16.rs', M.G5_SOURCES)
        self.assertIn('crates/airk-retrieval/src/arbiter/external_data/g5.rs', M.G5_SOURCES)

class G5EvidenceControls(unittest.TestCase):
    def test_actual_onnx_source_owner_is_crate_root(self):
        self.assertIn('crates/airk-retrieval/src/onnx.rs', M.G5_SOURCES)
        self.assertNotIn('crates/airk-retrieval/src/arbiter/onnx.rs', M.G5_SOURCES)
        self.assertTrue((ROOT / 'crates/airk-retrieval/src/onnx.rs').is_file())
    def test_g5_source_is_reopened_and_missing_group_entry_refuses(self):
        with patch.object(M, 'G5_SOURCES', ('literal.rs',)), patch.object(M, 'pin', return_value=row('literal.rs')):
            self.assertEqual(M.g5_source_facts({'files': {'literal.rs': 'a' * 64}}, ROOT), {str(ROOT / 'literal.rs'): row('literal.rs')})
            with self.assertRaisesRegex(ValueError, 'absent/changed'):
                M.g5_source_facts({'files': {}}, ROOT)
            with self.assertRaisesRegex(ValueError, 'absent/changed'):
                M.g5_source_facts({'files': {'literal.rs': 'b' * 64}}, ROOT)

    def test_missing_live_g5_source_refuses(self):
        with patch.object(M, 'G5_SOURCES', ('literal.rs',)), patch.object(M, 'pin', side_effect=FileNotFoundError), self.assertRaises(FileNotFoundError):
            M.g5_source_facts({'files': {'literal.rs': 'a' * 64}}, ROOT)

    def test_literal_g5_and_transition_roster_passes(self):
        # Independent expected identities from the reviewed G5/CPU contracts.
        fp16 = ['inherited_exact_settings_pass_and_late_setter_or_drift_refuses',
                'fp32_policy_remains_distinct_from_each_model_fp16_policy',
                'context_boundaries_and_exact_original_mask_positions_refuse_unsupported_input',
                'focus_all_live_and_extreme_left_pad_pass_without_renumbering',
                'half_initializer_bit_loading_preserves_half_values_without_rounding',
                'actual_final_placement_needs_exact_complete_attention_roster',
                'qualified_product_math_does_not_force_attention_debug_logging']
        external = ['three_literal_selected_rosters_admit_actual_half_boundaries',
                    'wrong_scale_grouping_causal_and_bias_refuse_before_session',
                    'wrong_half_cast_and_missing_layer_refuse', 'focus_bias_and_wrong_head_count_refuse',
                    'all_f32_and_overlap_are_not_half_weight_closure',
                    'large_half_metadata_is_additive_and_never_allocates_tensor_bytes',
                    'unknown_quantized_half_closure_refuses',
                    'focus_unwritten_padded_queries_require_explicit_sanitization']
        cpu = ['closed_profile_refuses_neural_metadata_fit_and_source_drift',
               'cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source',
               'current_runtime_predicate_preserves_the_exact_legacy_cpu_literal',
               'exact_legacy_descriptor_and_public_sources_remain_byte_stable',
               'native_version_scope_is_pre_190_exact_release_only',
               'source_row_refuses_wrong_current_core_or_task_policy',
               'publisher_census_row_refuses_old_task_and_preserves_legacy_payload',
               'transition_never_admits_context_or_relabels_a_current_profile',
               'witness_rechecks_the_exact_descriptor_and_keeps_new_input_provenance',
               'g5_supplement_binds_exact_current_source_without_relabelling_old_row']
        external += ['context_present_outputs_are_required_in_exact_order',
                     'context_present_outputs_cannot_collide_escape_or_be_consumed']
        names = ['arbiter::runtime::cuda::fp16::tests::' + x for x in fp16]
        names += ['arbiter::external_data::g5::tests::' + x for x in external]
        names += ['arbiter::runtime::cpu_source_transition::tests::' + x for x in cpu]
        def document(values):
            return ('<testsuites>' + ''.join('<testcase classname="airk-retrieval" name="' + name + '"/>' for name in values) + '</testsuites>').encode()
        M.validate_junit(document(names), 27)
        for omitted in names:
            remaining = [name for name in names if name != omitted]
            with self.subTest(omitted=omitted), self.assertRaisesRegex(ValueError, 'identities absent'):
                M.validate_junit(document(remaining), 26)

class FP32ProcessProofControls(unittest.TestCase):
    def fixture(self, model='compact'):
        large = model == 'large'
        host, device = (48 << 30, 44 << 30) if large else (32 << 30, 24 << 30)
        arena, warm, headroom = (40 << 30, 8 << 30, 4 << 30) if large else (22 << 30, 16 << 30, 2 << 30)
        image = row('native-image')
        sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
        proof = {'pid': 123, 'image': {'path': str(ROOT / 'native-image'), 'sha256': 'a' * 64},
                 'execution_user_sid': sid, 'default_owner_sid': sid}
        if not large:
            proof['component_job'] = {'process_bytes': 34359738368, 'job_bytes': 34359738368,
                                      'flags': 8968, 'active_limit': 2}
        receipt = {'owned': {'owned_leader_pid': 123, 'suspended_assignment': proof,
                            'peak_host_committed_bytes': host - 1}}
        native = {'pid': 123, 'execution_user': sid, 'default_owner': sid, 'native_binary_sha256': 'a' * 64,
                  'actual_job': {'process_memory_limit_bytes': host, 'job_memory_limit_bytes': host, 'flags': 8968},
                  'native_host_reservation_bytes': host, 'peak_process_rss_bytes': host - 1,
                  'applied_capacity': {'arena_limit_bytes': arena, 'warm_extra_bytes': warm, 'headroom_bytes': headroom},
                  'bounds': {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'deadline_ms': 120000,
                             'device_limit_bytes': device, 'host_limit_bytes': host},
                  'profile': '7e4481caaf7034a70779702edad73a0eb08321418d1b785a4b7f4f891522bdac' if large else
                             '840bd478675cbf03f2477c170401af75027d55de82b6ab69933f971a1104531d',
                  'complete': True, 'error': None, 'native_control_cleanup_error': None,
                  'session_teardown_observed': True, 'native_host_charge_release_observed': True,
                  'native_control_cleanup_observed': True, 'qualified': False, 'product_admission': False,
                  'process_cuda_poisoned': False, 'native_control_retained_for_unknown_cuda': False}
        return receipt, native, image, M.FP32[model], lambda owned: True

    def test_both_actual_suspended_os_proof_shapes(self):
        for model in ('compact', 'large'):
            with self.subTest(model=model):
                M.validate_fp32_process(*self.fixture(model))

    def test_missing_or_changed_suspended_proof_refuses(self):
        for key in ('pid', 'image', 'execution_user_sid', 'default_owner_sid', 'component_job'):
            for action in ('missing', 'changed'):
                args = self.fixture()
                proof = args[0]['owned']['suspended_assignment']
                if action == 'missing':
                    del proof[key]
                else:
                    proof[key] = 'other'
                with self.subTest(key=key, action=action), self.assertRaises(ValueError):
                    M.validate_fp32_process(*args)

    def test_original_process_job_device_caps_and_release_are_required(self):
        for model, host in (('compact', 32 << 30), ('large', 48 << 30)):
            for key, value in (('native_host_reservation_bytes', host + 1),
                               ('peak_process_rss_bytes', host + 1),
                               ('applied_capacity', {}), ('native_host_charge_release_observed', False),
                               ('actual_job', {'process_memory_limit_bytes': host, 'job_memory_limit_bytes': host + 1}),
                               ('native_control_retained_for_unknown_cuda', True)):
                args = self.fixture(model)
                args[1][key] = value
                with self.subTest(model=model, key=key), self.assertRaises(ValueError):
                    M.validate_fp32_process(*args)

    def test_actual_pid_and_parent_settlement_are_required(self):
        for value in (124, True):
            args = self.fixture()
            args[0]['owned']['owned_leader_pid'] = value
            with self.assertRaises(ValueError):
                M.validate_fp32_process(*args)
        args = self.fixture('large')
        args[0]['owned']['peak_host_committed_bytes'] = (48 << 30) + 1
        with self.assertRaises(ValueError):
            M.validate_fp32_process(*args)
        with self.assertRaises(ValueError):
            M.validate_fp32_process(*self.fixture('large')[:4], lambda owned: False)



class PreparationFileChecksTests(unittest.TestCase):
    def test_reopens_every_input_on_each_observation_and_refuses_late_drift(self):
        expected = list(range(89))
        first = list(M.preparation_file_checks(lambda index: index, expected))
        self.assertEqual(first, expected)
        changed = set()

        def check_again(index):
            if index == 88:
                raise ValueError('changed last input')
            return index

        with self.assertRaisesRegex(ValueError, 'changed last input'):
            changed.update(M.preparation_file_checks(check_again, expected))
        self.assertEqual(changed, set(range(88)))

    def test_original_deadline_exception_propagates(self):
        def expired(_):
            raise TimeoutError('original deadline')
        with self.assertRaisesRegex(TimeoutError, 'original deadline'):
            list(M.preparation_file_checks(expired, range(100)))


if __name__ == '__main__':
    unittest.main()
