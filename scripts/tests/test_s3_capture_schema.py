# SPDX-License-Identifier: Apache-2.0
"""Pure parts of scripts/dev/s3_capture_schema.py; no git worktree or Cargo run."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "s3_capture_schema", Path(__file__).resolve().parents[1] / "dev" / "s3_capture_schema.py"
)
S3 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S3)

REVISION = "0123456789abcdef0123456789abcdef01234567"


def row(kind, name, table, sql):
    return {"type": kind, "name": name, "tbl_name": table, "sql": sql}


def raw(label, objects=None, user_version=12, application_id=1095324235, sqlite_version="3.50.0"):
    return {"label": label, "user_version": user_version, "application_id": application_id,
            "sqlite_version": sqlite_version,
            "objects": objects if objects is not None else [row("table", "t", "t", "CREATE TABLE t(x)")]}


def raws_for(groups, **overrides):
    return {label: raw(label, **overrides) for label, _ in S3.labels(groups)}


class ObjectsDocumentTests(unittest.TestCase):
    def test_sorts_by_type_then_name_then_table_in_code_point_order(self):
        rows = [
            row("trigger", "b_capacity", "b", "CREATE TRIGGER b_capacity ..."),
            row("table", "b", "b", "CREATE TABLE b(x)"),
            row("index", "sqlite_autoindex_a_1", "a", None),
            row("table", "a", "a", "CREATE TABLE a(x)"),
            row("table", "B", "B", "CREATE TABLE B(x)"),
            row("table", "_z", "_z", "CREATE TABLE _z(x)"),
        ]
        names = [item["name"] for item in S3.objects_document(rows)]
        self.assertEqual(names, ["sqlite_autoindex_a_1", "B", "_z", "a", "b", "b_capacity"])

    def test_ordering_is_independent_of_input_order(self):
        rows = [row("table", name, name, "CREATE TABLE %s(x)" % name) for name in ("c", "a", "b")]
        self.assertEqual(S3.objects_document(rows), S3.objects_document(list(reversed(rows))))

    def test_only_crlf_in_sql_is_normalized(self):
        rows = [row("table", "t", "t", "CREATE TABLE t(\r\n  x,\r  y\n)\r\n"),
                row("index", "sqlite_autoindex_t_1", "t", None)]
        document = {item["name"]: item for item in S3.objects_document(rows)}
        self.assertEqual(document["t"]["sql"], "CREATE TABLE t(\n  x,\r  y\n)\n")
        self.assertIsNone(document["sqlite_autoindex_t_1"]["sql"])
        self.assertEqual(S3.objects_document([row("table", "t\r\n", "t", "x")])[0]["name"], "t\r\n")

    def test_rejects_duplicate_names_unknown_fields_and_non_text(self):
        with self.assertRaises(S3.CaptureError):
            S3.objects_document([row("table", "t", "t", "a"), row("index", "t", "t", "b")])
        with self.assertRaises(S3.CaptureError):
            S3.objects_document([dict(row("table", "t", "t", "a"), rootpage=2)])
        with self.assertRaises(S3.CaptureError):
            S3.objects_document([row("table", 1, "t", "a")])
        with self.assertRaises(S3.CaptureError):
            S3.objects_document([row("table", "t", "t", b"CREATE")])


class RenderingTests(unittest.TestCase):
    def test_json_rendering_is_lf_utf8_sorted_and_newline_terminated(self):
        data = S3.render_json({"b": "é\nx", "a": [1, None]})
        self.assertEqual(data, '{\n  "a": [\n    1,\n    null\n  ],\n  "b": "é\\nx"\n}\n'.encode("utf-8"))
        self.assertNotIn(b"\r", data)

    def test_meta_keeps_only_integer_pragmas(self):
        self.assertEqual(S3.meta_document(raw("x", user_version=13, application_id=0)),
                         {"user_version": 13, "application_id": 0})
        for bad in ({"user_version": "12", "application_id": 0}, {"user_version": True, "application_id": 0},
                    {"application_id": 0}):
            with self.assertRaises(S3.CaptureError):
                S3.meta_document(bad)

    def test_render_produces_objects_meta_and_manifest_for_every_label(self):
        groups = list(S3.GROUP_ORDER)
        files = S3.render(raws_for(groups), groups, REVISION, "1.8.16")
        expected = {label + suffix for label, _ in S3.labels(groups) for suffix in (".objects.json", ".meta.json")}
        self.assertEqual(set(files), expected | {"manifest.json"})
        manifest = json.loads(files["manifest.json"])
        self.assertEqual((manifest["source_revision"], manifest["source_version"], manifest["producer_sqlite_version"]),
                         (REVISION, "1.8.16", "3.50.0"))
        self.assertEqual(manifest["groups"], groups)
        self.assertEqual(json.loads(files["service-1.4.policy3-armed.meta.json"]),
                         {"application_id": 1095324235, "user_version": 12})

    def test_render_refuses_missing_extra_mislabelled_or_mixed_sqlite_dumps(self):
        groups = ["current"]
        complete = raws_for(groups)
        with self.assertRaises(S3.CaptureError):
            S3.render({"current.service": complete["current.service"]}, groups, REVISION, "1.8.16")
        with self.assertRaises(S3.CaptureError):
            S3.render(dict(complete, extra=raw("extra")), groups, REVISION, "1.8.16")
        with self.assertRaises(S3.CaptureError):
            S3.render(dict(complete, **{"current.service": raw("current.standalone")}), groups, REVISION, "1.8.16")
        with self.assertRaises(S3.CaptureError):
            S3.render(dict(complete, **{"current.service": raw("current.service", sqlite_version="3.1")}),
                      groups, REVISION, "1.8.16")
        with self.assertRaises(S3.CaptureError):
            S3.render(dict(complete, **{"current.service": raw("current.service", objects=[])}),
                      groups, REVISION, "1.8.16")

    def test_load_and_diff_detect_changed_missing_and_unexpected_files(self):
        groups = ["collection"]
        files = S3.render(raws_for(groups), groups, REVISION, "1.8.16")
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name, data in files.items():
                (directory / name).write_bytes(data)
            self.assertEqual(S3.diff_files(files, directory), [])
            (directory / "usage.base.meta.json").write_bytes(b'{\r\n}\r\n')
            (directory / "scheduler-1.2.objects.json").unlink()
            (directory / "stale.objects.json").write_bytes(b"[]\n")
            differences = S3.diff_files(files, directory)
            self.assertEqual(len(differences), 3)
            self.assertIn("missing scheduler-1.2.objects.json", differences)
            self.assertIn("unexpected stale.objects.json", differences)
            self.assertTrue(any("recaptured/usage.base.meta.json" in item for item in differences))

            raw_directory = directory / "raw"
            raw_directory.mkdir()
            (raw_directory / "usage.base.raw.json").write_text(json.dumps(raw("usage.base")), encoding="utf-8")
            (raw_directory / "notes.json").write_text("{}", encoding="utf-8")
            self.assertEqual(list(S3.load_raw(raw_directory)), ["usage.base"])


class ArgumentAndPlanTests(unittest.TestCase):
    def test_defaults_and_group_selection(self):
        args = S3.parse_args([])
        self.assertEqual(args.revision, "43d7005")
        self.assertEqual(args.groups, list(S3.GROUP_ORDER))
        self.assertFalse(args.check)
        self.assertIsNone(args.target_dir)
        args = S3.parse_args(["--groups", "collection, registry", "--check", "--target-dir", "t", "--revision", "HEAD"])
        self.assertEqual(args.groups, ["registry", "collection"])
        self.assertTrue(args.check)
        self.assertEqual(args.target_dir, Path("t"))
        for argv in (["--groups", "nope"], ["--groups", ","], ["--check", "--print-container-entries"]):
            with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
                S3.parse_args(argv)

    def test_cargo_command_selects_only_driver_targets_and_keeps_timings(self):
        args = S3.parse_args(["--target-dir", "T"])
        command = S3.cargo_command(args, args.groups, Path("T"))
        self.assertEqual(command[:7], ["cargo", "test", "--locked", "-p", "airk-store", "--target-dir", "T"])
        self.assertIn("--timings", command)
        self.assertEqual(command[-6:], ["--lib", "--test", "s3_capture_schema", "s3_capture_", "--", "--test-threads=1"])
        only = S3.parse_args(["--groups", "collection", "--no-timings", "--offline"])
        command = S3.cargo_command(only, only.groups, Path("T"))
        self.assertNotIn("--lib", command)
        self.assertNotIn("--timings", command)
        self.assertIn("--offline", command)

    def test_selected_test_count_sums_every_binary(self):
        output = "running 3 tests\ntest a ... ok\n\nrunning 1 test\ntest b ... ok\nrunning 0 tests\n"
        self.assertEqual(S3.selected_test_count(output), 4)
        self.assertEqual(S3.selected_test_count("no tests here"), 0)

    def test_driver_plan_uses_the_single_existing_layout_and_refuses_ambiguity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for group in ("registry", "adopted", "current"):
                path = root / S3.TARGETS[group][0]
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"// existing\n")
            (root / "crates/airk-store/tests").mkdir(parents=True)
            plan = S3.driver_plan(root, list(S3.GROUP_ORDER))
            self.assertEqual([(path, mode) for path, mode, _ in plan], [
                (S3.TARGETS["registry"][0], "append"), (S3.TARGETS["adopted"][0], "append"),
                (S3.TARGETS["current"][0], "append"), ("crates/airk-store/tests/s3_capture_schema.rs", "create")])
            for path, _, text in plan:
                self.assertEqual(text.count("fn s3_dump("), 1, path)
            for group, test in S3.TEST_NAMES.items():
                self.assertIn("fn %s()" % test, S3.DRIVERS[group])
                self.assertTrue(test.startswith(S3.TEST_PREFIX))
            S3.apply_plan(root, plan)
            appended = (root / S3.TARGETS["registry"][0]).read_bytes()
            self.assertTrue(appended.startswith(b"// existing\n"))
            self.assertNotIn(b"\r\n", appended)
            self.assertIn(b"fn s3_capture_registry_layers()", appended)
            with self.assertRaises(S3.CaptureError):
                S3.driver_plan(root, ["collection"])
            second = root / S3.TARGETS["current"][1]
            second.parent.mkdir(parents=True, exist_ok=True)
            second.write_text("", encoding="utf-8")
            with self.assertRaises(S3.CaptureError):
                S3.driver_plan(root, ["current"])

    def test_container_entries_list_each_fixture_file_in_every_location(self):
        entries = S3.container_entries(["registry"])
        files = 2 * len(S3.GROUPS["registry"])
        ordered = list(entries.values())
        self.assertEqual(ordered[0], ["!fixtures/schema/", "fixtures/schema/**",
                                      "!fixtures/schema/airk-state-1/", "fixtures/schema/airk-state-1/**"])
        self.assertEqual([len(lines) for lines in ordered[1:]], [files, files, files])
        self.assertEqual(ordered[1], sorted(ordered[1]))
        self.assertIn("COPY fixtures/schema/airk-state-1/service-1.4.schema11.meta.json "
                      "./fixtures/schema/airk-state-1/service-1.4.schema11.meta.json", ordered[2])


if __name__ == "__main__":
    unittest.main()
