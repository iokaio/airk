# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for the Focus Answer-evaluation training tools (no model, no private data).

Run: python -I -B -m unittest discover -s scripts/arbiter/training/focus_answer_eval_r1 -p "test_*.py"
"""
import json, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import focus_text as ft  # noqa: E402
import worker_math as wm  # noqa: E402
import build_examples as be  # noqa: E402
import score  # noqa: E402


class FakeTokenizer:
    """One id per character; enough to exercise width admission."""

    def encode(self, text):
        return [ord(c) % 50000 for c in text]


class FocusTextTests(unittest.TestCase):
    def test_model_view_matches_the_rust_quoting(self):
        self.assertEqual(ft.model_view(r'a\b<c>'), 'a\\\\b\\u003cc\\u003e')

    def test_identifiers_match_the_kernel_cases(self):
        found = ft.identifiers("Make `Gathering::observe` keep the queue when a FollowUp read fails; see plan_followups, "
                               "airk_core::arbiter::Budget and the README. Update GitHub docs for Gathering.", 8)
        self.assertEqual(found, ["Gathering", "observe", "FollowUp", "plan_followups", "Budget"])
        self.assertEqual(ft.identifiers("Rename `x` and `do it`", 4), [])
        self.assertEqual(len(ft.identifiers("snake_case CamelCase other_name", 2)), 2)
        self.assertEqual(ft.identifiers("Where is the constraint defined?", 4), [])

    def test_focus_window_matches_the_kernel_cases(self):
        text = "x\n" * 900 + "fn plan_followups() {}\n" + "é" * 900
        start, end = ft.focus_window(text, "Change plan_followups", 512)
        data = text.encode()
        self.assertTrue(0 < end - start <= 1024 - 42 - 320)
        self.assertTrue(data[start:end].decode().startswith("fn plan_followups"))
        self.assertEqual(ft.focus_window("short hit", "anything", 512), (0, len("short hit")))
        start, end = ft.focus_window("é" * 2000, "q", 512)
        self.assertEqual(start, 0)
        self.assertTrue(end <= 702 and end % 2 == 0)

    def test_compile_refuses_over_width_and_never_crops(self):
        tok = FakeTokenizer()
        ids = ft.compile_live_ids(tok, ft.SOFTWARE_EVIDENCE_V1, "question", "document", 1024)
        self.assertEqual(len(ids), len(ft.PREFIX) + len(ft.pair_body(ft.SOFTWARE_EVIDENCE_V1, "question", "document")) + len(ft.SUFFIX))
        self.assertIsNone(ft.compile_live_ids(tok, ft.SOFTWARE_EVIDENCE_V1, "q" * 900, "d", 1024))
        padded, mask = ft.padded([7, 8], 4)
        self.assertEqual((padded, mask), ([ft.PAD_ID, ft.PAD_ID, 7, 8], [0, 0, 1, 1]))

    def test_window_around_grows_to_target_and_caps_long_ranges(self):
        lines = [f"line {i:03d}" for i in range(1, 201)]
        text, a, b = ft.window_around(lines, 100, 100, target=100, cap=4096)
        self.assertTrue(a < 100 < b and len(text.encode()) >= 100)
        text, a, b = ft.window_around(lines, 1, 200, target=10, cap=50)
        self.assertEqual(a, 1)
        self.assertLessEqual(len(text.encode()), 50)


class MathTests(unittest.TestCase):
    def test_auroc_counts_ties_half(self):
        self.assertEqual(wm.auroc([1, 0], [1.0, 0.0]), 1.0)
        self.assertEqual(wm.auroc([1, 0], [0.0, 1.0]), 0.0)
        self.assertEqual(wm.auroc([1, 0], [0.5, 0.5]), 0.5)
        self.assertIsNone(wm.auroc([1, 1], [0.1, 0.2]))

    def test_ranking_hit_and_mrr(self):
        out = wm.ranking({"a": [(1, 2.0), (0, 1.0)], "b": [(1, 0.0), (0, 1.0), (0, 2.0)]})
        self.assertEqual(out["pairs"], 2)
        self.assertAlmostEqual(out["hit_at_1"], 0.5)
        self.assertAlmostEqual(out["mrr"], (1 + 1 / 3) / 2)

    def test_left_pad_refuses_over_width(self):
        with self.assertRaises(ValueError):
            wm.left_pad([1, 2, 3], 2)
        self.assertEqual(wm.target(1), wm.YES_ID)
        with self.assertRaises(ValueError):
            wm.target(True)

    def test_summary_and_gap(self):
        rows = []
        for split in ("train_sample", "heldout_id", "heldout_path"):
            for pair in range(3):
                for label in (1, 0):
                    rows.append({"id": f"{split}{pair}{label}", "pair_id": f"{split}{pair}", "set": split, "repo": "airk",
                                 "kind": "test", "label": label})
        good = [[r["id"], 0.0, 1.0 if r["label"] else -1.0] for r in rows]
        weak = [[r["id"], 0.0, (1.0 if r["label"] else -1.0) * (1 if r["set"] == "train_sample" else -1)] for r in rows]
        out = score.report(rows, {"scores": {"base": good, "candidate": weak}})
        self.assertEqual(out["arms"]["base"]["gaps"]["airk:train_minus_heldout_id"]["auroc"], 0.0)
        self.assertEqual(out["arms"]["candidate"]["gaps"]["airk:train_minus_heldout_id"]["auroc"], 1.0)
        self.assertEqual(out["memorization_signal"]["airk:train_minus_heldout_path"]["auroc"], 1.0)


class BuilderTests(unittest.TestCase):
    def test_query_cap_and_path_heldout_are_deterministic(self):
        self.assertEqual(be.cap_query("a b " * 400)[:4], "a b ")
        self.assertLessEqual(len(be.cap_query("word " * 400)), be.QUERY_CHARS)
        share = sum(be.path_heldout("airk", f"src/f{i}.rs") for i in range(4000)) / 4000
        self.assertTrue(0.07 < share < 0.13)
        self.assertEqual(be.path_heldout("airk", "x.rs"), be.path_heldout("airk", "x.rs"))

    def test_bm25_prefers_matching_windows_and_respects_exclusion(self):
        # Terms in more than a quarter of the windows are ignored, so the corpus needs filler.
        filler = [f"filler window number {i} about storage" for i in range(20)]
        index = be.Bm25(["fn render_chapter() {}", "unrelated text here", "render chapter body"] + filler)
        self.assertEqual(index.top("render_chapter", lambda i: True, 3)[0], 0)
        self.assertNotIn(0, index.top("render_chapter", lambda i: i != 0, 3))

    def test_interleave_spreads_replay(self):
        out = be.interleave(list(range(10)), ["r1", "r2"])
        self.assertEqual(len(out), 12)
        self.assertLess(out.index("r1"), out.index("r2"))


@unittest.skipUnless(__import__("importlib").util.find_spec("onnxruntime"), "onnxruntime absent")
class ExportTests(unittest.TestCase):
    def test_quantizer_is_deterministic_blockwise_8bit(self):
        import numpy as np
        import export_candidate as ex
        rng = np.random.default_rng(0)
        weight = rng.standard_normal((8, 64)).astype(np.float32)
        packed, scales = ex.quantize(weight)
        self.assertEqual(len(packed), 8 * 2 * 32)
        self.assertEqual(len(scales), 8 * 2 * 4)
        self.assertEqual((packed, scales), ex.quantize(weight))
        q = np.frombuffer(packed, np.uint8).reshape(8, 2, 32).astype(np.float32) - 128
        s = np.frombuffer(scales, np.float32).reshape(8, 2, 1)
        approx = (q * s).reshape(8, 64)
        self.assertLess(float(np.max(np.abs(approx - weight))), float(np.max(np.abs(weight))) / 100)

    def test_safetensors_reader_handles_bf16_and_f32(self):
        import numpy as np
        import struct as st
        import export_candidate as ex
        f32 = np.arange(4, dtype=np.float32)
        bf16 = (np.array([1.0, -2.0], np.float32).view(np.uint32) >> 16).astype(np.uint16)
        header = {"a": {"dtype": "F32", "shape": [4], "data_offsets": [0, 16]},
                  "b": {"dtype": "BF16", "shape": [2], "data_offsets": [16, 20]}}
        raw = json.dumps(header).encode()
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "m.safetensors"
            path.write_bytes(st.pack("<Q", len(raw)) + raw + f32.tobytes() + bf16.tobytes())
            reader = ex.SafeTensors(path)
            self.assertTrue(np.array_equal(reader.get("a"), f32))
            self.assertTrue(np.array_equal(reader.get("b"), np.array([1.0, -2.0], np.float32)))


if __name__ == "__main__":
    unittest.main()
