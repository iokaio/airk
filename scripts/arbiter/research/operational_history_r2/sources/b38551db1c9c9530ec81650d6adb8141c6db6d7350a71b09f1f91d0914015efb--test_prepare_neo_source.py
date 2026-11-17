# SPDX-License-Identifier: Apache-2.0
"""Artifact-tool regressions only; no real source export, Git commits or build."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('neo_export',Path(__file__).with_name('prepare-neo-source.py'))
EXPORT=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(EXPORT)

class ExportTests(unittest.TestCase):
    def test_git_context_cannot_redirect_source_or_restore_writes(self):
        original={'PATH':'preserved','GIT_DIR':'foreign-user-repository','GIT_INDEX_FILE':'foreign-index','GIT_WORK_TREE':'foreign-tree','GIT_OBJECT_DIRECTORY':'foreign-objects','GIT_CONFIG_COUNT':'1','GIT_CONFIG_KEY_0':'core.worktree','GIT_CONFIG_VALUE_0':'foreign','GIT_CONFIG_GLOBAL':'existing-read-only-user-config'}
        result=EXPORT.git_environment(original)
        self.assertEqual(result,{'PATH':'preserved','GIT_CONFIG_GLOBAL':'existing-read-only-user-config'})
        self.assertEqual(original['GIT_DIR'],'foreign-user-repository','never mutate the caller')
    def test_private_unsafe_and_index_paths_are_refused(self):
        for name in ('../escape','/absolute','docs/../escape','docs/.env','docs/temp/input.rs','crates/.airk/manifest.json','docs/state.sqlite','docs/private.key','packages/node_modules/module.js','docs\\alias','docs/a\x00b','.codex/config.toml'):
            with self.subTest(name=name),self.assertRaises(ValueError):EXPORT.safe_name(name)
        self.assertEqual(EXPORT.safe_name('docs/public-source.md'),'docs/public-source.md')

    def test_real_single_link_identity_and_budget_checks(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'fictional.txt';path.write_bytes(b'public\r\nbytes')
            self.assertEqual(EXPORT.bounded_bytes(path),b'public\r\nbytes')
            with self.assertRaises(ValueError):EXPORT.bounded_bytes(path,2)
            os.link(path,Path(folder)/'hardlink')
            with self.assertRaises(ValueError):EXPORT.bounded_bytes(path)

    def test_exact_archive_bytes_modes_and_original_files_survive(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'fictional';source.mkdir();(source/'docs').mkdir()
            data=b'#!/bin/sh\r\nprintf public\r\n';(source/'docs/example.sh').write_bytes(data)
            rows=[{'path':'docs/example.sh','bytes':len(data),'sha256':EXPORT.sha(data),'mode':0o755}]
            archive=root/'test.tar.gz';EXPORT.write_archive(archive,source,rows)
            output=root/'received';output.mkdir();EXPORT.archive_members(archive,rows,output)
            self.assertEqual((source/'docs/example.sh').read_bytes(),data)
            self.assertEqual((output/'docs/example.sh').read_bytes(),data)
            with tarfile.open(archive) as tar:self.assertEqual(tar.getmember('docs/example.sh').mode,0o755)
            (source/'docs/example.sh').write_bytes(b'changed after review')
            with self.assertRaisesRegex(ValueError,'changed'):EXPORT.write_archive(root/'changed.tar.gz',source,rows)

    def test_tar_redirects_extra_members_and_corrupt_bodies_never_restore(self):
        for name,kind,body in [('../escape',tarfile.REGTYPE,b'public'),('docs/link',tarfile.SYMTYPE,b''),('docs/input',tarfile.REGTYPE,b'changed')]:
            with self.subTest(name=name),tempfile.TemporaryDirectory() as folder:
                root=Path(folder);archive=root/'untrusted.tar.gz';destination=root/'received';destination.mkdir()
                with tarfile.open(archive,'w:gz') as tar:
                    info=tarfile.TarInfo(name);info.type=kind;info.size=len(body);info.mode=0o644;info.linkname='../../escape';tar.addfile(info,io.BytesIO(body))
                rows=[{'path':'docs/input','bytes':6,'sha256':EXPORT.sha(b'public'),'mode':0o644}]
                with self.assertRaises(ValueError):EXPORT.archive_members(archive,rows,destination)
                self.assertFalse((root/'escape').exists())

    def test_staged_changes_refuse_without_any_git_mutation(self):
        head=[{'path':'docs/public.md','mode':'100644','object':'a'*40}]
        with patch.object(EXPORT,'git',return_value=('100644 '+'b'*40+' 0\tdocs/public.md\0').encode()) as command:
            with self.assertRaisesRegex(ValueError,'Staged changes'):EXPORT.check_index(Path('.'),head)
            self.assertEqual(command.call_args.args[1:],('ls-files','--stage','-z'))

    def test_skip_worktree_and_assume_unchanged_flags_are_not_silently_lost(self):
        head=[{'path':'docs/public.md','mode':'100644','object':'a'*40}]
        normal=('100644 '+'a'*40+' 0\tdocs/public.md\0').encode()
        for flag in (b'h ',b'S '):
            with self.subTest(flag=flag),patch.object(EXPORT,'git',side_effect=[normal,flag+b'docs/public.md\0']):
                with self.assertRaisesRegex(ValueError,'Special index flags'):EXPORT.check_index(Path('.'),head)

    def test_finalize_without_review_and_changing_inspection_publish_no_frozen_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);base=root/'artifacts/local/one-airk-1814';base.mkdir(parents=True);(root/'target/local-vector-model').mkdir(parents=True)
            before={'head':'a'*40,'tree':'b'*40,'object_format':'sha1','native_source':{'sha256':'c'*64,'files':1},'files':[],'head_files':[],'deleted_from_head':[],'git_status_sha256':'d'*64}
            models={'model_roster_sha256':'e'*64}
            args=SimpleNamespace(root=root,output=base/'unreviewed',model_dir=None,finalize=True,freeze_from=None)
            with patch.object(EXPORT,'git',return_value=str(root).encode()),patch.object(EXPORT,'source_snapshot',return_value=dict(before)),patch.object(EXPORT,'model_snapshot',return_value=models),patch.object(EXPORT,'audit_history',return_value={}),patch.object(EXPORT,'write_archive') as write:
                with self.assertRaisesRegex(ValueError,'requires a reviewed'):EXPORT.produce(args)
                self.assertFalse(args.output.exists());write.assert_not_called()
            args.output=base/'changed';args.finalize=False
            after={**before,'files':[{'path':'docs/changed.md'}]}
            with patch.object(EXPORT,'git',return_value=str(root).encode()),patch.object(EXPORT,'source_snapshot',side_effect=[dict(before),after]),patch.object(EXPORT,'model_snapshot',return_value=models),patch.object(EXPORT,'audit_history',return_value={}):
                with self.assertRaisesRegex(ValueError,'Source changed during'):EXPORT.produce(args)
                self.assertFalse((args.output/'source-manifest.json').exists());self.assertFalse((args.output/'public-source.tar.gz').exists())

    def test_deleted_head_file_and_crlf_dirty_overlay_are_recorded(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'docs').mkdir();data=b'new dirty bytes\r\n';(root/'docs/kept.md').write_bytes(data)
            heads=[{'path':'docs/deleted.md','mode':'100644','object':'a'*40},{'path':'docs/kept.md','mode':'100644','object':'b'*40}]
            def git(_root,*args,**_kw):
                if args[0]=='ls-files':return b'docs/deleted.md\0docs/kept.md\0'
                if args[0]=='status':return b' D docs/deleted.md\0 M docs/kept.md\0'
                return {'HEAD':b'c'*40,'HEAD^{tree}':b'd'*40,'--show-object-format':b'sha1'}[args[1]]
            with patch.object(EXPORT,'head_rows',return_value=heads),patch.object(EXPORT,'check_index'),patch.object(EXPORT,'git',side_effect=git),patch.object(EXPORT,'load',return_value=SimpleNamespace(source_identity=lambda:{'sha256':'e'*64,'files':2})):
                report=EXPORT.source_snapshot(root)
            self.assertEqual(report['deleted_from_head'],heads[:1]);self.assertTrue(report['dirty']);self.assertEqual(report['files'][0]['sha256'],EXPORT.sha(data));self.assertEqual(report['files'][0]['mode'],0o644)

    def test_public_models_require_exact_pinned_roster(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);directory=root/'models';profile=directory/'fictional-profile';profile.mkdir(parents=True)
            assets=[]
            for name,data in [('model.onnx',b'fictional public model'),('tokenizer.json',b'{}'),('LICENSE',b'fictional public license')]:
                (profile/name).write_bytes(data);assets.append({'name':name,'url':'https://example.invalid/public','bytes':len(data),'sha256':EXPORT.sha(data)})
            manifest={'fictional':'public receipt'};(profile/'model.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode('utf-8'))
            owner=SimpleNamespace(CATALOG={'profiles':[{'id':'fictional-profile','assets':assets,'manifest':manifest}]},validate_asset_bindings=lambda *_:None)
            with patch.object(EXPORT,'load',return_value=owner):
                report=EXPORT.model_snapshot(root,directory);self.assertEqual(report['file_count'],4)
                (profile/'private-index.db').write_bytes(b'never export')
                with self.assertRaisesRegex(ValueError,'Only exact'):EXPORT.model_snapshot(root,directory)

if __name__=='__main__':unittest.main()
