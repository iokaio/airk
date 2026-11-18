# SPDX-License-Identifier: Apache-2.0
"""Inert fresh CUDA CLI controls. Every input and output operation is observed in memory."""

import io
import json
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from scripts.tests import test_cuda_fresh_projection as fixtures

SCRIPTS = Path(__file__).resolve().parents[1]
REPO = SCRIPTS.parent
cli = fixtures.load(SCRIPTS / "provision-cuda-static.py", "fresh_provision_controls")
runtime = fixtures.load(SCRIPTS / "cuda-static-runtime.py", "fresh_provision_runtime_controls")
env = fixtures.load(SCRIPTS / "cuda-runtime-environment.py", "fresh_provision_environment_controls")


class FreshProvision(unittest.TestCase):
    def setUp(self):
        self.root = REPO / "inert-fresh-provision"
        self.receipt = self.root / "fresh-aggregate.json"
        self.target = self.root / "environment.json"
        self.receipt_pin = fixtures.pin(self.receipt, b"reviewed receipt length")
        self.recipe_path = self.root / "configure-recipe.json"
        self.guard_source = SCRIPTS / "cuda-component-execution.py"
        self.recipe = {
            "schema": "airk.private.cuda-fresh-configure-recipe/1",
            "repository_root": str(REPO), "fresh_output": str(self.root),
            "execution_authorized": True, "qualification": False,
            "owners": {"guards": fixtures.pin(self.guard_source)},
            "checked_guard": fixtures.pin(SCRIPTS / "dev/assert-human-execution.ps1"),
            "powershell": fixtures.pin(self.root / "toolchain/pwsh.exe"),
        }
        self.recipe_bytes = json.dumps(self.recipe).encode("utf-8")
        self.result = {
            "schema": "airk.cuda-static-environment/6", "activation_pins_available": False,
            "request": {"destination": str(self.root), "aggregate_receipt": self.receipt_pin},
            "build": {"configure_recipe": fixtures.pin(self.recipe_path, self.recipe_bytes)},
            "aggregate": {"qualification": False}, "environment": {"ORT_LIB_PROFILE": "Release"},
        }
        self.base = [
            "provision", "--destination", str(self.root), "--fresh-aggregate-receipt", str(self.receipt),
            "--receipt-sha256", self.receipt_pin["sha256"],
        ]
        self.clock = 100
        self.deadline = 850
        self.events = []
        self.core = SimpleNamespace(plain=lambda path, **_: Path(path), MAX_JSON=16 << 20)
        self.verify = Mock(side_effect=self.handoff)
        self.guard = Mock(side_effect=self.audit)
        self.owner = Mock(return_value=SimpleNamespace(guard=self.guard))
        self.opened = Mock(side_effect=self.open_path)
        self.loaded = Mock(side_effect={"cuda-static-runtime": runtime, "cuda-runtime-environment": env}.__getitem__)
        self.printed = Mock(side_effect=lambda *_: self.events.append("stdout"))
        self.data = None
        self.fail_write = None

    def handoff(self, *_args, **_kwargs):
        self.events.append("verify")
        return self.result

    def audit(self, plan, path, created=False, *, deadline):
        self.events.append("audit-created" if created else "audit-parent")

    def open_path(self, path, mode):
        if path == self.recipe_path and mode == "rb":
            self.events.append("read-recipe")
            return io.BytesIO(self.recipe_bytes)
        if path != self.target or mode != "xb":
            raise AssertionError(f"unexpected open {path} {mode}")

        test = self

        class Target(io.BytesIO):
            def write(self, data):
                test.events.append("write")
                if test.fail_write is not None:
                    raise test.fail_write
                test.data = data
                return super().write(data)

        self.events.append("open-new")
        return Target()

    def stat(self, path):
        self.assertEqual(path, self.receipt)
        return SimpleNamespace(st_size=self.receipt_pin["bytes"])

    def main(self, argv):
        with patch.object(sys, "argv", argv), patch.object(cli, "load", self.loaded), \
                patch.object(cli.time, "monotonic", side_effect=lambda: self.clock), \
                patch.object(runtime, "load", return_value=self.core), \
                patch.object(env, "fresh_handoff_value", self.verify), \
                patch.object(env, "_fresh_owner", self.owner), \
                patch.object(Path, "stat", autospec=True, side_effect=self.stat), \
                patch.object(Path, "open", autospec=True, side_effect=self.opened), \
                patch("builtins.print", self.printed):
            cli.main()

    def fresh_args(self, *extra):
        return [*self.base, "--verification-seconds", "750", *extra]

    def test_stdout_runs_single_verified_handoff_with_original_deadline(self):
        self.main(self.fresh_args())
        self.verify.assert_called_once_with(self.root, self.receipt_pin, REPO, deadline=self.deadline)
        self.opened.assert_not_called()
        self.owner.assert_not_called()
        self.assertEqual(self.events, ["verify", "stdout"])
        self.assertEqual(json.loads(self.printed.call_args.args[0]), self.result)

    def test_refuses_legacy_mix_and_fresh_mutating_modes_before_loading(self):
        for extra in (
            ("--producer-file", str(self.root / "old.py")),
            ("--incremental-proposal", str(self.root / "other")),
            ("--aggregate",),
            ("--stage-providers", str(self.root)),
        ):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                self.main(self.fresh_args(*extra))
        self.loaded.assert_not_called()
        self.verify.assert_not_called()

    def test_requires_positive_finite_duration_and_explicit_fresh_mode(self):
        variants = [self.base, *([*self.base, "--verification-seconds", duration] for duration in
                                 ("0", "-1", "inf", "nan", "1e-300"))]
        variants.append(["provision", "--destination", str(self.root), "--receipt-sha256",
                         self.receipt_pin["sha256"], "--verification-seconds", "1"])
        for argv in variants:
            with self.subTest(argv=argv), self.assertRaisesRegex(ValueError, "verification"):
                self.main(argv)
        self.loaded.assert_not_called()

    def test_refuses_wrong_receipt_child_or_digest_before_opening(self):
        for flag, value in (
            ("--fresh-aggregate-receipt", str(self.root / "different.json")),
            ("--receipt-sha256", "not-a-reviewed-digest"),
            ("--destination", "relative-build"),
        ):
            with self.subTest(flag=flag), self.assertRaisesRegex(ValueError, "absolute fresh build"):
                self.main(self.fresh_args(flag, value))
        self.opened.assert_not_called()
        self.verify.assert_not_called()

    def test_writes_exclusively_with_exact_recipe_guard_and_post_audit_before_stdout(self):
        self.main(self.fresh_args("--write-environment", str(self.target)))
        self.verify.assert_called_once_with(self.root, self.receipt_pin, REPO, deadline=self.deadline)
        self.owner.assert_called_once_with(self.guard_source, self.recipe["owners"]["guards"],
                                           "fresh_handoff_output_guard", self.core, runtime, self.deadline)
        self.assertEqual(self.events, ["verify", "read-recipe", "audit-parent", "open-new", "write",
                                       "audit-created", "stdout"])
        self.assertEqual([call.args for call in self.guard.call_args_list],
                         [(self.recipe, self.target), (self.recipe, self.target, True)])
        self.assertEqual([call.kwargs for call in self.guard.call_args_list],
                         [{"deadline": self.deadline}] * 2)
        self.assertEqual(self.data, (self.printed.call_args.args[0] + "\n").encode("utf-8"))

    def test_refuses_output_outside_verified_root_and_recipe_drift_before_writing(self):
        for target in (self.root / "nested/environment.json", REPO / "elsewhere.json"):
            with self.subTest(target=target), self.assertRaisesRegex(ValueError, "direct child"):
                self.main(self.fresh_args("--write-environment", str(target)))
        self.opened.assert_not_called()
        self.recipe_bytes += b"\n"
        with self.assertRaisesRegex(ValueError, "authenticated fresh configure recipe bytes differ"):
            self.main(self.fresh_args("--write-environment", str(self.target)))
        self.assertEqual(self.opened.call_args.args, (self.recipe_path, "rb"))
        self.owner.assert_not_called()
        self.printed.assert_not_called()

    def test_refuses_overwrite_and_failed_audit_without_success_stdout(self):
        def existing(path, mode):
            if mode == "xb":
                raise FileExistsError("existing original")
            return self.open_path(path, mode)

        self.opened.side_effect = existing
        with self.assertRaises(FileExistsError):
            self.main(self.fresh_args("--write-environment", str(self.target)))
        self.assertEqual(self.events, ["verify", "read-recipe", "audit-parent"])
        self.printed.assert_not_called()
        self.opened.side_effect = self.open_path

        def failed_post(plan, path, created=False, *, deadline):
            self.audit(plan, path, created, deadline=deadline)
            if created:
                raise ValueError("created ownership audit refused")

        self.guard.side_effect = failed_post
        with self.assertRaisesRegex(ValueError, "created ownership audit refused"):
            self.main(self.fresh_args("--write-environment", str(self.target)))
        self.assertIn("audit-created", self.events)
        self.printed.assert_not_called()

    def test_late_verifier_refuses_before_write_and_failed_write_is_still_audited(self):
        def late(*args, **kwargs):
            self.clock = self.deadline
            return self.handoff(*args, **kwargs)

        self.verify.side_effect = late
        with self.assertRaises(TimeoutError):
            self.main(self.fresh_args("--write-environment", str(self.target)))
        self.opened.assert_not_called()
        self.clock = 100
        self.verify.side_effect = self.handoff
        self.fail_write = OSError("write failed")
        with self.assertRaisesRegex(OSError, "write failed"):
            self.main(self.fresh_args("--write-environment", str(self.target)))
        self.assertIn("audit-created", self.events)
        self.printed.assert_not_called()


if __name__ == "__main__":
    unittest.main()
