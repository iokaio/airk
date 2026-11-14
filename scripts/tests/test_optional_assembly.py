# SPDX-License-Identifier: Apache-2.0
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
from scripts.tests import test_github_release as fixtures
from scripts.tests.test_optional_publication import PARTS, hypothetical_projection

release = fixtures.release


class OptionalAssembly(unittest.TestCase):
    def test_four_target_assembly_keeps_optional_parts_separate_and_binds_all_published_bytes(self):
        # Fictional native/signature/VSIX fixtures establish packaging custody,
        # never native, OS-signature or optional model qualification.
        version, tag = '1.9.0', 'v1.9.0'
        producer = fixtures.split_producer()
        projection = hypothetical_projection()
        portable = release.module('optional_test_portable', 'package-portable.py')
        with tempfile.TemporaryDirectory(dir=release.ROOT / 'artifacts') as temporary, patch.dict(os.environ, {'GITHUB_REF': 'refs/heads/main'}), patch.object(release.optional, 'projection', return_value=projection):
            root = Path(temporary)
            inputs = root / 'inputs'
            inputs.mkdir()
            for target in ('x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'aarch64-apple-darwin', 'x86_64-apple-darwin'):
                original = root / 'originals' / target
                fixtures.split_fixture(original, target, version=version)
                release.write_source_record(inputs / f'airk-source-{target}-{fixtures.COMMIT}-456', original, producer, target, fixtures.COMMIT)
                signed = target != 'x86_64-unknown-linux-gnu'
                runtime = inputs / f"{'airk-signed' if signed else 'airk'}-native-{target}-{fixtures.COMMIT}-456"
                fixtures.split_fixture(runtime, target, signed, version=version)
                packaged = root / 'portable' / target
                portable.package(runtime, packaged, channel='preview')
                shutil.copytree(packaged / 'publish', inputs / f'airk-portable-{target}-{fixtures.COMMIT}-456')
            for name, data in PARTS.items():
                part = inputs / ('airk-arbiter-part-' + name + '-' + fixtures.COMMIT + '-456')
                part.mkdir()
                (part / name).write_bytes(data)
            vsix = root / 'airk-1.9.0.vsix'
            vsix.write_bytes(b'fictional package')
            manifest = root / 'vsix-manifest.json'
            manifest.write_text('{}')
            output = root / 'assembly'
            with patch.object(release, 'universal_vsix', return_value=(manifest, vsix)):
                record = release.assemble_split(inputs, output, fixtures.COMMIT, version, tag, 'preview', '456', '1', producer,
                    tree=fixtures.SPLIT_TREE, pinned=True, source_records=True)
            self.assertEqual(record['schema'], 'airk-github-release/3')
            self.assertEqual(record['optional_models'], projection)
            self.assertEqual(len(record['assets']), 22)
            self.assertEqual(len(list(output.iterdir())), 21, 'small assembly holds 18 base files + 3 records')
            for name, asset in projection['assets'].items():
                self.assertFalse((output / name).exists())
                self.assertIn(asset['sha256'] + '  ' + name + '\n', (output / 'SHA256SUMS').read_text())
            self.assertEqual(release.verify_bundle(output, fixtures.COMMIT, version, tag, '456', '1'), record)
            # Final publication adds the unchanged original parts, preserving all
            # original record bytes; partial sets never qualify a full snapshot.
            original_records = {name: (output / name).read_bytes() for name in ('release.json', 'SHA256SUMS')}
            first = next(iter(PARTS))
            (output / first).write_bytes(PARTS[first])
            with self.assertRaisesRegex(ValueError, 'Unexpected release bundle files'):
                release.verify_bundle(output, fixtures.COMMIT, version, tag, '456', '1')
            for name, data in PARTS.items(): (output / name).write_bytes(data)
            self.assertEqual(release.verify_bundle(output, fixtures.COMMIT, version, tag, '456', '1'), record)
            self.assertEqual({name: (output / name).read_bytes() for name in original_records}, original_records)
            (output / first).write_bytes(b'x')
            with self.assertRaisesRegex(ValueError, 'part digest differs'):
                release.verify_bundle(output, fixtures.COMMIT, version, tag, '456', '1')


if __name__ == '__main__':
    unittest.main()
