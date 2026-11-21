# SPDX-License-Identifier: Apache-2.0
"""Zero-model controls for the R8 Compact Read study input and launcher."""
from copy import deepcopy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r8_cutlass_study_inputs as study
import r8_cutlass_study_report as description
import run_r8_cutlass_study as launcher
import stage_cutlass_study_image as stager

PLAN = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c"
AUDIT = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
GRAPH = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"


def pin(name):
    return {"path": str(study.CAMPAIGN / name), "bytes": 1, "sha256": name[0] * 64}


def evidence():
    rosters = [{"cards": [{"id": f"read-{i}"}, {"id": f"stop-{i}"}]} for i in range(100)]
    panel = {
        "projection": pin("projection.json"), "compiler": pin("compiler.json"), "graph": pin("graph.json"),
        "artifact": dict(study.IDENTITY), "calibration_fitted": False, "phases": [],
        "rows": [{"ordinal": i, "id": f"case-{i}", "source_phase": i // 8, "input_digest": f"digest-{i}",
                  "scores": [1.0, float(-i)]} for i in range(100)],
    }
    certificate = {
        "complete": True, "qualified_profiles": [], "lifetime_plan_sha256": PLAN,
        "launch_audit_sha256": AUDIT, "focus_tokens": 1024,
        "rows": [{"id": f"case-{i}", "split": "calibration", "source_group": f"group-{i % 9}",
                  "request": {"roster": rosters[i]}} for i in range(100)],
    }
    projection = {"reference_preflight": pin("certificate.json"), "plan_sha256": PLAN,
                  "launch_audit": {"sha256": AUDIT}}
    compiled = [{"ordinal": i, "id": f"case-{i}", "roster": rosters[i],
                 "compiled": {"input_digest": f"digest-{i}"}} for i in range(100)]
    descriptor = {"panel": {"path": str(study.CAMPAIGN / "panel.json"), "sha256": "p" * 64},
                  "certificate": {"path": str(study.CAMPAIGN / "certificate.json"), "sha256": "c" * 64},
                  "provider_sha256": ["f" * 64] * 6}
    return descriptor, panel, certificate, projection, compiled


class R8Study(unittest.TestCase):
    def prepare(self, change=None, descriptor_change=None):
        descriptor, panel, certificate, projection, compiled = evidence()
        retained = deepcopy(panel)
        if change:
            change(panel, certificate, projection, compiled, retained)
        if descriptor_change:
            descriptor_change(descriptor)
        files = {"panel.json": retained, "projection.json": projection, "certificate.json": certificate}

        def selected(projection_pin, compiler_pin, graph_pin, first, count):
            self.assertEqual([projection_pin, compiler_pin, graph_pin],
                             [panel[key] for key in ("projection", "compiler", "graph")])
            return compiled[first:first + count], {}

        with patch.object(study, "reference", side_effect=lambda path, sha: {
                "path": str(path), "bytes": 1, "sha256": sha}), \
             patch.object(study, "read", side_effect=lambda ref: files[Path(ref["path"]).name]), \
             patch.object(study, "panel", return_value=panel) as owner, \
             patch.object(study.r8_reference_inputs, "selected", side_effect=selected):
            result = study.prepare(descriptor)
        owner.assert_called_once_with(["f" * 64] * 6)
        return result

    def test_packet_binds_r8_plan_scores_and_certificate_rows(self):
        packet = self.prepare()
        self.assertEqual((packet["schema"], packet["plan"], packet["graph"], packet["comparisons"]),
                         ("airk.private.cutlass-study-verified-input/1", PLAN, GRAPH, None))
        raw = packet["raw"]
        self.assertEqual((raw["complete"], raw["task_qualified"], raw["release_qualified"], len(raw["rows"])),
                         (True, False, False, 100))
        self.assertEqual(raw["rows"][99], {
            "id": "case-99", "ordinal": 99, "status": "raw_scores_only", "input_digest": "digest-99",
            "scores": [1.0, -99.0], "split": "calibration", "source_group": "group-0",
            "roster": {"cards": [{"id": "read-99"}, {"id": "stop-99"}]}})
        self.assertEqual(packet["preflight"]["lifetime_plan_sha256"], PLAN)

    def test_descriptor_must_name_exactly_the_panel_certificate_and_phases(self):
        for change in (lambda d: d.pop("provider_sha256"), lambda d: d.update(scores=[]),
                       lambda d: d["panel"].update(bytes=1),
                       lambda d: d["panel"].update(path=str(study.CAMPAIGN.parent / "panel.json"))):
            with self.assertRaises(ValueError):
                self.prepare(descriptor_change=change)

    def test_retained_panel_must_reproduce_from_native_receipts(self):
        def altered_score(panel, certificate, projection, compiled, retained):
            retained["rows"][55]["scores"][0] = 9.0

        def fitted(panel, certificate, projection, compiled, retained):
            panel["calibration_fitted"] = retained["calibration_fitted"] = True

        def foreign_graph(panel, certificate, projection, compiled, retained):
            panel["artifact"]["model_sha256"] = retained["artifact"]["model_sha256"] = "0" * 64

        for change in (altered_score, fitted, foreign_graph):
            with self.subTest(change=change.__name__), self.assertRaisesRegex(ValueError, "does not reproduce"):
                self.prepare(change)

    def test_certificate_must_be_the_one_bound_to_the_projection(self):
        with self.assertRaisesRegex(ValueError, "not the one bound"):
            self.prepare(descriptor_change=lambda d: d["certificate"].update(sha256="0" * 64))

        def r5_plan(panel, certificate, projection, compiled, retained):
            certificate["lifetime_plan_sha256"] = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"

        def incomplete(panel, certificate, projection, compiled, retained):
            certificate["complete"] = False

        def narrow_focus(panel, certificate, projection, compiled, retained):
            certificate["focus_tokens"] = 512

        for change in (r5_plan, incomplete, narrow_focus):
            with self.subTest(change=change.__name__), self.assertRaisesRegex(ValueError, "wrong R8 certificate"):
                self.prepare(change)

    def test_each_score_must_match_its_compiled_input_and_certified_roster(self):
        def reordered(panel, certificate, projection, compiled, retained):
            compiled[3], compiled[4] = compiled[4], compiled[3]

        def other_input(panel, certificate, projection, compiled, retained):
            compiled[98]["compiled"]["input_digest"] = "other"

        def other_roster(panel, certificate, projection, compiled, retained):
            certificate["rows"][10]["request"]["roster"] = {"cards": [{"id": "read-10"}, {"id": "other"}]}

        def missing_score(panel, certificate, projection, compiled, retained):
            panel["rows"][44]["scores"] = retained["rows"][44]["scores"] = [1.0]

        def training_row(panel, certificate, projection, compiled, retained):
            certificate["rows"][0]["split"] = "train"

        for change in (reordered, other_input, other_roster, missing_score, training_row):
            with self.subTest(change=change.__name__), self.assertRaisesRegex(ValueError, "row differs"):
                self.prepare(change)

    def test_launcher_uses_the_r8_builder_and_requires_the_r8_plan_in_the_report(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            for plan, complete, accepted in ((PLAN, True, True), ("0" * 64, True, False), (PLAN, False, False)):
                def native(descriptor, output):
                    self.assertIs(launcher.study.prepare, study.prepare)
                    (output / "execution.json").write_text(
                        json.dumps({"complete": complete, "report": {"plan": plan}}), "utf-8")

                with self.subTest(plan=plan, complete=complete), \
                     patch.object(launcher.study, "prepare"), \
                     patch.object(launcher.study, "execute", side_effect=native) as owner:
                    if accepted:
                        launcher.execute({"evidence": {}}, out)
                    else:
                        with self.assertRaisesRegex(ValueError, "did not bind the R8 plan"):
                            launcher.execute({"evidence": {}}, out)
                    owner.assert_called_once_with({"evidence": {}}, out)


class Staging(unittest.TestCase):
    def linked(self, change=None, lines=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            target = root / "target"
            target.mkdir()
            image = target / "airk_kernel-0123.exe"
            image.write_bytes(b"image")
            receipt = {"schema": "airk-readiness-command/1", "passed": True, "inputs_unchanged": True,
                       "command_inputs_unchanged": True, "groups": {"native": {"sha256": "same"}},
                       "target_directory": str(target)}
            build = receipt | {"command": ["cargo", "test", "--no-run"], "log": "image.log"}
            unit = deepcopy(receipt) | {"command": ["cargo", "nextest", "run"], "selected_tests": 2}
            if change:
                change(build, unit)
            artifact = {"reason": "compiler-artifact", "target": {"name": "airk_kernel"},
                        "profile": {"test": True}, "executable": str(image)}
            other = artifact | {"target": {"name": "airk_core"}}
            text = [json.dumps(item) for item in (lines or [other, artifact])]
            (root / "image.log").write_text("   Compiling airk-kernel\n" + "\n".join(text) + "\n", "utf-8")
            with patch.object(stager, "read", side_effect=lambda pin: {"build": build, "unit": unit}[pin["name"]]):
                return stager.linked_image({"name": "build", "path": str(root / "image.json")},
                                           {"name": "unit"}).name

    def test_names_the_one_linked_kernel_test_image(self):
        self.assertEqual(self.linked(), "airk_kernel-0123.exe")

    def test_refuses_receipts_that_do_not_describe_one_unit_tested_image(self):
        changes = (lambda build, unit: unit["groups"].update(native={"sha256": "edited"}),
                   lambda build, unit: build.update(inputs_unchanged=False),
                   lambda build, unit: unit.update(passed=False),
                   lambda build, unit: unit.update(selected_tests=0),
                   lambda build, unit: build.update(command=["cargo", "test"]),
                   lambda build, unit: unit.update(target_directory="elsewhere"))
        for index, change in enumerate(changes):
            with self.subTest(change=index), self.assertRaises(ValueError):
                self.linked(change)
        with self.assertRaisesRegex(ValueError, "exactly one linked Kernel test image"):
            self.linked(lines=[{"reason": "build-finished"}])


def card(kind, name):
    if kind == "read":
        return {"id": name, "action": {"kind": "read", "choice": name}, "gap": "unsupported",
                "reason": "additional_evidence_may_help",
                "assessments": [{"role": "unknown", "relevance": "uncertain"}]}
    return {"id": name, "action": {"kind": kind}, "gap": "sufficient", "reason": "other", "assessments": []}


def described(rows, native, admitted):
    """rows: (scores, certified Read cards "a"/"b") over Read a, Sufficient s and Read b in that slot order."""
    raw, pre, outcomes = [], [], []
    for index, (scores, certified) in enumerate(rows):
        cards = [card("read", f"a{index}"), card("sufficient", f"s{index}"), card("read", f"b{index}")]
        raw.append({"id": str(index), "status": "raw_scores_only", "scores": scores, "roster": {"cards": cards}})
        pre.append({"id": str(index), "reference_proxies": {"valid_reference_proxy_whole_cards": [
            {"card": f"{name}{index}", "proxy": description.PROXY} for name in certified]}})
        outcomes.append({"card": f"a{index}" if index in admitted else None})
    packet = {"raw": {"rows": raw}, "preflight": {"rows": pre}}
    return description.describe(packet, {"plan": PLAN, "minimum_raw_margin": native, "outcomes": outcomes})


class Description(unittest.TestCase):
    ROWS = [([5.0, 0.0, 1.0], "a"),   # certified Read winner, margin 4
            ([9.0, 0.0, 3.0], ""),    # uncertified Read winner, margin 6
            ([0.0, 2.0, 1.0], "b"),   # Sufficient wins; the certified card ranks second
            ([7.0, 0.0, 5.5], "a")]   # certified Read winner, margin 1.5

    def test_ranks_use_single_precision_and_a_tie_has_no_margin(self):
        self.assertEqual(description.ranked([4.0, 1.0, 2.5]), ([0, 2, 1], 1.5))
        self.assertEqual(description.ranked([1.0, 1.0]), ([0, 1], None))
        self.assertEqual(description.ranked([0.1, 0.1 + 1e-12])[1], None)

    def test_tradeoff_keeps_the_frozen_rule_at_zero_tolerance(self):
        cutoff = lambda winners, tolerated: description.tradeoff(winners, tolerated)["cutoff"]
        self.assertIsNone(cutoff([(1.0, True), (1.0, False)], 0))
        self.assertEqual(cutoff([(2.0, True), (1.0, False), (0.5, True)], 0), 2.0)
        self.assertEqual(description.tradeoff([(2.0, True), (1.0, False), (0.5, True)], 1), {
            "tolerated_uncertified": 1, "certified_admitted": 2, "uncertified_admitted": 1, "cutoff": 0.5})
        self.assertEqual(description.tradeoff([(3.0, False)], 5)["certified_admitted"], 0)

    def test_unsupported_panel_reports_counts_ranks_and_margins(self):
        result = described(self.ROWS, None, set())
        self.assertEqual(result["native_outcome"], "unsupported_no_positive_safe_margin")
        self.assertEqual(result["top_card_counts"], {"Read": 3, "Sufficient": 1})
        self.assertEqual([(row["ordinal"], row["margin"]) for row in result["certified_read_winners"]],
                         [(0, 4.0), (3, 1.5)])
        self.assertEqual([(row["ordinal"], row["margin"]) for row in result["uncertified_read_winners"]],
                         [(1, 6.0)])
        self.assertEqual([(row["ordinal"], row["certified_ranks"], row["top_card"], row["winner_is_certified"])
                          for row in result["certified_rows"]],
                         [(0, [1], "Read", True), (2, [2], "Sufficient", False), (3, [1], "Read", True)])
        self.assertEqual([(item["tolerated_uncertified"], item["certified_admitted"], item["cutoff"])
                          for item in result["descriptive_tradeoffs_not_a_proposed_rule"]],
                         [(0, 0, None), (1, 2, 1.5), (2, 2, 1.5), (3, 2, 1.5), (5, 2, 1.5)])
        self.assertTrue(result["descriptive_only"])

    def test_description_must_agree_with_the_native_owner(self):
        safe = self.ROWS[:1] + self.ROWS[2:]
        self.assertEqual(described(safe, 1.5, {0, 2})["native_outcome"], "cutoff")
        for native, admitted in ((None, set()), (4.0, {0, 2}), (1.5, {0})):
            with self.subTest(native=native, admitted=admitted), self.assertRaisesRegex(ValueError, "disagrees"):
                described(safe, native, admitted)
        with self.assertRaisesRegex(ValueError, "disagrees with the native cutoff"):
            described(self.ROWS, 1.5, {0, 3})


if __name__ == "__main__":
    unittest.main()
