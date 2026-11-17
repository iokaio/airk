# SPDX-License-Identifier: Apache-2.0
import unittest
from attention_backend import observe

NODES = ("/airk_g5/attention/0/efficient", "/airk_g5/attention/1/efficient")
ROSTER = ("warmup/pair-0", "sample-1/pair-0")
def line(node, operator="GroupQueryAttention", dtype="fp16", backend="FLASH_ATTENTION"):
    return f"Operator={operator} Node={node} DataType={dtype} SdpaKernel={backend}\n"
def capture(nodes=NODES, forwards=2):
    return "".join(line(n) for _ in range(forwards) for n in nodes).encode()
def check(raw, **kwargs):
    return observe(raw, nodes=NODES, forward_roster=ROSTER,
                   completed_forwards=ROSTER, operator="GroupQueryAttention", backend="FLASH_ATTENTION", **kwargs)

class Controls(unittest.TestCase):
    def test_complete_original_denominator_and_raw_identity(self):
        raw = capture()
        result = check(raw)
        self.assertEqual([g["forward"] for g in result["groups"]], ["warmup/pair-0", "sample-1/pair-0"])
        self.assertEqual(result["raw_bytes"], len(raw))
        self.assertFalse(result["qualification_granted"])
        self.assertEqual(raw, capture())
    def test_scheduling_permutation_per_forward_and_stock_ansi(self):
        raw = capture(tuple(reversed(NODES))).decode()
        raw = "AttentionKernelOptions: FLASH_ATTENTION=1 MATH=0\n" + "".join(
            "\x1b[36m" + v + "\x1b[0m\n" for v in raw.splitlines())
        self.assertEqual(len(check(raw.encode())["groups"]), 2)
    def test_configuration_flags_alone_do_not_prove_execution(self):
        with self.assertRaises(ValueError):
            check(b"AttentionKernelOptions: FLASH_ATTENTION=1 EFFICIENT_ATTENTION=0 MATH=0\n")
    def test_missing_extra_or_partial_forward_refuses(self):
        for raw in (capture(forwards=1), capture(forwards=3), capture() + line(NODES[0]).encode()):
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError): check(raw)
    def test_missing_duplicate_and_unknown_nodes_refuse(self):
        for raw in ((line(NODES[0]) * 4).encode(),
                    capture().replace(NODES[1].encode(), b"/foreign/node")):
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError): check(raw)
    def test_math_fallback_wrong_dtype_operator_and_wrong_backend_refuse(self):
        for old, new in ((b"FLASH_ATTENTION", b"MATH"), (b"fp16", b"fp32"),
                         (b"GroupQueryAttention", b"MultiHeadAttention"),
                         (b"FLASH_ATTENTION", b"EFFICIENT_ATTENTION")):
            with self.subTest(new=new):
                with self.assertRaises(ValueError): check(capture().replace(old, new))
    def test_malformed_interleaved_or_unexpected_ansi_refuses(self):
        for raw in (capture().replace(b" DataType=", b"\nDataType=", 1),
                    capture().replace(b"Operator=", b"\x1b[31mOperator=", 1)):
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError): check(raw)
    def test_caller_rosters_cannot_be_empty_duplicate_or_list(self):
        for nodes in ((), (NODES[0], NODES[0]), list(NODES)):
            with self.subTest(nodes=nodes):
                with self.assertRaises(ValueError):
                    observe(capture(), nodes=nodes, forward_roster=ROSTER,
                            completed_forwards=ROSTER, operator="GroupQueryAttention", backend="FLASH_ATTENTION")
        for roster in ((), ("same", "same"), list(ROSTER)):
            with self.subTest(roster=roster):
                with self.assertRaises(ValueError):
                    observe(capture(), nodes=NODES, forward_roster=roster,
                            completed_forwards=ROSTER, operator="GroupQueryAttention", backend="FLASH_ATTENTION")
    def test_focus_mea_uses_same_strict_complete_roster(self):
        raw = capture().replace(b"GroupQueryAttention", b"MultiHeadAttention").replace(
            b"FLASH_ATTENTION", b"EFFICIENT_ATTENTION")
        result = observe(raw, nodes=NODES, forward_roster=ROSTER,
                         completed_forwards=ROSTER, operator="MultiHeadAttention", backend="EFFICIENT_ATTENTION")
        self.assertEqual(result["backend"], "EFFICIENT_ATTENTION")
    def test_wrong_recipe_and_nonbytes_refuse(self):
        for raw, operator, backend in ((capture(), "GroupQueryAttention", "EFFICIENT_ATTENTION"),
                                       ("text", "GroupQueryAttention", "FLASH_ATTENTION")):
            with self.assertRaises(ValueError):
                observe(raw, nodes=NODES, forward_roster=ROSTER, completed_forwards=ROSTER, operator=operator, backend=backend)

    def test_logs_cannot_substitute_for_actual_native_completion(self):
        for completed in ((), ROSTER[:1], tuple(reversed(ROSTER)), list(ROSTER)):
            with self.subTest(completed=completed):
                with self.assertRaises(ValueError):
                    observe(capture(), nodes=NODES, forward_roster=ROSTER, completed_forwards=completed,
                            operator="GroupQueryAttention", backend="FLASH_ATTENTION")

if __name__ == "__main__":
    unittest.main()
