# SPDX-License-Identifier: Apache-2.0
"""Pure/byte controls only; no Torch/ORT/NN/GPU process."""
from pathlib import Path
import ast,hashlib,json,runpy,tempfile,unittest
HERE=Path(__file__).resolve().parent;OLD=HERE.parents[1]/"visible-prefix-r1"
prepare=runpy.run_path(str(HERE/"prepare_seal.py"));launch=runpy.run_path(str(HERE/"launch.py"))
class Preparation(unittest.TestCase):
    def test_all_python_sources_compile_without_execution(self):
        for path in HERE.glob("*.py"):compile(path.read_bytes(),str(path),"exec")
    def test_exact_documented_identity_provenance_adapter(self):
        audit=json.loads((prepare["HERE"]/"adaptation.json").read_bytes());raw=Path(audit["parent_source"]).read_bytes();self.assertEqual(hashlib.sha256(raw).hexdigest(),audit["parent_sha256"]);text=raw.decode().replace("\r\n","\n")
        for change in audit["changes"]:
            self.assertEqual(text.count(change["old"]),1);text=text.replace(change["old"],change["new"])
        self.assertEqual(text,(HERE/"runner.py").read_text(encoding="utf-8"));self.assertEqual(hashlib.sha256((HERE/"runner.py").read_bytes()).hexdigest(),audit["candidate_sha256"])
    def test_numeric_and_train_only_helpers_are_byte_exact(self):
        for name in ["safe_int8.py","training_recipe.py","training_recipe_audit.py","verify_encoder_inputs.py","verify_encoder_fp32.py"]:self.assertEqual((HERE/name).read_bytes(),(OLD/name).read_bytes(),name)
    def test_rust_owner_is_separate_and_pins_actual_stage_before_use(self):
        text=(HERE/"native_parent_bag.rs.in").read_text();self.assertNotIn("mod regenerated_queries;",text);self.assertIn("native-parent-bag-r1-u8",text);self.assertIn("native-parent-bag-r1-s8",text);self.assertIn("AIRK_PARENT_BAG_ENCODER_INPUT_SHA256",text);self.assertNotIn("visible-prefix-mining",text)
        self.assertEqual(text.count("@ORACLE_SHA256@"),1);self.assertEqual(text.count("@U8_ONNX_SHA256@"),1);self.assertIn("all fixed64 original questions required",text);self.assertIn("quality.len() == 20 && refused.len() == 44",text)
    def test_python_does_not_construct_native_manifest(self):
        text=(HERE/"runner.py").read_text(encoding="utf-8");self.assertNotIn("manifest={'schema':2",text);self.assertNotIn("(folder/'model.json').write_text",text);self.assertIn("native_manifest_pending_Rust_owner",text)
    def test_seal_extension_preserves_parent_rows_and_refuses_conflict(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"source";path.write_bytes(b"original");row={"path":str(path.resolve()),"sha256":prepare["sha"](path),"bytes":8}
            self.assertEqual(prepare["add_rows"]([row],[path]),[row]);path.write_bytes(b"changed")
            with self.assertRaises(ValueError):prepare["add_rows"]([row],[path])
    def test_duplicate_parent_pin_refuses(self):
        row={"path":str(HERE/"runner.py"),"sha256":"a"*64,"bytes":1}
        with self.assertRaises(ValueError):prepare["add_rows"]([row,row],[])
    def test_natural_owner_refuses_each_incomplete_state(self):
        row={"exit_code":0,"timed_out":False,"log_oversized":False,"owned_descendants_settled":True,"owned_job_forced_termination":False,"owned_job_final_active_processes":0,"owned_job_final_member_pids":[],"owned_total_processes":2,"unexpected_third_process":False};self.assertTrue(launch["natural"](row))
        for key,bad in {"exit_code":1,"timed_out":True,"log_oversized":True,"owned_descendants_settled":False,"owned_job_forced_termination":True,"owned_job_final_active_processes":1,"owned_job_final_member_pids":[1],"owned_total_processes":3,"unexpected_third_process":True}.items():
            with self.subTest(key=key):self.assertFalse(launch["natural"]({**row,key:bad}))
    def test_checkpoint_and_weak_training_provenance(self):
        value=json.loads((prepare["HERE"]/"provenance.json").read_bytes());self.assertTrue(value["weak_parent_bags"]);self.assertFalse(value["fragment_positive_labels"]);self.assertFalse(value["model_promotion"])
        for name,pin in value["checkpoint"].items():self.assertEqual(hashlib.sha256(Path(name).read_bytes()).hexdigest(),pin)
    def test_cpu_resource_and_original_numeric_floors_are_retained(self):
        text=(HERE/"stage.py").read_text();self.assertIn("CUDA_VISIBLE_DEVICES",text);self.assertIn("len(numerical['cases'])!=12",text);self.assertIn("result['cosine']<.995 or result['l2']>.1",text)
        text=(HERE/"launch.py").read_text();self.assertIn('"whole_seconds":600',text);self.assertIn('"aggregate_host_bytes":8589934592',text);self.assertIn('"job_processes":2',text);self.assertIn('"CUDA_VISIBLE_DEVICES":"-1"',text)
    def test_same_image_full_six_roles_and_native_prerequisite_chain(self):
        grade=runpy.run_path(str(HERE/"grade_dev.py"));self.assertEqual(set(grade["EXPECTED"].values()),{"native-parent-bag-r1-u8","native-parent-bag-r1-s8","original-shipped-u8-ort130-evaluation","original-mixed-u8-ort130-evaluation","original-shipped-s8-ort130-evaluation","original-mixed-s8-ort130-evaluation"})
        native=(HERE/"run_native.py").read_text();self.assertIn("same-image/master complete native numeric proof required before DEV",native);self.assertIn("same-packet complete native emission proof required",native);self.assertIn('"master_sha256":sha(master)',native);self.assertIn('"capture_sha256":sha(output)',native)
    def test_old_experiments_and_final_are_not_called(self):
        for name in ["stage.py","run_native.py","launch.py","grade_dev.py"]:
            text=(HERE/name).read_text();self.assertNotIn("fresh_final",text);self.assertNotIn("native_retrieval_capture_uses_current_kernel_pipeline",text);self.assertNotIn("minilm-visible-prefix-mining-r1/epoch-1",text)
if __name__=="__main__":unittest.main(verbosity=2)
