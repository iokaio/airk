# SPDX-License-Identifier: Apache-2.0
"""Prepare exact current-baseline consumers privately; historical legal archive stays unchanged."""
from pathlib import Path
import difflib,hashlib,json
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def once(text,old,new):
    if text.count(old)!=1:raise ValueError("nonunique consumer correction: "+old[:80])
    return text.replace(old,new)

def main():
    rows={};before={}
    def source(path):before[path]=(ROOT/path).read_text();return before[path]
    path="crates/airk-host/build.rs";text=source(path)
    text=text.replace("minilm-l6-v2-u8-v2","minilm-l6-v2-u8-v3").replace("minilm-l6-v2-s8-v2","minilm-l6-v2-s8-v3")
    text=text.replace('=> "mixed-u8"','=> "uint8"').replace('=> "mixed-s8"','=> "int8"')
    text=once(text,'''        assert!(
            profile["quantization"]["fp32_embeddings_retained"]
                .as_bool()
                .unwrap()
        );''','''        assert!(profile.get("quantization").is_none_or(|value|value.is_null()),"original graph must not inherit trained mixed policy");''')
    text=text.replace("Prepare BOTH trained mixed-INT8 models","Prepare BOTH exact original INT8 models")
    rows[path]=text
    path="crates/airk-host/src/bundled_assets.rs";rows[path]=source(path).replace("minilm-l6-v2-u8-v2","minilm-l6-v2-u8-v3").replace("minilm-l6-v2-s8-v2","minilm-l6-v2-s8-v3")
    path="crates/airk-retrieval/src/model_catalog/policy_cases.rs";source(path);rows[path]=(HERE/"policy_cases.rs.proposed").read_text()
    path="crates/airk-retrieval/src/model/object_metadata_tests.rs"
    # Metadata fix's appended tests land separately, so Root can apply this
    # two-literal ID change atop its reviewed metadata source without dropping it.
    rows[path]=source(path).replace("minilm-l6-v2-u8-v2","minilm-l6-v2-u8-v3").replace("minilm-l6-v2-s8-v2","minilm-l6-v2-s8-v3")
    path="scripts/provision-local-model.py";rows[path]=source(path).replace('PROFILE["precision"] != "mixed-u8"','PROFILE["precision"] != "uint8"').replace("trained-v2 assets are local-build inputs","current assets use authenticated local inputs").replace("trained assets require explicit authenticated local build inputs","assets require explicit authenticated local inputs")
    replacements={"minilm-l6-v2-u8-v2":"minilm-l6-v2-u8-v3","minilm-l6-v2-s8-v2":"minilm-l6-v2-s8-v3",
        "e7399e81737d3f9707a304be116a229983bc9fff0974d1b087293789021d2d7b":"b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",
        "4788597b6d26061b778ac6255723d4bb004e4927446c480ecaebe19e4ea7f47c":"4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474",
        "9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd":"cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
        "69_325_926":"23_046_789","69_355_444":"23_026_053"}
    for path in ["crates/airk-cli/tests/model_catalog.rs","scripts/tests/test_trained_model_inputs.py","scripts/tests/test_service_runtime_build.py","scripts/tests/test_model_provisioning.py"]:
        text=source(path)
        for old,new in replacements.items():text=text.replace(old,new)
        if path.endswith("model_catalog.rs"):
            text=text.replace('"mixed-u8"','"uint8"').replace('"mixed-s8"','"int8"')
            start=text.index('        assert_eq!(\n            profile["quantization"]["schema"]')
            end=text.index('        assert_eq!(\n            profile["assets"]',start)
            text=text[:start]+'        assert!(profile.get("quantization").is_none_or(|value|value.is_null()));\n'+text[end:]
            text=text.replace('"bytes":34889','"bytes":11358')
        elif path.endswith("test_trained_model_inputs.py"):
            old='''            self.assertEqual(profile["quantization"]["quantized_linear_parameters"], 7_077_888)
            self.assertEqual(profile["quantization"]["constant_linear_parameters"], 10_616_832)
            self.assertEqual(len(profile["quantization"]["protected_fp32_nodes"]), 6)
            self.assertTrue(profile["quantization"]["fp32_embeddings_retained"])'''
            text=once(text,old,'''            self.assertIsNone(profile.get("quantization"))
            self.assertEqual(profile["qualification"]["verified_platforms"], [])
            self.assertEqual(profile["manifest"]["license_sha256"], "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30")''')
            text=text.replace("Independent trained-v2 acquisition","Independent current-original acquisition")
        elif path.endswith("test_model_provisioning.py"):text=text.replace('"mixed-u8"','"uint8"')
        rows[path]=text
    patch=[]
    for path,text in rows.items():
        output=HERE/"consumers"/path;output.parent.mkdir(parents=True,exist_ok=True);output.write_text(text)
        patch.extend(difflib.unified_diff(before[path].splitlines(True),text.splitlines(True),fromfile=path,tofile=path))
    (HERE/"consumers.patch").write_text("".join(patch))
    report={"before":{path:hashlib.sha256((ROOT/path).read_bytes()).hexdigest() for path in rows},
        "copies":{str(path.relative_to(HERE)):hashlib.sha256(path.read_bytes()).hexdigest() for path in (HERE/"consumers").rglob('*') if path.is_file()},
        "object_metadata_merge":"Apply its two ID literals atop reviewed metadata fix; do not overwrite appended metadata tests",
        "historical_model_legal_payloads":"test_model_legal_payloads.py and original trained ZIP/legal pins deliberately untouched",
        "tests_or_Cargo_or_NN_executed":False,"public_writes":False}
    (HERE/"consumers-manifest.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"consumer_files":len(rows),"manifest_sha256":hashlib.sha256((HERE/"consumers-manifest.json").read_bytes()).hexdigest()}))

if __name__=="__main__":main()
