# SPDX-License-Identifier: Apache-2.0
"""The build-host digest path supports Python3.9 without changing identities."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

SPEC=importlib.util.spec_from_file_location("static_digest",Path(__file__).parents[1]/"provision-static-onnx-runtime.py")
MODULE=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StreamDigestTests(unittest.TestCase):
    def test_independent_known_sha256_and_sha1_vectors(self):
        self.assertEqual(MODULE.stream_digest(io.BytesIO(b"abc")).hexdigest(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        self.assertEqual(MODULE.stream_digest(io.BytesIO(b"abc"),"sha1").hexdigest(),"a9993e364706816aba3e25717850c26c9cd0d89d")

    def test_large_partial_chunks_and_path_use_identical_bytes(self):
        data=bytes(range(256))*(8192+17)+b"tail"
        class Partial(io.BytesIO):
            def read(self,count=-1):
                return super().read(min(count,7919))
        expected=hashlib.sha256(data).hexdigest()
        self.assertEqual(MODULE.stream_digest(Partial(data)).hexdigest(),expected)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"input.bin"
            path.write_bytes(data)
            self.assertEqual(MODULE.digest(path),expected)


if __name__=="__main__":unittest.main()
