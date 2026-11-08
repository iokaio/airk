# SPDX-License-Identifier: Apache-2.0
"""Embedded defaults must not be confused with explicit custom-model inputs."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

SPEC = importlib.util.spec_from_file_location("transformer_qualification", Path(__file__).parents[1] / "check-transformer-model.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class TransformerQualificationTests(unittest.TestCase):
    def test_healthy_native_provider_accepts_omitted_optional_failure_but_refuses_unhealthy_owner(self):
        import copy
        healthy = {"available":True,"loads":1,"process_id":None,
                   "provider":{"kind":"in_process","admission_open":True}}
        CHECK.assert_healthy_embedded_provider(healthy)
        CHECK.assert_healthy_embedded_provider({**healthy,"provider":{**healthy["provider"],"failure":None}})
        for field, value, nested in (("failure","owner_failed",True), ("failure",{},True),
                ("admission_open",False,True), ("kind","helper",True), ("available",False,False),
                ("loads",0,False), ("process_id",123,False)):
            observation = copy.deepcopy(healthy)
            (observation["provider"] if nested else observation)[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(AssertionError):
                CHECK.assert_healthy_embedded_provider(observation)

    def test_empty_owner_metadata_is_allowed_but_extracted_assets_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            control = Path(temporary)
            CHECK.assert_embedded_default_not_extracted(control)
            directory = control / "model-files-1.4"
            sets = directory / "sets"
            sets.mkdir(parents=True)
            (directory / "owner.txt").write_text("owner metadata")
            CHECK.assert_embedded_default_not_extracted(control)
            (sets / "unexpected-model-set").mkdir()
            with self.assertRaisesRegex(AssertionError, "extracted model assets"):
                CHECK.assert_embedded_default_not_extracted(control)

    def test_default_reference_cannot_hide_behind_empty_sets(self):
        with tempfile.TemporaryDirectory() as temporary:
            control = Path(temporary)
            directory = control / "model-files-1.4"
            directory.mkdir()
            (directory / "default-profile.json").write_text("{}")
            with self.assertRaisesRegex(AssertionError, "extracted a model reference"):
                CHECK.assert_embedded_default_not_extracted(control)

    def test_unknown_embedded_manifest_never_selects_an_arbitrary_model(self):
        fixture = Mock()
        fixture.run.return_value.stdout = json.dumps({"bundled_model": {"manifest": {"name": "unapproved"}}})
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            with self.assertRaisesRegex(AssertionError, "pinned input catalog"):
                CHECK.embedded_custom_input(fixture, base)
            self.assertEqual(list(base.iterdir()), [])
        fixture.run.assert_called_once_with("model-catalog")
