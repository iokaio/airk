# SPDX-License-Identifier: Apache-2.0
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch
from scripts.tests import test_campaign_release as campaign
from scripts.tests.test_optional_publication import hypothetical_projection
from scripts.tests.test_optional_publication import PARTS

OWNER = campaign.release.producers
COMMIT = campaign.COMMIT


class OptionalSignedOrigin(unittest.TestCase):
    def test_complete_handoff_downloads_each_original_part_and_rechecks_live_origin(self):
        for fault in (None, 'changed-part', 'late-origin'):
            owner, artifacts, jobs, api = self.observations()
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary, patch.object(OWNER, 'api', side_effect=api), patch.object(OWNER.optional, 'projection', return_value=owner):
                record = OWNER.attest_signed_release(456, COMMIT, branch='feat/advanced-split', optional_projection=owner)
                producer = OWNER.attest(123, COMMIT, allow_manual=True, version='1.9.0', branch='feat/advanced-split')
                originals = {'release.json': b'original document', 'native.zip': b'original signed bytes'}
                downloads, snapshots = [], []
                def download(artifact, directory):
                    directory.mkdir()
                    downloads.append(artifact['id'])
                    if artifact['id'] == record['artifact']['id']:
                        for name, data in originals.items(): (directory / name).write_bytes(data)
                    else:
                        name = next(name for name, row in record['parts'].items() if row['id'] == artifact['id'])
                        (directory / name).write_bytes(b'x' if fault == 'changed-part' else PARTS[name])
                        if fault == 'late-origin': artifacts[0]['digest'] = 'sha256:' + 'f' * 64
                def verify(directory, *args):
                    snapshots.append({p.name: p.read_bytes() for p in directory.iterdir()})
                    return {'producer': producer, 'channel': 'stable', 'tree': 'b' * 40}
                fake = SimpleNamespace(identity=lambda version: 'v'+version, verify_bundle=verify)
                real_spec = OWNER.importlib.util.spec_from_file_location
                real_module = OWNER.importlib.util.module_from_spec
                sentinel = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module: None))
                def specification(name, path): return sentinel if name == 'signed_release' else real_spec(name, path)
                def module(spec): return fake if spec is sentinel else real_module(spec)
                target = Path(temporary) / 'complete'
                with patch.object(OWNER, 'download', side_effect=download), patch.object(OWNER.importlib.util, 'spec_from_file_location', side_effect=specification), patch.object(OWNER.importlib.util, 'module_from_spec', side_effect=module):
                    if fault is not None:
                        with self.assertRaises(ValueError): OWNER.download_signed_release(record, COMMIT, target, '1.9.0', 'stable', tree='b'*40, full_bundle=True)
                    else:
                        OWNER.download_signed_release(record, COMMIT, target, '1.9.0', 'stable', tree='b'*40, full_bundle=True)
                        self.assertEqual(downloads, [record['artifact']['id'], 500, 501, 502, 503])
                        self.assertEqual({p.name:p.read_bytes() for p in target.iterdir()}, {**originals, **PARTS})
                        self.assertEqual(snapshots, [originals, originals, {**originals, **PARTS}])

    def observations(self):
        source = campaign.Origin()
        owner = hypothetical_projection()
        artifacts, jobs = [], []
        for index, name in enumerate(owner['assets']):
            artifacts.append({'id': 500 + index, 'name': 'airk-arbiter-part-' + name + '-' + COMMIT + '-456',
                'expired': False, 'digest': 'sha256:' + str(index + 1) * 64,
                'created_at': '2026-11-14T00:00:30Z',
                'workflow_run': {'id': 456, 'head_sha': COMMIT, 'head_branch': 'feat/advanced-split'}})
            jobs.append({'name': 'optional-parts / ' + name, 'conclusion': 'success',
                'started_at': '2026-11-14T00:00:00Z', 'completed_at': '2026-11-14T00:01:00Z'})
        def api(path, paginate=False):
            if path == 'actions/runs/456/artifacts?per_page=100':
                return [{'artifacts': [source.bundle, *artifacts]}]
            if path == 'actions/runs/456/attempts/3/jobs?per_page=100':
                original = source.api(path, paginate)
                return [{'jobs': [*original[0]['jobs'], *jobs]}]
            return source.api(path, paginate)
        return owner, artifacts, jobs, api

    def test_actual_origin_projection_preserves_small_assembly_and_binds_every_original_part(self):
        owner, artifacts, jobs, api = self.observations()
        with patch.object(OWNER, 'api', side_effect=api):
            record = OWNER.attest_signed_release(456, COMMIT, branch='feat/advanced-split', optional_projection=owner)
        self.assertEqual(record['schema'], 'airk-signed-release-producer/3')
        self.assertEqual(len(record['parts']), 4)
        self.assertEqual(record['artifact']['id'], campaign.Origin().bundle['id'])
        self.assertEqual(record['parts']['arbiter-a.part01']['id'], 500)
        self.assertEqual(record['optional_models'], owner)

    def test_each_part_requires_actual_same_run_attempt_job_interval_and_unexpired_digest(self):
        for fault in ('missing', 'duplicate', 'skipped', 'outside', 'branch', 'commit', 'run', 'expired', 'digest', 'id'):
            owner, artifacts, jobs, api = self.observations()
            if fault == 'missing': jobs.pop()
            elif fault == 'duplicate': jobs.append(deepcopy(jobs[0]))
            elif fault == 'skipped': jobs[0]['conclusion'] = 'skipped'
            elif fault == 'outside': artifacts[0]['created_at'] = '2026-11-13T23:59:59Z'
            elif fault == 'branch': artifacts[0]['workflow_run']['head_branch'] = 'main'
            elif fault == 'commit': artifacts[0]['workflow_run']['head_sha'] = 'f' * 40
            elif fault == 'run': artifacts[0]['workflow_run']['id'] = 455
            elif fault == 'expired': artifacts[0]['expired'] = True
            elif fault == 'digest': artifacts[0]['digest'] = 'sha256:' + 'z' * 64
            else: artifacts[0]['id'] = artifacts[1]['id']
            with self.subTest(fault=fault), patch.object(OWNER, 'api', side_effect=api), self.assertRaises(ValueError):
                OWNER.attest_signed_release(456, COMMIT, branch='feat/advanced-split', optional_projection=owner)


if __name__ == '__main__':
    unittest.main()
