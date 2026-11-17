# SPDX-License-Identifier: Apache-2.0
"""Synthetic source inventory controls, no native execution or source approval."""
import copy
import unittest
from launch_native import check_combined_source_bridge

class SourceBridge(unittest.TestCase):
    def setUp(self):
        self.before={'scoring.rs':'unchanged','observer.rs':'old','admin.rs':'old-admin','removed.rs':'old-removed'}
        self.after={'scoring.rs':'unchanged','observer.rs':'new','admin.rs':'new-admin','added.rs':'new-added'}
        self.bridge={'schema':'airk.private.reviewed-native-source-bridge/1','old_native_group':'old-group',
            'new_native_group':'new-group','changes':{
                'observer.rs':{'before_sha256':'old','after_sha256':'new','reviewed_scope':'synthetic observer review'},
                'admin.rs':{'before_sha256':'old-admin','after_sha256':'new-admin','reviewed_scope':'synthetic separate review'},
                'removed.rs':{'before_sha256':'old-removed','after_sha256':None,'reviewed_scope':'synthetic removal review'},
                'added.rs':{'before_sha256':None,'after_sha256':'new-added','reviewed_scope':'synthetic additive review'}}}
    def check(self,bridge):
        check_combined_source_bridge(bridge,'old-group','new-group',self.before,self.after)
    def test_complete_combined_delta_including_other_reviewed_scope(self):
        self.check(self.bridge)
    def test_omitted_actual_delta_refuses(self):
        b=copy.deepcopy(self.bridge);del b['changes']['admin.rs']
        with self.assertRaises(ValueError):self.check(b)
    def test_invented_delta_refuses(self):
        b=copy.deepcopy(self.bridge);b['changes']['scoring.rs']={}
        with self.assertRaises(ValueError):self.check(b)
    def test_before_after_and_review_scope_changes_refuse(self):
        for field,value in [('before_sha256','wrong'),('after_sha256','wrong'),('reviewed_scope','')]:
            b=copy.deepcopy(self.bridge);b['changes']['observer.rs'][field]=value
            with self.assertRaises(ValueError):self.check(b)
    def test_group_or_schema_substitution_refuses(self):
        for field in ['schema','old_native_group','new_native_group']:
            b=copy.deepcopy(self.bridge);b[field]='wrong'
            with self.assertRaises(ValueError):self.check(b)

if __name__=='__main__':unittest.main()
