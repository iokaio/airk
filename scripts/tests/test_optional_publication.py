# SPDX-License-Identifier: Apache-2.0
"""Pure publication cases use fictional parts; they never authorize a model."""
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('optional_publication', ROOT / 'scripts/optional-publication.py')
OWNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNER)
PRIOR = json.loads((ROOT / 'fixtures/split/receipts/publication_2.json').read_bytes())
# Independent upload-file expectations. This is not a Core Profile fixture and
# cannot replace a qualified native projection when constructing a release.
PARTS = {'arbiter-a.part01': b'a', 'arbiter-b.part01': b'bb',
         'arbiter-c.part01': b'ccc', 'arbiter-d.part01': b'dddd'}


def hypothetical_projection():
    return {'schema': 'airk.arbiter.publication/1', 'tag': 'v1.9.0',
            'compiled_catalog_sha256': 'f' * 64, 'profiles': ['1' * 64, '2' * 64, '3' * 64, '4' * 64],
            'assets': {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()} for name, data in PARTS.items()}}


def document(kind):
    base = deepcopy(next(case['input'] for case in PRIOR['cases'] if case['kind'] == kind and case['accepted'] and case['input']['channel'] == 'stable'))
    # Reuse independently retained original base rosters, preserving old bytes.
    base = json.loads(json.dumps(base).replace('1.8.35', '1.9.0'))
    base['schema'] = OWNER.CATALOG if kind == 'catalog' else OWNER.RELEASE
    base['tag'] = 'v1.9.0'
    base['optional_models'] = hypothetical_projection()
    base['assets'].update({name: hashlib.sha256(data).hexdigest() for name, data in PARTS.items()})
    if kind == 'catalog':
        base['release_inputs'] += ['airk-arbiter-part-' + name + '-' + base['commit'] + '-2002' for name in PARTS]
    return base


class OptionalPublication(unittest.TestCase):
    def test_twenty_independent_literal_publication_v3_documents(self):
        fixture=json.loads((ROOT/'fixtures/split/receipts/publication_3.json').read_bytes())
        self.assertEqual(fixture['schema'],'airk.publication-cases/3')
        self.assertEqual(len(fixture['cases']),20)
        accepted=0
        for row in fixture['cases']:
            before=deepcopy(row['input'])
            with self.subTest(name=row['name']):
                if row['accepted']:
                    getattr(OWNER,'validate_'+row['kind'])(row['input'],fixture['owner']);accepted+=1
                else:
                    with self.assertRaises(ValueError):getattr(OWNER,'validate_'+row['kind'])(row['input'],fixture['owner'])
            self.assertEqual(row['input'],before)
        self.assertEqual(accepted,4)

    def test_signed_producer_binds_separate_bounded_part_artifacts_without_widening_old_bundle(self):
        value = {'schema':'airk-signed-release-producer/3','repository':'iokaio/airk','workflow_id':42,
            'commit':'a'*40,'run_id':'456','attempt':'3','artifact_name':'airk-release-'+'a'*40+'-456-3',
            'artifact':{'id':90,'digest':'sha256:'+'9'*64},'event':'workflow_dispatch','source_branch':'feat/advanced-split',
            'qualified_commit':'a'*40,'optional_models':hypothetical_projection(),
            'parts':{name:{'id':index+1,'digest':'sha256:'+str(index+1)*64,
                     'artifact_name':'airk-arbiter-part-'+name+'-'+'a'*40+'-456'} for index,name in enumerate(PARTS)}}
        self.assertEqual(OWNER.validate_signed(value,hypothetical_projection()),'airk-signed-release-producer/3')
        for fault in ('missing','duplicate','digest','float','attempt','branch','extra','native'):
            changed=deepcopy(value)
            first=next(iter(changed['parts']))
            if fault=='missing':changed['parts'].pop(first)
            elif fault=='duplicate':changed['parts'][first]['id']=90
            elif fault=='digest':changed['parts'][first]['digest']='sha256:'+'g'*64
            elif fault=='float':changed['parts'][first]['id']=1.0
            elif fault=='attempt':changed['artifact_name']=changed['artifact_name'][:-1]+'2'
            elif fault=='branch':changed['source_branch']='main'
            elif fault=='extra':changed['parts'][first]['extra']='unreviewed'
            else:changed['optional_models']['compiled_catalog_sha256']='0'*64
            with self.subTest(fault=fault),self.assertRaises(ValueError):OWNER.validate_signed(changed,hypothetical_projection())

    def test_complete_projection_adds_exact_four_parts_preserving_all_previous_assets(self):
        for kind, schema in [('catalog', 'airk-release-catalog/3'), ('release', 'airk-github-release/3')]:
            value = document(kind)
            before = deepcopy(value)
            self.assertEqual(getattr(OWNER, 'validate_' + kind)(value, hypothetical_projection()), schema)
            self.assertEqual(value, before)
            self.assertEqual(len(value['assets']), 30)
            if kind == 'catalog': self.assertEqual(len(value['release_inputs']), 17)

    def test_missing_added_or_changed_part_and_changed_native_authority_refuse(self):
        for kind in ('catalog', 'release'):
            for fault in ('missing', 'extra', 'digest', 'native', 'tag', 'catalog_digest', 'profiles', 'unknown'):
                value = document(kind)
                if fault == 'missing': value['assets'].pop('arbiter-a.part01')
                elif fault == 'extra': value['assets']['foreign.part01'] = '0' * 64
                elif fault == 'digest': value['assets']['arbiter-b.part01'] = '0' * 64
                elif fault == 'native': value['optional_models']['assets']['arbiter-a.part01']['bytes'] = True
                elif fault == 'tag': value['tag'] = 'v1.9.1'
                elif fault == 'catalog_digest': value['optional_models']['compiled_catalog_sha256'] = '0' * 64
                elif fault == 'profiles': value['optional_models']['profiles'] = value['optional_models']['profiles'][:3]
                else: value['optional_models']['unknown'] = 'not native'
                with self.subTest(kind=kind, fault=fault), self.assertRaises(ValueError):
                    getattr(OWNER, 'validate_' + kind)(value, hypothetical_projection())
        for fault in ('missing', 'duplicate'):
            value = document('catalog')
            if fault == 'missing': value['release_inputs'].pop()
            else: value['release_inputs'].append(value['release_inputs'][-1])
            with self.assertRaises(ValueError): OWNER.validate_catalog(value, hypothetical_projection())

    def test_actual_part_staging_is_exact_and_rechecked_and_refuses_wrong_bytes(self):
        value = document('catalog')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, data in PARTS.items():
                target = root / 'inputs' / ('airk-arbiter-part-' + name + '-' + value['commit'] + '-2002')
                target.mkdir(parents=True)
                (target / name).write_bytes(data)
            result = OWNER.stage_parts(root / 'inputs', root / 'output', hypothetical_projection(), value['commit'], '2002')
            self.assertEqual(set(result), set(PARTS))
            self.assertEqual({p.name: p.read_bytes() for p in (root / 'output').iterdir()}, PARTS)
            target = root / 'inputs' / ('airk-arbiter-part-arbiter-a.part01-' + value['commit'] + '-2002')
            (target / 'arbiter-a.part01').write_bytes(b'x')
            with self.assertRaises(ValueError): OWNER.stage_parts(root / 'inputs', root / 'refused', hypothetical_projection(), value['commit'], '2002')
            self.assertFalse((root / 'refused').exists())


if __name__ == '__main__':
    unittest.main()
