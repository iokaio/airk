# SPDX-License-Identifier: Apache-2.0
import json
import tempfile
import os
import hashlib
import unittest
from pathlib import Path
from prepare_raw_packet import cargo_image
from run_raw_reviewed import source_pin,verify_absences,capture_execution,sha
from unittest.mock import patch
class CargoImageTests(unittest.TestCase):
    def row(self,path):return json.dumps({'reason':'compiler-artifact','target':{'name':'airk_kernel'},'profile':{'test':True},'executable':path})
    def test_exact_actual_kernel_image(self):
        raw=('diagnostic line\n'+self.row('actual-kernel.exe')+'\n').encode()
        self.assertEqual(cargo_image(raw),Path('actual-kernel.exe').resolve())
    def test_missing_image_refuses(self):
        with self.assertRaises(ValueError):cargo_image(b'{}\n')
    def test_ambiguous_images_refuse(self):
        with self.assertRaises(ValueError):cargo_image((self.row('first.exe')+'\n'+self.row('second.exe')).encode())
class SourceDeletionTests(unittest.TestCase):
    def test_exact_missing_sentinel_preserves_absence(self):
        with tempfile.TemporaryDirectory()as root:
            p=Path(root)/'deleted.js';pins={};absences={}
            source_pin(pins,absences,p,'missing');self.assertFalse(pins);self.assertEqual(absences,{str(p):'missing'});verify_absences(absences)
    def test_present_file_cannot_satisfy_missing(self):
        with tempfile.TemporaryDirectory()as root:
            p=Path(root)/'deleted.js';p.write_bytes(b'present')
            with self.assertRaises(ValueError):source_pin({}, {},p,'missing')
    def test_real_hash_missing_file_refuses(self):
        with tempfile.TemporaryDirectory()as root:
            with self.assertRaises(OSError):source_pin({}, {},Path(root)/'missing.js','01'*32)
    def test_post_absence_creation_refuses(self):
        with tempfile.TemporaryDirectory()as root:
            p=Path(root)/'deleted.js';absences={str(p):'missing'};p.write_bytes(b'reappeared')
            with self.assertRaises(ValueError):verify_absences(absences)
    def test_dangling_entry_detector_is_not_resolved_away(self):
        with patch('run_raw_reviewed.os.path.lexists',return_value=True):
            with self.assertRaises(ValueError):source_pin({}, {},'synthetic-dangling.js','missing')
    def test_dangling_symlink_is_not_absence(self):
        with tempfile.TemporaryDirectory()as root:
            p=Path(root)/'deleted.js'
            try:p.symlink_to(Path(root)/'nonexistent')
            except OSError as error:self.skipTest('Host symlink creation unavailable: '+str(error))
            with self.assertRaises(ValueError):source_pin({}, {},p,'missing')
class CaptureOwnedChainTests(unittest.TestCase):
    def fixture(self,root):
        r=Path(root);release=r/'release.json';release.write_text('{}');capture=r/'capture.json';capture.write_text('{}')
        row={'path':str(release),'sha256':sha(release)}
        packet=r/'packet.json';packet.write_text(json.dumps({'release':row,'capture_driver':row,'runtime_python':row,'owner':row}))
        receipt=r/'execution.json';value={'schema':'airk.private-selected-cal-capture-owned-execution/1','complete':True,'qualification_granted':False,'post_input_drift':[],'capture':{'path':str(capture),'sha256':sha(capture)},'packet':{'path':str(packet),'sha256':sha(packet)},'owned':{'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False}}
        receipt.write_text(json.dumps(value));return receipt,value,{'path':str(capture),'sha256':sha(capture)},row
    def test_exact_actual_owned_capture_chain(self):
        with tempfile.TemporaryDirectory()as root:
            path,value,capture,release=self.fixture(root);self.assertIn(str(path),capture_execution(path,sha(path),capture,release))
    def test_inner_capture_alone_is_not_owned_proof(self):
        with tempfile.TemporaryDirectory()as root:
            path,value,capture,release=self.fixture(root);value['owned']['owned_job_forced_termination']=True;path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):capture_execution(path,sha(path),capture,release)
    def test_foreign_capture_receipt_refuses(self):
        with tempfile.TemporaryDirectory()as root:
            path,value,capture,release=self.fixture(root);capture['sha256']='02'*32
            with self.assertRaises(ValueError):capture_execution(path,sha(path),capture,release)
if __name__=='__main__':unittest.main()
