# SPDX-License-Identifier: Apache-2.0
"""Prepare exact private Rust/catalog-consumer proposals only; no native DTO/NN/Cargo."""
from pathlib import Path
import hashlib,json,difflib
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def once(text,old,new):
    if text.count(old)!=1:raise ValueError("nonunique source amendment: "+old[:90])
    return text.replace(old,new)

def main():
    original={};metadata={};promotion={}
    def read(name):
        if name not in original:original[name]=(ROOT/name).read_text()
        return original[name]
    model="crates/airk-retrieval/src/model.rs"
    metadata[model]=read(model)+"\n"+(HERE/"metadata_active.rs.fragment").read_text()
    catalog="crates/airk-retrieval/src/model_catalog.rs"
    helper='''pub(crate) fn contains_active_manifest(manifest: &OnnxManifest) -> Result<bool> {
    let catalog: ModelCatalog=serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate()?;
    Ok(catalog.profiles.iter().any(|profile|profile.manifest==*manifest))
}

pub(crate) fn contains_active_asset_references(set: &airk_core::model_objects::ModelObjectSet) -> Result<bool> {
    use airk_core::model_objects::ModelAssetName;
    set.validate()?;
    let catalog: ModelCatalog=serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate()?;
    Ok(catalog.profiles.iter().any(|profile| {
        [(ModelAssetName::ModelOnnx,"model.onnx"),(ModelAssetName::TokenizerJson,"tokenizer.json"),(ModelAssetName::License,"LICENSE")]
            .into_iter().all(|(name,file)| {
                let Some(reference)=set.assets.get(&name) else {return false;};
                profile.assets.iter().any(|asset|asset.name==file && reference.sha256.to_string()==asset.sha256 && reference.bytes==asset.bytes)
            })
    }))
}

'''
    metadata[catalog]=once(read(catalog),'pub fn approved_manifest(manifest: &OnnxManifest) -> Result<()> {',helper+'pub fn approved_manifest(manifest: &OnnxManifest) -> Result<()> {')
    shared="crates/airk-kernel/src/shared_models.rs"
    metadata[shared]=once(read(shared),'''    let descriptor = descriptor(store, objects)?;
    Ok(!airk_retrieval::model::onnx_descriptor_uses_current_runtime(&descriptor))''','''    let observed = if objects.shared.is_some() {
        store.shared_model_source(objects).and_then(|source| airk_retrieval::model::onnx_asset_metadata_uses_active_catalog(&source,objects))
    } else {
        airk_retrieval::model::onnx_object_metadata_uses_active_catalog(store.directory(),objects)
    };
    match observed {
        Ok(active)=>Ok(!active),
        Err(error)=> {
            let absent=error.chain().any(|cause| cause.downcast_ref::<std::io::Error>().is_some_and(|io|io.kind()==std::io::ErrorKind::NotFound));
            if absent && let Some(shared)=&objects.shared {
                let runtime=store.shared_model_runtime().context("shared model runtime is not bound")?;
                ensure!(shared.owner==airk_store::service::model_files::model_scope(runtime)?,"shared model belongs to another runtime");
                if !airk_retrieval::model::onnx_asset_references_may_be_active(objects)? {
                    return Ok(true);
                }
            }
            Err(error)
        }
    }''')
    tests="crates/airk-retrieval/src/model/object_metadata_tests.rs"
    metadata[tests]=read(tests)+"\n"+(HERE/"active_metadata_tests.rs.fragment").read_text()
    tests="crates/airk-kernel/src/shared_models/embedded_tests.rs"
    metadata[tests]=read(tests)+"\n"+(HERE/"kernel_pending_test.rs.fragment").read_text()
    indexing="crates/airk-kernel/src/current/indexing.rs"
    metadata[indexing]=once(read(indexing),'retained vectors belong to an older native runtime;','retained vectors belong to a retired native model or runtime;')
    replacements={
        "minilm-l6-v2-u8-v2":"minilm-l6-v2-u8-v3","minilm-l6-v2-s8-v2":"minilm-l6-v2-s8-v3",
        "51517cd30b9318cfabfffcf844d6312486a4d84a5027ff88e32ce73da523b906":"e4d2c66590e830cc9fedde5061e86a5195891db490a7eb967e3ec9c52c566d29",
        "e7399e81737d3f9707a304be116a229983bc9fff0974d1b087293789021d2d7b":"1d9ca474df1025a006f0b8fdfc6372d2bcb78502f6c8f46ee7c72a588fbb248b",
        "4788597b6d26061b778ac6255723d4bb004e4927446c480ecaebe19e4ea7f47c":"d918cfad34439a7258ddd403e1dbbacf44109a228be50985407dcbdd01151ccd",
        "9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd":"1efe4b67fd59a127e3c43ed3e7bfb3a6c152764a45b9edab5d520707b29056d0",
        "69_325_926":"69_325_925","34_889":"33_198",
    }
    paths=[catalog,"crates/airk-host/build.rs","crates/airk-host/src/bundled_assets.rs","crates/airk-cli/tests/model_catalog.rs",
        "crates/airk-retrieval/src/model/object_metadata_tests.rs","scripts/tests/test_trained_model_inputs.py",
        "scripts/tests/test_model_legal_payloads.py","scripts/tests/test_service_runtime_build.py"]
    for path in paths:
        text=metadata.get(path,read(path))
        for old,new in replacements.items():text=text.replace(old,new)
        promotion[path]=text
    promotion[catalog]=once(promotion[catalog],"mod policy_cases;","mod policy_cases;\n#[cfg(test)]\nmod promotion;")
    proposal_path="crates/airk-retrieval/src/model_catalog/promotion.rs"
    original[proposal_path]="";promotion[proposal_path]=(HERE/"promotion.rs.proposed").read_text()
    for label,rows in [("metadata",metadata),("epoch1-promotion",promotion)]:
        patch=[]
        for path,text in rows.items():
            source=metadata.get(path,original[path]) if label=="epoch1-promotion" else original[path]
            # New module compares against absent source; metadata changes remain
            # distinct from the conditional promotion step.
            if path==proposal_path:source=""
            output=HERE/label/path;output.parent.mkdir(parents=True,exist_ok=True);output.write_text(text)
            patch.extend(difflib.unified_diff(source.splitlines(True),text.splitlines(True),fromfile=path,tofile=path))
        (HERE/(label+".patch")).write_text("".join(patch))
    # The historical six-family ignored fixture remains untouched. Its one
    # current-default comparison is noted for review, never a reason to execute it.
    report={"scope":"private conditional source proposals; no active catalog or native DTO emitted",
        "source_baselines":{path:sha(ROOT/path) for path in original if original[path]},
        "profile_ID_decision":"Proposed new immutable v3 pair; Root contract decision required before consumers",
        "generated_only_by_Rust":["crates/airk-retrieval/src/model_catalog.json","fixtures/models/model-policy.json"],
        "historical_ignored_fixture":"evaluation_tests.rs old six-family owner unchanged; no old FINAL205 run",
        "metadata_source_count":len(metadata),"conditional_promotion_source_count":len(promotion),
        "NN":False,"Cargo":False,"public_writes":False,"qualified_platforms_added":[]}
    (HERE/"source-proposals.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"metadata_sources":len(metadata),"promotion_sources":len(promotion),"public_writes":False,"native_catalog_emitted":False}))

if __name__=="__main__":main()
