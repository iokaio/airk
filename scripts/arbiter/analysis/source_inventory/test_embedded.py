# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('embedded',Path(__file__).with_name('embedded.py'))
owner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
class Indicators(unittest.TestCase):
    def test_source_json(self):
        rows=owner.indicators({'source_body':'def run():\n    return 1\n'})
        self.assertTrue(rows)
        self.assertTrue(all('def run' not in str(row) for row in rows))
    def test_plain_receipt(self):
        self.assertEqual(owner.indicators({'path':'source.py','sha256':'abcd','command':['python','worker.py']}),[])
    def test_patch(self):
        self.assertTrue(owner.indicators('diff --git a/x b/x\n--- a/x\n+++ b/x\n'))
    def test_nested_pointer(self):
        self.assertEqual(owner.indicators({'x/y':['import os\nprint(1)']})[0]['location'],'$/x~1y/0')
    def test_depth_bound(self):
        with self.assertRaises(ValueError):owner.indicators({},depth=81)
spec=importlib.util.spec_from_file_location('historical_links',Path(__file__).with_name('historical_links.py'))
links_owner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(links_owner)
class HistoricalLinks(unittest.TestCase):
    def test_raw_and_normalized_remain_distinct(self):
        report={'head':'head','candidates':[{'path':'a','source_sha256':'sha','classification':'committed_git_blob_match','git_blob_oid':'raw'}, {'path':'b','source_sha256':'sha2','classification':'normalized_text_committed_blob_match','git_blob_oid':'other','normalized_text_git_blob_oid':'lf'}]}
        rows=links_owner.links(report,{'raw':'old/a','lf':'old/b'})
        self.assertEqual([row['identity'] for row in rows],['raw_bytes','CRLF_to_LF_text_only'])
        self.assertTrue(all(not row['current_owner_or_callers_proven'] for row in rows))
    def test_missing_historical_object_refuses(self):
        with self.assertRaises(ValueError):
            links_owner.links({'head':'h','candidates':[{'path':'a','source_sha256':'s','classification':'committed_git_blob_match','git_blob_oid':'missing'}]}, {})
if __name__=='__main__':unittest.main()
