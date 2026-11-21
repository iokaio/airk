# SPDX-License-Identifier: Apache-2.0
"""Controls for the probe variant's graph surgery. No model, weight or device work."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import variant


class Attribute:
    def __init__(self, name, value):
        self.name, self.i, self.type = name, value, 2


class Node:
    def __init__(self, name, op_type, inputs, outputs, attributes=()):
        self.name, self.op_type = name, op_type
        self.input, self.output, self.attribute = list(inputs), list(outputs), list(attributes)


class Value:
    def __init__(self, name):
        self.name = name


class Graph:
    def __init__(self, nodes, outputs):
        self.node, self.output = list(nodes), [Value(name) for name in outputs]


def attention(name, bias, causal=1):
    inputs = ["q" + name, "k" + name, "v" + name, "", "", "seqlen", "total", "", "", "", bias]
    return Node(name, "GroupQueryAttention", inputs, ["out" + name, "pk" + name, "pv" + name],
                [Attribute("causal", causal), Attribute("num_heads", 16)])


def graph(causal=1):
    nodes = [Node("mask_and", "And", ["live", "tri"], ["and"]), Node("mask", "Where", ["and", "zero", "low"], ["bias"]),
             Node("len", "Shape", ["ids"], ["seqlen"]), Node("tot", "Shape", ["ids"], ["total"]),
             attention("0", "bias", causal), attention("1", "bias", causal),
             Node("head", "Add", ["out0", "out1"], ["scores"])]
    return Graph(nodes, ["scores"])


class VariantSurgery(unittest.TestCase):
    def test_bias_is_removed_and_its_producers_pruned(self):
        g = graph()
        changed, removed = variant.drop_attention_bias(g)
        self.assertEqual((changed, removed), (2, 2))
        self.assertEqual([n.name for n in g.node], ["len", "tot", "0", "1", "head"])
        for node in g.node[2:4]:
            self.assertEqual(node.input, ["q" + node.name, "k" + node.name, "v" + node.name, "", "", "seqlen", "total"])

    def test_a_non_causal_attention_node_is_refused(self):
        with self.assertRaises(ValueError):
            variant.drop_attention_bias(graph(causal=0))

    def test_an_attention_node_without_a_bias_is_refused(self):
        g = graph()
        del g.node[4].input[10:]
        with self.assertRaises(ValueError):
            variant.drop_attention_bias(g)

    def test_consumers_index_every_named_input(self):
        used = variant.consumers(graph())
        self.assertEqual(sorted((n.name, i) for n, i in used["bias"]), [("0", 10), ("1", 10)])
        self.assertNotIn("", used)


if __name__ == "__main__":
    unittest.main()
