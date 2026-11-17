# SPDX-License-Identifier: Apache-2.0
"""Actual research legal bytes must survive generator/runtime/installer staging.

Small fictional image fixtures prove legal closure only; no native installation,
model training, source-rights admission or artifact qualification is asserted.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LEGAL = load("scripts/license-notices.py", "research_legal")
FIXTURES = load("scripts/tests/test_package_native.py", "research_pair_fixtures")
LOCAL = load("scripts/build-local-service-installer.py", "research_local_installer")
PACKAGE = FIXTURES.package
EXPECTED = {
    'research/ARBITER-RESEARCH-NOTICES.md': '6653556938d87782d1ffa6f0051aaff63f7e11c2f12a93092c2ddd643df5ab9b',
    'research/CoCLR-MIT-LICENSE.txt': '5f1ce9c03c594b38df127d923515ae50c5999076e376f01cb94d1b369c54d243',
    'research/C-UDA-1.0.md': '69aeb2c407f2ffce7f31c3aba0f1eeffac248009ec2070fcd989586ac8229c82',
    'research/CodeSearchNet-MIT-LICENSE.txt': '5ba1fd8a344040f2698ed3234aeb8f4b3e85211aa54a37048021f3eb0043be22',
    'research/ContextBench-Apache-2.0-LICENSE.txt': 'c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4',
    'research/provenance.json': '71af118045869138893e3d4fc94212152f2bc57db3a08c8e6c74ec6d9743b7b7',
    'models/minilm-v2-TRAINING-NOTICES.md': '2daba18214e777ecb68e1154c26eb9d50c9f8ef25895e4dcc8af3d9d1275e4be',
    'models/minilm-v2-LICENSE.txt': '9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd',
    'models/minilm-v2-provenance.json': '6f6a29bff30ef64212b0c6fc56095d94df3184856e488023820d995fbb68da81',
    'models/CC-BY-4.0.txt': '9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411',
}


class ResearchNoticeContracts(unittest.TestCase):
    def fixture_source(self, root):
        for name in EXPECTED:
            source = ROOT / "third_party" / name
            target = root / "third_party" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)

    def runtime(self, root, *, version="1.8.207", research=True):
        root.mkdir()
        target = "x86_64-pc-windows-msvc"
        source = {"sha256":"1" * 64,"files":4}
        pair, build = FIXTURES.split_runtime(root, target, source, version=version)
        LEGAL.stage_model_notices(ROOT, root / "notices")
        if research:
            LEGAL.stage_research_notices(ROOT, root / "notices")
        FIXTURES.seal_split(root, pair, build)
        return source, pair, build

    def test_complete_independent_roster_preserves_every_legal_byte_and_original_credit(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "notices"
            LEGAL.stage_research_notices(ROOT, output)
            actual = {p.relative_to(output).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in output.rglob("*") if p.is_file()}
            self.assertEqual(actual, EXPECTED)
            for name in EXPECTED:
                self.assertEqual((output / name).read_bytes(), (ROOT / "third_party" / name).read_bytes())
            original = (output / "models/minilm-v2-LICENSE.txt").read_bytes()
            self.assertIn(b"Copyright 2019 Kenneth Reitz", original)
            provenance = json.loads((output / "models/minilm-v2-provenance.json").read_bytes())
            self.assertEqual(len(provenance["qasper"]["training_papers"]), 36)
            self.assertEqual(hashlib.sha256((output / "research/CoCLR-MIT-LICENSE.txt").read_bytes()).hexdigest(),
                             "5f1ce9c03c594b38df127d923515ae50c5999076e376f01cb94d1b369c54d243")
            self.assertEqual(hashlib.sha256((output / "research/C-UDA-1.0.md").read_bytes()).hexdigest(),
                             "69aeb2c407f2ffce7f31c3aba0f1eeffac248009ec2070fcd989586ac8229c82")

    def test_mutated_legal_sources_refuse_before_any_research_output(self):
        for name in EXPECTED:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.fixture_source(root / "source")
                source = root / "source/third_party" / name
                with source.open("ab") as changed:
                    changed.write(b"altered")
                with self.assertRaisesRegex(ValueError, "Research notice digest mismatch"):
                    LEGAL.stage_research_notices(root / "source", root / "output")
                self.assertFalse((root / "output").exists())

    def test_missing_legal_sources_refuse_before_any_research_output(self):
        for name in EXPECTED:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.fixture_source(root / "source")
                (root / "source/third_party" / name).unlink()
                with self.assertRaisesRegex(ValueError, "Missing regular research notice"):
                    LEGAL.stage_research_notices(root / "source", root / "output")
                self.assertFalse((root / "output").exists())

    def test_unreviewed_source_files_are_not_added_to_research_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture_source(root / "source")
            (root / "source/third_party/research/raw-training-data.json").write_text("must not ship")
            LEGAL.stage_research_notices(root / "source", root / "output")
            self.assertEqual({p.relative_to(root / "output").as_posix()
                              for p in (root / "output").rglob("*") if p.is_file()}, set(EXPECTED))

    def test_actual_generator_main_includes_research_separately_from_cargo_rows(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "notices"
            previous = Path.cwd()
            try:
                os.chdir(ROOT)
                with patch.object(sys, "argv", ["license-notices.py", "--output", str(output), "--release"]), \
                     patch.object(LEGAL.subprocess, "check_output", return_value=json.dumps({"workspace_members":[],"packages":[],"resolve":{"nodes":[]}})) as cargo:
                    LEGAL.main()
                cargo.assert_called_once()
            finally:
                os.chdir(previous)
            self.assertEqual(json.loads((output / "inventory.json").read_text()), [])
            self.assertEqual({name:hashlib.sha256((output / name).read_bytes()).hexdigest() for name in EXPECTED}, EXPECTED)

    def test_current_runtime_cannot_reseal_missing_or_mutated_research_notices(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "runtime"
            source, pair, build = self.runtime(root)
            PACKAGE.validate_runtime(root, "1.8.207", "x86_64-pc-windows-msvc", source)
            for name in EXPECTED:
                path = root / "notices" / name
                original = path.read_bytes()
                for replacement in (None, original + b"altered"):
                    with self.subTest(name=name,missing=replacement is None):
                        if replacement is None:
                            path.unlink()
                        else:
                            path.write_bytes(replacement)
                        FIXTURES.seal_split(root, pair, build)
                        with self.assertRaisesRegex(ValueError,"Pinned research notice"):
                            PACKAGE.validate_runtime(root,"1.8.207","x86_64-pc-windows-msvc",source)
                        path.write_bytes(original)
                FIXTURES.seal_split(root, pair, build)

    def test_extra_research_payload_is_refused_even_with_a_resealed_outer_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "runtime"
            source, pair, build = self.runtime(root)
            (root / "notices/research/unreviewed.json").write_text("must refuse")
            FIXTURES.seal_split(root,pair,build)
            with self.assertRaisesRegex(ValueError,"Unexpected runtime payload"):
                PACKAGE.validate_runtime(root,"1.8.207","x86_64-pc-windows-msvc",source)

    def test_historical_runtime_retains_its_original_legal_roster(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "runtime"
            source, _, _ = self.runtime(root,version="1.8.206",research=False)
            PACKAGE.validate_runtime(root,"1.8.206","x86_64-pc-windows-msvc",source)
            self.assertFalse((root / "notices/research").exists())

    def test_real_installer_staging_preserves_legal_closure_and_payload_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            runtime = folder / "runtime"
            source, _, _ = self.runtime(runtime)
            files, images = PACKAGE.validate_runtime(runtime,"1.8.207","x86_64-pc-windows-msvc",source)
            archive = folder / "fixture.zip"
            PACKAGE.write_archive(runtime,archive,files,images)
            PACKAGE.verify_archive(archive,files,images)
            layout = folder / "layout"
            with patch.object(LOCAL,"admin_digests",return_value={}):
                inventory = LOCAL.stage(layout,runtime / "airk.exe",runtime / "airkctl.exe","1.8.207",
                                        LOCAL.windows_signing(),verified_notices=runtime / "notices")
            for name, digest in EXPECTED.items():
                installed = "notices/dependencies/" + name
                self.assertEqual((layout / installed).read_bytes(),(runtime / "notices" / name).read_bytes())
                self.assertEqual(inventory["files"][installed]["sha256"],digest)
            inno = (ROOT / "installers/windows/airk-service.iss").read_text(encoding="utf-8")
            self.assertIn('Source: "{#AirkPayload}\\notices\\*";',inno)
            self.assertIn('Flags: ignoreversion recursesubdirs createallsubdirs;',inno)

    def test_readmes_and_shipped_notice_credit_real_authors_without_new_training_claim(self):
        base = (ROOT / "README.md").read_text(encoding="utf-8")
        arbiter = (ROOT / "docs/arbiter/README.md").read_text(encoding="utf-8")
        notice = (ROOT / "third_party/research/ARBITER-RESEARCH-NOTICES.md").read_text(encoding="utf-8")
        for name in ["CoSQA","CodeSearchNet","ContextBench","QASPER","HotpotQA","Requests"]:
            self.assertIn(name,base)
            self.assertIn(name,arbiter)
            self.assertIn(name,notice)
        for author in ["Junjie Huang","Ho-Hsiang Wu","Pradeep Dasigi","Zhilin Yang","Kenneth Reitz","Earl T. Barr"]:
            self.assertIn(author,arbiter)
            self.assertIn(author,notice)
        for text in [arbiter,notice]:
            self.assertIn("evaluation-only",text.lower().replace("evaluation only","evaluation-only"))
            self.assertIn("C-UDA",text)
            self.assertIn("CC BY-SA 4.0",text)
            self.assertIn("no raw",text.lower())
        self.assertIn("no training admission is asserted",arbiter)
        self.assertIn("no new trained weights are asserted",arbiter)


if __name__ == "__main__":
    unittest.main()
