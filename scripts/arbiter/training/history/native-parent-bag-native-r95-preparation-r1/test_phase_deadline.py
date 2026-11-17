# SPDX-License-Identifier: Apache-2.0
"""Injected-clock launcher tests; all process/ACL observers are inert mocks."""
from pathlib import Path
import json
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock,patch
import run_native as owner


class Clock:
    def __init__(self):self.now=100
    def __call__(self):return self.now
    def advance(self,seconds):self.now+=seconds


class WholePhaseDeadline(unittest.TestCase):
    def test_original_deadline_does_not_refill_after_prework_and_refuses_expiry(self):
        clock=Clock();phase=owner.PhaseDeadline(clock=clock)
        clock.advance(120);self.assertEqual(phase.remaining(),1680)
        clock.advance(300);self.assertEqual(phase.remaining(),1380)
        clock.advance(1380)
        self.assertFalse(phase.within())
        with self.assertRaises(TimeoutError):phase.remaining()
        clock.now=99
        with self.assertRaises(TimeoutError):phase.remaining()

    def packet(self,root):
        binary=root/"inert.exe";binary.write_bytes(b"never executed")
        packet=root/"packet.json"
        packet.write_text(json.dumps({"binary":{"path":str(binary)},"master":str(root/"out/master.json"),
            "output":str(root/"out"),"pins":{},"limits":{"seconds":1800,"host_bytes":8589934592,"max_job_processes":2}}))
        return packet

    def test_listing_expiry_refuses_before_anchor_or_native_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);packet=self.packet(root);clock=Clock()
            original=owner.PhaseDeadline
            support=Mock();support.anchor_path.return_value=root/"never-created-anchor"
            def listing(*args,**kwargs):
                self.assertEqual(kwargs["timeout"],10)
                clock.advance(1800)
                return SimpleNamespace(returncode=0,stdout="unused")
            argv=["run_native.py","--packet",str(packet),"--packet-sha256",owner.sha(packet),"--stage","emit","--execute-reviewed"]
            with patch.object(owner,"PhaseDeadline",lambda:original(clock=clock)),patch.object(owner,"load_anchor_support",return_value=support),patch.object(owner.subprocess,"run",side_effect=listing),patch("sys.argv",argv):
                with self.assertRaises(TimeoutError):owner.main()
            support.create.assert_not_called()
            self.assertFalse((root/"out").exists())

    def test_native_receives_only_remaining_and_post_expiry_is_failed(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);packet=self.packet(root);clock=Clock();original=owner.PhaseDeadline
            proof={"inert":"no actual ACL proof"};support=Mock()
            support.anchor_path.return_value=root/"never-created-anchor"
            def support_loaded(*args):clock.advance(20);return support
            def listing(*args,**kwargs):
                clock.advance(30)
                return SimpleNamespace(returncode=0,stdout="inference_owner::evaluation_tests::native_parent_bag::emit_parent_bag_epoch1_native_manifests: test\n")
            def create(*args,**kwargs):clock.advance(120);return proof
            support.create.side_effect=create
            def post(*args,**kwargs):clock.advance(1800);return proof
            support.observe.side_effect=post
            native=Mock()
            native.run_windows_owned_training.return_value={"inert_owned_result":True}
            spec=SimpleNamespace(loader=Mock())
            argv=["run_native.py","--packet",str(packet),"--packet-sha256",owner.sha(packet),"--stage","emit","--execute-reviewed"]
            with patch.object(owner,"PhaseDeadline",lambda:original(clock=clock)),patch.object(owner,"load_anchor_support",side_effect=support_loaded),patch.object(owner.subprocess,"run",side_effect=listing),patch.object(owner.importlib.util,"spec_from_file_location",return_value=spec),patch.object(owner.importlib.util,"module_from_spec",return_value=native),patch("sys.argv",argv):
                with self.assertRaisesRegex(RuntimeError,"retained evidence is not a pass"):owner.main()
            self.assertEqual(native.run_windows_owned_training.call_args.args[3],1630)
            receipt=json.loads((root/"out/execution-emit/execution.json").read_bytes())
            self.assertFalse(receipt["complete"])
            self.assertEqual(receipt["owned"],{"inert_owned_result":True})
            self.assertFalse(receipt["native_completion_unknown"])
            self.assertEqual(receipt["failure"]["anchor_error"]["type"],"TimeoutError")


if __name__=="__main__":unittest.main()
