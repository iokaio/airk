# SPDX-License-Identifier: Apache-2.0
import tempfile
import time
import unittest
from pathlib import Path
from registry_events import RegistryEvents

class NativeEventTests(unittest.TestCase):
    def test_actual_file_creation_wakes_existing_observer(self):
        with tempfile.TemporaryDirectory() as directory:
            events = RegistryEvents(directory)
            try:
                (Path(directory) / 'actual-native-event').write_bytes(b'observed')
                events.wait(time.monotonic_ns() + 2_000_000_000)
            finally:
                events.close()

    def test_original_expired_deadline_refuses_without_wait(self):
        with tempfile.TemporaryDirectory() as directory:
            events = RegistryEvents(directory)
            try:
                with self.assertRaises(TimeoutError):
                    events.wait(time.monotonic_ns() - 1)
            finally:
                events.close()

if __name__ == '__main__':
    unittest.main()
