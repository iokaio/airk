# SPDX-License-Identifier: Apache-2.0
"""Fictional bytes and mocked native trust; no signing or installer run occurs."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("standalone_windows", Path(__file__).parents[1] / "package-standalone-windows.py")
windows = importlib.util.module_from_spec(spec)
spec.loader.exec_module(windows)


class WindowsInstallerContracts(unittest.TestCase):
    def fixture(self, root):
        runtime = root / "runtime"
        runtime.mkdir()
        (runtime / "airk.exe").write_bytes(b"MZ fictional signed native executable")
        (runtime / "airk-install.json").write_text("{}")
        iscc = root / "ISCC.exe"
        iscc.write_bytes(b"MZ fictional compiler")
        (root / "license.txt").write_text("Fictional compiler license")
        native = {"version": "1.8.14", "target": windows.TARGET, "commit": "a" * 40,
            "cargo_lock_sha256": "b" * 64, "runtime_install_sha256": windows.signer.digest(runtime / "airk-install.json"),
            "runtime_source": "c" * 64, "payload": {name: windows.signer.digest(runtime / name)
                for name in ("airk.exe", "airk-install.json")}, "native_signing_sha256": "d" * 64}
        compiler = {"version": windows.INNO_VERSION, "sha256": windows.signer.digest(iscc)}
        return runtime, native, iscc, compiler

    def prepared(self, root):
        runtime, native, iscc, compiler = self.fixture(root)
        output = root / "installer"
        def compile(context, work, *, first):
            self.assertTrue(first)
            (work / "uninstallers/unins-fixture.e32").write_bytes(b"MZ unsigned fictional uninstaller")
            return 2  # ISCC's manual uninstaller-signing boundary is not success.
        with patch.object(windows.sys, "platform", "win32"), \
                patch.object(windows, "native_inputs", return_value=(runtime, native)), \
                patch.object(windows, "compiler_identity", return_value=compiler), \
                patch.object(windows, "compile_installer", side_effect=compile):
            windows.prepare(runtime, root / "signed", output, iscc)
        return output, runtime, native, compiler

    def signature(self, binary):
        return {"platform": "Windows", "identity": "e" * 40, "team_id": None,
            "azure": windows.signer.AZURE_SIGNING, "binary_sha256": windows.signer.digest(binary),
            "verification": {"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "e" * 40}}

    def context_patches(self, runtime, native, compiler):
        return (patch.object(windows, "native_inputs", return_value=(runtime, native)),
                patch.object(windows, "compiler_identity", return_value=compiler))

    def built(self, root):
        output, runtime, native, compiler = self.prepared(root)
        with (output / "uninstaller.exe").open("ab") as stream:
            stream.write(b" fictional Authenticode signature")
        def compile(context, work, *, first):
            self.assertFalse(first)
            prepared = json.loads((output / "prepared.json").read_text())
            self.assertEqual((work / "uninstallers" / prepared["cache_name"]).read_bytes(),
                             (output / "uninstaller.exe").read_bytes())
            (work / "compiled" / context["artifact"]).write_bytes(b"MZ unsigned fictional outer installer")
            return 0
        p1, p2 = self.context_patches(runtime, native, compiler)
        with p1, p2, patch.object(windows, "verified_signature", side_effect=self.signature), \
                patch.object(windows, "compile_installer", side_effect=compile):
            windows.build(output)
        return output, runtime, native, compiler

    def test_inno_uses_native_per_user_lifecycle_and_failure_preserving_uninstall(self):
        script = windows.SCRIPT.read_text()
        for required in ("PrivilegesRequired=lowest", "SignedUninstaller=yes", "GetSHA256OfFile",
                         "setup --runtime-source", "--approved-sha256", "uninstall --yes",
                         "function InitializeUninstall", "Result := False", "if ExitCode <> 0",
                         "AirkInnoLicense"):
            self.assertIn(required, script)
        self.assertNotIn("[UninstallDelete]", script)
        self.assertNotIn(".vsix", script.lower())
        self.assertNotIn("code --install", script.lower())
        initialize = script[script.index("function InitializeUninstall"):script.index("procedure CurUninstallStepChanged")]
        self.assertIn("VerifiedBinary := NativeBinary;", initialize)
        self.assertNotIn("Exec(", initialize)
        self.assertNotIn("RemoveOwnedUserPath;", initialize)

    def test_user_path_is_published_only_after_native_setup_and_removed_only_after_native_uninstall(self):
        script = windows.SCRIPT.read_text()
        self.assertIn("ChangesEnvironment=yes", script)
        setup = script[script.index("procedure CurStepChanged"):script.index("function InitializeUninstall")]
        self.assertLess(setup.index("if ExitCode <> 0"), setup.index("AddOwnedUserPath;"))
        uninstall = script[script.index("procedure CurUninstallStepChanged"):]
        self.assertLess(uninstall.index("if ExitCode <> 0"), uninstall.index("RemoveOwnedUserPath;"))
        self.assertIn("Abort;", uninstall[uninstall.index("except"):])

    def test_user_path_preserves_registry_kind_and_requires_durable_exact_entry_ownership(self):
        script = windows.SCRIPT.read_text()
        self.assertIn("RegQueryStringValue(HKCU, 'Environment', 'Path'", script)
        self.assertIn("RegWriteStringValue(HKCU, 'Environment', 'Path'", script)
        self.assertNotIn("HKLM", script)
        self.assertNotIn("GetEnv('PATH')", script)
        self.assertIn("'airk-user-path/1'", script)
        self.assertIn("SaveStringsToUTF8File", script)
        self.assertIn("LoadStringsFromFile", script)
        self.assertIn("Lines[1] := 'pending'", script)
        self.assertIn("Lines[1] := 'owned'", script)
        self.assertIn("Lines[1] <> 'owned'", script)
        self.assertIn("ExactCount <> 1", script)
        self.assertIn("Parts[I] = Entry", script)

    def test_user_path_receipt_cannot_claim_a_manual_entry_or_erase_later_empty_entries(self):
        script = windows.SCRIPT.read_text()
        add = script[script.index("procedure AddOwnedUserPath;"):script.index("procedure RemoveOwnedUserPath;")]
        self.assertLess(add.index("else if HasPathEntry(Before, Entry)"), add.index("Lines[1] := 'pending'"))
        manual = add[add.index("else if HasPathEntry(Before, Entry)"):add.index("SetArrayLength(Lines, 6)")]
        self.assertIn("Exit;", manual)
        self.assertLess(add.index("Lines[1] := 'pending'"), add.index("WriteUserPath("))
        self.assertLess(add.index("WriteUserPath("), add.index("Lines[1] := 'owned'"))
        remove = script[script.index("procedure RemoveOwnedUserPath;"):script.index("function NativeBinary:")]
        self.assertIn("(Lines[3] = '0') and (Kept = 0)", remove)
        self.assertLess(remove.index("Lines[1] <> 'owned'"), remove.index("WriteUserPath("))
        self.assertLess(remove.index("ExactCount <> 1"), remove.index("WriteUserPath("))
        self.assertIn("manually edited entries were preserved", remove)

    def test_uninstall_confirmation_precedes_every_native_or_path_side_effect(self):
        script = windows.SCRIPT.read_text()
        hook = "procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);"
        self.assertIn(hook, script)
        initialize = script[script.index("function InitializeUninstall"):script.index(hook)]
        self.assertNotIn("Exec(", initialize)
        self.assertNotIn("RemoveOwnedUserPath;", initialize)
        confirmed = script[script.index(hook):]
        self.assertIn("if CurUninstallStep = usUninstall then begin", confirmed)
        self.assertLess(confirmed.index("if ExitCode <> 0"), confirmed.index("RemoveOwnedUserPath;"))
        failure = confirmed[confirmed.index("except"):]
        self.assertIn("Abort;", failure)
        self.assertLess(failure.index("MsgBox("), failure.index("Abort;"))

    def test_path_dedup_does_not_expand_untyped_registry_values(self):
        script = windows.SCRIPT.read_text()
        compare = script[script.index("function ComparablePathEntry"):script.index("function HasPathEntry")]
        self.assertNotIn("GetEnv(", compare)
        self.assertNotIn("ExpandEnvironmentStrings", compare)
        self.assertIn("Result := Trim(Value);", compare)

    def test_prepare_exposes_unsigned_uninstaller_and_never_a_publish_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            output, _, _, _ = self.prepared(Path(temporary))
            prepared = json.loads((output / "prepared.json").read_text())
            self.assertEqual(prepared["phase"], "unsigned_uninstaller")
            self.assertEqual(prepared["unsigned_sha256"], windows.signer.digest(output / "uninstaller.exe"))
            self.assertFalse((output / "publish").exists())

    def test_wrong_first_compiler_result_or_missing_uninstaller_is_not_prepared(self):
        for result in (0, 2):
            with self.subTest(result=result), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                runtime, native, iscc, compiler = self.fixture(root)
                output = root / "installer"
                with patch.object(windows.sys, "platform", "win32"), \
                        patch.object(windows, "native_inputs", return_value=(runtime, native)), \
                        patch.object(windows, "compiler_identity", return_value=compiler), \
                        patch.object(windows, "compile_installer", return_value=result):
                    with self.assertRaisesRegex(ValueError, "single unsigned-uninstaller"):
                        windows.prepare(runtime, root / "signed", output, iscc)
                self.assertFalse((output / "prepared.json").exists())

    def test_build_refuses_unsigned_or_untrusted_uninstaller_without_compiling(self):
        for trusted in (False, True):
            with self.subTest(trusted=trusted), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output, runtime, native, compiler = self.prepared(root)
                p1, p2 = self.context_patches(runtime, native, compiler)
                verifier = dict(side_effect=self.signature) if trusted else dict(side_effect=RuntimeError("native trust failed"))
                with p1, p2, patch.object(windows, "verified_signature", **verifier), \
                        patch.object(windows, "compile_installer") as compile:
                    with self.assertRaises((ValueError, RuntimeError)):
                        windows.build(output)
                    compile.assert_not_called()
                self.assertFalse((output / "built.json").exists())

    def test_signed_uninstaller_and_outer_exe_are_both_required_for_public_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            output, runtime, native, compiler = self.built(Path(temporary))
            context = json.loads((output / "inputs.json").read_text())
            p1, p2 = self.context_patches(runtime, native, compiler)
            with p1, p2, patch.object(windows, "verified_signature", side_effect=self.signature):
                with self.assertRaisesRegex(ValueError, "outer signature"):
                    windows.record(output)
            self.assertFalse((output / "publish").exists())
            with (output / context["artifact"]).open("ab") as stream:
                stream.write(b" fictional outer signature")
            p1, p2 = self.context_patches(runtime, native, compiler)
            with p1, p2, patch.object(windows, "verified_signature", side_effect=self.signature):
                windows.record(output)
            receipt = json.loads((output / "publish/installer.json").read_text())
            self.assertEqual(receipt["installer_kind"], "windows_inno")
            self.assertEqual(receipt["runtime_install_sha256"], native["runtime_install_sha256"])
            self.assertTrue(receipt["signing"]["verification"]["timestamp_present"])
            self.assertTrue(receipt["uninstaller"]["verification"]["timestamp_present"])
            self.assertIn("notices/inno-setup/LICENSE.txt", receipt["payload"])
            self.assertEqual(set(path.name for path in (output / "publish").iterdir()),
                             {context["artifact"], "installer.json", "SHA256SUMS"})

    def test_changed_runtime_or_compiler_refuses_before_second_compile(self):
        for changed in ("runtime", "compiler"):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                output, runtime, native, compiler = self.prepared(Path(temporary))
                if changed == "runtime":
                    native = dict(native, runtime_install_sha256="f" * 64)
                else:
                    compiler = dict(compiler, sha256="f" * 64)
                p1, p2 = self.context_patches(runtime, native, compiler)
                with p1, p2, patch.object(windows, "verified_signature") as verifier, \
                        patch.object(windows, "compile_installer") as compile:
                    with self.assertRaisesRegex(ValueError, "changed"):
                        windows.build(output)
                    verifier.assert_not_called()
                    compile.assert_not_called()

    def test_workflow_signs_exact_installer_files_before_verification_and_azure_cleanup(self):
        workflow = (windows.ROOT / ".github/workflows/github-release.yml").read_text()
        prepare = workflow.index("Prepare the standalone Windows uninstaller")
        uninstaller = workflow.index("Sign the standalone Windows uninstaller", prepare)
        build = workflow.index("Verify and embed the signed Windows uninstaller", uninstaller)
        installer = workflow.index("Sign the standalone Windows installer", build)
        record = workflow.index("Verify the standalone Windows installer and uninstaller", installer)
        logout = workflow.index("Clear Azure login", record)
        self.assertTrue(prepare < uninstaller < build < installer < record < logout)
        self.assertIn("files: artifacts/standalone/${{ matrix.target }}/uninstaller.exe", workflow)
        self.assertIn("files: artifacts/standalone/${{ matrix.target }}/airk-${{ needs.preflight.outputs.version }}-${{ matrix.target }}-installer.exe", workflow)
        self.assertEqual(workflow.count("certificate-profile-name: WritingForgePro"), 3)

    def test_attestations_bind_final_validated_bytes_without_changing_release_asset_inventory(self):
        workflow = (windows.ROOT / ".github/workflows/github-release.yml").read_text()
        verify = workflow.index("Verify complete signed platform set and stage public downloads")
        vsix = workflow.index("Attest the final GitHub-hosted VSIX bytes", verify)
        linux = workflow.index("Attest the final standalone Linux installer bytes", vsix)
        self.assertTrue(verify < vsix < linux)
        self.assertEqual(workflow.count("actions/attest@59d89421af93a897026c735860bf21b6eb4f7b26"), 2)
        self.assertIn("subject-path: artifacts/github-release/*.vsix", workflow)
        self.assertIn("attestations: write", workflow)
        self.assertIn("airk-release-attestations-", workflow)
        self.assertNotIn("artifacts/github-release/attestation", workflow)


if __name__ == "__main__":
    unittest.main()
