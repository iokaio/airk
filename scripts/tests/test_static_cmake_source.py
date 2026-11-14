# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
from pathlib import Path, PurePosixPath
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location('cmake_source', Path(__file__).parents[1] / 'provision-static-cmake-source.py')
SOURCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCE)


class CmakeSourcePolicy(unittest.TestCase):
    def test_effective_bootstrap_library_cache_requires_every_exact_off_value(self):
        spec = importlib.util.spec_from_file_location('static_cache_owner', Path(__file__).parents[1] / 'provision-static-onnx-runtime.py')
        owner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(owner)
        # Independent actual CMake 3.31.6 cache rows; umbrella input is absent.
        names = ['BZIP2','CPPDAP','CURL','EXPAT','FORM','JSONCPP','KWIML',
                 'LIBARCHIVE','LIBLZMA','LIBRHASH','LIBUV','NGHTTP2','ZLIB','ZSTD']
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary).resolve() / 'CMakeCache.txt'
            original = ''.join('CMAKE_USE_SYSTEM_' + name + ':BOOL=OFF\n' for name in names)
            cache.write_text(original)
            self.assertEqual(len(SOURCE.verify_source_cache(cache, [], owner)),14)
            for name in names:
                row = 'CMAKE_USE_SYSTEM_' + name + ':BOOL=OFF\n'
                for replacement in [row.replace('OFF','ON'), '']:
                    cache.write_text(original.replace(row,replacement))
                    with self.assertRaisesRegex(ValueError,'CMAKE_USE_SYSTEM_' + name):
                        SOURCE.verify_source_cache(cache, [], owner)

    def test_completion_preserves_failed_attempt_and_requires_full_existing_receipt_verification(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            original = {'result':'failed','error':'Actual static ORT CMake cache differs: CMAKE_USE_SYSTEM_LIBRARIES'}
            core = SimpleNamespace(plain=Path,read_json=Mock(return_value=original),digest=Mock(return_value='a'*64),write_json=Mock())
            with patch.object(SOURCE,'verify_receipt',side_effect=ValueError('actual output changed')):
                with self.assertRaisesRegex(ValueError,'actual output changed'):
                    SOURCE.complete_retained_build(root,'aarch64-apple-darwin',core,object())
            core.write_json.assert_not_called()
            with patch.object(SOURCE,'verify_receipt') as verify, patch.object(SOURCE,'verify',return_value={'verified':True}):
                self.assertEqual(SOURCE.complete_retained_build(root,'aarch64-apple-darwin',core,object()),{'verified':True})
            verify.assert_called_once()
            self.assertEqual(core.write_json.call_args.args[0],root/'cmake-source-receipt.json')
            self.assertEqual(core.write_json.call_args.args[1]['retained_attempt_sha256'],'a'*64)
            self.assertEqual(original['result'],'failed')

    def test_actual_environment_helper_rejects_ambient_make_shell_loader_and_compiler_controls(self):
        supplied = {key:'injected' for key in ('MAKEFLAGS','MFLAGS','MAKEOVERRIDES','GNUMAKEFLAGS','MAKE',
            'ENV','BASH_ENV','SHELLOPTS','BASHOPTS','CMAKE_GENERATOR','DYLD_INSERT_LIBRARIES','CFLAGS',
            'CXXFLAGS','CPPFLAGS','LDFLAGS','CPATH','LIBRARY_PATH','SDKROOT','CC','CXX')}
        supplied.update(HOME='/owner',TMPDIR='/private/tmp',PATH='/unreviewed')
        environment = SOURCE.source_environment('/usr/bin/clang','/usr/bin/clang++',supplied)
        self.assertEqual(environment,{'HOME':'/owner','TMPDIR':'/private/tmp','PATH':'/usr/bin:/bin:/usr/sbin:/sbin',
            'CC':'/usr/bin/clang','CXX':'/usr/bin/clang++','MAKE':'/usr/bin/make',
            'MACOSX_DEPLOYMENT_TARGET':'13.3','PYTHONNOUSERSITE':'1'})
        self.assertEqual(supplied['MAKEFLAGS'],'injected')

    def test_official_source_and_full_license_pins_are_exact(self):
        SOURCE.validate_lock()
        self.assertEqual(SOURCE.LOCK['sha256'], '653427f0f5014750aafff22727fb2aa60c6c732ca91808cfb78ce22ddd9e55f0')
        for key, value in [('sha256', '0'*64), ('version', '3.31.7'), ('url', 'https://other.invalid/source.tgz')]:
            changed = {**SOURCE.LOCK, key:value}
            with patch.object(SOURCE, 'LOCK', changed), self.assertRaises(ValueError): SOURCE.validate_lock()

    def test_owned_source_recipe_uses_bundled_dependencies_and_never_installs_or_resigns(self):
        root = PurePosixPath('/owned')
        bootstrap, build = SOURCE.commands(root, PurePosixPath('/usr/bin/cc'), PurePosixPath('/usr/bin/c++'))
        self.assertEqual(bootstrap[:5], ['/bin/sh','/owned/cmake-source-source/bootstrap','--parallel=2','--no-system-libs','--no-qt-gui'])
        self.assertIn('-DFETCHCONTENT_FULLY_DISCONNECTED=ON',bootstrap)
        self.assertIn('-DCMAKE_OSX_ARCHITECTURES=arm64',bootstrap)
        self.assertIn('-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3',bootstrap)
        self.assertEqual(build,['/usr/bin/make','-j2','cmake'])
        self.assertFalse(any('codesign' in value or value == 'install' for value in bootstrap+build))

    def test_existing_attempt_refuses_before_fetch_or_compilation_and_completed_receipt_reuses(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            core = SimpleNamespace(host_target=lambda:'aarch64-apple-darwin',plain=lambda value:Path(value),fetch=Mock())
            (root/'cmake-source-attempt.json').write_text('original failed attempt')
            with self.assertRaisesRegex(ValueError,'no automatic retry'):
                SOURCE.build(root,'aarch64-apple-darwin',True,core,object())
            core.fetch.assert_not_called()
            self.assertEqual((root/'cmake-source-attempt.json').read_text(),'original failed attempt')
            (root/'cmake-source-receipt.json').write_text('original completed receipt')
            with patch.object(SOURCE,'verify',return_value={'verified':True}) as verification:
                self.assertEqual(SOURCE.build(root,'aarch64-apple-darwin',False,core,object()),{'verified':True})
                verification.assert_called_once()
            core.fetch.assert_not_called()
            self.assertEqual((root/'cmake-source-receipt.json').read_text(),'original completed receipt')


if __name__ == '__main__':
    unittest.main()
