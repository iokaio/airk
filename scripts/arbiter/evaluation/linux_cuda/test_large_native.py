# SPDX-License-Identifier: Apache-2.0
"""Check Linux parent diagnostics with the original Windows graph as a read-only parser fixture."""
import json
import runpy
import unittest
import uuid

from configure_ort import CAMPAIGN, REPO, sha
from large_native import confirm_graph


class LargeGraphReceipt(unittest.TestCase):
    def test_checks_original_graph_and_refuses_adapter_or_forward_changes(self):
        files = {
            "windows-large-fp16-graph-r1/native.json":
                "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6",
            "windows-large-fp16-metadata-r1/native.json":
                "97cb9f5bedb61c8d42518dc244921c706aa98ffe538ef69cbe13ccec10d7d45d",
            "windows-large-fp16-metadata-r1/metadata/probe-request.json":
                "ff78022cf167c8c20ec730abb7f666137e00d9f309e85adc3ecf696c8396a7f7",
        }
        contents = []
        for name, digest in files.items():
            path = CAMPAIGN / name
            self.assertEqual(sha(path), digest)
            contents.append(json.loads(path.read_bytes()))
        graph, metadata, request = contents
        validators = [runpy.run_path(str(REPO / p)) for p in (
            "scripts/cuda-component-execution.py",
            "scripts/arbiter-fp16-execution/probe_large.py",
            "scripts/arbiter-fp16-execution/large_placement.py",
        )]
        adapter = uuid.UUID("08e2b3f6-6a39-3456-77cb-a57287a702ec")
        parent = {}
        confirm_graph(graph, parent, metadata, request, adapter, validators)
        self.assertIs(parent["selected_logs_complete"], False)
        self.assertGreater(parent["selected_logs_omitted"], 0)
        with self.assertRaises(ValueError):
            confirm_graph(dict(graph, selected_logs_complete=True), {}, metadata, request, adapter, validators)
        measured = parent["observed_large_cpu_values"]
        self.assertEqual(measured["summed_unique_payload_bytes"], 12067)
        self.assertEqual(measured["maximum_value_bytes"], 32)
        self.assertEqual(measured["provider_nodes"]["CPUExecutionProvider"], 478)
        with self.assertRaises(ValueError):
            confirm_graph(graph, {}, metadata, request, uuid.UUID(int=0), validators)
        with self.assertRaises(ValueError):
            confirm_graph(dict(graph, forward_dispatched=True), {}, metadata, request, adapter, validators)
