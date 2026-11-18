# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from closure import checked_json, resolve_sources, source_body
from prepare_packets import partitions, prepare


class Preparation(unittest.TestCase):
    def test_all_twenty_six_reviewed_source_versions_have_exact_bodies(self):
        root=Path(__file__).resolve().parents[4]
        catalogue=json.loads(Path(__file__).with_name("source-selections.json").read_text())
        maps=[json.loads((root/path).read_text()) for path in catalogue["source_maps"]]
        resolved=resolve_sources(catalogue["selections"],maps,root)
        self.assertEqual(len(resolved),26)
        self.assertNotEqual(resolved["cal_native_client"]["sha256"],resolved["a6_native_client"]["sha256"])
        self.assertNotEqual(resolved["cal_native_session"]["sha256"],resolved["a6_native_session"]["sha256"])

    def test_another_experiment_version_is_not_substituted(self):
        catalogue=json.loads(Path(__file__).with_name("source-selections.json").read_text())
        selected=catalogue["selections"]
        selected["cal_native_client"]=selected["a6_native_client"]
        with self.assertRaisesRegex(ValueError,"Section7 source version"):
            prepare({"schema":"airk.selected-goal-preparation-request/1","source_maps":[],"source_selections":selected,"inputs":{}},Path.cwd())

    def test_literal_thirteen_ranges_and_full_order(self):
        ids=["question-"+str(i) for i in range(100)]
        result=partitions(ids)
        self.assertEqual([(r["question_start"],r["question_end"]) for r in result],
                         [(0,8),(8,16),(16,24),(24,32),(32,40),(40,48),(48,56),(56,64),(64,72),(72,80),(80,88),(88,96),(96,100)])
        self.assertEqual([x for r in result for x in r["questions"]],ids)

    def test_missing_duplicate_and_nonstring_ids_refused(self):
        for ids in (["x"]*100, list(map(str,range(99))),list(range(100))):
            with self.assertRaises(ValueError): partitions(ids)

    def test_partial_sources_refused_before_any_input_read(self):
        with self.assertRaisesRegex(ValueError,"helper-role"):
            prepare({"schema":"airk.selected-goal-preparation-request/1","source_maps":[],"source_selections":{},"inputs":{}},Path.cwd())

    def test_exact_source_and_prefix_inverse(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/"scripts").mkdir()
            body=b"print('historical; never execute')\n";prefix=b"# SPDX-License-Identifier: Apache-2.0\n"
            (root/"scripts/old.py").write_bytes(prefix+body)
            row={"source":"private/exact/old.py","destination":"scripts/old.py","original_sha256":hashlib.sha256(body).hexdigest(),
                 "bytes":len(body),"maintained_sha256":hashlib.sha256(prefix+body).hexdigest(),"maintained_bytes":len(prefix+body),
                 "changes":[{"kind":"spdx-prefix","prefix":prefix.decode()}]}
            selected={"role":{"source":row["source"],"original_sha256":row["original_sha256"]}}
            self.assertEqual(resolve_sources(selected,[{"files":[row]}],root)["role"]["bytes"],len(prefix+body))
            selected["role"]["source"]="other/old.py"
            with self.assertRaisesRegex(ValueError,"missing or ambiguous"):resolve_sources(selected,[{"files":[row]}],root)
            with self.assertRaises(ValueError):source_body({**row,"destination":"../old.py"},root)

    def test_duplicate_json_and_changed_hash_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"input.json";raw=b'{"a":1,"a":2}';path.write_bytes(raw)
            ref={"path":str(path),"sha256":hashlib.sha256(raw).hexdigest(),"bytes":len(raw)}
            with self.assertRaisesRegex(ValueError,"duplicate"):checked_json(ref)
            with self.assertRaisesRegex(ValueError,"bytes changed"):checked_json({**ref,"sha256":"0"*64})

    def test_complete_packet_uses_exact_maps_and_reads_census_once(self):
        repository = Path(__file__).resolve().parents[4]
        catalogue = json.loads(Path(__file__).with_name("source-selections.json").read_bytes())
        map_paths = (
            "scripts/arbiter/evaluation/research_history/source-provenance.json",
            "scripts/arbiter/export/history/source-map.json",
            "scripts/arbiter/research/proposals/native-client-r1/source-provenance.json",
            "scripts/arbiter/evaluation/selected_goal/current-owner-map.json",
        )

        def pin(path):
            body = path.read_bytes()
            return {"path": str(path), "sha256": hashlib.sha256(body).hexdigest(), "bytes": len(body)}

        source_maps = [pin(repository / path) for path in map_paths]
        ids = [f"q-{number}" for number in range(100)]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            census = root / "census.json"
            census.write_text(json.dumps({"schema": "airk.selected-goal-census/1", "question_ids": ids}),
                              encoding="utf-8")
            inputs = {"census": pin(census)}
            for role in ("corpus", "source_manifest", "rubric", "task_policy", "profiles",
                         "native_image_receipt", "process_owner_contract"):
                path = root / role
                path.write_bytes(f"opaque bytes for {role}".encode())
                inputs[role] = pin(path)
            request = {
                "schema": "airk.selected-goal-preparation-request/1",
                "source_maps": source_maps,
                "source_selections": catalogue["selections"],
                "inputs": inputs,
            }

            with patch.object(Path, "open", autospec=True, side_effect=Path.open) as opened:
                packet = prepare(request, repository)
            self.assertEqual(sum(call.args[0] == census for call in opened.call_args_list), 1)

            self.assertEqual(packet["schema"], "airk.selected-goal-review-packet/1")
            self.assertEqual(packet["status"], "prepared_for_review")
            self.assertIs(packet["execution_authorized"], False)
            self.assertIs(packet["qualification_granted"], False)
            self.assertIs(packet["operational_adapters_complete"], False)
            self.assertEqual(packet["source_maps"], source_maps)
            self.assertEqual(packet["inputs"], inputs)
            self.assertEqual(set(packet["sources"]), set(catalogue["selections"]))
            self.assertEqual([len(phase["questions"]) for phase in packet["phases"]], [8] * 12 + [4])
            self.assertEqual([item for phase in packet["phases"] for item in phase["questions"]], ids)

            substituted = {**source_maps[0], "path": str(root / Path(source_maps[0]["path"]).name)}
            invalid_rosters = {
                "missing": source_maps[:-1],
                "reordered": list(reversed(source_maps)),
                "substituted": [substituted, *source_maps[1:]],
            }
            for case, maps in invalid_rosters.items():
                with self.subTest(case=case), self.assertRaisesRegex(ValueError, "catalogue order"):
                    prepare({**request, "source_maps": maps}, repository)


if __name__ == "__main__": unittest.main()
