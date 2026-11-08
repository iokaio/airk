# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import ssl
import tempfile
import unittest
from unittest.mock import patch


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).parents[1] / (name + '.py'))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


GATE = module('check-http-port-relocation')
HTTP = module('http_fixture')


class PortRelocationContractTests(unittest.TestCase):
    def test_reviewed_project_has_real_storage_parent_and_native_owned_missing_leaf(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary)
            calls = []
            class Fixture:
                def register(self, name, source, data):
                    # Fixture.register passes data/name into native setup. The
                    # native directory pin requires its parent to already exist.
                    leaf = data / name
                    self_registration = (name, source, leaf)
                    calls.append(("register", self_registration))
                    self_test.assertEqual(leaf.parent.resolve(strict=True), data.resolve(strict=True))
                    self_test.assertFalse(leaf.exists())
                    self_test.assertEqual(source.parent, owner)
                    self_test.assertEqual(source.joinpath("sentinel.txt").read_text(encoding="utf-8"),
                                          "Port relocation preserves this fictional authority.\n")
                def job(self, name, kind):
                    calls.append(("job", name, kind))
            self_test = self
            GATE.register_project(Fixture(), owner)
            self.assertEqual([call[0] for call in calls], ["register", "job"])
            self.assertEqual(calls[1], ("job", "port-relocation", "index"))
            self.assertEqual(set(path.name for path in owner.iterdir()), {"source", "data"})
            self.assertEqual(list((owner / "data").iterdir()), [])

    def connections(self):
        before = {'connection_id': '11' * 16, 'selection': {'kind':'scoped', 'selection':{
            'namespace':'22' * 16, 'grant_id':'grt_fixed', 'expected_grant_revision':1,
            'index_id':'idx_fixed', 'project_ids':['prj_fixed'], 'expected_catalog_revision':1}},
            'endpoint': {'installation':'33'*32, 'user':'fictional-user', 'namespace':'22'*16,
                'ca_sha256':'44'*32, 'executable_sha256':'55'*32, 'generation':1,
                'service_instance':'66'*16, 'url':'https://127.0.0.1:43111/mcp'}}
        after = copy.deepcopy(before)
        after['endpoint'].update(generation=2, service_instance='77'*16, url='https://127.0.0.1:43112/mcp')
        after['selection']['selection']['expected_catalog_revision'] = 2
        old = {'instance':'66'*16,'runtime_sha256':'88'*32,'executable_sha256':'55'*32}
        new = {**old, 'instance':'77'*16}
        return before, after, old, new

    def test_relocation_preserves_complete_immutable_authority_and_approved_image(self):
        GATE.validate_relocation(*self.connections())
        for key in ('installation','user','namespace','ca_sha256','executable_sha256','generation','url','service_instance'):
            before, after, old, new = self.connections()
            after['endpoint'][key] = before['endpoint'][key] if key in ('generation','url','service_instance') else 'changed'
            with self.subTest(endpoint=key), self.assertRaises(AssertionError):
                GATE.validate_relocation(before, after, old, new)
        for key in ('namespace','grant_id','expected_grant_revision','index_id','project_ids'):
            before, after, old, new = self.connections()
            after['selection']['selection'][key] = 'changed'
            with self.subTest(authority=key), self.assertRaises(AssertionError):
                GATE.validate_relocation(before, after, old, new)
        before, after, old, new = self.connections()
        after['connection_id'] = '99'*16
        with self.assertRaises(AssertionError):
            GATE.validate_relocation(before, after, old, new)
        before, after, old, new = self.connections()
        new['runtime_sha256'] = '99'*32
        with self.assertRaises(AssertionError):
            GATE.validate_relocation(before, after, old, new)
        before, after, old, new = self.connections()
        after['selection']['selection']['expected_catalog_revision'] = 0
        with self.assertRaises(AssertionError):
            GATE.validate_relocation(before, after, old, new)

    def test_owned_port_collision_is_unavailable_without_contacting_existing_listener(self):
        foreign = GATE.ForeignListener(0)
        try:
            with self.assertRaises(GATE.Unavailable):
                GATE.ForeignListener(foreign.port)
            foreign.require_uncontacted()
        finally:
            foreign.close()

    def test_unavailable_collision_records_no_native_qualification_or_service_action(self):
        foreign = GATE.ForeignListener(0)
        try:
            with tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary)
                runtime = base / 'runtime'; runtime.mkdir()
                manifest = b'{"version":"1.8.14"}'
                (runtime / 'airk-install.json').write_bytes(manifest)
                (runtime / 'airk-runtime.json').write_text('{"schema":2}')
                (runtime / 'airk-build.json').write_text('{"source":{"sha256":"fictional"}}')
                report = base / 'report.json'
                with patch.object(GATE, 'PREFERRED_PORT', foreign.port), patch.object(GATE, 'load', side_effect=AssertionError('native fixture must not start')):
                    with self.assertRaises(GATE.Unavailable):
                        GATE.qualify(runtime, hashlib.sha256(manifest).hexdigest(), report)
                result = json.loads(report.read_text())
                self.assertEqual(result['status'], 'unavailable')
                self.assertNotIn('stop_receipt', result)
                self.assertEqual((runtime / 'airk-install.json').read_bytes(), manifest)
                foreign.require_uncontacted()
                with self.assertRaisesRegex(ValueError, 'outside the approved runtime'):
                    GATE.qualify(runtime, hashlib.sha256(manifest).hexdigest(), runtime / 'report.json')
        finally:
            foreign.close()

    def test_actual_stale_socket_refuses_tls_before_any_http_or_bearer_delivery(self):
        roots = ssl.create_default_context().get_ca_certs(binary_form=True)
        if not roots:
            self.skipTest('A normal public trust anchor is needed for the real TLS client fixture')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            ca = root / 'ca.pem'; token = root / 'credential.token'
            ca.write_text(ssl.DER_cert_to_PEM_cert(roots[0]), encoding='ascii')
            token.write_text('airk1.' + '11'*16 + '.' + '22'*32, encoding='ascii')
            ca.chmod(0o600); token.chmod(0o600)
            foreign = GATE.ForeignListener(0)
            try:
                connection = {'schema':1, 'connection_id':'11'*16,
                    'selection': {'kind':'scoped','selection': {'namespace':'33'*16}},
                    'endpoint': {'schema':1,'state':'ready','namespace':'33'*16,
                        'url':f'https://127.0.0.1:{foreign.port}/mcp',
                        'ca_sha256':hashlib.sha256(roots[0]).hexdigest(), 'certificate_sha256':'44'*32},
                    'ca_certificate_file':str(ca),'credential_file':str(token)}
                result = GATE.stale_probe(HTTP, connection, root, foreign)
                self.assertTrue(result['tls_refused_before_client_construction'])
                self.assertEqual(len(result['occupant_observations']), 1)
                self.assertNotIn('airk1.', json.dumps(result))
                self.assertFalse(result['occupant_observations'][0]['application_data'])
            finally:
                foreign.close()


if __name__ == '__main__':
    unittest.main()
