# SPDX-License-Identifier: Apache-2.0
import hashlib
import unittest
from compare_public_inventory import matches


class PublicInventoryTests(unittest.TestCase):
    def test_lfs_exact_size_and_sha_required_not_name(self):
        asset = {'name': 'chosen.onnx', 'bytes': 7, 'sha256': 'a' * 64}
        self.assertTrue(matches(asset, {'rfilename': 'other.bin', 'size': 7, 'lfs': {'size': 7, 'sha256': 'a' * 64}}))
        self.assertFalse(matches(asset, {'rfilename': 'chosen.onnx', 'size': 7, 'lfs': {'size': 7, 'sha256': 'b' * 64}}))
        self.assertFalse(matches(asset, {'size': 8, 'lfs': {'size': 8, 'sha256': 'a' * 64}}))

    def test_git_blob_requires_full_small_bytes_and_both_hashes(self):
        raw = b'full notice\n'
        asset = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        remote = {'size': len(raw), 'blobId': hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()}
        self.assertTrue(matches(asset, remote, raw))
        self.assertFalse(matches(asset, remote, None))
        self.assertFalse(matches(asset, {**remote, 'blobId': '0' * 40}, raw))
        self.assertFalse(matches(asset, remote, b'bad notice!\n'))

    def test_other_model_or_quantization_is_never_a_match(self):
        asset = {'bytes': 7, 'sha256': 'a' * 64}
        self.assertFalse(matches(asset, {'size': 7, 'lfs': {'size': 7, 'sha256': 'b' * 64}}))
        self.assertFalse(matches(asset, {'size': 7, 'lfs': {'size': 6, 'sha256': 'a' * 64}}))

    def test_filename_or_unpinned_metadata_cannot_establish_identity(self):
        asset = {'name': 'model.onnx', 'bytes': 7, 'sha256': 'a' * 64}
        self.assertFalse(matches(asset, {'rfilename': 'model.onnx', 'size': 7}))
        self.assertFalse(matches(asset, {'size': '7', 'lfs': {'size': 7, 'sha256': 'a' * 64}}))


if __name__ == '__main__':
    unittest.main(verbosity=2)
