# SPDX-License-Identifier: Apache-2.0
"""Replay authentic pilot/list responses and the retained unavailable status without opening a service."""
import json
from pathlib import Path
import runpy
from tempfile import TemporaryDirectory
import unittest

HERE = Path(__file__).resolve().parent
gates = runpy.run_path(str(HERE / "observe_aws_public_gates.py"))
probe = runpy.run_path(str(HERE / "probe_aws_canceled_worker.py"))
E = probe["E"]


def load(path):
    return json.loads(path.read_bytes())


class PublicGates(unittest.TestCase):
    def test_three_real_pages_and_revision_drift(self):
        pilot = E / "aws-candidate-pilot-r3"
        probe["n"]["checked"](pilot / "grant-lifetimes.private.json",
                               "ec2e09361a95b78e0b8c723a3f0dccfc947ad0fd2a847746c573c120e686ae08")
        pages = [load(pilot / "commands" / f"lifetime-first-page-{i}.stdout")["structuredContent"] for i in range(3)]
        members = {"source.md", "second.md", "third.md"}
        def run(data):
            def tool(label, method, args):
                n = int(label.removeprefix("menu-page-"))
                self.assertEqual(method, "airk_list_files")
                self.assertEqual(args["cursor"], data[n - 1]["cursor"])
                self.assertEqual(args["limit"], 1)
                return data[n]
            return list(gates["pages"](data[0], tool, members, 1))
        self.assertEqual({row for page in run(pages) for row in page}, members)
        with self.assertRaisesRegex(ValueError, "publication or public scope changed"):
            run([pages[0], {**pages[1], "publication": {"token": "wrong revision"}}, pages[2]])

    def test_partial_results_persist_across_independent_refusals(self):
        retained = E / "aws-candidate-retained-r4/commands"
        first = load(retained / "activation-first-page-retry-3.stdout")["structuredContent"]
        status = load(retained / "source-status-retry-6.stdout")["structuredContent"]
        menu = probe["menu"]
        tree, cases, _ = menu["inputs"](load(E / menu["INVENTORY"]))
        w = runpy.run_path(str(probe["n"]["WRAPPER"]))
        def tool(label, method, args):
            if method == "airk_status":
                return status
            raise ValueError("controlled independent refusal")
        with TemporaryDirectory() as scratch:
            out = Path(scratch)
            gates["source_gates"](w, out, first, cases, E / tree, tool)
            receipt = load(out / "public-native-census.private.json")
            self.assertFalse(receipt["terminal_page_verified"])
            self.assertEqual((receipt["native_pages_observed"], receipt["native_files_observed"]), (1, 100))
            self.assertEqual((receipt["positive_files_verified"], receipt["positive_files_total"]), (0, 27))
            self.assertEqual((receipt["source_ranges_verified"], receipt["source_ranges_total"]), (0, 41))
            self.assertEqual(receipt["status"]["observation"]["index_error"]["code"], "index_status_unavailable")


if __name__ == "__main__":
    unittest.main()
