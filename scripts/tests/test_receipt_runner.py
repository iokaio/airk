# SPDX-License-Identifier: Apache-2.0
"""The receipt runner must record consumed inputs and refuse empty test selections."""
import contextlib
import errno
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("receipt_runner", ROOT / "scripts" / "dev" / "receipt_runner.py")
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class SelectedTests(unittest.TestCase):
    def test_nextest_failure_output_does_not_add_captured_child_selections(self):
        log = ("    Starting 4 tests across 2 binaries (81 tests skipped)\n"
               "  stdout ---\n    running 1 test\n    running 1 test\n"
               "    running 1 test\n"
               "     Summary [0.55s] 4 tests run: 1 passed, 3 failed\n")
        self.assertEqual(RUNNER.selected_tests(log), 4)

    def test_mixed_runner_blocks_keep_their_own_selections(self):
        log = ("running 2 tests\n"
               "    Starting 4 tests across 2 binaries\n"
               "    running 1 test\n"
               "     Summary [0.55s] 4 tests run: 1 passed, 3 failed\n"
               "Ran 3 tests in 0.01s\n# tests 5\n")
        self.assertEqual(RUNNER.selected_tests(log), 14)

    def test_node_spec_reporter_records_its_selection(self):
        self.assertEqual(RUNNER.selected_tests("ℹ tests 99\nℹ pass 87\nℹ fail 12\n"), 99)

    def test_recognizes_runner_summaries(self):
        self.assertEqual(RUNNER.selected_tests("    Starting 5 tests across 1 binary"), 5)
        self.assertEqual(RUNNER.selected_tests("# tests 580\n# pass 576"), 580)
        self.assertEqual(RUNNER.selected_tests("Ran 12 tests in 0.5s"), 12)
        self.assertEqual(RUNNER.selected_tests("running 0 tests\nrunning 3 tests"), 3)
        self.assertIsNone(RUNNER.selected_tests("Finished dev profile"))

    def test_identifies_test_commands(self):
        self.assertTrue(RUNNER.is_test_command(["cargo", "nextest", "run", "-p", "airk-core"]))
        self.assertTrue(RUNNER.is_test_command(["cargo", "test", "-p", "airk-core"]))
        self.assertTrue(RUNNER.is_test_command([r"C:\tools\cargo.exe", "nextest", "run"]))
        self.assertTrue(RUNNER.is_test_command(["node", "--test", "out/test/a.test.js"]))
        self.assertTrue(RUNNER.is_test_command(["python", "-m", "unittest", "discover"]))
        self.assertFalse(RUNNER.is_test_command(["python", "scripts/dev-check.py", "-p", "airk-core"]))

    def test_cargo_compilation_target_selectors_are_not_test_execution(self):
        observed = ["cargo", "clippy", "-p", "airk-cli", "--test", "admin_plane", "--all-features", "--locked", "--timings", "--", "-D", "warnings"]
        self.assertFalse(RUNNER.is_test_command(observed))
        for subcommand in ("clippy", "check", "build"):
            for selector in ("--test", "--bench", "--tests", "--benches", "--all-targets"):
                with self.subTest(subcommand=subcommand, selector=selector):
                    self.assertFalse(RUNNER.is_test_command(["cargo", subcommand, selector, "unittest"]))
        for command in (["cargo", "bench", "--bench", "test"], ["cargo", "run", "--", "--test"],
                        ["cargo", "nextest", "list"], ["cargo", "test", "--no-run"],
                        ["cargo", "test", "--", "--list"], ["cargo", "test", "--help"]):
            with self.subTest(command=command):
                self.assertFalse(RUNNER.is_test_command(command))

    def test_runner_identity_and_global_options_preserve_real_test_execution(self):
        for command in ([r"C:\tools\cargo.exe", "+1.98.0", "--color", "always", "--config=net.offline=true", "test", "--test", "admin_plane"],
                        ["/home/human/.cargo/bin/cargo", "-Z", "unstable-options", "nextest", "run", "-E", "test(example)"],
                        ["cargo-nextest", "nextest", "run"], ["nextest", "run"],
                        ["python3.13", "-I", "-B", "-X", "utf8", "-m", "unittest", "discover"],
                        ["py.exe", "-3.13", "-m", "unittest"],
                        ["node", "--import", "bootstrap.mjs", "--test", "--test-reporter", "spec", "a.test.js"]):
            with self.subTest(command=command):
                self.assertTrue(RUNNER.is_test_command(command))

    def test_env_prefix_and_maintained_wrapper_preserve_command_semantics(self):
        prefix = ["/usr/bin/env", "-i", "--unset", "OLD", "--chdir=/source", "LABEL=unittest", "--"]
        wrapper = ["python3.13", "-B", "scripts/static-runtime-environment.py", "--environment-file", "static.json", "--"]
        for before in (prefix, wrapper, prefix + wrapper):
            self.assertTrue(RUNNER.is_test_command([*before, "cargo", "test", "--test", "admin_plane"]))
            self.assertFalse(RUNNER.is_test_command([*before, "cargo", "clippy", "--test", "admin_plane"]))
        self.assertFalse(RUNNER.is_test_command(["env", "-S", "cargo test"]))
        self.assertFalse(RUNNER.is_test_command(["env", "--unset"]))

    def test_arguments_and_inline_code_do_not_impersonate_a_test_runner(self):
        for command in (["python", "-c", "print('unittest')", "--test"],
                        ["python", "scripts/dev-check.py", "--test", "unittest"],
                        ["node", "script.js", "--test"], ["node", "--", "--test"],
                        ["node", "--eval", "'--test'"], ["node", "--test", "--help"],
                        ["python", "-m", "unittest", "--help"], [], ["echo", "cargo", "test"]):
            with self.subTest(command=command):
                self.assertFalse(RUNNER.is_test_command(command))

    def test_rejects_bad_names_and_missing_commands(self):
        for argv in (["Bad_Name", "echo"], ["ok-name"]):
            with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
                RUNNER.parse(argv)


class Receipts(unittest.TestCase):
    def test_long_nonpath_argument_reaches_real_child_and_keeps_file_hash(self):
        selector = 'test(original_request_is_retained) | ' * 100
        native_stat = Path.stat

        def mac_stat(path, *args, **kwargs):
            if path == ROOT / selector:
                raise OSError(errno.ENAMETOOLONG, 'File name too long', str(path))
            return native_stat(path, *args, **kwargs)

        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            script = out / 'child.py'
            script.write_text('import sys; print(len(sys.argv[1]))')
            # Keep a real repository-local input so discovery must still hash it.
            source = ROOT / 'scripts/tests/test_receipt_runner.py'
            with patch.object(Path, 'stat', mac_stat), contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(['--groups', '', '--out', str(out), 'long-selector',
                                      sys.executable, str(script), selector, str(source)])
            self.assertEqual(status, 0)
            receipt = json.loads((out / 'long-selector.json').read_text())
            self.assertEqual(receipt['command'][-2], selector)
            self.assertEqual((out / 'long-selector.log').read_text().strip(), str(len(selector)))
            self.assertEqual(receipt['command_inputs']['scripts/tests/test_receipt_runner.py'],
                             hashlib.sha256(source.read_bytes()).hexdigest())
            self.assertTrue(receipt['command_inputs_unchanged'])

    def test_command_input_discovery_propagates_permission_and_io_errors(self):
        for code in (errno.EACCES, errno.EIO):
            with self.subTest(errno=code), patch.object(Path, 'stat', side_effect=OSError(code, 'refused')):
                with self.assertRaises(OSError) as caught:
                    RUNNER.command_input_files(['source.py'])
                self.assertEqual(caught.exception.errno, code)

    def test_nonpath_discovery_does_not_hide_file_read_errors(self):
        with patch.object(RUNNER, 'digest', side_effect=PermissionError(errno.EACCES, 'read refused')):
            with self.assertRaises(PermissionError):
                RUNNER.command_input_files(['scripts/tests/test_receipt_runner.py'])

    def test_nonfile_arguments_are_not_sealed(self):
        errors = [FileNotFoundError(errno.ENOENT, 'missing'),
                  NotADirectoryError(errno.ENOTDIR, 'not a directory'),
                  OSError(errno.ENAMETOOLONG, 'too long')]
        for native_code in (123, 206):
            error = OSError(errno.EINVAL, 'invalid Windows filename')
            error.winerror = native_code
            errors.append(error)
        for error in errors:
            with self.subTest(error=error), patch.object(Path, 'stat', side_effect=error):
                self.assertEqual(RUNNER.command_input_files(['nonpath-argument']), {})

    @unittest.skipUnless(sys.platform == "win32", "Windows Cargo proxy precedence")
    def test_explicit_native_cargo_and_rustc_precede_proxy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            toolchain = root / "toolchain"
            toolchain.mkdir()
            cargo, rustc = toolchain / "cargo.exe", toolchain / "rustc.exe"
            cargo.touch()
            rustc.touch()
            args = RUNNER.parse(["--groups", "", "native-tools", sys.executable, "-c", "pass"])
            with patch.dict(RUNNER.os.environ, {"CARGO": str(cargo), "RUSTC": str(rustc)}):
                selected = RUNNER.environment(args, root)
            self.assertTrue(cargo.samefile(RUNNER.shutil.which("cargo.exe", path=selected["PATH"])))
            self.assertTrue(rustc.samefile(RUNNER.shutil.which("rustc.exe", path=selected["PATH"])))
            other = root / "rustc.exe"
            other.touch()
            with patch.dict(RUNNER.os.environ, {"CARGO": str(cargo), "RUSTC": str(other)}):
                ordinary = RUNNER.environment(args, root)
            self.assertEqual(Path(ordinary["PATH"].split(RUNNER.os.pathsep)[0]), Path.home() / ".cargo" / "bin")

    def test_child_tempfile_uses_owned_base_instead_of_inherited_tmpdir(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            foreign = base/'foreign'
            foreign.mkdir()
            private = base/'owned-fixtures'
            args = RUNNER.parse(['--fixture-base',str(private),'--groups','',
                                 'temp-choice',sys.executable,'-c','pass'])
            with patch.dict(RUNNER.os.environ,{'TMPDIR':str(foreign)}):
                environment = RUNNER.environment(args,base/'receipts')
            child = ('import tempfile; from pathlib import Path; '
                     'owner=tempfile.TemporaryDirectory(); '
                     'print(Path(owner.name).resolve().parent); owner.cleanup()')
            actual = subprocess.run([sys.executable,'-I','-B','-c',child],env=environment,
                                    capture_output=True,text=True,check=True,timeout=15)
            self.assertEqual(Path(actual.stdout.strip()),private)
            for name in ('TEMP','TMP','TMPDIR'):
                self.assertEqual(environment[name],str(private))

    def test_child_git_environment_has_no_synthetic_trust_configuration(self):
        inherited = {key:value for key,value in RUNNER.os.environ.items() if not key.upper().startswith("GIT_")}
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            args = RUNNER.parse(["--groups", "", "git-environment", sys.executable, "-c", "pass"])
            with patch.dict(RUNNER.os.environ, inherited, clear=True):
                environment = RUNNER.environment(args, out)
            self.assertFalse(any(key.upper().startswith("GIT_") for key in environment))
            child = subprocess.run([sys.executable, "-I", "-B", "-c", "import json,os; print(json.dumps({k:v for k,v in os.environ.items() if k.upper().startswith('GIT_')}))"],
                env=environment, capture_output=True, text=True, check=True, timeout=15)
            self.assertEqual(json.loads(child.stdout), {})

    def test_child_preserves_explicit_inherited_git_overrides_for_native_refusal(self):
        inherited = {key:value for key,value in RUNNER.os.environ.items() if not key.upper().startswith("GIT_")}
        explicit = {"GIT_CONFIG_COUNT":"1", "GIT_CONFIG_KEY_0":"core.fsmonitor", "GIT_CONFIG_VALUE_0":"unreviewed-fixture-command", "GIT_CONFIG_GLOBAL":"explicit-user-config"}
        inherited.update(explicit)
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            args = RUNNER.parse(["--groups", "", "inherited-git", sys.executable, "-c", "pass"])
            with patch.dict(RUNNER.os.environ, inherited, clear=True):
                environment = RUNNER.environment(args, out)
            self.assertEqual({key:value for key,value in environment.items() if key.upper().startswith("GIT_")}, explicit)
            child = subprocess.run([sys.executable, "-I", "-B", "-c", "import json,os; print(json.dumps({k:v for k,v in os.environ.items() if k.upper().startswith('GIT_')}))"],
                env=environment, capture_output=True, text=True, check=True, timeout=15)
            self.assertEqual(json.loads(child.stdout), explicit)

    def test_authentic_flushed_start_and_runner_snapshot_precede_real_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            source = ROOT / 'scripts/tests/test_receipt_runner.py'
            child = (
                'import hashlib,json,sys; from pathlib import Path; '
                'start=json.loads(Path(sys.argv[1]).read_text(encoding="utf-8")); '
                'assert start["status"]=="started" and start["passed"] is False; '
                'assert start["exit_code"] is None and start["elapsed_seconds"] is None; '
                'assert start["child"]["state"]=="not_observed"; '
                'assert start["command_inputs"]["scripts/tests/test_receipt_runner.py"]=='
                'hashlib.sha256(Path(sys.argv[3]).read_bytes()).hexdigest(); '
                'assert start["runner_sha256"]==hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(); '
                'assert start["runtime_seal"] is None; print("authentic start preceded child")')
            result = self.run_runner(out, 'durable-start', sys.executable, '-c', child,
                str(out / 'durable-start-start.json'), str(out / 'durable-start-runner.py'), str(source),
                extra=('--groups', ''))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            start = json.loads((out / 'durable-start-start.json').read_text(encoding='utf-8'))
            receipt = json.loads((out / 'durable-start.json').read_text(encoding='utf-8'))
            self.assertEqual(start['status'], 'started')
            self.assertIsNone(start['ended_unix'])
            self.assertIsNone(start['runner_exit_code'])
            self.assertEqual(receipt['start_receipt']['sha256'], RUNNER.digest(out / 'durable-start-start.json'))
            self.assertEqual(receipt['status'], 'completed')
            self.assertTrue(receipt['passed'])
            self.assertEqual(receipt['child']['state'], 'exited')
            self.assertEqual(receipt['child']['returncode'], 0)
            self.assertGreaterEqual(receipt['ended_unix'], receipt['started_unix'])
            self.assertGreaterEqual(receipt['elapsed_seconds'], 0)
            self.assertIn('authentic start preceded child', (out / 'durable-start.log').read_text())

    def test_sigint_records_actual_running_child_without_claiming_settlement_or_replay(self):
        original_popen = subprocess.Popen
        previous_handler = signal.getsignal(signal.SIGINT)
        children = []
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            def spawn(command, **options):
                start = json.loads((out / 'interrupted-start.json').read_text(encoding='utf-8'))
                self.assertEqual(start['status'], 'started')
                process = original_popen(command, stdin=subprocess.PIPE, **options)
                wait = process.wait
                children.append((process, wait))
                def interrupt_wait():
                    signal.raise_signal(signal.SIGINT)
                    self.fail('SIGINT did not interrupt the wait')
                process.wait = interrupt_wait
                return process
            try:
                with patch.object(RUNNER.subprocess, 'Popen', side_effect=spawn) as launch, \
                        contextlib.redirect_stdout(io.StringIO()):
                    status = RUNNER.main(['--groups', '', '--out', str(out), 'interrupted', sys.executable,
                        '-c', 'import sys; sys.stdin.buffer.read(); print("owned child released", flush=True)'])
                self.assertEqual(status, 130)
                launch.assert_called_once()
                receipt = json.loads((out / 'interrupted.json').read_text(encoding='utf-8'))
                self.assertEqual(receipt['status'], 'interrupted')
                self.assertFalse(receipt['passed'])
                self.assertTrue(receipt['interrupted'])
                self.assertEqual(receipt['interruption']['reason'], 'SIGINT')
                self.assertEqual(receipt['child']['pid'], children[0][0].pid)
                self.assertEqual(receipt['child']['state'], 'running')
                self.assertIsNone(receipt['child']['returncode'])
                self.assertIsNone(receipt['exit_code'])
                self.assertIn('descendant settlement is not certified', receipt['child']['scope'])
                self.assertIsNone(children[0][0].poll())
                self.assertIs(signal.getsignal(signal.SIGINT), previous_handler)
            finally:
                # Release/reap only the exact Popen child this test created.
                # The earlier receipt remains the honest interruption snapshot.
                for child, wait in children:
                    child.stdin.close()
                    self.assertEqual(wait(timeout=15), 0)

    def test_keyboard_interrupt_during_launch_retains_unknown_child_and_prelaunch_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            runtime = out / 'runtime'; runtime.mkdir()
            identity = {'version': 'fixture', 'files': {'airk': 'a' * 64}, 'executables': ['airk']}
            with patch.object(RUNNER, 'runtime_identity', return_value=identity) as seal, \
                    patch.object(RUNNER.subprocess, 'Popen', side_effect=KeyboardInterrupt) as launch, \
                    contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(['--groups', '', '--out', str(out), '--runtime', str(runtime),
                                      'launch-interrupted', sys.executable, '-c', 'pass'])
            self.assertEqual(status, 130)
            launch.assert_called_once()
            seal.assert_called_once()
            receipt = json.loads((out / 'launch-interrupted.json').read_text())
            start = json.loads((out / 'launch-interrupted-start.json').read_text())
            self.assertEqual(start['runtime_seal'], identity)
            self.assertEqual(receipt['runtime_seal'], identity)
            self.assertIsNone(receipt['runtime_unchanged'])
            self.assertIsNone(receipt['child']['pid'])
            self.assertEqual(receipt['child']['state'], 'not_observed')
            self.assertFalse(receipt['passed'])
            self.assertEqual(receipt['interruption']['reason'], 'KeyboardInterrupt')

    def test_interrupt_during_evidence_is_nonpass_even_after_observed_zero_child_exit(self):
        calls = []
        def observe(groups):
            calls.append(groups)
            if len(calls) == 2:
                signal.raise_signal(signal.SIGINT)
            return {}
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            with patch.object(RUNNER, 'observe', side_effect=observe), contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(['--groups', '', '--out', str(out), 'evidence-interrupted',
                                      sys.executable, '-c', 'pass'])
            self.assertEqual(status, 130)
            receipt = json.loads((out / 'evidence-interrupted.json').read_text())
            self.assertTrue(receipt['inputs_unchanged'])
            self.assertEqual(receipt['child']['state'], 'exited')
            self.assertEqual(receipt['child']['returncode'], 0)
            self.assertEqual(receipt['exit_code'], 0)
            self.assertEqual(receipt['runner_exit_code'], 130)
            self.assertEqual(receipt['status'], 'interrupted')
            self.assertFalse(receipt['passed'])

    def test_existing_start_cannot_replay_or_overwrite_an_uncertain_attempt(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            start = out / 'retained-start.json'
            original = b'{"status":"started","original":"retained exact bytes"}\n'
            start.write_bytes(original)
            with patch.object(RUNNER.subprocess, 'Popen') as launch, self.assertRaises(FileExistsError):
                RUNNER.main(['--groups', '', '--out', str(out), 'retained', sys.executable, '-c', 'pass'])
            launch.assert_not_called()
            self.assertEqual(start.read_bytes(), original)
            self.assertEqual(sorted(path.name for path in out.iterdir()), ['retained-start.json'])

    def test_launch_failure_and_postprocess_failure_retain_nonpass_dispositions(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            with contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(['--groups', '', '--out', str(out), 'launch-failed', str(out / 'missing-command')])
            self.assertEqual(status, 127)
            receipt = json.loads((out / 'launch-failed.json').read_text())
            self.assertEqual(receipt['status'], 'launch_failed')
            self.assertFalse(receipt['passed'])
            self.assertEqual(receipt['execution_error']['type'], 'FileNotFoundError')
            self.assertIsNone(receipt['exit_code'])
            with patch.object(RUNNER, 'observe', side_effect=[{}, PermissionError('retained final evidence refusal')]), \
                    contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(['--groups', '', '--out', str(out), 'evidence-failed', sys.executable, '-c', 'pass'])
            self.assertEqual(status, 5)
            receipt = json.loads((out / 'evidence-failed.json').read_text())
            self.assertEqual(receipt['status'], 'evidence_failed')
            self.assertFalse(receipt['passed'])
            self.assertIsNone(receipt['inputs_unchanged'])
            self.assertEqual(receipt['child']['returncode'], 0)
            self.assertEqual(receipt['evidence_errors'][0]['operation'], 'input_groups')
            self.assertEqual(receipt['evidence_errors'][0]['type'], 'PermissionError')

    def run_runner(self, out, *command, extra=()):
        argv = [sys.executable, str(ROOT / "scripts/dev/receipt_runner.py"), "--groups", "fixtures",
                "--out", str(out), *extra, *command]
        return subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=300)

    def test_writes_receipt_and_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            done = self.run_runner(out, "plain-run", sys.executable, "-c", "print('hello')")
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
            receipt = json.loads((out / "plain-run.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt["schema"], RUNNER.SCHEMA)
            self.assertTrue(receipt["inputs_unchanged"])
            self.assertIn("fixtures", receipt["groups"])
            self.assertIn("hello", (out / "plain-run.log").read_text(encoding="utf-8"))
            self.assertTrue((out / "plain-run-runner.py").is_file())

    def test_empty_test_selection_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            empty_source = out / "empty-tests"
            empty_source.mkdir()
            # Real unittest discovery exercises the guard without synthetic markers.
            empty = (sys.executable, "-B", "-m", "unittest", "discover", "-s", str(empty_source))
            # Python 3.13+ itself exits 5 for no tests; earlier versions exit 0.
            # Preserve that observed child failure instead of replacing it with 4.
            baseline = subprocess.run(empty, cwd=ROOT, capture_output=True, text=True, timeout=30)
            self.assertIn(baseline.returncode, (0, 5), baseline.stdout + baseline.stderr)
            done = self.run_runner(out, "empty-selection", *empty)
            self.assertEqual(done.returncode, baseline.returncode or 4, done.stdout + done.stderr)
            self.assertEqual(json.loads((out / "empty-selection.json").read_text())["selected_tests"], 0)
            allowed = self.run_runner(out, "empty-allowed", *empty, extra=("--allow-no-tests",))
            self.assertEqual(allowed.returncode, baseline.returncode, allowed.stdout + allowed.stderr)

    def test_command_failure_is_propagated(self):
        with tempfile.TemporaryDirectory() as temporary:
            done = self.run_runner(Path(temporary), "failing", sys.executable, "-c", "raise SystemExit(7)")
            self.assertEqual(done.returncode, 7)

    def test_junit_requires_nextest_in_this_commands_log(self):
        for name, summary, expected in (
            ("python-report", "Ran 1 tests in 0.001s", False),
            ("libtest-report", "running 1 test", False),
            ("nextest-report", "Starting 1 test across 1 binary", True),
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                out = Path(temporary)
                target = out / "target"
                junit = target / "nextest/ci/junit.xml"
                # A concurrent Cargo owner can write a fresh report during an
                # unrelated Python check. Freshness alone proves no provenance.
                script = ("from pathlib import Path; import sys; "
                          "p=Path(sys.argv[1]); p.parent.mkdir(parents=True); "
                          "p.write_text('<testsuites/>'); print(sys.argv[2])")
                done = self.run_runner(out, name, sys.executable, "-c", script, str(junit), summary,
                                       extra=("--groups", "", "--target-dir", str(target)))
                self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
                receipt = json.loads((out / (name + ".json")).read_text())
                self.assertEqual("junit" in receipt, expected)
                self.assertEqual((out / (name + "-junit.xml")).exists(), expected)
                if expected:
                    self.assertEqual(receipt["junit"]["sha256"], RUNNER.digest(junit))



class CargoTimingAttribution(unittest.TestCase):
    def test_direct_cargo_requires_requested_timings(self):
        for command in (["cargo", "build", "--timings"], ["cargo.exe", "nextest", "run", "--timings"],
                        ["cargo", "clippy", "--timings=html"]):
            self.assertTrue(RUNNER.cargo_timing_command(command, ROOT), command)
        for command in (["cargo", "metadata"], ["cargo", "fmt", "--all"], []):
            self.assertFalse(RUNNER.cargo_timing_command(command, ROOT), command)

    def test_owned_maintained_build_wrappers_keep_indirect_timings(self):
        commands = [
            [sys.executable, "scripts/build-service-runtime.py", "--destination", "artifacts/runtime"],
            [sys.executable, "-I", "-B", "scripts/build-service-container.py", "--destination", "artifacts/container"],
            [sys.executable, "scripts/dev-check.py", "-p", "airkctl"],
            [sys.executable, "scripts/build-local-service-installer.py", "--force-build"],
            ["pwsh", "-NoProfile", "-File", "scripts/build-local-service-installer.ps1", "-ForceBuild"],
            ["pwsh", "-NoProfile", "-File", "scripts/build-local-installers.ps1", "-ForceBuild"],
            [sys.executable, "scripts/static-runtime-environment.py", "--environment-file", "verified.json", "--",
                sys.executable, "scripts/build-service-runtime.py", "--destination", "artifacts/runtime"],
        ]
        for command in commands:
            with self.subTest(command=command):
                self.assertTrue(RUNNER.cargo_timing_command(command, ROOT))

    def test_unrelated_or_nonbuilding_modes_cannot_adopt_another_cargo_report(self):
        commands = [
            [sys.executable, "-c", "print('cargo build --timings')"],
            [sys.executable, "scripts/tests/test_local_wrappers.py"],
            [sys.executable, str(ROOT.parent / "other/scripts/build-service-runtime.py")],
            ["node", "--test", "some.test.cjs"],
            ["pwsh", "-Command", "cargo build --timings"],
            [sys.executable, "scripts/build-local-installers.ps1", "-ForceBuild"],
            ["pwsh", "-File", "scripts/build-service-runtime.py", "--destination", "artifacts/runtime"],
            [sys.executable, "scripts/dev-check.py", "-p", "airkctl", "--dry-run"],
            [sys.executable, "scripts/build-local-service-installer.py", "--runtime-dir", "tested", "--admin-inventory", "checked.json"],
            ["pwsh", "-File", "scripts/build-local-service-installer.ps1", "-RuntimeDir", "tested", "-AdminInventory", "checked.json"],
            [sys.executable, "scripts/static-runtime-environment.py", "--environment-file", "verified.json"],
            [sys.executable, "scripts/static-runtime-environment.py", "--environment-file", "verified.json", "--", sys.executable, "-c", "pass"],
        ]
        for command in commands:
            with self.subTest(command=command):
                self.assertFalse(RUNNER.cargo_timing_command(command, ROOT))

    def timing_receipt(self, command, *, banner=True, outside=False, stale=False, expected_status=0, summary=None, allow_no_tests=False):
        # A fake child creates this test's private artifacts and Cargo banner.
        # The newest alias deliberately belongs to another owner. No Cargo runs.
        class Child:
            pid = 12345
            returncode = 0
            def wait(self): return 0
            def poll(self): return 0
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            target = out / "target"
            timing = target / "cargo-timings/cargo-timing-observed.html"
            alias = timing.with_name("cargo-timing.html")
            def launch(*args, **kwargs):
                timing.parent.mkdir(parents=True)
                timing.write_bytes(b"the command's actual saved Cargo report")
                RUNNER.os.utime(timing, (999 if stale else 1001, 999 if stale else 1001))
                alias.write_bytes(b"newer incidental alias from another Cargo owner")
                RUNNER.os.utime(alias, (1002, 1002))
                reported = timing
                if outside:
                    reported = out / "other-target/cargo-timings/cargo-timing-observed.html"
                    reported.parent.mkdir(parents=True)
                    reported.write_bytes(b"unselected target report")
                    RUNNER.os.utime(reported, (1001, 1001))
                if banner:
                    kwargs["stdout"].write(f"      Timing report saved to {reported}\n".encode())
                if summary is not None:
                    kwargs["stdout"].write((summary + "\n").encode())
                return Child()
            with patch.object(RUNNER, "ROOT", ROOT), patch.object(RUNNER.subprocess, "Popen", side_effect=launch), \
                    patch.object(RUNNER.time, "time", return_value=1000), contextlib.redirect_stdout(io.StringIO()):
                status = RUNNER.main(["--groups", "", "--out", str(out), "--target-dir", str(target), *(["--allow-no-tests"] if allow_no_tests else []), "timing-owner", *command])
            self.assertEqual(status, expected_status)
            receipt = json.loads((out / "timing-owner.json").read_text(encoding="utf-8"))
            copied = out / "timing-owner-cargo-timing.html"
            return receipt, copied.read_bytes() if copied.exists() else None

    def test_compile_target_receipt_passes_without_inventing_test_selections(self):
        for subcommand in ("clippy", "check", "build"):
            with self.subTest(subcommand=subcommand):
                receipt, copied = self.timing_receipt(["cargo", subcommand, "-p", "airk-cli", "--test", "admin_plane", "--all-features", "--locked", "--timings", "--", "-D", "warnings"])
                self.assertTrue(receipt["passed"])
                self.assertEqual(receipt["exit_code"], 0)
                self.assertEqual(receipt["runner_exit_code"], 0)
                self.assertIsNone(receipt["selected_tests"])
                self.assertEqual(copied, b"the command's actual saved Cargo report")

    def test_real_test_command_without_selection_still_refuses_receipt(self):
        for command in (["cargo", "test", "--test", "admin_plane", "--timings"],
                        ["cargo", "nextest", "run", "--timings"], ["node", "--test", "a.test.js"],
                        ["python", "-m", "unittest", "discover"]):
            with self.subTest(command=command):
                receipt, _ = self.timing_receipt(command, expected_status=4)
                self.assertFalse(receipt["passed"])
                self.assertEqual(receipt["exit_code"], 0)
                self.assertEqual(receipt["runner_exit_code"], 4)
                self.assertIsNone(receipt["selected_tests"])

    def test_successful_test_runner_zero_summary_requires_explicit_allowance(self):
        for command, summary in ((["cargo", "test", "--test", "admin_plane"], "running 0 tests"),
                                 (["cargo", "nextest", "run"], "Starting 0 tests across 1 binary"),
                                 (["node", "--test"], "ℹ tests 0"),
                                 (["python", "-m", "unittest"], "Ran 0 tests in 0.001s")):
            with self.subTest(command=command):
                refused, _ = self.timing_receipt(command, summary=summary, expected_status=4)
                self.assertEqual(refused["selected_tests"], 0)
                self.assertFalse(refused["passed"])
                allowed, _ = self.timing_receipt(command, summary=summary, allow_no_tests=True)
                self.assertEqual(allowed["selected_tests"], 0)
                self.assertTrue(allowed["passed"])

    def test_fresh_incidental_report_is_not_copied_for_python_or_node(self):
        for command in ([sys.executable, "-c", "pass"], ["node", "a-script.cjs"]):
            with self.subTest(command=command):
                receipt, copied = self.timing_receipt(command)
                self.assertNotIn("cargo_timings", receipt)
                self.assertIsNone(copied)
                self.assertEqual(receipt["status"], "completed")
                self.assertTrue(receipt["passed"])

    def test_exact_logged_report_survives_a_newer_concurrent_alias_for_cargo_and_builder(self):
        for command in (["cargo", "build", "--timings"], [sys.executable, "scripts/build-service-runtime.py", "--destination", "artifacts/runtime"]):
            with self.subTest(command=command):
                receipt, copied = self.timing_receipt(command)
                self.assertEqual(receipt["cargo_timings"], "timing-owner-cargo-timing.html")
                self.assertEqual(copied, b"the command's actual saved Cargo report")

    def test_capable_wrapper_without_actual_cargo_banner_never_claims_incidental_timings(self):
        receipt, copied = self.timing_receipt([sys.executable, "scripts/dev-check.py", "-p", "airkctl"], banner=False)
        self.assertNotIn("cargo_timings", receipt)
        self.assertIsNone(copied)

    def test_logged_report_from_another_target_is_not_this_receipts_evidence(self):
        receipt, copied = self.timing_receipt(["cargo", "build", "--timings"], outside=True)
        self.assertNotIn("cargo_timings", receipt)
        self.assertIsNone(copied)

    def test_stale_logged_report_cannot_be_replaced_by_a_fresh_incidental_alias(self):
        receipt, copied = self.timing_receipt(["cargo", "build", "--timings"], stale=True)
        self.assertNotIn("cargo_timings", receipt)
        self.assertIsNone(copied)


if __name__ == "__main__":
    unittest.main()
