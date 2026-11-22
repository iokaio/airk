# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for cuda_pair.py and the two-entry mode of gpu_fixture.py.

No service, GPU or private input is used: every profile, asset and receipt is
written to a temporary directory.
"""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import cuda_pair as cp
import gpu_fixture as gf


def write(path, value):
    data = (json.dumps(value) + "\n").encode()
    Path(path).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def digest(byte):
    return format(byte, "02x") * 32


class Synthetic:
    """One synthetic profile with three assets, an asset map and a pass-1 review."""

    def __init__(self, root, name, model, manifest_byte):
        self.root = root
        files = root / f"{name}-sources"
        files.mkdir()
        assets, mapping = [], {}
        for asset_name, role, body in (("weights.bin", "external_weights", b"w" * 4096),
                                       ("numeric-fixture.json", "fixture", b"{}\n"),
                                       ("compiler.json", "compiler", b"{\"c\":1}\n")):
            path = files / asset_name
            path.write_bytes(body)
            assets.append({"name": asset_name, "role": role, "bytes": len(body),
                           "sha256": hashlib.sha256(body).hexdigest()})
            mapping[asset_name] = str(path)
        device = {"backend": "cuda", "graph": digest(1), "precision": "float16",
                  "placement": {"mode": "bounded_hybrid", "policy": digest(9)},
                  "memory": {"kind": "dedicated", "required_peak_bytes": 8}}
        self.profile_value = {"profile_id": f"private-{name}", "model": model, "manifest": digest(manifest_byte),
                              "assets": assets, "device_execution": device}
        self.profile = root / f"{name}-profile.json"
        self.profile_sha = write(self.profile, self.profile_value)
        self.map = root / f"{name}-map.json"
        self.map_sha = write(self.map, mapping)
        pass1 = dict(device, placement={"mode": "bounded_hybrid", "policy": digest(0)},
                     memory={"kind": "dedicated", "required_peak_bytes": 1})
        self.pass1_sha = digest(4)
        self.review_value = {"schema": "airk.private.l5a-gpu/1", "step": "graph", "complete": True,
                             "cpu_payload_check": None, "bounded_policy": digest(9),
                             "adapter_before": {"uuid": list(range(1, 17))},
                             "profile": {"id": f"private-{name}", "model": model, "sha256": self.pass1_sha,
                                         "device_execution": pass1}}
        self.review = root / f"{name}-review.json"
        self.review_sha = write(self.review, self.review_value)

    def entry(self, assets):
        return {"profile": str(self.profile), "profile_sha256": self.profile_sha, "assets": str(assets),
                "review": str(self.review), "review_sha256": self.review_sha,
                "pass1_profile_sha256": self.pass1_sha}


class PairTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.saved = cp.ARTIFACTS
        cp.ARTIFACTS = self.root
        self.compact = Synthetic(self.root, "compact", "arbiter-context-qwen3-17b", 5)
        self.large = Synthetic(self.root, "large", "arbiter-context-qwen3-4b", 6)

    def tearDown(self):
        cp.ARTIFACTS = self.saved
        self.temp.cleanup()

    def assets(self, synthetic, name):
        out = self.root / name
        receipt, _ = cp.build_assets(synthetic.profile, synthetic.profile_sha, synthetic.map, synthetic.map_sha,
                                     out, artifacts=self.root)
        return out, receipt

    def test_assets_are_hard_links_of_authenticated_sources(self):
        out, receipt = self.assets(self.compact, "compact-assets")
        self.assertEqual(sorted(p.name for p in out.iterdir()), ["compiler.json", "numeric-fixture.json", "weights.bin"])
        self.assertEqual(receipt["copied_bytes"], 0)
        for row in receipt["assets"]:
            self.assertTrue(Path(row["link"]).samefile(row["source"]))
        self.assertTrue((self.root / "compact-assets.receipt.json").is_file())
        # A second build into the same directory is refused.
        with self.assertRaises(ValueError):
            self.assets(self.compact, "compact-assets")

    def test_a_changed_source_is_refused_before_any_link(self):
        (self.root / "compact-sources" / "weights.bin").write_bytes(b"x" * 4096)
        with self.assertRaises(ValueError):
            self.assets(self.compact, "bad-assets")
        self.assertFalse((self.root / "bad-assets").exists())

    def test_pair_pins_roster_reviews_adapter_and_rows(self):
        compact, _ = self.assets(self.compact, "compact-assets")
        large, _ = self.assets(self.large, "large-assets")
        pair, _ = cp.build_pair({"context-compact-fp16-cuda": self.compact.entry(compact),
                                 "context-large-fp16-cuda": self.large.entry(large)},
                                {"focus-w8-1024-cuda": {"stage": "focus", "manifest": digest(7)}},
                                self.root / "pair")
        roster = json.loads((self.root / "pair" / "roster.json").read_text())
        self.assertEqual(len(roster), 2)
        self.assertTrue(all(row["fixtures"].endswith("numeric-fixture.json") for row in roster))
        self.assertEqual([r["profile_sha256"] for r in pair["reviews"]], [self.compact.pass1_sha] * 2)
        self.assertEqual(pair["adapter_uuid"], list(range(1, 17)))
        self.assertEqual(pair["profiles"]["context-large-fp16-cuda"]["manifest"], digest(6))
        self.assertEqual(pair["profiles"]["focus-w8-1024-cuda"]["stage"], "focus")

    def test_failed_foreign_or_doubled_entries_are_refused(self):
        compact, _ = self.assets(self.compact, "compact-assets")
        failed = dict(self.compact.review_value, cpu_payload_check="over cap")
        self.compact.review_sha = write(self.compact.review, failed)
        with self.assertRaises(ValueError):
            cp.build_pair({"a": self.compact.entry(compact)}, {}, self.root / "p1")
        self.compact.review_sha = write(self.compact.review, self.compact.review_value)
        foreign = self.compact.entry(compact) | {"pass1_profile_sha256": digest(8)}
        with self.assertRaises(ValueError):
            cp.build_pair({"a": foreign}, {}, self.root / "p2")
        with self.assertRaises(ValueError):
            cp.build_pair({"a": self.compact.entry(compact), "b": self.compact.entry(compact)}, {}, self.root / "p3")


class PairEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        roster_sha = write(self.root / "roster.json", [{"profile": "p"}, {"profile": "q"}])
        reviews = []
        for name in ("c", "l"):
            reviews.append({"path": str(self.root / f"{name}.json"), "sha256": write(self.root / f"{name}.json", {}),
                            "profile_sha256": digest(4)})
        self.pair = {"schema": gf.PAIR_SCHEMA, "roster": {"path": str(self.root / "roster.json"),
                                                          "sha256": roster_sha},
                     "reviews": reviews, "adapter_uuid": list(range(1, 17)),
                     "profiles": {"context-compact-fp16-cuda": {"stage": "context", "device": "cuda",
                                                                "manifest": digest(5), "model": "compact"},
                                  "context-large-fp16-cuda": {"stage": "context", "device": "cuda",
                                                              "manifest": digest(6), "model": "large"},
                                  "focus-w8-1024-cuda": {"stage": "focus", "device": "cuda", "manifest": digest(7)}}}
        self.pair_sha = write(self.root / "pair.json", self.pair)

    def tearDown(self):
        self.temp.cleanup()

    def test_pair_variables_reach_the_service_environment(self):
        env = gf.pair_environment(self.root / "pair.json", self.pair_sha, "focus")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256"], self.pair["roster"]["sha256"])
        self.assertEqual(json.loads(env["AIRK_ARBITER_EVALUATION_CUDA_REVIEWS"]), self.pair["reviews"])
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU"], "focus")
        self.assertNotIn("AIRK_ARBITER_EVALUATION_CUDA_GRAPH", env)

    def test_changed_inputs_refuse(self):
        with self.assertRaises(ValueError):
            gf.pair_environment(self.root / "pair.json", digest(1))
        (self.root / "roster.json").write_text("[]\n")
        with self.assertRaises(ValueError):
            gf.pair_environment(self.root / "pair.json", self.pair_sha)

    def test_stage_names_resolve_from_the_lifetime_rows(self):
        life = self.root / "L1"
        life.mkdir()
        write(life / "gpu.json", {"cuda_rows": self.pair["profiles"]})
        rows = gf.lifetime_rows(life)
        # Both Context entries and the CUDA Focus row are stageable by name in one lifetime,
        # so run_plan.py switch_context can disable one and enable the other.
        self.assertEqual(set(rows), set(self.pair["profiles"]))
        self.assertEqual(rows["focus-w8-1024-cuda"], {"stage": "focus", "device": "cuda", "manifest": digest(7)})
        self.assertEqual(gf.lifetime_rows(self.root / "missing"), {})


if __name__ == "__main__":
    unittest.main()
