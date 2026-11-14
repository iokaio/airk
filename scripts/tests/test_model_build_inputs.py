# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import zipfile

SPEC = importlib.util.spec_from_file_location('model_build_inputs', Path(__file__).parents[1] / 'model-build-inputs.py')
owner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(owner)


class ModelBuildInputs(unittest.TestCase):
    def test_ci_and_container_use_authenticated_acquisition_before_provisioning(self):
        workflow = (owner.ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
        docker = (owner.ROOT / 'Dockerfile').read_text(encoding='utf-8')
        self.assertLess(workflow.index('model-build-inputs.py acquire'), workflow.index('provision-local-model.py --destination'))
        self.assertIn('GH_TOKEN: ${{ github.token }}', workflow)
        self.assertIn('github_token=${{ github.token }}', workflow)
        self.assertIn('--secret id=github_token,env=GH_TOKEN', workflow)
        self.assertIn('--mount=type=secret,id=github_token,required=true', docker)
        self.assertIn('--token-file /run/secrets/github_token', docker)
        self.assertNotIn('ARG GH_TOKEN', docker)
        self.assertNotIn('ARG GITHUB_TOKEN', docker)
        self.assertNotIn('provision-local-model.py --destination /build/model-assets --download', docker)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = {f'{profile}/{name}': (profile + name).encode()
                      for profile in ('model-u8', 'model-s8') for name in ('model.onnx', 'tokenizer.json', 'LICENSE')}
        self.expected = {name: (len(data), hashlib.sha256(data).hexdigest()) for name, data in self.files.items()}
        for name, data in self.files.items():
            path = self.root / 'source' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    def archive(self):
        archive = self.root / 'inputs.zip'
        owner.bundle(self.root / 'source', archive, self.expected)
        return archive

    def test_deterministic_complete_public_only_bundle_and_verified_cache(self):
        archive = self.archive()
        other = self.root / 'other.zip'
        (self.root / 'source' / 'parentreceipt.json').write_text('PRIVATE')
        owner.bundle(self.root / 'source', other, self.expected)
        self.assertEqual(archive.read_bytes(), other.read_bytes())
        target = self.root / 'target'
        owner.unpack(archive, target, self.expected)
        owner.unpack(archive, target, self.expected)
        self.assertEqual({p.relative_to(target).as_posix(): p.read_bytes() for p in target.rglob('*') if p.is_file()}, self.files)
        (target / 'model-u8/model.onnx').write_bytes(b'wrong')
        with self.assertRaises(ValueError):
            owner.unpack(archive, target, self.expected)

    def test_source_change_during_archive_construction_refuses_a_candidate_result(self):
        original = zipfile.ZipFile.writestr
        calls = 0
        names = sorted(self.expected)
        def write(archive, entry, data, *args, **kwargs):
            nonlocal calls
            calls += 1
            if calls == 1:
                path = self.root / 'source' / names[1]
                path.write_bytes(b'x' * path.stat().st_size)
            return original(archive, entry, data, *args, **kwargs)
        with patch.object(zipfile.ZipFile, 'writestr', new=write), self.assertRaisesRegex(ValueError, 'Packaged model input digest differs'):
            owner.bundle(self.root / 'source', self.root / 'changed.zip', self.expected)

    def test_extra_duplicate_traversal_and_changed_bytes_refused_before_publication(self):
        for mutation in ('extra', 'duplicate', 'traversal', 'bytes', 'symlink'):
            with self.subTest(mutation=mutation):
                archive = self.root / (mutation + '.zip')
                with zipfile.ZipFile(archive, 'w') as zipped:
                    for name, data in self.files.items():
                        entry = zipfile.ZipInfo(name)
                        if mutation == 'symlink':
                            entry.external_attr = 0o120777 << 16
                        zipped.writestr(entry, b'x' * len(data) if mutation == 'bytes' else data)
                    if mutation in ('extra', 'traversal', 'duplicate'):
                        zipped.writestr({'extra': 'parentreceipt.json', 'traversal': '../private', 'duplicate': next(iter(self.files))}[mutation], b'private')
                target = self.root / ('target-' + mutation)
                with self.assertRaises(ValueError):
                    owner.unpack(archive, target, self.expected)
                self.assertFalse(target.exists())

    def test_actual_asset_metadata_archive_digest_and_per_file_pins_all_required(self):
        archive = self.archive().read_bytes()
        metadata = {'id': 42, 'url': owner.API + '42', 'state': 'uploaded', 'size': len(archive),
                    'digest': 'sha256:' + hashlib.sha256(archive).hexdigest()}
        for fault in (None, 'id', 'digest', 'state', 'size', 'url', 'payload'):
            with self.subTest(fault=fault):
                document = dict(metadata)
                payload = archive
                if fault == 'payload':
                    payload = archive[:-1] + b'x'
                elif fault:
                    document[fault] = {'id': 43, 'digest': 'sha256:' + '0' * 64, 'state': 'new', 'size': True, 'url': 'https://other.invalid'}[fault]
                def fetched(url, token, binary=False):
                    self.assertEqual((url, token), (owner.API + '42', 'private-token'))
                    return io.BytesIO(payload if binary else json.dumps(document).encode())
                with patch.object(owner, 'fetch', side_effect=fetched):
                    target = self.root / ('acquired-' + str(fault))
                    if fault is None:
                        self.assertEqual(owner.acquire(42, target, 'private-token', self.expected)['asset_id'], 42)
                    else:
                        with self.assertRaises(ValueError):
                            owner.acquire(42, target, 'private-token', self.expected)
                        self.assertFalse(target.exists())

    def test_redirect_strips_credential_and_refuses_other_hosts(self):
        for location in ('https://release-assets.githubusercontent.com/file?signature=secret', 'https://evil.invalid/file'):
            class Opener:
                def __init__(self): self.requests = []
                def open(self, request, timeout):
                    self.requests.append(request)
                    if len(self.requests) == 1:
                        raise urllib.error.HTTPError(request.full_url, 302, 'redirect', {'Location': location}, None)
                    return io.BytesIO(b'asset')
            opener = Opener()
            if 'evil.invalid' in location:
                with self.assertRaises(ValueError): owner.fetch(owner.API + '42', 'private-token', binary=True, opener=opener)
                self.assertEqual(len(opener.requests), 1)
            else:
                with owner.fetch(owner.API + '42', 'private-token', binary=True, opener=opener) as response:
                    self.assertEqual(response.read(), b'asset')
                self.assertEqual(opener.requests[0].get_header('Authorization'), 'Bearer private-token')
                self.assertIsNone(opener.requests[1].get_header('Authorization'))


if __name__ == '__main__':
    unittest.main()
