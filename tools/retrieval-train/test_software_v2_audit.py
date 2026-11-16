# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import unittest

from software_v2_audit import audit, validate_source_metadata


def fixture():
    rows = []
    for index, partition in enumerate(["TRAIN", "DEV", "CAL", "policy_confirmation", "FINAL"]):
        rows.append({"partition": partition, "source_sha256": str(index + 1) * 64,
            "repository_family": "repository-" + str(index), "revision": "commit-" + str(index),
            "issue_symbol_family": "issue-" + str(index), "clone_family": "clone-" + str(index),
            "dataset": "synthetic-control", "final_exposure": "unexposed",
            "rights": {"decision_sha256": "a" * 64, "license_artifact_sha256": "b" * 64,
                "notice_artifact_sha256": ["c" * 64], "permitted_partitions": [partition]},
            "annotation_review": {"authority": "existing_human_judgment",
                "artifact_sha256": "d" * 64, "id": "review-" + str(index)}})
    return {"schema": "airk.development-software-source-metadata/1", "sources": rows,
        "reserved_final_source_sha256": ["5" * 64]}


class SourceMetadataControls(unittest.TestCase):
    def test_distinct_partition_metadata_preserves_all_five_counts(self):
        self.assertEqual(validate_source_metadata(fixture()),
            {"TRAIN": 1, "DEV": 1, "CAL": 1, "policy_confirmation": 1, "FINAL": 1})

    def test_repository_issue_and_clone_leaks_each_refuse(self):
        for field in ["repository_family", "issue_symbol_family", "clone_family"]:
            with self.subTest(field=field):
                data = fixture()
                data["sources"][1][field] = data["sources"][0][field]
                with self.assertRaisesRegex(ValueError, "crosses partitions"):
                    validate_source_metadata(data)

    def test_final_source_cannot_enter_train_even_under_other_family(self):
        data = fixture()
        data["reserved_final_source_sha256"].append("1" * 64)
        with self.assertRaisesRegex(ValueError, "reserved FINAL"):
            validate_source_metadata(data)

    def test_hotpot_train_is_refused(self):
        data = fixture()
        data["sources"][0]["dataset"] = "HotpotQA"
        with self.assertRaisesRegex(ValueError, "evaluation-only"):
            validate_source_metadata(data)

    def test_wrong_rights_use_or_missing_notice_refuses(self):
        for change in ["wrong-use", "missing-notice"]:
            with self.subTest(change=change):
                data = fixture()
                data["sources"][0]["rights"]["permitted_partitions" if change == "wrong-use"
                    else "notice_artifact_sha256"] = []
                with self.assertRaises(ValueError):
                    validate_source_metadata(data)

    def test_mechanical_review_does_not_become_human_gold(self):
        data = fixture()
        data["sources"][0]["annotation_review"]["authority"] = "native_projection"
        with self.assertRaisesRegex(ValueError, "not accepted human"):
            validate_source_metadata(data)

    def test_exposed_final_is_refused(self):
        data = fixture()
        data["sources"][4]["final_exposure"] = "historically_exposed"
        with self.assertRaisesRegex(ValueError, "not untouched"):
            validate_source_metadata(data)

    def test_changed_source_duplicate_across_partitions_is_refused(self):
        data = fixture()
        data["sources"][1]["source_sha256"] = "1" * 64
        with self.assertRaisesRegex(ValueError, "content crosses"):
            validate_source_metadata(data)


class ProtocolControls(unittest.TestCase):
    def setUp(self):
        self.directory = Path(__file__).parent
        self.protocol = json.loads((self.directory / "software-v2-freeze.json").read_bytes())

    def test_actual_additive_freeze_is_criteria_only_and_never_authorizes_training(self):
        result = audit(self.protocol, self.directory)
        self.assertEqual(result["status"], "criteria_only_inputs_not_admitted")
        self.assertFalse(result["training_authorized"])

    def test_executable_pending_or_missing_data_refuses(self):
        data = copy.deepcopy(self.protocol)
        data["executable"] = True
        with self.assertRaisesRegex(ValueError, "pending bindings"):
            audit(data, self.directory)
        data["pending_bindings"] = []
        with self.assertRaisesRegex(ValueError, "source metadata"):
            audit(data, self.directory)
        with self.assertRaisesRegex(ValueError, "accepted data/native/probe"):
            audit(data, self.directory, fixture())

    def test_historical_protocol_drift_refuses(self):
        data = copy.deepcopy(self.protocol)
        data["original_protocols"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "historical protocol changed"):
            audit(data, self.directory)


if __name__ == "__main__":
    unittest.main()
