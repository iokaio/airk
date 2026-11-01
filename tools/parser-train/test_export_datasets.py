# SPDX-License-Identifier: Apache-2.0
"""Exact dataset publication, restore and provenance refusal contracts."""
import gzip
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import export_datasets as export


class DatasetPublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.tools = self.root / 'tools/parser-train'
        self.tools.mkdir(parents=True)
        (self.tools / '.gitattributes').write_bytes(('\n'.join(export.ATTRIBUTE_RULES) + '\n').encode())
        (self.root / 'target/parser-train/data').mkdir(parents=True)
        (self.root / 'LICENSE').write_text('Original Apache-2.0 license fixture\n', encoding='utf-8')
        self.write('tools/parser-train/repositories.lock.json', {'schema': 1, 'repositories': []})
        self.write('tools/parser-train/online.lock.json', {'schema': 1, 'sources': []})
        (self.tools / 'requirements.txt').write_text('numpy==2.3.2\n', encoding='utf-8')
        self.rows = [{'text': 'find documents about fictional retries', 'intent': 'search',
                      'origin': 'synthetic-template', 'split': 'train', 'example_id': 0},
                     {'text': 'find definition FictionalClient', 'intent': 'find_definition',
                      'origin': 'synthetic-template', 'split': 'train', 'example_id': 1}]
        self.set_data(self.rows)

    def write(self, path, value):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(export.encoded(value))

    def set_data(self, rows):
        self.raw = b''.join((json.dumps(row, sort_keys=True) + '\n').encode() for row in rows)
        sha = export.digest(self.raw)
        self.original = 'target/parser-train/data/synthetic-' + sha[:16] + '.jsonl'
        (self.root / self.original).write_bytes(self.raw)
        self.manifest = {'asset_sha256': 'a' * 64, 'feature_spec': 1, 'seed': 7,
                         'dataset': {'path': self.original, 'sha256': sha, 'rows': len(rows)}}
        self.write('tools/parser-train/manifest-smoke.json', self.manifest)

    def catalog(self):
        return json.loads((self.root / export.PUBLIC / 'catalog.json').read_bytes())

    def test_exact_bytes_reproducible_gzip_and_clean_clone_restore(self):
        first = export.publish(self.root)
        catalog = self.catalog()
        self.assertIn(export.ATTRIBUTES, {source['path'] for source in catalog['source_files']})
        item = catalog['datasets'][0]
        zipped = (self.root / item['path']).read_bytes()
        self.assertEqual(gzip.decompress(zipped), self.raw)
        self.assertEqual(zipped[4:8], b'\0' * 4, 'gzip timestamp is fixed')
        self.assertEqual(zipped[3] & 8, 0, 'gzip header contains no host filename')
        self.assertEqual(export.publish(self.root), first)
        self.assertEqual((self.root / item['path']).read_bytes(), zipped)
        (self.root / self.original).unlink()
        self.assertEqual(export.verify(self.root), first, 'verification needs no ignored originals or checkout')
        self.assertEqual(export.restore(self.root), first)
        self.assertEqual((self.root / self.original).read_bytes(), self.raw)

    def test_git_attributes_preserve_frozen_metadata_and_are_bound(self):
        export.publish(self.root)
        attributes = self.tools / '.gitattributes'
        attributes.write_text('*.json text eol=lf\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'byte-preserving Git attributes'):
            export.verify(self.root)
        attributes.write_bytes(('\n'.join(export.ATTRIBUTE_RULES) + '\n# altered source bytes\n').encode())
        with self.assertRaisesRegex(ValueError, 'provenance input changed'):
            export.verify(self.root)

    def test_unreferenced_native_or_private_files_are_never_selected(self):
        private = self.root / 'target/parser-train/data/private.jsonl'
        private.write_text('{"source_body":"PRIVATE_BODY_MUST_NOT_PUBLISH"}\n')
        export.publish(self.root)
        catalog = self.catalog()
        self.assertEqual(catalog['totals']['files'], 1)
        self.assertEqual(catalog['datasets'][0]['original_paths'], [self.original])
        self.assertFalse(any('private' in path.name for path in (self.root / export.PUBLIC).iterdir()))

    def test_conflicting_existing_restore_is_preserved(self):
        export.publish(self.root)
        destination = self.root / self.original
        destination.write_bytes(b'existing contributor data\n')
        with self.assertRaisesRegex(ValueError, 'preserves different existing'):
            export.restore(self.root)
        self.assertEqual(destination.read_bytes(), b'existing contributor data\n')

    def test_exact_reference_digest_and_row_count_are_required(self):
        (self.root / self.original).write_bytes(self.raw + b'\n')
        with self.assertRaisesRegex(ValueError, 'digest'):
            export.publish(self.root)
        (self.root / self.original).write_bytes(self.raw)
        self.manifest['dataset']['rows'] += 1
        self.write('tools/parser-train/manifest-smoke.json', self.manifest)
        with self.assertRaisesRegex(ValueError, 'row count'):
            export.publish(self.root)

    def test_conflicting_public_model_references_refuse(self):
        second = {**self.manifest, 'datasets': {'train': {**self.manifest['dataset'], 'rows': 99}}}
        second.pop('dataset')
        self.write('tools/parser-train/qualification.json', second)
        with self.assertRaisesRegex(ValueError, 'Conflicting dataset identities'):
            export.publish(self.root)

    def test_unknown_row_schema_or_provenance_refuses_without_redaction(self):
        for mutation in ({'origin': 'private-source'}, {'source_body': 'untrusted body'}):
            with self.subTest(mutation=mutation):
                self.set_data([{**self.rows[0], **mutation}])
                with self.assertRaisesRegex(ValueError, 'provenance/schema'):
                    export.publish(self.root)

    def test_path_traversal_absolute_paths_and_other_roots_refuse(self):
        for path in ('target/parser-train/data/../private.jsonl', '/private.jsonl',
                     'D:/private.jsonl', 'target/parser-train/indexes/private.jsonl',
                     'target\\parser-train\\data\\private.jsonl'):
            with self.subTest(path=path):
                self.manifest['dataset']['path'] = path
                self.write('tools/parser-train/manifest-smoke.json', self.manifest)
                with self.assertRaises(ValueError):
                    export.publish(self.root)

    def test_symlink_data_input_refuses(self):
        original = self.root / self.original
        original.unlink()
        other = self.root / 'other.jsonl'
        other.write_bytes(self.raw)
        try:
            original.symlink_to(other)
        except OSError as error:
            self.skipTest('Host cannot create symlink fixtures: ' + str(error))
        with self.assertRaisesRegex(ValueError, 'symlink'):
            export.publish(self.root)

    def test_model_source_or_compressed_bytes_changed_refuses(self):
        export.publish(self.root)
        (self.tools / 'requirements.txt').write_text('numpy==0.0.0\n')
        with self.assertRaisesRegex(ValueError, 'provenance input changed'):
            export.verify(self.root)
        (self.tools / 'requirements.txt').write_text('numpy==2.3.2\n')
        zipped = self.root / self.catalog()['datasets'][0]['path']
        zipped.write_bytes(zipped.read_bytes() + b'changed')
        with self.assertRaisesRegex(ValueError, 'Compressed dataset differs'):
            export.verify(self.root)

    def test_repo_revision_and_license_identity_are_checked(self):
        license_bytes = b'Original fictional repository Apache notice\n'
        revision = 'b' * 40
        self.write('tools/parser-train/repositories.lock.json', {'repositories': [{
            'repository': 'fictional/demo', 'revision': revision, 'license_files': [{
                'path': 'LICENSE', 'sha256': export.digest(license_bytes), 'revision': revision,
                'detected_license': 'Apache-2.0'}]}]})
        license_path = self.root / 'target/parser-train/repos/fictional--demo/LICENSE'
        license_path.parent.mkdir(parents=True)
        license_path.write_bytes(license_bytes)
        row = {'text': 'find the declaration of FictionalClient', 'intent': 'find_definition',
               'role': 'developer', 'split': 'train', 'family': 'original-test',
               'origin': 'original-phrase-family', 'repository': 'fictional/demo',
               'revision': revision, 'vocabulary_sha256': 'c' * 64}
        self.set_data([row])
        export.publish(self.root)
        license_path.write_bytes(b'different text')
        with self.assertRaisesRegex(ValueError, 'license differs'):
            export.publish(self.root)
        self.set_data([{**row, 'revision': 'd' * 40}])
        with self.assertRaisesRegex(ValueError, 'pinned public repository'):
            export.publish(self.root)

    def test_expansion_is_bounded(self):
        export.publish(self.root)
        with patch.object(export, 'MAX_FILE', len(self.raw) - 1):
            with self.assertRaisesRegex(ValueError, 'Expanded dataset differs'):
                export.verify(self.root)

    def test_identical_raw_datasets_are_stored_once_with_all_references(self):
        duplicate = self.original.replace('synthetic-', 'copy-')
        (self.root / duplicate).write_bytes(self.raw)
        other = {'asset_sha256': 'd' * 64, 'datasets': {'train': {
            'path': duplicate, 'sha256': export.digest(self.raw), 'rows': len(self.rows)}}}
        self.write('tools/parser-train/qualification.json', other)
        result = export.publish(self.root)
        self.assertEqual(result['files'], 1)
        self.assertEqual(result['rows'], len(self.rows))
        self.assertEqual(set(self.catalog()['datasets'][0]['original_paths']), {self.original, duplicate})
        self.assertEqual(len(self.catalog()['datasets'][0]['references']), 2)

    def test_catalog_cannot_omit_required_license_or_model_reference(self):
        export.publish(self.root)
        catalog = self.catalog()
        catalog['license_files'] = []
        self.write(export.PUBLIC + '/catalog.json', catalog)
        with self.assertRaisesRegex(ValueError, 'pinned license'):
            export.verify(self.root)
        export.publish(self.root)
        catalog = self.catalog()
        catalog['datasets'][0]['references'] = []
        self.write(export.PUBLIC + '/catalog.json', catalog)
        with self.assertRaisesRegex(ValueError, 'model references'):
            export.verify(self.root)

    def test_nondeterministic_gzip_header_is_refused_even_with_updated_hash(self):
        export.publish(self.root)
        catalog = self.catalog()
        item = catalog['datasets'][0]
        path = self.root / item['path']
        content = bytearray(path.read_bytes())
        content[4] = 1
        path.write_bytes(content)
        item['gzip_sha256'] = export.digest(content)
        self.write(export.PUBLIC + '/catalog.json', catalog)
        with self.assertRaisesRegex(ValueError, 'header is not deterministic'):
            export.verify(self.root)

    def test_live_embedded_model_must_match_current_manifest(self):
        model = b'original fixed model fixture'
        model_path = self.root / 'crates/airk-retrieval/assets/intent-v1.bin'
        model_path.parent.mkdir(parents=True)
        model_path.write_bytes(model)
        current = {**self.manifest, 'asset_sha256': export.digest(model), 'asset_bytes': len(model)}
        report = {'asset_sha256': export.digest(model), 'datasets': {'train': self.manifest['dataset']}}
        self.write('tools/parser-train/qualification.json', report)
        current.update({'datasets': report['datasets'], 'qualification_report': 'tools/parser-train/qualification.json',
                        'qualification_report_sha256': export.digest(export.encoded(report))})
        self.write('tools/parser-train/manifest.json', current)
        export.publish(self.root)
        model_path.write_bytes(b'different model bytes')
        with self.assertRaisesRegex(ValueError, 'Embedded intent model differs'):
            export.verify(self.root)

    def test_role_authority_and_fictional_tokens_are_required(self):
        self.write('tools/parser-train/repositories.lock.json', {'repositories': [{
            'repository': 'fictional/demo', 'revision': 'b' * 40, 'license_files': []}]})
        source = {'queries': [{'family': 'writer/find-doc', 'intent': 'search', 'role': 'writer',
                               'expected_tools': ['search'], 'target': '{s}'}]}
        self.write('tools/parser-train/role-queries.json', source)
        self.write('tools/parser-train/role-phrase-splits-v5.json', {
            'role_source_sha256': export.digest(export.encoded(source)), 'contradictory_labels': 0,
            'families': [{'family': 'writer/find-doc', 'group': 'joint-v5/fixture', 'split': 'train'}]})
        token = 'airk-r1:' + '1' * 32 + ':12:' + '2' * 64
        row = {'text': 'Find the fictional retry guide', 'intent': 'search', 'role': 'writer',
               'split': 'train', 'family': 'joint-v5/fixture', 'authored_family': 'writer/find-doc',
               'query_id': 'role/0', 'origin': 'original-role-inventory', 'repository': 'fictional/demo',
               'revision': 'b' * 40, 'vocabulary_sha256': 'c' * 64,
               'literal_slots': {'s': 'fictional retry guide', 'p': 'docs/guide.md', 'scope': 'docs',
                                 'k': 'struct', 'token': token, 'baseline': token},
               'expected_tools': ['search'], 'expected_target': 'fictional retry guide',
               'label_provenance': 'Original agent-authored finite-tool intent; human review pending.'}
        self.set_data([row])
        export.publish(self.root)
        self.set_data([{**row, 'literal_slots': {**row['literal_slots'], 'token': 'airk-r1:actual-private-token'}}])
        with self.assertRaisesRegex(ValueError, 'non-fictional receipt'):
            export.publish(self.root)
        self.set_data([{**row, 'expected_target': 'different target'}])
        with self.assertRaisesRegex(ValueError, 'public authored query contract'):
            export.publish(self.root)

    def test_original_readme_whitespace_is_compressed_without_altering_legal_text(self):
        revision = 'e' * 40
        legal = b'Original legal text\r\n'
        readme = b'# Original upstream README  \r\nAttribution with intentional spaces.  \r\n'
        prefix = 'target/parser-train/online/clinc150-' + revision[:12] + '/'
        inputs = []
        for name, content in [('LICENSE', legal), ('README.md', readme)]:
            path = self.root / (prefix + name)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            inputs.append({'path': name, 'local_path': prefix + name, 'sha256': export.digest(content),
                           'bytes': len(content), 'url': 'https://example.invalid/' + name})
        self.write('tools/parser-train/online.lock.json', {'sources': [{
            'id': 'clinc150', 'repository': 'clinc/oos-eval', 'revision': revision,
            'license': 'CC-BY-3.0', 'attribution': 'Original public attribution fixture', 'inputs': inputs}]})
        plain = export.PUBLIC + '/licenses/online-clinc150-' + revision[:12] + '-README-md.txt'
        old = self.root / plain
        old.parent.mkdir(parents=True, exist_ok=True)
        old.write_bytes(readme)
        export.publish(self.root)
        self.assertFalse(old.exists(), 'Only the matching generated plaintext README is retired')
        zipped = self.root / (plain + '.gz')
        self.assertEqual(gzip.decompress(zipped.read_bytes()), readme)
        notice = self.root / (export.PUBLIC + '/licenses/online-clinc150-' + revision[:12] + '-LICENSE.txt')
        self.assertEqual(notice.read_bytes(), legal, 'Original license stays byte-exact plaintext')
        entry = next(row for row in self.catalog()['license_files'] if row['path'] == plain + '.gz')
        self.assertEqual(entry['raw_sha256'], export.digest(readme))
        self.assertEqual(entry['raw_bytes'], len(readme))
        self.assertEqual(entry['sha256'], export.digest(zipped.read_bytes()))
        self.assertEqual(entry['bytes'], zipped.stat().st_size)
        export.verify(self.root)
        old.write_bytes(b'Existing contributor work must survive\n')
        with self.assertRaisesRegex(ValueError, 'differs; it was preserved'):
            export.publish(self.root)
        self.assertEqual(old.read_bytes(), b'Existing contributor work must survive\n')


if __name__ == '__main__':
    unittest.main()
