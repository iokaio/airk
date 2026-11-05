# SPDX-License-Identifier: Apache-2.0
"""Core/host golden metadata templates remain lossless and fail closed."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("combined_evidence", Path(__file__).parents[1] / "combined_evidence.py")
EVIDENCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EVIDENCE)


class CombinedEvidenceTests(unittest.TestCase):
    def fixture(self):
        # Frozen Core native_header_template_digest_matches_host_golden vector.
        template = {"fields": {"freshness": {"observed_unix_ms": None, "state": "unavailable"}},
                    "slots": [{"path": ["freshness", "observed_unix_ms"], "kind": "integer"}]}
        identity = "3c2b1d950550b1d7514895aa1645c730a1221132ba50b864215167c9d49f3bc9"
        self.assertEqual(EVIDENCE.framed_digest("airk/shared-native-header-template/1", template), identity)
        return {"schema": "airk.combined-read/1", "shared_values": {"schema": EVIDENCE.HEADER_SCHEMA,
            "values": {identity: template}}, "members": [{"index_id": "index-a", "result": {
                EVIDENCE.HEADER_REFERENCE: {"id": identity, "values": [1001]}, "publication": {"member": 0},
                "hits": [{"citation": {"path": "original.rs"}, "text": "original fixture"}]}}],
            "publication_vector": {"members": [{"index_id": "index-a", "publication": {"token": "original-token"}}]}}

    def test_core_host_golden_preserves_raw_inline_hits_and_member_publication(self):
        value = self.fixture()
        raw = copy.deepcopy(value)
        decoded = EVIDENCE.member_result(value, "index-a")
        self.assertEqual(decoded["freshness"], {"observed_unix_ms": 1001, "state": "unavailable"})
        self.assertEqual(decoded["publication"], {"token": "original-token"})
        self.assertEqual(decoded["hits"], raw["members"][0]["result"]["hits"])
        self.assertEqual(value, raw)

    def test_native_headers_reject_corruption_and_cross_member_publication(self):
        for mutation in ("kind", "count", "digest", "collision", "target", "schema", "foreign", "mixed", "oversize"):
            with self.subTest(mutation=mutation):
                value = self.fixture()
                body = value["members"][0]["result"]
                reference = body[EVIDENCE.HEADER_REFERENCE]
                template = next(iter(value["shared_values"]["values"].values()))
                if mutation == "kind": reference["values"] = [True]
                if mutation == "count": reference["values"] = []
                if mutation == "digest": template["fields"]["freshness"]["state"] = "changed"
                if mutation == "collision": body["freshness"] = {}
                if mutation == "target": template["slots"][0]["path"] = ["hits", 0]
                if mutation == "schema": value["shared_values"]["schema"] = "other"
                if mutation == "foreign": body["publication"] = {"member": 1}
                if mutation == "mixed": body[EVIDENCE.CAPABILITY_REFERENCE] = "00" * 32
                if mutation == "oversize": reference["values"] = [9007199254740992]
                with self.assertRaises((ValueError, KeyError)):
                    EVIDENCE.member_result(value, "index-a")

    def test_publication_inside_header_slot_is_resolved_only_after_validating_template(self):
        value = self.fixture()
        body = value["members"][0]["result"]
        body.pop("publication")
        template = {"fields": {"publication": {"member": None}},
                    "slots": [{"path": ["publication", "member"], "kind": "integer"}]}
        identity = EVIDENCE.framed_digest("airk/shared-native-header-template/1", template)
        value["shared_values"]["values"] = {identity: template}
        body[EVIDENCE.HEADER_REFERENCE] = {"id": identity, "values": [0]}
        self.assertEqual(EVIDENCE.member_result(value, "index-a")["publication"], {"token": "original-token"})
        body[EVIDENCE.HEADER_REFERENCE]["values"] = [1]
        with self.assertRaises(ValueError):
            EVIDENCE.member_result(value, "index-a")
