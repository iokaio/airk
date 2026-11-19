# SPDX-License-Identifier: Apache-2.0
"""Literal layer-zero replay controls using CPU tensors and in-memory receipts."""

import copy
import importlib.util
from pathlib import Path
import unittest

import torch


ROOT = Path(__file__).resolve().parents[2]


def load(relative, name):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


R = load("scripts/arbiter-fp16-execution/replay_layer0.py", "layer0_replay")
REFERENCE = load("scripts/arbiter-fp16-reference.py", "layer0_reference")
INPUT = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
NAMES = (
    "/airk_g5/attention/0/q16",
    "/airk_g5/attention/0/k16",
    "/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16",
    "/airk_g5/attention/0/raw16",
)


def receipts():
    sealed = {
        "schema": "airk.private.compact-fp16-bounded-t32-component/1",
        "native_forward_returned": True,
        "forward_synchronized": True,
        "numeric_forward_attempts": 1,
        "complete": False,
        "rows": [{
            "name": "diagnostic-all-live-32", "tokens": 32,
            "input_digest": INPUT, "raw_scores": [-0.0, 1, -2, 3, -4, 5, -6, 7],
        }],
    }
    capture = copy.deepcopy(sealed)
    capture.update(schema="airk.private.compact-fp16-layer0-taps/1", complete=True)
    capture["layer0_taps"] = [
        {"name": name, "width": width, "bits": [0] * (32 * width)}
        for name, width in zip(NAMES, (2048, 1024, 1024, 2048))
    ]
    return capture, sealed


class Layer0ReplayControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def test_decode_preserves_token_head_lane_and_signed_zero(self):
        capture, sealed = receipts()
        q = capture["layer0_taps"][0]
        q["bits"][3 * 2048 + 5 * 128 + 7] = 0x3C00
        q["bits"][2 * 2048 + 4 * 128 + 6] = 0x8000
        tensor = R.head_tensor(torch, R.validated_taps(capture, sealed)[0])
        self.assertEqual(tensor.shape, (1, 16, 32, 128))
        self.assertEqual(tensor.dtype, torch.float16)
        self.assertEqual(tensor[0, 5, 3, 7].item(), 1.0)
        self.assertEqual(tensor.view(torch.int16)[0, 4, 2, 6].item(), -32768)

    def test_both_cpu_paths_against_literal_native_bits(self):
        capture, sealed = receipts()
        q, k, v, raw = [tap["bits"] for tap in capture["layer0_taps"]]
        for token in range(32):
            for head in range(16):
                q[token * 2048 + head * 128] = 0x3C00
                raw[token * 2048 + head * 128] = 0x3C00 if token == 0 else 0x20B0
            for head in range(8):
                if token:
                    k[token * 1024 + head * 128] = 0xC800 if token == 1 else 0xE400
        for head in range(8):
            v[head * 128] = 0x3C00
            v[1024 + head * 128] = 0xC000
        # Negative zero differs in bits even when the numerical error is zero.
        raw[1] = 0x8000
        observed = R.replay(torch, REFERENCE, R.validated_taps(capture, sealed))
        self.assertEqual(observed, {
            "tap_elements_compared": 65536,
            "dense_fp32_softmax": {"max_abs_error": 17 / 131072, "exact_bit_mismatches": 16 * 31 + 1},
            "single_tile_flash": {"max_abs_error": 0.0, "exact_bit_mismatches": 1},
        })
        self.assertFalse(torch.cuda.is_initialized())

    def test_refuses_different_score_bits_and_frozen_row(self):
        for field, value, error in [
            ("input_digest", "a" * 64, "frozen native T32"),
            ("tokens", 32.0, "frozen native T32"),
            ("raw_scores", [0.0, 1, -2, 3, -4, 5, -6, 7], "captured raw scores differ"),
            ("raw_scores", [-0.0, 1, -2, 3, -4, 5, -6, float("nan")], "eight finite"),
        ]:
            capture, sealed = receipts()
            capture["rows"][0][field] = value
            with self.subTest(field=field, value=value), self.assertRaisesRegex(ValueError, error):
                R.validated_taps(capture, sealed)

    def test_refuses_nonfinite_booleans_wrong_roster_and_missing_words(self):
        for word in [0x7C00, 0xFC00, 0x7E00, True, -1, 0x10000]:
            capture, sealed = receipts()
            capture["layer0_taps"][0]["bits"][0] = word
            with self.subTest(word=word), self.assertRaisesRegex(ValueError, "finite binary16"):
                R.validated_taps(capture, sealed)
        capture, sealed = receipts()
        capture["layer0_taps"][2]["bits"].pop()
        with self.assertRaisesRegex(ValueError, "finite binary16"):
            R.validated_taps(capture, sealed)
        capture["layer0_taps"][0], capture["layer0_taps"][1] = capture["layer0_taps"][1], capture["layer0_taps"][0]
        with self.assertRaisesRegex(ValueError, "tap name, order or width"):
            R.validated_taps(capture, sealed)

    def test_refuses_incomplete_or_unsynchronized_capture(self):
        capture, sealed = receipts()
        capture["complete"] = False
        with self.assertRaisesRegex(ValueError, "complete layer-zero capture"):
            R.validated_taps(capture, sealed)
        capture["forward_synchronized"] = False
        with self.assertRaisesRegex(ValueError, "single synchronized"):
            R.validated_taps(capture, sealed)
