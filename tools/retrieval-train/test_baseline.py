# SPDX-License-Identifier: Apache-2.0
"""Independent tiny golds for retrieval metrics and frozen-source refusal."""
import importlib.util
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("retrieval_baseline", Path(__file__).with_name("baseline.py"))
baseline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def test_complementary_role_is_not_satisfied_by_redundant_definition_hits(self):
        result = baseline.assess([0, 1, 2], [{0, 1}, {2}], 2)
        self.assertEqual(result["role_recall"], 0.5)
        self.assertFalse(result["all_roles"])
        self.assertEqual(result["roles_hit"], [True, False])

    def test_late_critical_evidence_does_not_count_beyond_selected_budget(self):
        self.assertEqual(baseline.assess([0, 1, 2], [{2}], 2)["role_recall"], 0)
        self.assertEqual(baseline.assess([0, 1, 2], [{2}], 3)["role_recall"], 1)

    def test_missing_gold_anchor_refuses_instead_of_zero_denominator_success(self):
        task = {"id": "independent", "split": "development",
                "required_evidence": [{"role": "exception", "path": "a.md", "anchor": "missing"}]}
        with self.assertRaisesRegex(ValueError, "missing independent gold"):
            baseline.gold_sets([task], [{"path": "a.md", "text": "ordinary rule"}])
        with self.assertRaisesRegex(ValueError, "empty task"):
            baseline.gold_sets([], [])

    def test_dirty_source_bytes_do_not_satisfy_frozen_revision(self):
        source = {"revision": "a" * 40, "sources": [{"path": "a.md", "sha256": baseline.sha256(b"original")}]}
        with patch.object(baseline, "git_bytes", return_value=b"changed"):
            with self.assertRaisesRegex(ValueError, "frozen corpus source differs"):
                baseline.documents(source)

    def test_source_path_may_not_escape_frozen_tree(self):
        for revision, path in [("HEAD", "a.md"), ("a" * 40, "../outside.md"), ("a" * 40, "/outside.md")]:
            with self.assertRaisesRegex(ValueError, "invalid fixed source identity"):
                baseline.git_bytes(revision, path)

    def test_session_uses_retained_approved_bytes_and_reports_all_model_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = {"fixture": "independent fake manifest; no actual ONNX execution"}
            (root / "model.json").write_text(json.dumps(manifest))
            contents = {"model.onnx": b"approved weight fixture", "tokenizer.json": b'{"tokenizer":"approved"}',
                        "LICENSE": b"independent license fixture"}
            assets = []
            for name, data in contents.items():
                (root / name).write_bytes(data)
                assets.append({"name": name, "bytes": len(data), "sha256": baseline.sha256(data)})
            catalog = json.dumps({"profiles": [{"manifest": manifest, "assets": assets}]}).encode()
            seen = []
            tokenizer = types.SimpleNamespace(no_padding=lambda: None, no_truncation=lambda: None)

            def tokenizer_from_str(value):
                seen.append(("tokenizer", value))
                # Deterministic interleaving: both paths change after validation,
                # before constructing the inference session.
                (root / "model.onnx").write_bytes(b"later unapproved weights")
                (root / "tokenizer.json").write_bytes(b"later unapproved tokenizer")
                return tokenizer

            def session(value, **options):
                seen.append(("model", value))
                return "fake session"

            fake_ort = types.SimpleNamespace(SessionOptions=types.SimpleNamespace,
                ExecutionMode=types.SimpleNamespace(ORT_SEQUENTIAL="sequential"), InferenceSession=session)
            fake_tokens = types.SimpleNamespace(Tokenizer=types.SimpleNamespace(from_str=tokenizer_from_str))
            with patch.dict("sys.modules", {"onnxruntime": fake_ort, "tokenizers": fake_tokens}):
                _, _, loaded, seals = baseline.model_session(root, catalog)
            self.assertEqual(loaded, "fake session")
            self.assertEqual(seen, [("tokenizer", contents["tokenizer.json"].decode()), ("model", contents["model.onnx"])])
            self.assertEqual(set(Path(path).name for path in seals), {"model.json", "model.onnx", "tokenizer.json", "LICENSE"})
            with self.assertRaisesRegex(ValueError, "consumed inputs changed"):
                baseline.check_seals(seals)
            for name, data in contents.items():
                (root / name).write_bytes(data)
            (root / "model.json").write_text('{"changed":"manifest"}')
            with self.assertRaisesRegex(ValueError, "consumed inputs changed"):
                baseline.check_seals(seals)


if __name__ == "__main__":
    unittest.main()
