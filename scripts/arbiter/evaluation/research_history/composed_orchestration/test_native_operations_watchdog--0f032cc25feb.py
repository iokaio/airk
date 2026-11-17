# SPDX-License-Identifier: Apache-2.0
"""Cheap clock/prefix controls; optional Rust-emitted event cases after native build."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import unittest
from native_operations_watchdog import Operations,ObservationFailure

class OriginalClockControls(unittest.TestCase):
    def test_inactive_clock_never_mints_an_operation(self):
        value=Operations({},[],10)
        self.assertIsNone(value.expiry());value.check_expiry(999999999)
        self.assertIsNone(value.active);self.assertEqual(value.position,0)
    def test_original_expiry_has_no_notification_renewal(self):
        value=Operations({},[],10);value.active={'expires_monotonic_ns':200,'phase':'started'}
        value.check_expiry(199)
        with self.assertRaises(ObservationFailure):value.check_expiry(200)
        with self.assertRaises(ObservationFailure):value.check_expiry(201)
        self.assertEqual(value.expiry(),200)
    def test_returned_bytes_do_not_clear_end_fsync_deadline(self):
        value=Operations({},[],10);value.active={'expires_monotonic_ns':200,'phase':'returned'}
        value.check_expiry(199)
        with self.assertRaises(ObservationFailure):value.check_expiry(200)
        self.assertEqual(value.expiry(),200)
    def test_journal_prefix_cannot_be_replaced(self):
        value=Operations({},[],10);value.raw=b'original-prefix'
        with self.assertRaises(ObservationFailure):value.feed(b'different-prefix',100)
        self.assertEqual(value.raw,b'original-prefix');self.assertEqual(value.sequence,0)
    def test_journal_bound_refuses_without_parsing_or_dispatch(self):
        value=Operations({},[],10)
        with self.assertRaises(ObservationFailure):value.feed(b'x'*131073,100)
        self.assertEqual(value.sequence,0);self.assertIsNone(value.active)
    def test_incomplete_line_never_becomes_a_native_observation(self):
        value=Operations({},[],10);value.feed(b'{',100)
        self.assertEqual(value.sequence,0);self.assertIsNone(value.active)
        with self.assertRaises(ObservationFailure):value.feed(b'{}\n',100)
        self.assertEqual(value.sequence,0)

def rust_cases(path,expected):
    raw=Path(path).read_bytes()
    if hashlib.sha256(raw).hexdigest()!=expected:raise ValueError('actual Rust event fixture changed')
    events=json.loads(raw)
    if len(events)!=2:raise ValueError('actual Rust start/end fixture required')
    # Literal independently reviewed test identity/clock in operation/tests.rs.
    if events[0]['kind']!='sample' or events[0]['ordinal']!=1 or events[0]['started_monotonic_ns']!=1000:
        raise ValueError('wrong native operation owner test fixture')
    identity=events[0]['identity']
    def fresh():return Operations(identity,[{'kind':'sample','ordinal':1,'input_digest':events[0]['operation_input_digest']}],10)
    def encoded(value):return b''.join(json.dumps(row,separators=(',',':')).encode()+b'\n' for row in value)
    value=fresh();value.feed(encoded(events),8_000_001)
    assert value.sequence==2 and value.position==1 and value.active['phase']=='returned'
    assert value.expiry()==10_001_000
    try:value.check_expiry(10_001_000)
    except ObservationFailure:pass
    else:raise AssertionError('raw Returned released original end-retention clock')
    mutations=[(0,'/identity/process_id',99),(0,'/expires_monotonic_ns',10_001_001),
        (1,'/sequence',3),(1,'/started_monotonic_ns',1001),
        (1,'/operation_input_digest','0'*64),(0,'/clock','unix_monotonic')]
    for offset,pointer,replacement in mutations:
        changed=copy.deepcopy(events);target=changed[offset];parts=pointer.strip('/').split('/')
        for part in parts[:-1]:target=target[part]
        target[parts[-1]]=replacement
        try:fresh().feed(encoded(changed),8_000_001)
        except ObservationFailure:continue
        raise AssertionError('accepted native identity/clock mutation '+pointer)
    try:fresh().feed(encoded(events+events),8_000_001)
    except ObservationFailure:pass
    else:raise AssertionError('accepted duplicate native operation sequence')
    print('Rust-emitted decoder controls: 1 accepted, 8 refusal observations; no native execution')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--rust-cases');parser.add_argument('--rust-cases-sha256')
    args=parser.parse_args()
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(OriginalClockControls))
    if not result.wasSuccessful():raise SystemExit(1)
    if args.rust_cases:
        if not args.rust_cases_sha256:raise ValueError('actual Rust fixture SHA required')
        rust_cases(args.rust_cases,args.rust_cases_sha256)
    else:print('Rust-emitted decoder cases pending new native image; not selected or claimed')

if __name__=='__main__':main()
