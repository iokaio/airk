# SPDX-License-Identifier: Apache-2.0
"""Private active-baseline legal inventory update; old trained archive fixtures stay exact."""
from pathlib import Path
import difflib,hashlib,json
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def once(text,old,new):
    if text.count(old)!=1:raise ValueError("nonunique active legal amendment: "+old[:90])
    return text.replace(old,new)

def main():
    rows={};before={}
    def source(path):before[path]=(ROOT/path).read_text();return before[path]
    path="scripts/license-notices.py";text=source(path)
    start=text.index("MODEL_COMPANIONS = {");end=text.index("\n}\n",start)+3
    text=text[:start]+'''MODEL_COMPANIONS = {
    'minilm-LICENSE.txt': ('LICENSE', 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'),
}
'''+text[end:]
    text=text.replace("complete trained-model rights text","complete original-model rights text").replace("trained-model notice","model notice").replace("Trained-model notice","Model notice")
    rows[path]=text
    path="scripts/package-extension.py";text=source(path)
    text=once(text,'"LICENSE", "TRAINING-NOTICES.md", "training-provenance.json", "CC-BY-4.0.txt", "provenance.json"))','"LICENSE", "provenance.json"))')
    text=text.replace("Unexpected trained-model notice inventory","Unexpected model notice inventory");rows[path]=text
    path="scripts/package-native.py";text=source(path).replace("Pinned trained-model notice","Pinned model notice");rows[path]=text
    path="scripts/tests/test_license_declarations.py";text=source(path)
    start=text.index("        expected = {'minilm-v2-LICENSE.txt'");end=text.index("\n        with tempfile.TemporaryDirectory()",start)
    text=text[:start]+"        expected = {'minilm-LICENSE.txt':'LICENSE'}"+text[end:]
    text=once(text,"            self.assertIn(b'Requests', (staged/'LICENSE').read_bytes())","            self.assertEqual(hashlib.sha256((staged/'LICENSE').read_bytes()).hexdigest(), 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30')")
    text=once(text,"        for changed in ['minilm-v2-LICENSE.txt','minilm-v2-TRAINING-NOTICES.md',\n                        'minilm-v2-provenance.json','CC-BY-4.0.txt']:","        for changed in ['minilm-LICENSE.txt']:")
    rows[path]=text
    path="scripts/tests/test_package_extension.py";text=source(path)
    old='''        expected = {"LICENSE": "minilm-v2-LICENSE.txt", "TRAINING-NOTICES.md": "minilm-v2-TRAINING-NOTICES.md",
                    "training-provenance.json": "minilm-v2-provenance.json", "CC-BY-4.0.txt": "CC-BY-4.0.txt"}'''
    text=once(text,old,'        expected = {"LICENSE": "minilm-LICENSE.txt"}')
    text=text.replace("Unexpected trained-model notice inventory","Unexpected model notice inventory");rows[path]=text
    path="scripts/tests/test_package_native.py";text=source(path)
    start=text.index('        companions = {\n',text.index('def test_trained_model_notices_require_exact_authenticated_five_file_closure'))
    end=text.index('\n        for target in SPLIT_IMAGES:',start)
    text=text[:start]+'''        companions = {
            "minilm-LICENSE.txt": ("LICENSE", "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"),
        }'''+text[end:]
    text=once(text,'''                                 {"LICENSE", "TRAINING-NOTICES.md", "training-provenance.json",
                                  "CC-BY-4.0.txt", "provenance.json"})''','''                                 {"LICENSE", "provenance.json"})''')
    text=once(text,'''                for name in ("LICENSE", "TRAINING-NOTICES.md", "training-provenance.json",
                             "CC-BY-4.0.txt", "provenance.json"):''','''                for name in ("LICENSE", "provenance.json"):''')
    text=text.replace("Pinned trained-model notice","Pinned model notice");rows[path]=text
    patch=[]
    for path,text in rows.items():
        output=HERE/"legal-consumers"/path;output.parent.mkdir(parents=True,exist_ok=True);output.write_text(text)
        patch.extend(difflib.unified_diff(before[path].splitlines(True),text.splitlines(True),fromfile=path,tofile=path))
    (HERE/"legal-consumers.patch").write_text("".join(patch))
    report={"before":{path:hashlib.sha256((ROOT/path).read_bytes()).hexdigest() for path in rows},
        "copies":{str(path.relative_to(HERE)):hashlib.sha256(path.read_bytes()).hexdigest() for path in (HERE/"legal-consumers").rglob('*') if path.is_file()},
        "active_inventory":["LICENSE","provenance.json"],"license_sha256":"cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
        "historical_preservation":"test_model_legal_payloads.py, old trained ZIP, .gitattributes and all historical third_party/models payloads unchanged",
        "tests_or_Cargo_or_NN_executed":False,"public_writes":False}
    (HERE/"legal-consumers-manifest.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"files":len(rows),"manifest_sha256":hashlib.sha256((HERE/"legal-consumers-manifest.json").read_bytes()).hexdigest()}))

if __name__=="__main__":main()
