# SPDX-License-Identifier: Apache-2.0
"""Focused offline controls for the pinned Linux producer and protected-fd projection."""
import hashlib
from pathlib import Path
import unittest

from bootstrap_projection import link_argv, project_env
from configure_ort import BASE, HERE
from notices import legal_name


class Buffer:
    def __init__(self, data):
        self.data = data

    def read_bytes(self):
        return self.data

    def write_bytes(self, value):
        self.data = value


class Producer(unittest.TestCase):
    def test_exact_published_bootstrap_source_and_refuse_second_projection(self):
        original = BASE/"linux-cuda-ort-r3/source/onnxruntime/core/platform/posix/env.cc"
        source = Buffer(original.read_bytes())
        body = (HERE/"ort_bootstrap.inc").read_bytes().replace(b"\r\n", b"\n")
        project_env(source, body)
        self.assertEqual(hashlib.sha256(source.data).hexdigest(),
                         "abb9531637f0cea1fc6a52d07302ffcf7eae37e23030c8cf5c6fbc84a3260ab9")
        saved = source.data
        with self.assertRaisesRegex(ValueError, "anchor"):
            project_env(source, body)
        self.assertEqual(source.data, saved)

    def test_missing_vendor_anchor_has_no_side_effect(self):
        source = Buffer(b"namespace onnxruntime {")
        with self.assertRaisesRegex(ValueError, "anchor"):
            project_env(source, b"example")
        self.assertEqual(source.data, b"namespace onnxruntime {")

    def test_relink_redirects_only_checked_native_module(self):
        compiler = Path("/verified/cxx")
        parent = Path("/verified/build")
        dest = Path("/separate/libonnxruntime_providers_cuda.so")
        cmd = ": && /verified/cxx -fPIC -Wl,--dependency-file=CMakeFiles/onnxruntime_providers_cuda.dir/link.d -shared -o libonnxruntime_providers_cuda.so kernel.o && :"
        self.assertEqual(link_argv(cmd,parent,dest,compiler),
            [str(compiler),"-fPIC","-Wl,-soname,libonnxruntime_providers_cuda.so","-shared","-o",str(dest),"kernel.o"])
        for bad in (cmd.replace("/verified/cxx","/usr/bin/c++"), cmd.replace("-shared","-shared -Wl,-rpath,$ORIGIN/tmp"),
                    cmd.replace("kernel.o","kernel.o && touch /tmp/x"), cmd.replace("-o libonnx","-o wrong")):
            with self.assertRaises(ValueError):
                link_argv(bad,parent,dest,compiler)

    def test_private_files_are_not_legal_notices(self):
        for name in ("LICENSE", "Third_Party_Licenses.txt", "ACKNOWLEDGEMENTS.md", "NOTICE"):
            self.assertTrue(legal_name(name))
        for name in ("notes.log","private-token.txt","LICENSEx.exe"):
            self.assertFalse(legal_name(name))


if __name__ == "__main__":
    unittest.main()
