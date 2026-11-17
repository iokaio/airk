# SPDX-License-Identifier: Apache-2.0
"""Recorded original-baseline decision to private Rust owner proposals; no native JSON emitted."""
from pathlib import Path
import hashlib,json,difflib
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
META=HERE.parent/"native-minilm-production-decision-r1/metadata"
DECISION=HERE.parent/"original-minilm-retention-decision-r1.json"

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
    if sha(DECISION)!="cef9af82caf81a6bd755161ffb8a00943d954cf7110eeb822ba775789a4433c2":raise ValueError("recorded Root baseline decision changed")
    name="crates/airk-retrieval/src/model_catalog.rs"
    source=(META/name).read_text()
    start=source.index("impl ModelCatalog {");end=source.index("fn legacy_catalog()",start)
    body='''impl ModelCatalog {
    fn validate(&self)->Result<()> {
        ensure!(self.schema==2 && self.profiles.len()==2 && self.default_profile=="minilm-l6-v2-u8-v3","unsupported original baseline catalog");
        let retained=legacy_catalog()?;let mut ids=BTreeSet::new();
        for profile in &self.profiles {
            let (hash,size,precision)=match (profile.id.as_str(),profile.precision.as_str()) {
                ("minilm-l6-v2-u8-v3","uint8")=>("b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",23046789,"uint8"),
                ("minilm-l6-v2-s8-v3","int8")=>("4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474",23026053,"int8"),
                _=>anyhow::bail!("only exact original CPU profiles are active"),
            };
            ensure!(ids.insert(&profile.id) && profile.runtime==ONNX_RUNTIME && profile.quantization.is_none(),"invalid original baseline profile/policy");
            let original=retained.profiles.iter().find(|old|old.precision==precision).context("original baseline metadata missing")?;
            ensure!(profile.manifest==original.manifest && profile.manifest.model_sha256==hash,"original publisher manifest/ABI differs");
            ensure!(profile.assets.len()==3,"original asset inventory differs");let mut names=BTreeSet::new();
            for asset in &profile.assets {
                let (pin,length)=match asset.name.as_str() {
                    "model.onnx"=>(&profile.manifest.model_sha256,size),
                    "tokenizer.json"=>(&profile.manifest.tokenizer_sha256,466247),
                    "LICENSE"=>(&profile.manifest.license_sha256,11358),
                    _=>anyhow::bail!("unsupported original model asset"),
                };
                ensure!(names.insert(&asset.name) && asset.sha256==*pin && asset.bytes==length && asset.url.is_none(),"original asset identity/length/download policy differs");
            }
        }
        Ok(())
    }
}
'''
    amended=source[:start]+body+source[end:]
    amended=amended.replace('impl QuantizationPolicy {','#[cfg(test)]\nimpl QuantizationPolicy {',1)
    amended=amended.replace('"minilm-l6-v2-u8-v2"','"minilm-l6-v2-u8-v3"').replace('"minilm-l6-v2-s8-v2"','"minilm-l6-v2-s8-v3"')
    amended=amended.replace('mod policy_cases;','mod policy_cases;\n#[cfg(test)]\nmod baseline_emitter;',1)
    target=HERE/"rust"/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(amended)
    emitter="crates/airk-retrieval/src/model_catalog/baseline_emitter.rs"
    target=HERE/"rust"/emitter;target.parent.mkdir(parents=True,exist_ok=True);target.write_text((HERE/"baseline_emitter.rs.proposed").read_text())
    patch="".join(difflib.unified_diff((ROOT/name).read_text().splitlines(True),amended.splitlines(True),fromfile=name,tofile=name))
    patch+="".join(difflib.unified_diff([],target.read_text().splitlines(True),fromfile=emitter,tofile=emitter))
    (HERE/"catalog-rust.patch").write_text(patch)
    manifest={"decision_sha256":sha(DECISION),"public_source_before":{name:sha(ROOT/name)},
        "private_rust":{str(path.relative_to(HERE)):sha(path) for path in (HERE/"rust").rglob('*.rs')},
        "native_catalog_JSON_emitted":False,"NN":False,"Cargo":False,"public_writes":False,
        "metadata_fence":"Reviewed separately at native-minilm-production-decision-r1/metadata-frozen-r1.json; catalog proposal includes its two pure predicate helpers"}
    (HERE/"catalog-rust-manifest.json").write_text(json.dumps(manifest,indent=2)+"\n")
    print(json.dumps({"manifest":str(HERE/"catalog-rust-manifest.json"),"sha256":sha(HERE/"catalog-rust-manifest.json"),"public_writes":False}))

if __name__=="__main__":main()
