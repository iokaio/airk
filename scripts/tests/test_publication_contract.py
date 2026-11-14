# SPDX-License-Identifier: Apache-2.0
"""Versioned publication contracts over independent complete retained documents."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location('publication',ROOT/'scripts/publication-contract.py')
PUBLICATION=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(PUBLICATION)
CASES=json.loads((ROOT/'fixtures/split/receipts/publication_2.json').read_text(encoding='utf-8'))

class PublicationTests(unittest.TestCase):
    def test_independent_full_documents_and_refusals(self):
        self.assertEqual(CASES['schema'],'airk.publication-cases/2')
        self.assertEqual(len(CASES['cases']),25)
        accepted=0
        for row in CASES['cases']:
            before=copy.deepcopy(row['input'])
            with self.subTest(name=row['name'],kind=row['kind']):
                if row['accepted']:
                    accepted+=1
                    self.assertEqual(getattr(PUBLICATION,'validate_'+row['kind'])(row['input']),
                                     'airk-release-catalog/2' if row['kind']=='catalog' else 'airk-github-release/2')
                else:
                    with self.assertRaises(ValueError):getattr(PUBLICATION,'validate_'+row['kind'])(row['input'])
                self.assertEqual(row['input'],before)
        self.assertEqual(accepted,4)
    def test_original_catalog_fixture_retains_its_original_future_version_contract(self):
        original=json.loads((ROOT/'fixtures/split/receipts/release_catalog.json').read_text(encoding='utf-8'))
        self.assertEqual(original['base']['package_version'],'1.9.0')
        self.assertEqual(PUBLICATION.legacy.validate_release_catalog(original['base']),'airk-release-catalog/1')
        current=CASES['cases'][0]['input'];_,inputs,assets=PUBLICATION.catalog_names('1.8.35','stable',current['commit'],current['producer'],current['release'])
        self.assertEqual(inputs,set(current['release_inputs']))
        self.assertEqual(assets,set(current['assets']))
        self.assertEqual(len(assets),26);self.assertEqual(len(inputs),13)
        self.assertEqual(len(PUBLICATION.release_assets('1.8.24','stable')),18)

if __name__=='__main__':unittest.main()
