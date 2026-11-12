# SPDX-License-Identifier: Apache-2.0
"""Keep installer smoke expectations aligned with platform login ownership."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

spec = importlib.util.spec_from_file_location(
    "service_install_check", Path(__file__).parents[1] / "check-service-install.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ServiceInstallCheckTests(unittest.TestCase):
    def test_native_invocation_keeps_lifecycle_outer_allowance_and_exact_single_submission(self):
        for arguments, override, expected in (
                (("runtime", "ensure", "--install-dir", "owned"), None, 180),
                (("runtime", "activate", "--install-dir", "owned"), None, 480),
                (("runtime", "rollback", "--install-dir", "owned"), None, 480),
                (("runtime", "activate", "--install-dir", "owned"), 7, 7),
                (("runtime", "ensure", "--install-dir", "owned"), 240, 240),
                # A split airk serves the same operations behind the U93 selector.
                (("__airk_role", "setup-interim", "runtime", "activate", "--install-dir", "owned"), None, 480),
                (("__airk_role", "setup-interim", "runtime", "rollback", "--install-dir", "owned"), None, 480),
                (("__airk_role", "setup-interim", "runtime", "ensure", "--install-dir", "owned"), None, 180),
                (("service", "rpc", "--runtime-dir", "owned"), None, 180)):
            with self.subTest(arguments=arguments, override=override), mock.patch.object(
                    smoke.subprocess, "run", return_value=SimpleNamespace(returncode=0, stdout='{"ok":true}', stderr="")) as run:
                self.assertEqual(smoke.invoke_native("owned-airk", *arguments, environment={}, timeout=override), {"ok":True})
                run.assert_called_once()
                self.assertEqual(run.call_args.args[0], ["owned-airk", *arguments])
                self.assertEqual(run.call_args.kwargs["timeout"], expected)

    def test_lifecycle_timeout_retains_original_streams_and_bounded_diagnostic_tails_without_retry(self):
        output = b"x" * 5000 + b"last native progress"
        stderr = b"y" * 5000 + b"native_checkpoint_pending: last pending reason"
        timeout = smoke.subprocess.TimeoutExpired(["owned-airk", "runtime", "activate"], 480, output=output, stderr=stderr)
        with mock.patch.object(smoke.subprocess, "run", side_effect=timeout) as run:
            with self.assertRaises(smoke.subprocess.TimeoutExpired) as raised:
                smoke.invoke_native("owned-airk", "runtime", "activate", environment={})
        run.assert_called_once()
        self.assertIs(raised.exception, timeout)
        self.assertEqual(raised.exception.stdout, output)
        self.assertEqual(raised.exception.stderr, stderr)
        note = raised.exception.__notes__[0]
        self.assertIn("last native progress", note)
        self.assertIn("native_checkpoint_pending: last pending reason", note)
        self.assertLess(len(note), 8400)

    def images(self, root, operating_system, version):
        payload, installed = root / "source", root / "installed"
        payload.mkdir()
        builder = smoke.runtime_builder()
        main, helper = builder.runtime_names(operating_system)
        launcher = builder.launcher_name(operating_system, version)
        names = {main} if builder.single_image_version(version) else {main, helper, launcher}
        for name in names:
            image = payload / name
            image.parent.mkdir(parents=True, exist_ok=True)
            image.write_bytes(("Original approved fixture: " + name).encode())
            image.chmod(0o700)
        inventory = {"os": operating_system, "version": version,
                     "files": {name: smoke.sha256(payload / name) for name in names}}
        (payload / "airk-install.json").write_text(json.dumps(inventory), encoding="utf-8")
        shutil.copytree(payload, installed)
        if launcher == main and not builder.single_image_version(version):
            for role in ("airk-service", "airk-index", "airk-source"):
                name = role + (".exe" if operating_system == "windows" else "")
                shutil.copy2(installed / main, installed / name)
        calls = []

        def invoke(image, *arguments, raw=False):
            calls.append((image.relative_to(installed).as_posix(), arguments, raw))
            return f"airk {version}" if raw else {"self_test": "passed"}

        return payload, installed, inventory, invoke, calls

    def test_single_image_all_platforms_verify_main_without_helpers_or_role_copies(self):
        for operating_system in ("windows", "linux", "macos"):
            with self.subTest(os=operating_system), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, calls = self.images(Path(temporary), operating_system, "1.8.14")
                main, helper = smoke.runtime_builder().runtime_names(operating_system)
                self.assertEqual(smoke.check_installed_images(payload, installed, inventory, invoke),
                                 (False, inventory["files"][main]))
                self.assertEqual(calls, [(main, ("runtime", "self-test"), False)])
                self.assertFalse((installed / helper).exists())
                smoke.check_payload_unchanged(payload, inventory, smoke.sha256(payload / "airk-install.json"))

    def test_single_image_refuses_retired_files_even_when_listed_or_outside_inventory(self):
        for name, listed in (("airk-embed", True), ("airk-launcher", False), ("airk-source", False)):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, _ = self.images(Path(temporary), "linux", "1.8.14")
                (installed / name).write_bytes(b"retired payload")
                if listed:
                    inventory["files"][name] = smoke.sha256(installed / name)
                with self.assertRaisesRegex(RuntimeError, "retired"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_split_login_plans_use_the_interim_role_and_keep_the_mac_entry(self):
        install = Path(__file__).parent.resolve()
        for platform, arguments in (("darwin", ["runtime", "login-serve", "--channel", "stable"]),
                                    ("linux", ["__airk_role", "setup-interim", "runtime", "managed-serve", "--install-dir", str(install)]),
                                    ("win32", ["__airk_role", "setup-interim", "runtime", "managed-serve", "--install-dir", str(install)])):
            plan = {"id":"airksvc-fixture", "platform":"smappservice_user", "arguments":arguments}
            smoke.check_login_plan(plan, install, platform, version="1.8.24")
            if platform != "darwin":
                for changed in (arguments[2:], ["__airk_role", "setup-interim", "runtime", "foreground", "--install-dir", str(install)],
                                [*arguments[:-1], str(install.parent)]):
                    with self.subTest(platform=platform, changed=changed), self.assertRaises(RuntimeError):
                        smoke.check_login_plan({**plan, "arguments":changed}, install, platform, version="1.8.24")

    def test_split_images_verify_both_pair_images_and_self_test_behind_the_interim_role(self):
        for operating_system in ("windows", "linux", "macos"):
            with self.subTest(os=operating_system), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                payload, installed = root / "source", root / "installed"
                main, cli = smoke.runtime_builder().split_images(operating_system)
                for name in (main, cli):
                    image = payload / name
                    image.parent.mkdir(parents=True, exist_ok=True)
                    image.write_bytes(("Original approved fixture: " + name).encode())
                    image.chmod(0o700)
                inventory = {"os": operating_system, "version": "1.8.24",
                             "files": {name: smoke.sha256(payload / name) for name in (main, cli)}}
                shutil.copytree(payload, installed)
                calls = []
                def invoke(image, *arguments, raw=False):
                    calls.append((image.relative_to(installed).as_posix(), arguments, raw))
                    return "airkctl 1.8.24" if raw else {"self_test": "passed"}
                self.assertEqual(smoke.check_installed_images(payload, installed, inventory, invoke),
                                 (False, inventory["files"][main]))
                self.assertEqual(calls, [(cli, ("--version",), True),
                                         (main, ("__airk_role", "setup-interim", "runtime", "self-test"), False)])
                (installed / cli).write_bytes(b"changed")
                with self.assertRaisesRegex(RuntimeError, "modified"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)
                del inventory["files"][cli]
                with self.assertRaisesRegex(RuntimeError, "airkctl"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_modern_login_plans_use_direct_manager_entry_and_keep_mac_main_sealed(self):
        install = Path(__file__).parent.resolve()
        for platform, arguments in (("darwin", ["runtime", "login-serve", "--channel", "stable"]),
                                    ("linux", ["runtime", "managed-serve", "--install-dir", str(install)]),
                                    ("win32", ["runtime", "managed-serve", "--install-dir", str(install)])):
            plan = {"id":"airksvc-fixture", "platform":"smappservice_user", "arguments":arguments}
            smoke.check_login_plan(plan, install, platform, version="1.8.14")
            with self.assertRaises(RuntimeError):
                smoke.check_login_plan({**plan, "arguments":["runtime", "foreground", "--install-dir", str(install)]},
                                       install, platform, version="1.8.14")

    def test_modern_lifecycle_contract_retains_scope_disconnect_and_exact_stop_fences(self):
        cases = [(fault, False) for fault in (None, "authority", "instance", "stop_pending", "envelope")]
        cases += [(None, True), ("instance", True)]  # Split pair: airkctl user commands, airk U93 operations.
        for fault, split in cases:
            with self.subTest(fault=fault, split=split), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source, control, install = root / "source", root / "control", root / "install"
                source.mkdir(); control.mkdir(); install.mkdir()
                (control / "http-1").mkdir(mode=0o700)
                native_transport = smoke.load_fixture("test_install_native_setup", "http_fixture.py")
                (source / "sentinel.txt").write_bytes(b"Original source proof\n")
                main = install / "airk"; main.write_bytes(b"fictional image")
                cli = install / "airkctl" if split else None
                images = []
                selected = {"namespace":"a" * 32, "expected_catalog_revision":4, "index_id":"idx_original"}
                project = {"binding":"b" * 64,"root":str(source),"data_root":str(root / "data"),"collection":"fixture"}
                health = {"instance":"c" * 32,"runtime_sha256":"d" * 64}
                clients, calls, enrollments = [], [], []
                body = {"hits":[{"citation":{"path":"sentinel.txt", "content_hash":smoke.sha256(source / "sentinel.txt")},"text":"Original source proof"}]}
                envelope = {"content":[{"type":"text","text":json.dumps(body)}],"isError":False}
                class Client:
                    def __init__(self, *_):
                        self.responses = {}; self.closed = False; clients.append(self)
                        self.session = "owned-session-" + str(len(clients))
                    def send(self, message):
                        method = message["method"]
                        if method == "initialize": result = {"serverInfo":{"name":"airk"}}
                        elif method == "ping":
                            self_outer.assertTrue(clients[0].closed)
                            result = {}
                        elif method == "tools/call": result = envelope
                        else: return
                        self.responses[message["id"]] = {"result":result}
                    def receive(self, identifier): return self.responses.pop(identifier)
                    def api_read(self, *_):
                        return {**envelope,"future_provenance":{"unselected":True}} if fault == "envelope" else envelope
                    def close(self): self.closed = True
                self_outer = self
                def invoke(image, *arguments, input=None, rejected=False, timeout=None):
                    images.append((image, arguments[:2]))
                    if split and image == main:
                        self.assertEqual(arguments[:2], smoke.INTERIM_SELECTOR, "airk receives only U93 operations")
                        arguments = arguments[2:]
                        self.assertEqual(arguments[0], "runtime")
                    elif split:
                        self.assertEqual(image, cli)
                        self.assertNotEqual(arguments[:2], smoke.INTERIM_SELECTOR)
                        self.assertTrue(arguments[:2] == ("service", "rpc") or arguments[-1] == "index", arguments)
                    else:
                        self.assertEqual(image, main)
                        self.assertNotEqual(arguments[:2], smoke.INTERIM_SELECTOR)
                    calls.append((arguments, rejected))
                    if arguments[:2] == ("service","rpc"):
                        method = json.loads(input)
                        if method == "health":
                            return {"observation":{**health,"instance":"e" * 32 if fault == "instance" else health["instance"]}}
                        if method == "request_epoch": return {"namespace":"a" * 32,"epoch":2}
                        if "job" in method: return {"job":{"id":"job-owned","state":"succeeded"}}
                        raise AssertionError("Unexpected console metadata request: " + str(method))
                    if arguments[:2] == ("runtime", "http-connection"):
                        self.assertIsNotNone(timeout); self.assertGreater(timeout,0); self.assertLessEqual(timeout,60)
                        selection_file = Path(arguments[arguments.index("--selection-file") + 1])
                        requested = {"connection_id":arguments[arguments.index("--connection-id") + 1],"selection":json.loads(selection_file.read_text())}
                        self.assertEqual(arguments[arguments.index("--client") + 1], "unknown")
                        enrollments.append(requested)
                        authority = {**selected,"expected_catalog_revision":5}
                        if fault == "authority": authority["index_id"] = "idx_broader"
                        credential = control / "http-1" / ("credential-" + requested["connection_id"] + ".token")
                        ca = control / "http-1/ca.pem"
                        credential.write_text("fixture owned reference"); ca.write_text("fixture owned CA reference")
                        return {"schema":1,"connection_id":requested["connection_id"],"selection":{"kind":"scoped","selection":authority},"credential_file":str(credential),"ca_certificate_file":str(ca)}
                    if arguments[-1] == "index": return {"job":{"id":"job-owned"}}
                    action = arguments[1]
                    if action == "prepare-client": return {"launcher":None,"executable":str(main),"service_lifecycle_changed":False}
                    if action == "lifecycle":
                        request = json.loads(input)
                        self.assertEqual(request["expected_instance"], health["instance"])
                        self.assertEqual(request["runtime"], health["runtime_sha256"])
                        self.assertFalse(request["force"])
                        return {"record":{"request":request,"phase":"draining" if fault == "stop_pending" else "complete"}}
                    if action == "inspect": return {"lifecycle":{"explicitly_stopped":True,"pending":None}}
                    if action == "ensure": self.assertTrue(rejected); return None
                    if action == "start": return {**health,"instance":"f" * 32}
                    raise AssertionError(arguments)
                reviewed = SimpleNamespace(reviewed_fixture=lambda rpc, *_args, **_kwargs:(project, selected))
                transport = SimpleNamespace(McpClient=Client, enroll_native=native_transport.enroll_native, validate_native_identity=lambda *_:None, parity_value=lambda value:value)
                reads = SimpleNamespace(metadata_rpc=lambda invoke,method:invoke(json.dumps(method),None), read_retry_delay=lambda _:None)
                def load(_name,file):
                    return {"check-runtime-upgrade.py":reviewed,"http_fixture.py":transport,"check-service-clients.py":reads}[file]
                with mock.patch.object(smoke,"load_fixture",side_effect=load):
                    args = (invoke, main, main, install, control, source, root / "data", health, root / "proof.json")
                    if fault is None:
                        stopped_checks = []
                        def stopped_probe():
                            self.assertEqual(calls[-1][0][:2], ("runtime", "ensure"))
                            self.assertFalse(any(arguments[:2] == ("runtime", "start") for arguments, _ in calls))
                            stopped_checks.append(True)
                        result = smoke.modern_client_lifecycle(*args, stopped_probe=stopped_probe, cli=cli)
                        self.assertEqual(stopped_checks, [True])
                        self.assertNotEqual(result["instance"],health["instance"])
                        self.assertEqual(len(enrollments),2)
                        self.assertNotEqual(enrollments[0]["connection_id"],enrollments[1]["connection_id"])
                        self.assertEqual(enrollments[0]["selection"],enrollments[1]["selection"])
                        self.assertEqual(sum(arguments[:2] == ("runtime","lifecycle") for arguments,_ in calls),1)
                    elif fault == "authority":
                        with self.assertRaisesRegex(ValueError, "immutable selection"):
                            smoke.modern_client_lifecycle(*args)
                    else:
                        with self.assertRaisesRegex(RuntimeError, "authority|replaced|Stop|parity"):
                            smoke.modern_client_lifecycle(*args, cli=cli)
                self.assertTrue(all(client.closed for client in clients))
                if split:
                    self.assertIn((cli, ("service", "rpc")), images)
                    self.assertIn((main, smoke.INTERIM_SELECTOR), images)
                self.assertEqual((source / "sentinel.txt").read_bytes(),b"Original source proof\n")

    def test_complete_envelope_parity_normalizes_only_documented_timing(self):
        transport = smoke.load_fixture("installer_parity_transport", "http_fixture.py")
        first = {"content":[{"type":"text","text":json.dumps({"request_id":"first","evidence":{"hash":"abc"}})}],
                 "structuredContent":{"request_id":"first","evidence":{"hash":"abc"}},
                 "future_provenance":{"scope":"selected"}}
        second = json.loads(json.dumps(first))
        second["content"][0]["text"] = json.dumps({"request_id":"second","evidence":{"hash":"abc"}})
        second["structuredContent"]["request_id"] = "second"
        self.assertEqual(smoke.parity_envelope(first,transport),smoke.parity_envelope(second,transport))
        second["structuredContent"]["evidence"]["hash"] = "different"
        self.assertNotEqual(smoke.parity_envelope(first,transport),smoke.parity_envelope(second,transport))
        second = json.loads(json.dumps(first)); second["future_provenance"]["scope"] = "broader"
        self.assertNotEqual(smoke.parity_envelope(first,transport),smoke.parity_envelope(second,transport))
        self.assertIn("first",first["content"][0]["text"], "comparison changed retained raw proof")

    def test_current_mac_uses_approved_standalone_image_and_sealed_main_without_role_aliases(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, installed, inventory, invoke, calls = self.images(Path(temporary), "macos", "1.8.8")
            result = smoke.check_installed_images(payload, installed, inventory, invoke)
            image = smoke.runtime_builder().launcher_name("macos", "1.8.8")
            self.assertEqual(result, (True, inventory["files"][image]))
            self.assertEqual(calls, [("AIRK.app/Contents/MacOS/airk", ("runtime", "self-test"), False),
                                     (image, ("--version",), True)])
            self.assertFalse(any((installed / name).exists() for name in ["airk-service", "airk-index", "airk-source"]))
            smoke.check_payload_unchanged(payload, inventory, smoke.sha256(payload / "airk-install.json"))

    def test_legacy_mac_windows_and_linux_keep_exact_unaliased_role_checks(self):
        for operating_system, version in [("macos", "1.8.7"), ("windows", "1.8.8"), ("linux", "1.8.8")]:
            with self.subTest(operating_system=operating_system), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, calls = self.images(Path(temporary), operating_system, version)
                self.assertFalse(smoke.check_installed_images(payload, installed, inventory, invoke)[0])
                self.assertEqual(len(calls), 3)
                name = "airk-index" + (".exe" if operating_system == "windows" else "")
                (installed / name).write_bytes(b"changed role image")
                with self.assertRaisesRegex(RuntimeError, "modified"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_current_mac_refuses_missing_unlisted_or_modified_launcher_and_extra_aliases(self):
        for fault in ["missing", "unlisted", "modified", "extra_alias"]:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, _ = self.images(Path(temporary), "macos", "1.8.8")
                name = smoke.runtime_builder().launcher_name("macos", "1.8.8")
                if fault == "missing":
                    (installed / name).unlink()
                elif fault == "unlisted":
                    inventory["files"].pop(name)
                elif fault == "modified":
                    (installed / name).write_bytes(b"changed standalone image")
                else:
                    (installed / "airk-service").write_bytes(b"unexpected copied sealed main")
                with self.assertRaises(RuntimeError):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_current_mac_refuses_aliased_image_and_unexecutable_unix_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload, installed, inventory, invoke, _ = self.images(root, "macos", "1.8.8")
            name = smoke.runtime_builder().launcher_name("macos", "1.8.8")
            alias = root / "unowned-alias"
            os.link(installed / name, alias)
            with self.assertRaisesRegex(RuntimeError, "aliased"):
                smoke.check_installed_images(payload, installed, inventory, invoke)
            alias.unlink()
            if os.name != "nt":
                (installed / name).chmod(0o600)
                with self.assertRaisesRegex(RuntimeError, "nonexecutable"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_copied_mac_launcher_requires_signature_exact_bytes_and_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, installed, inventory, invoke, _ = self.images(Path(temporary), "macos", "1.8.8")
            image = installed / smoke.runtime_builder().launcher_name("macos", "1.8.8")
            observed = []
            smoke.check_copied_mac_launcher(image, smoke.sha256(image), invoke, "1.8.8", observed.append)
            self.assertEqual(observed, [image])
            with self.assertRaisesRegex(RuntimeError, "version"):
                smoke.check_copied_mac_launcher(image, smoke.sha256(image), lambda *args, **kwargs: "airk 1.8.7", "1.8.8", observed.append)
            def reject(_image):
                raise RuntimeError("injected strict signature rejection")
            with self.assertRaisesRegex(RuntimeError, "signature"):
                smoke.check_copied_mac_launcher(image, smoke.sha256(image), invoke, "1.8.8", reject)
            with self.assertRaisesRegex(RuntimeError, "modified"):
                smoke.check_copied_mac_launcher(image, "0" * 64, invoke, "1.8.8", observed.append)

    def test_lifecycle_source_preservation_check_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, _, inventory, _, _ = self.images(Path(temporary), "macos", "1.8.8")
            identity = smoke.sha256(payload / "airk-install.json")
            smoke.check_payload_unchanged(payload, inventory, identity)
            main = smoke.runtime_builder().runtime_names("macos")[0]
            (payload / main).write_bytes(b"source changed during lifecycle")
            with self.assertRaisesRegex(RuntimeError, "source payload"):
                smoke.check_payload_unchanged(payload, inventory, identity)

    def test_lifecycle_source_preservation_check_rejects_unlisted_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, _, inventory, _, _ = self.images(Path(temporary), "macos", "1.8.8")
            identity = smoke.sha256(payload / "airk-install.json")
            (payload / "unexpected-role-copy").write_bytes(b"unlisted source mutation")
            with self.assertRaisesRegex(RuntimeError, "topology"):
                smoke.check_payload_unchanged(payload, inventory, identity)

    def test_mac_requires_app_registration_and_fixed_channel_entry(self):
        plan = {"id": "airksvc-fixture", "platform": "smappservice_user",
                "arguments": ["runtime", "login-foreground", "--channel", "stable"]}
        smoke.check_login_plan(plan, Path("fixture"), "darwin")
        for change in ({"id": "airk-legacy"}, {"platform": "launchagent"},
                       {"arguments": ["runtime", "foreground", "--install-dir", "fixture"]},
                       {"arguments": ["runtime", "login-foreground", "--channel", "preview"]}):
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                smoke.check_login_plan({**plan, **change}, Path("fixture"), "darwin")

    def test_windows_and_linux_require_exact_installation_dispatch(self):
        install = Path(__file__).parent.resolve()
        plan = {"id": "airksvc-fixture", "arguments": ["runtime", "foreground", "--install-dir", str(install)]}
        for platform in ("win32", "linux"):
            with self.subTest(platform=platform):
                smoke.check_login_plan(plan, install, platform)
                for change in ({"id": "airk-legacy"},
                               {"arguments": ["runtime", "foreground", "--install-dir", str(install / "other")]},
                               {"arguments": ["runtime", "login-foreground", "--channel", "stable"]}):
                    with self.subTest(change=change), self.assertRaises(RuntimeError):
                        smoke.check_login_plan({**plan, **change}, install, platform)

    def test_fixture_parent_is_created_without_a_prior_native_build(self):
        with tempfile.TemporaryDirectory(prefix="airk-install-root-") as root:
            parent = smoke.fixture_parent(Path(root))
            self.assertEqual(parent, Path(root) / "target")
            self.assertTrue(parent.is_dir())
            self.assertEqual(smoke.fixture_parent(Path(root)), parent)


if __name__ == "__main__":
    unittest.main()
