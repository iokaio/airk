# SPDX-License-Identifier: Apache-2.0
"""Independent operand controls for the Large multi-tile CPU reference."""

import math
import runpy
import struct
import unittest
from pathlib import Path

import torch


BLOCK = runpy.run_path(str(Path(__file__).with_name("large_flash.py")))["flash_block"]


def half(value):
    return struct.unpack("e", struct.pack("e", value))[0]


class FlashTiles(unittest.TestCase):
    def test_two_tiles_obey_causality_and_normalize_once(self):
        query = torch.zeros(1, 1, 64, 128)
        keys = torch.zeros(1, 1, 128, 128)
        values = torch.arange(128, dtype=torch.float32).view(1, 1, 128, 1).expand(-1, -1, -1, 128)
        visits = []
        actual = BLOCK(torch, query, keys, values, 64, lambda: visits.append(1))
        self.assertEqual(len(visits), 2)
        for index in range(64):
            self.assertTrue((actual[0, 0, index] == (index + 64) / 2).all())

    def test_reverse_rescale_keeps_original_denominator_and_narrows_tile(self):
        # The rightmost block is processed first. Its maximum is 0 and
        # the older block's maximum is sqrt(128), so the old state scales by 1/e.
        query = torch.zeros(1, 1, 64, 128)
        query[..., 0] = 1
        keys = torch.zeros(1, 1, 128, 128)
        keys[..., :64, 0] = half(math.sqrt(128))
        keys[..., :32, 0] = half(-math.sqrt(128))
        values = torch.zeros_like(keys)
        values[..., :32, :] = -21
        values[..., 32:64, :] = 3
        values[..., 64:, :] = 7
        actual = BLOCK(torch, query, keys, values, 64, lambda: None)
        level = half(math.sqrt(128)) / math.sqrt(128)
        low, carried = math.exp(-2 * level), math.exp(-level)
        for row in (0, 17, 63):
            count = row + 1
            numerator = 32 * (-21 * half(low) + 3) + count * carried * 7
            denominator = 32 * (low + 1) + count * carried
            self.assertEqual(actual[0, 0, row, 0].item(), half(numerator / denominator))


if __name__ == "__main__":
    unittest.main()
