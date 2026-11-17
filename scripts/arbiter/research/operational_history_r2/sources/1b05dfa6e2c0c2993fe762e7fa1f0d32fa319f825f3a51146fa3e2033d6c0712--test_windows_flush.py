# SPDX-License-Identifier: Apache-2.0
"""Actual Windows held-open journal notification controls. No NN/native model."""
import importlib.util,json,os,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
HELPER=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102/registry_events.py'
spec=importlib.util.spec_from_file_location('existing_events',HELPER);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class Controls(unittest.TestCase):
    @unittest.skipUnless(os.name=='nt','actual Windows directory notification control')
    def test_synced_held_open_record_wakes_two_original_watchers(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as folder:
            with (Path(folder)/'journal').open('wb',buffering=0) as file:
                a=module.RegistryEvents(folder);b=module.RegistryEvents(folder)
                try:
                    record=b'{"actual_os_control":true}\n';file.write(record);os.fsync(file.fileno())
                    original_deadline=time.monotonic_ns()+2_000_000_000
                    a.wait(original_deadline);b.wait(original_deadline)
                    self.assertFalse(file.closed)
                    self.assertEqual((Path(folder)/'journal').read_bytes(),record)
                finally:a.close();b.close()
    @unittest.skipUnless(os.name=='nt','actual Windows directory notification control')
    def test_synced_completion_after_prior_unrelated_write_is_not_lost(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as folder:
            journal=Path(folder)/'journal'
            with journal.open('wb',buffering=0) as file:
                events=module.RegistryEvents(folder)
                try:
                    (Path(folder)/'unrelated').write_bytes(b'not authority')
                    events.wait(time.monotonic_ns()+2_000_000_000)
                    self.assertEqual(journal.read_bytes(),b'')
                    file.write(b'{"completion":true}\n');os.fsync(file.fileno())
                    events.wait(time.monotonic_ns()+2_000_000_000)
                    self.assertEqual(json.loads(journal.read_bytes()),{'completion':True})
                finally:events.close()
if __name__=='__main__':unittest.main()
