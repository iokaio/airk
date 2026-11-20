# SPDX-License-Identifier: Apache-2.0
import copy
import sys
from pathlib import Path
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import r6_epoch as epoch


class Epoch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original=epoch.read(epoch.reference(epoch.R5,epoch.R5_SHA))
        cls.ids=[key for phase in cls.original["phase_ids"] for key in phase]

    def test_correct_partition_and_bound(self):
        epoch.original_clock(self.original,self.original,self.ids)

    def test_short_population_extended_clock_or_changed_anchor(self):
        for issue in ("less","clock","anchor","different-group","debit"):
            with self.subTest(issue=issue):
                plan=copy.deepcopy(self.original)
                if issue=="less":plan["phase_ids"][-1].pop()
                if issue=="clock":plan["deadline_ms"]+=900000
                if issue=="anchor":plan["installation"]["ready"]["sha256"]="0"*64
                if issue=="different-group":plan["phase_groups"][0].pop()
                if issue=="debit":plan["setup_elapsed_ms"]-=1
                with self.assertRaises(ValueError):
                    epoch.original_clock(plan,self.original,self.ids)

    def test_rejects_r5_output_before_opening_any_question(self):
        with self.assertRaisesRegex(ValueError,"dedicated"):
            epoch.preflight(epoch.R5.parent,{},{},"0"*64)

    def test_actual_candidate_projection_and_drift(self):
        group=HERE.parent/"cal_capture/grouped"
        sys.path.insert(0,str(group))
        import r6_candidate, r6_effective_sources, r6_search_handoff
        proof="060256cdb4d9a47004bb0b1a2ec1cf1796ccb563e34208c1d4bdd3111c13a257"
        accepted=r6_candidate.checked_epoch(proof,r6_search_handoff.seal)
        service="77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
        epoch.audit_for_epoch(accepted["audit"],service)
        self.assertEqual(accepted["audit"]["sha256"],"aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1")
        target=epoch.ROOT/"candidate-sources"
        bytes_by_name=r6_candidate.projected(accepted,target)
        legacy=epoch.read(self.original["input_audit"])
        args=(HERE.parents[4],group.parent,target,accepted["audit"]["sha256"],Path(legacy["inputs"]["service"]["path"]).parent)
        r6_effective_sources.verify(*args,bytes_by_name)
        for name in bytes_by_name:
            with self.subTest(name=name), self.assertRaises(ValueError):
                r6_effective_sources.verify(*args,bytes_by_name | {name:bytes_by_name[name]+b"\n"})

    def test_genuine_search_stable_fields(self):
        import sys
        sys.path.insert(0,str(HERE.parent/"cal_capture/grouped"))
        import r6_search_handoff as handoff
        path=handoff.OLD/"commands/search-1.stdout"
        body=handoff.body(handoff.read(handoff.seal(path),path))
        self.assertTrue(handoff.equivalent(body,body))
        self.assertFalse(handoff.equivalent(body,body|{"hits":[]}))

    def test_r7_named_scope_uses_only_the_retired_native_selection(self):
        import r7_restart as restart
        evidence = {name: epoch.read(epoch.reference(path, sha)) for name, (path, sha) in restart.INPUTS.items()
                    if name in ("bindings", "ready")}
        compared_path = restart.CAMPAIGN/"cal100-fused-canonical-search-r1/fixture/enable-context.bindings.json"
        compared = epoch.read(epoch.reference(compared_path,
            "216bfe2464aeaf5d54cea45a8c68cba47b72cb6cc8d6aae5da189e3fb18867ff"))
        baseline, ready = evidence["bindings"], evidence["ready"]
        anchor = epoch.read(self.original["installation"]["bindings"])
        restart.validate_binding(baseline, compared, ready, anchor)
        for field, value in (("catalog_revision", 4), ("scope_identity", "0"*64)):
            changed = copy.deepcopy(baseline)
            changed["bindings"][0]["status_result"]["structuredContent"]["scope_observation"]["selection"][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "revision-eight"):
                restart.validate_binding(changed, compared, ready, anchor)

    def test_r7_control_requires_only_the_observed_mutable_changes(self):
        import r7_restart as restart
        modified = ("http-1/endpoint.json", "logs-1.4/current.jsonl", "service-1.4.sqlite3")
        base = {"control_root": "control", "directories": ["http-1", "logs-1.4"],
                "files": dict.fromkeys((*modified, "model"), "0"*64)}
        previous = base | {"immutable": {"model": "0"*64}}
        current = base | {"files": base["files"] | dict.fromkeys(modified, "1"*64)}
        restart.validate_control(current, previous)
        current["files"]["model"] = "2"*64
        with self.assertRaisesRegex(ValueError, "unexpected changed bytes"):
            restart.validate_control(current, previous)

    def test_r7_projection_reverses_the_single_scope_baseline_change(self):
        group = HERE.parent/"cal_capture/grouped"
        sys.path.insert(0, str(group))
        import r6_candidate, r6_effective_sources
        digest = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
        target = epoch.R7_ROOT/"candidate-sources"
        files = r6_candidate.projected({"audit": {"sha256": digest}}, target)
        old = b'linked(original["bindings"])'
        new = b'linked(plan["scope_baseline"])'
        self.assertEqual(files["group_service.py"].count(old), 1)
        resumed = files | {"group_service.py": files["group_service.py"].replace(old, new)}
        legacy = epoch.read(self.original["input_audit"])
        args = (HERE.parents[4], group.parent, target, digest, Path(legacy["inputs"]["service"]["path"]).parent)
        r6_effective_sources.verify(*args, resumed, resume_r6=True)
        with self.assertRaises(ValueError):
            r6_effective_sources.verify(*args, files, resume_r6=True)
        with self.assertRaises(ValueError):
            r6_effective_sources.verify(*args, resumed)

    def test_foreign_service_does_not_inherit_original_audit(self):
        with self.assertRaisesRegex(ValueError,"unbound service"):
            epoch.audit_for_epoch(self.original["input_audit"],"0"*64)


if __name__=="__main__":
    unittest.main()
