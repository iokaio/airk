# SPDX-License-Identifier: Apache-2.0
"""Archive dependency selection; synthetic archives do not qualify native link."""
import importlib.util
from pathlib import Path
import unittest
from types import SimpleNamespace
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MODULE = load("static-onnx-aggregate")
CORE = load("provision-static-onnx-runtime")


def graph():
    lines = [f"build onnxruntime_{name}.lib: CXX_STATIC_LIBRARY_LINKER__runtime_Release source.obj || model_package/model_package.lib"
             for name in CORE.STATIC_COMPONENTS]
    lines += ["build model_package/model_package.lib: CXX_STATIC_LIBRARY_LINKER__model_Release model.obj || _deps/protobuf-build/libprotobuf-lite.lib _deps/abseil_cpp-build/absl/hash/absl_hash.lib",
              "build _deps/protobuf-build/libprotobuf-lite.lib: CXX_STATIC_LIBRARY_LINKER__protobuf_Release proto.obj",
              "build _deps/abseil_cpp-build/absl/hash/absl_hash.lib: CXX_STATIC_LIBRARY_LINKER__hash_Release hash.obj || _deps/abseil_cpp-build/absl/hash/absl_city.lib",
              "build _deps/abseil_cpp-build/absl/hash/absl_city.lib: CXX_STATIC_LIBRARY_LINKER__city_Release city.obj",
              "build _deps/re2-build/re2.lib: CXX_STATIC_LIBRARY_LINKER__re2_Release re2.obj",
              "build _deps/pytorch_cpuinfo-build/cpuinfo.lib: C_STATIC_LIBRARY_LINKER__cpuinfo_Release cpuinfo.obj",
              "build _deps/protobuf-build/libprotobuf.lib: CXX_STATIC_LIBRARY_LINKER__full_Release duplicate.obj",
              "build _deps/protobuf-build/libprotoc.lib: CXX_STATIC_LIBRARY_LINKER__compiler_Release compiler.obj",
              "build onnxruntime_providers_shared.dll onnxruntime_providers_shared.lib: CXX_SHARED_LIBRARY_LINKER__shared_Release shared.obj"]
    return "\n".join(lines)


class StaticAggregateTests(unittest.TestCase):
    def test_readonly_verifier_normalizes_its_owned_relative_destination_first(self):
        original = Path('relative-owned')
        resolved = Path('normalized-owned').absolute()
        core = SimpleNamespace(owned_destination=Mock(return_value=resolved),
                               read_json=Mock(side_effect=ValueError('stop after normalized read')))
        with self.assertRaisesRegex(ValueError,'normalized read'):
            MODULE.verify(original,'mac',{},core)
        core.owned_destination.assert_called_once_with(original,'mac')
        core.read_json.assert_called_once_with(resolved/'static-aggregate-receipt.json')

    def test_macos_uses_native_libtool_and_exact_filelist(self):
        base = Path("owned")
        fake = SimpleNamespace(TARGETS={"mac":("Darwin","aarch64")},child=lambda root,name:root/name)
        selected = {"inputs":[{"path":"b/Release/libonnxruntime_session.a"},{"path":"b/Release/model_package/libmodel_package.a"}]}
        original = {"receipt":{"toolchain":{"compiler":{"version_output":
            "Apple clang version 21.0.0\nInstalledDir: /Library/Developer/CommandLineTools/usr/bin\n"}}}}
        request = MODULE.invocation(base,"mac",original,selected,fake)
        self.assertEqual(str(request["tool"]),str(Path("/Library/Developer/CommandLineTools/usr/bin/libtool")))
        self.assertEqual(request["argv"][1:3],["-static","-o"])
        self.assertEqual(request["text"],"\n".join(str(base/row["path"]) for row in selected["inputs"])+"\n")
        self.assertEqual(request["argv"][-2:],["-filelist",str(base/"aggregate/inputs.filelist")])
        for observed in ['Apple clang version 21.0.0', 'InstalledDir: relative/bin',
                         'InstalledDir: /developer/../bin',
                         'InstalledDir: /first/bin\nInstalledDir: /second/bin']:
            original['receipt']['toolchain']['compiler']['version_output'] = observed
            with self.assertRaises(ValueError):MODULE.invocation(base,'mac',original,selected,fake)

    def test_linux_uses_real_ar_mri_with_fixed_literal_shell_arguments(self):
        fake = SimpleNamespace(TARGETS={"linux":("Linux","x86_64")},child=lambda root,name:root/name)
        request = MODULE.invocation(Path("owned"),"linux",{}, {"inputs":[{"path":"b/Release/libonnxruntime_session.a"}]},fake)
        self.assertEqual(request["argv"][:4],["/bin/sh","-c",'exec "$1" -M < "$2"',"airk-static-aggregate"])
        self.assertEqual(request["argv"][4],str(Path("/usr/bin/ar")))
        self.assertTrue(request["text"].startswith("CREATE "))
        self.assertIn("\nADDLIB ",request["text"])
        self.assertTrue(request["text"].endswith("\nSAVE\nEND\n"))
        with self.assertRaisesRegex(ValueError,"whitespace"):
            MODULE.invocation(Path("owned space"),"linux",{}, {"inputs":[]},fake)

    def test_posix_static_graph_keeps_same_runtime_dependency_semantics(self):
        text = graph().replace(".lib",".a")
        for prefix in ["onnxruntime_","model_package.a","re2.a","cpuinfo.a","absl_hash.a","absl_city.a"]:
            text = text.replace(prefix,"lib"+prefix)
        # Protobuf's lib prefix already exists on Windows.
        paths=MODULE.runtime_closure(text,CORE.STATIC_COMPONENTS,False,CORE.relative_name)
        self.assertEqual(len(paths),16)
        self.assertIn("model_package/libmodel_package.a",paths)
        self.assertNotIn("_deps/protobuf-build/libprotobuf.a",paths)
    def closure(self, text):
        return MODULE.runtime_closure(text, CORE.STATIC_COMPONENTS, True, CORE.relative_name)

    def test_real_dependency_closure_keeps_nested_libraries_without_old_abseil_fictions(self):
        selected = self.closure(graph())
        self.assertEqual(len(selected), 16)
        self.assertIn("model_package/model_package.lib", selected)
        self.assertIn("_deps/protobuf-build/libprotobuf-lite.lib", selected)
        self.assertIn("_deps/pytorch_cpuinfo-build/cpuinfo.lib", selected)
        self.assertNotIn("_deps/protobuf-build/libprotobuf.lib", selected)
        self.assertNotIn("_deps/protobuf-build/libprotoc.lib", selected)
        self.assertNotIn("onnxruntime_providers_shared.lib", selected)
        self.assertFalse(any("low_level_hash" in name for name in selected))

    def test_missing_genuine_target_refuses_instead_of_fabricating_an_archive(self):
        changed = graph().replace("build model_package/model_package.lib:", "build foreign.lib:")
        with self.assertRaisesRegex(ValueError, "Missing genuine"):
            self.closure(changed)

    def test_cycle_and_duplicate_static_targets_refuse(self):
        with self.assertRaisesRegex(ValueError, "Cyclic"):
            self.closure(graph().replace("hash.obj ||", "hash.obj || onnxruntime_common.lib"))
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            self.closure(graph() + "\nbuild onnxruntime_common.lib: CXX_STATIC_LIBRARY_LINKER__duplicate_Release duplicate.obj")

    def test_dependency_paths_cannot_escape_or_use_ninja_expansion(self):
        for bad in ["../foreign.lib", "/absolute.lib", "C:/absolute.lib", "$injected.lib"]:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.closure(graph().replace("_deps/protobuf-build/libprotobuf-lite.lib", bad))

    def test_unsupported_target_spelling_and_graph_size_refuse(self):
        with self.assertRaises(ValueError):
            self.closure(graph().replace("build onnxruntime_common.lib:", "build two names.lib:"))
        # Bound checked before parsing, without any filesystem writes.
        with self.assertRaisesRegex(ValueError, "exceeds bound"):
            self.closure("x" * (64 * 1024 * 1024 + 1))


if __name__ == "__main__":
    unittest.main()
