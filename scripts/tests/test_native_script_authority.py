# SPDX-License-Identifier: Apache-2.0
"""Fixture routing/containment regressions, not installed native qualification."""
import copy
import contextlib
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).parents[1] / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


storage, usage = load('check-storage-lifecycle'), load('check-usage-inputs')


class NativeScriptAuthorityTests(unittest.TestCase):
    def test_usage_fixture_honors_private_base_and_retains_uncertain_stop_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve(); original = base/'preserve'; original.write_bytes(b'original')
            with patch.dict(usage.clients.os.environ, {'AIRK_PRIVATE_CENSUS_BASE': str(base)}), \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                with usage.retained_usage_fixture() as completed:
                    self.assertEqual(Path(completed).parent,base)
                self.assertFalse(Path(completed).exists())
                with self.assertRaisesRegex(RuntimeError,'uncertain Stop') as failure:
                    with usage.retained_usage_fixture() as retained:
                        Path(retained,'native-stop-evidence.json').write_text('{"outcome":"unknown"}')
                        raise RuntimeError('uncertain Stop')
            self.assertTrue(Path(retained,'native-stop-evidence.json').is_file())
            self.assertTrue(any(retained in note for note in failure.exception.__notes__))
            self.assertEqual(original.read_bytes(),b'original')

    def test_usage_accounting_and_mutations_use_exact_owner_without_changing_evidence_grant(self):
        registration = {'binding': 'exact-binding', 'root': 'original-source', 'data_root': 'exact-data', 'collection': 'fixture'}
        fixture = usage.clients.Fixture.__new__(usage.clients.Fixture)
        fixture.control, fixture.projects = Path('owned-control'), {'p': registration}
        fixture.arguments = Mock(return_value=['--catalog-session', 'exact-read-grant'])
        expected = ['--service-runtime-dir', 'owned-control', '--service-project', 'exact-binding', '--root', 'original-source',
                    '--data-dir', 'exact-data', '--collection', 'fixture']
        for command in [('usage', 'report'), ('usage', 'report', '--price-scenario', 'prices.json'), ('usage', 'policy'),
                        ('index',), ('usage', 'import', 'measurement.json'), ('usage', 'calibrate', '--record'),
                        ('usage', 'configure', '--mode', 'disabled'), ('usage', 'clear')]:
            self.assertEqual(usage.usage_arguments(fixture, 'p', command), expected)
        fixture.arguments.assert_not_called()
        self.assertEqual(storage.owner_arguments(fixture, 'p'), expected)
        with self.assertRaisesRegex(AssertionError, 'Unsupported'):
            usage.usage_arguments(fixture, 'p', ('runtime', 'uninstall'))
        self.assertEqual(registration['binding'], 'exact-binding')

    def test_storage_quota_views_require_owner_but_allowed_metadata_keeps_exact_grant(self):
        fixture = SimpleNamespace(management_arguments=Mock(return_value=['exact-owner']),
                                  arguments=Mock(return_value=['--catalog-session', 'original-grant']))
        for view in ('quota_policy', 'quota_status'):
            self.assertEqual(storage.settings_arguments(fixture, 'registered', view), ['exact-owner'])
        self.assertEqual(fixture.management_arguments.call_count, 2)
        fixture.arguments.assert_not_called()
        for view in ('extraction_policy', 'vector_selection', 'vector_status', 'roots', 'scan_limits'):
            self.assertEqual(storage.settings_arguments(fixture, 'registered', view), ['--catalog-session', 'original-grant'])
        self.assertEqual(fixture.arguments.call_count, 5)
        self.assertEqual(fixture.management_arguments.call_count, 2)
        with self.assertRaisesRegex(AssertionError, 'Unsupported'):
            storage.settings_arguments(fixture, 'registered', 'unknown_future_view')
        self.assertEqual(fixture.arguments.call_count, 5)
        self.assertEqual(fixture.management_arguments.call_count, 2)

    def test_replacement_requires_new_owned_storage_and_native_index_grant(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary).resolve()
            source, fresh = owner / 'source', owner / 'fresh'
            source.mkdir(); fresh.mkdir()
            original = source / 'source.txt'; original.write_bytes(b'Preserve original source')
            previous = {'binding': 'removed-binding', 'root': str(source), 'data_root': str(owner/'removed'), 'collection': 'fixture'}
            selected = {'namespace': 'exact-namespace', 'index_id': 'old-index', 'grant_id': 'old-grant', 'project_ids': ['old-project']}
            fixture = SimpleNamespace(owner=owner, single_image=True, projects={'p': copy.deepcopy(previous)}, scopes={'p': copy.deepcopy(selected)})
            def native_setup(name, root, parent):
                target = parent / 'fixture'; target.mkdir()
                fixture.projects[name] = {**previous, 'binding': 'native-new-binding', 'data_root': str(target)}
                fixture.scopes[name] = {**selected, 'index_id': 'native-new-index', 'grant_id': 'native-new-grant', 'project_ids': ['native-new-project']}
            fixture.register = Mock(side_effect=native_setup)
            old, grant = storage.replacement_registration(fixture, 'p', source, fresh)
            fixture.register.assert_called_once_with('p', source, fresh)
            self.assertEqual(old, previous); self.assertEqual(grant, selected)
            self.assertEqual(original.read_bytes(), b'Preserve original source')
            self.assertEqual(selected['grant_id'], 'old-grant')

    def test_replacement_refuses_foreign_or_nonempty_storage_before_native_setup(self):
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as foreign:
            owner = Path(temporary).resolve(); occupied = owner/'occupied'; occupied.mkdir()
            marker = occupied/'preserve'; marker.write_bytes(b'unknown')
            fixture = SimpleNamespace(owner=owner, single_image=True, projects={'p': {'binding': 'old'}}, scopes={'p': {}}, register=Mock())
            for candidate in [Path(foreign).resolve(), occupied]:
                with self.assertRaises(AssertionError):
                    storage.replacement_registration(fixture, 'p', owner, candidate)
            fixture.register.assert_not_called(); self.assertEqual(marker.read_bytes(), b'unknown')

    def test_removed_grant_requires_exact_permanent_unsubmitted_scope_refusal(self):
        selected = {'expected_catalog_revision': 4, 'grant_id': 'original-grant', 'index_id': 'original-index'}
        body = {'error': {'code': 'scoped_connection_unavailable', 'query_submitted': False, 'retryable': False},
                'requested_scope': {'confirmed': False, 'selection': {**selected, 'expected_catalog_revision': 5}}}
        def response(value):
            return SimpleNamespace(returncode=1, stdout=json.dumps({'isError': True,
                'content': [{'type': 'text', 'text': json.dumps(value)}], 'structuredContent': value}))
        storage.assert_removed_selection(response(body), selected)
        for mutate in [lambda v: v['error'].update(retryable=True), lambda v: v['error'].update(query_submitted=True),
                       lambda v: v['requested_scope']['selection'].update(grant_id='foreign'),
                       lambda v: v['requested_scope']['selection'].update(expected_catalog_revision=3)]:
            changed = copy.deepcopy(body); mutate(changed)
            with self.assertRaises(AssertionError):
                storage.assert_removed_selection(response(changed), selected)

    def test_actual_cli_tombstone_preflight_refusal_requires_exact_native_retirement(self):
        selected = {'namespace':'native-namespace','index_id':'original-index'}
        retirement = {'status':'catalog_index_retirement_evidence','index_id':'original-index',
                      'evidence':{'index':{'tombstone':True},'catalog_clock':{'namespace':'native-namespace'}}}
        response = SimpleNamespace(returncode=1,stdout='',stderr='Error: selected catalog index metadata is not ready\n')
        storage.assert_removed_selection(response,selected,retirement)
        with self.assertRaises(AssertionError): storage.assert_removed_selection(response,selected)
        for error in ['Error: broker_busy','Error: service_unavailable','Error: selected catalog index is unknown']:
            with self.assertRaises(AssertionError):
                storage.assert_removed_selection(SimpleNamespace(returncode=1,stdout='',stderr=error),selected,retirement)


if __name__ == '__main__':
    unittest.main()
