# SPDX-License-Identifier: Apache-2.0
"""Publication adapters consume the reviewed /2 policy; no native binaries execute."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]

def load(name):
    spec=importlib.util.spec_from_file_location('adapter_'+name.replace('-','_'),ROOT/'scripts'/f'{name}.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

CANDIDATES=load('ci-candidates');SITE=load('build-downloads')
CASES=json.loads((ROOT/'fixtures/split/receipts/publication_2.json').read_text(encoding='utf-8'))
RELEASE=next(row['input'] for row in CASES['cases'] if row['name']=='complete stable official installer release')
CATALOG=next(row['input'] for row in CASES['cases'] if row['name']=='complete stable portable catalog')

class PublicationAdapters(unittest.TestCase):
    def test_same_run_input_catalog_includes_all_four_portable_artifacts(self):
        commit=CATALOG['commit'];expected=set(CATALOG['release_inputs'])
        self.assertEqual(CANDIDATES.release_input_names(commit,'2002',version='1.8.35'),expected)
        artifacts=[{'id':number+1,'name':name,'expired':False,'digest':'sha256:'+'a'*64,
                    'workflow_run':{'id':2002,'head_sha':commit,'head_branch':'main'}} for number,name in enumerate(sorted(expected))]
        run={'id':2002,'run_attempt':1,'head_sha':commit,'head_branch':'main','event':'workflow_dispatch',
             'path':'.github/workflows/github-release.yml','repository':{'full_name':'iokaio/airk'},
             'head_repository':{'full_name':'iokaio/airk'}}
        for target in ('aarch64-apple-darwin','x86_64-apple-darwin','x86_64-pc-windows-msvc','x86_64-unknown-linux-gnu'):
            missing=f'airk-portable-{target}-{commit}-2002'
            with tempfile.TemporaryDirectory() as temporary,patch.object(CANDIDATES,'api',return_value=run),patch.object(CANDIDATES,'run_artifacts',return_value=[a for a in artifacts if a['name']!=missing]),patch.object(CANDIDATES,'download') as download:
                output=Path(temporary)/'download'
                with self.subTest(target=target),self.assertRaisesRegex(ValueError,'13 unique'):
                    CANDIDATES.release_inputs(commit,'2002',output,version='1.8.35')
                self.assertFalse(output.exists());download.assert_not_called()
    def test_complete_same_run_input_selection_downloads_exact_thirteen_bound_artifacts(self):
        commit=CATALOG['commit'];expected=set(CATALOG['release_inputs'])
        artifacts=[{'id':number+1,'name':name,'expired':False,'digest':'sha256:'+'a'*64,
                    'workflow_run':{'id':2002,'head_sha':commit,'head_branch':'main'}} for number,name in enumerate(sorted(expected))]
        run={'id':2002,'run_attempt':1,'head_sha':commit,'head_branch':'main','event':'workflow_dispatch',
             'path':'.github/workflows/github-release.yml','repository':{'full_name':'iokaio/airk'},'head_repository':{'full_name':'iokaio/airk'}}
        def download(_artifact,destination):
            destination.mkdir();(destination/('source.json' if destination.name.startswith('airk-source-') else 'opaque')).write_text('{}')
        with tempfile.TemporaryDirectory() as temporary,patch.object(CANDIDATES,'api',return_value=run),patch.object(CANDIDATES,'run_artifacts',return_value=artifacts),patch.object(CANDIDATES,'download',side_effect=download) as copied:
            output=Path(temporary)/'download';CANDIDATES.release_inputs(commit,'2002',output,version='1.8.35')
            self.assertEqual({path.name for path in output.iterdir()},expected);self.assertEqual(copied.call_count,13)

    def release_metadata(self, document):
        encoded=json.dumps(document).encode();tag=document['tag']
        names=set(document['assets'])|{'release.json','release-notes.md','SHA256SUMS'}
        release={'tag_name':tag,'draft':False,'prerelease':document['channel']=='preview','published_at':'2026-11-14T00:00:00Z',
                 'assets':[{'id':number+1,'name':name,'state':'uploaded','size':len(encoded) if name=='release.json' else 1234,
                            'digest':'sha256:'+(SITE.hashlib.sha256(encoded).hexdigest() if name=='release.json' else document['assets'].get(name,'0'*64)),
                            'browser_download_url':f'{SITE.GITHUB}/releases/download/{tag}/{name}'} for number,name in enumerate(sorted(names))]}
        return release,encoded
    def test_site_exposes_user_portable_zips_separate_from_developer_payload(self):
        release,record=self.release_metadata(RELEASE)
        selected=SITE.select_releases([release],lambda _:record)
        html=SITE.render_downloads(selected)
        self.assertEqual(html.count('Portable AIRK Service ZIP (manual included)'),4)
        self.assertEqual(html.count('Developer native payload'),4)
        self.assertEqual(html.count('>Download installer '),4)
        for target in ('aarch64-apple-darwin','x86_64-apple-darwin','x86_64-pc-windows-msvc','x86_64-unknown-linux-gnu'):
            self.assertIn(f'/airk-1.8.35-{target}-portable.zip',html)
            self.assertIn(f'/airk-1.8.35-{target}-portable.manifest.json',html)
    def test_malformed_official_installer_capability_is_refused_even_with_matching_record_digest(self):
        record=copy.deepcopy(RELEASE);record['installers']['x86_64-pc-windows-msvc']['headless']['prompts']=0
        release,encoded=self.release_metadata(record)
        with self.assertRaisesRegex(ValueError,'capability'):
            SITE.select_releases([release],lambda _:encoded)

class PortableAssembly(unittest.TestCase):
    def setUp(self):
        import os
        context = patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"})
        context.start()
        self.addCleanup(context.stop)

    def test_actual_four_target_portable_verifiers_bind_assembled_and_published_runtime_bytes(self):
        # Native images/signature receipts are fictional, as in existing release
        # tests. Only universal VSIX verification is replaced here: this case
        # targets real portable construction, assembly and final bundle rechecks.
        from scripts.tests import test_github_release as fixtures
        release=fixtures.release;owner=load('package-portable');version='1.8.35';producer=fixtures.split_producer()
        with tempfile.TemporaryDirectory(dir=ROOT/'artifacts') as temporary:
            root=Path(temporary);inputs=root/'inputs';inputs.mkdir()
            for target in ('x86_64-unknown-linux-gnu','x86_64-pc-windows-msvc','aarch64-apple-darwin','x86_64-apple-darwin'):
                original=root/'originals'/target;fixtures.split_fixture(original,target,version=version)
                release.write_source_record(inputs/f'airk-source-{target}-{fixtures.COMMIT}-456',original,producer,target,fixtures.COMMIT)
                signed=target!='x86_64-unknown-linux-gnu'
                native_dir=inputs/f"{'airk-signed' if signed else 'airk'}-native-{target}-{fixtures.COMMIT}-456"
                fixtures.split_fixture(native_dir,target,signed,version=version)
                packaged=root/'portable'/target;owner.package(native_dir,packaged,channel='preview')
                import shutil
                shutil.copytree(packaged/'publish',inputs/f'airk-portable-{target}-{fixtures.COMMIT}-456')
            vsix=root/f'airk-{version}.vsix';vsix.write_bytes(b'fictional universal package; no package qualification claim')
            manifest=root/'vsix-manifest.json';manifest.write_text('{}')
            output=root/'release'
            with patch.object(release,'universal_vsix',return_value=(manifest,vsix)):
                record=release.assemble_split(inputs,output,fixtures.COMMIT,version,'v1.8.35-build.1','preview','456','1',producer,
                                              tree=fixtures.SPLIT_TREE,pinned=True,source_records=True)
            self.assertEqual(record['schema'],'airk-github-release/2');self.assertEqual(record['installers'],{})
            self.assertEqual(len(record['assets']),18)
            verified=release.verify_bundle(output,fixtures.COMMIT,version,'v1.8.35-build.1','456','1')
            self.assertEqual(verified,record)
            path=output/'airk-1.8.35-x86_64-unknown-linux-gnu-portable.zip';path.write_bytes(b'changed after assembly')
            with self.assertRaisesRegex(ValueError,'digest mismatch'):
                release.verify_bundle(output,fixtures.COMMIT,version,'v1.8.35-build.1','456','1')

if __name__=='__main__':unittest.main()
