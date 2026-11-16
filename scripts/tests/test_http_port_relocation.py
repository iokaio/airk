# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import ssl
import tempfile
import unittest
from unittest.mock import patch, Mock
from types import SimpleNamespace


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
        before = {'installation':'33'*32, 'user':'fictional-user', 'namespace':'22'*16,
            'ca_sha256':'44'*32, 'executable_sha256':'55'*32, 'generation':1,
            'service_instance':'66'*16, 'url':'https://127.0.0.1:43111/mcp'}
        after = copy.deepcopy(before)
        after.update(generation=2, service_instance='77'*16, url='https://127.0.0.1:43112/mcp')
        old = {'instance':'66'*16,'runtime_sha256':'88'*32,'executable_sha256':'55'*32}
        new = {**old, 'instance':'77'*16}
        return before, after, old, new

    def test_relocation_preserves_complete_immutable_authority_and_approved_image(self):
        GATE.validate_relocation(*self.connections())
        for key in ('installation','user','namespace','ca_sha256','executable_sha256','generation','url','service_instance'):
            before, after, old, new = self.connections()
            after[key] = before[key] if key in ('generation','url','service_instance') else 'changed'
            with self.subTest(endpoint=key), self.assertRaises(AssertionError):
                GATE.validate_relocation(before, after, old, new)
        for field in ('runtime_sha256', 'executable_sha256'):
            before, after, old, new = self.connections()
            new[field] = '99'*32
            with self.subTest(runtime=field), self.assertRaises(AssertionError):
                GATE.validate_relocation(before, after, old, new)
        # U110 has no enrolled authority in an endpoint. The exact read scope
        # is separately exercised by read_marker below, with MCP roots absent.

    def test_read_marker_carries_the_same_explicit_index_to_both_transports(self):
        endpoint = self.connections()[0]
        scope = {'index_ids': ['idx_fixed']}
        body = {'hits': [{'text': 'Port relocation preserves this fictional authority.'}]}
        envelope = {'content': [{'type': 'text', 'text': json.dumps(body)}]}
        network = Mock()
        network.receive.side_effect = [
            {'result': {'serverInfo': {'name': 'airk'}}}, {'result': envelope}]
        network.api_read.return_value = envelope
        transport = SimpleNamespace(McpClient=Mock(return_value=network))
        result = GATE.read_marker(transport, endpoint, Path('owned-control'), scope)
        transport.McpClient.assert_called_once_with(endpoint, Path('owned-control'), scope=scope)
        self.assertEqual(network.send.call_args_list[0].args[0]['params']['capabilities'], {})
        self.assertEqual(network.send.call_args_list[-1].args[0]['params']['arguments'],
                         {'path': 'sentinel.txt', 'start_line': 1, 'end_line': 1, 'response_mode': 'full'})
        network.api_read.assert_called_once_with('read_file', network.send.call_args_list[-1].args[0]['params']['arguments'])
        network.close.assert_called_once()
        self.assertEqual(result['mcp']['result'], result['api'])

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
            private = root / 'http-1'; private.mkdir()
            ca = private / 'ca.pem'
            ca.write_text(ssl.DER_cert_to_PEM_cert(roots[0]), encoding='ascii')
            ca.chmod(0o600)
            foreign = GATE.ForeignListener(0)
            try:
                connection = {'schema':1,'state':'ready','namespace':'33'*16,
                    'url':f'https://127.0.0.1:{foreign.port}/mcp',
                    'ca_sha256':hashlib.sha256(roots[0]).hexdigest(), 'certificate_sha256':'44'*32}
                result = GATE.stale_probe(HTTP, connection, root, foreign)
                self.assertTrue(result['tls_refused_before_client_construction'])
                self.assertEqual(len(result['occupant_observations']), 1)
                self.assertNotIn('airk1.', json.dumps(result))
                self.assertFalse(result['occupant_observations'][0]['application_data'])
            finally:
                foreign.close()


if __name__ == '__main__':
    unittest.main()
