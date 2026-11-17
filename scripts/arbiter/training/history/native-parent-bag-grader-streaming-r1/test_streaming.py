# SPDX-License-Identifier: Apache-2.0
"""Cheap SHA semantics and bounded-read controls, no grade/data/model execution."""
from pathlib import Path
import unittest
from unittest.mock import patch
import grade_dev as owner

class Stream:
    def __init__(self,payload):self.payload=payload;self.requests=[]
    def __enter__(self):return self
    def __exit__(self,*args):return False
    def read(self,count):
        if not 0<count<=1024*1024:raise MemoryError("whole-file read forbidden")
        self.requests.append(count);result=self.payload[:count];self.payload=self.payload[count:];return result

class StreamingSHA(unittest.TestCase):
    def test_known_sha_and_no_whole_file_read(self):
        source=Stream(b"abc")
        with patch.object(Path,"read_bytes",side_effect=MemoryError("observed original failure")),patch.object(Path,"open",return_value=source):
            self.assertEqual(owner.sha("inert"),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        self.assertEqual(source.requests,[1024*1024,1024*1024])
    def test_empty_sha_preserves_original_semantics(self):
        with patch.object(Path,"open",return_value=Stream(b"")):
            self.assertEqual(owner.sha("inert"),"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")

if __name__=="__main__":unittest.main()
