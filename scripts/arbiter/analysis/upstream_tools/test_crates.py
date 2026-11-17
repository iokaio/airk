# SPDX-License-Identifier: Apache-2.0
"""Pure registry binding and shared archive refusal controls; no disk fixtures."""
import copy
import io
import tarfile
import unittest
import project_crates as owner

class CrateTests(unittest.TestCase):
    def setUp(self):
        self.spec={'name':'example','version':'1.2.3','archive':{'sha256':'a'*64},'root':'artifacts/vendor/example','archive_prefix':'example-1.2.3/'}
        self.raw=('[[package]]\nname="example"\nversion="1.2.3"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="'+'a'*64+'"\n').encode()
    def test_exact_registry_binding(self):
        owner.package_binding(self.raw,self.spec)
    def test_wrong_identity_checksum_registry(self):
        for before,after in [(b'example',b'other'),(b'1.2.3',b'1.2.4'),(b'a'*64,b'b'*64),(b'registry+',b'git+')]:
            with self.subTest(before=before),self.assertRaises(ValueError):owner.package_binding(self.raw.replace(before,after),self.spec)
    def test_duplicate_package_refused(self):
        with self.assertRaises(ValueError):owner.package_binding(self.raw+self.raw,self.spec)
    def test_root_boundary_and_member(self):
        self.assertEqual(owner.selected_member('artifacts/vendor/example/src/lib.rs',self.spec),'example-1.2.3/src/lib.rs')
        self.assertIsNone(owner.selected_member('artifacts/vendor/example-other/src/lib.rs',self.spec))
        with self.assertRaises(ValueError):owner.selected_member('artifacts/vendor/example/../bad',self.spec)
    def archive(self, duplicate=False, symlink=False):
        buffer=io.BytesIO()
        with tarfile.open(fileobj=buffer,mode='w') as archive:
            for _ in range(2 if duplicate else 1):
                info=tarfile.TarInfo('example-1.2.3/src/lib.rs');info.size=3
                if symlink:info.type=tarfile.SYMTYPE;info.linkname='target';info.size=0
                archive.addfile(info,io.BytesIO(b'abc') if not symlink else None)
        buffer.seek(0)
        return tarfile.open(fileobj=buffer,mode='r:')
    def test_shared_archive_exact_member(self):
        with self.archive() as archive:
            row=owner.shared.archive_roster(archive,'tar')['example-1.2.3/src/lib.rs']
            self.assertEqual(owner.shared.member_digest(archive,'tar',row),{'sha256':'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad','bytes':3})
    def test_shared_alias_duplicate_refusal(self):
        for option in ({'duplicate':True},{'symlink':True}):
            with self.archive(**option) as archive,self.assertRaises(ValueError):owner.shared.archive_roster(archive,'tar')
    def test_unknown_git_identity_refused_before_git(self):
        spec=copy.deepcopy(self.spec);spec['lock']={'commit':'HEAD','blob_oid':'b'*40,'path':'Cargo.lock'}
        with self.assertRaises(ValueError):owner.lock_bytes(None,spec)

if __name__=='__main__':unittest.main()
