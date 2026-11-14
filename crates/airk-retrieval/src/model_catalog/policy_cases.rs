// SPDX-License-Identifier: Apache-2.0
//! Native-owned policy capture for build/provision/package consumers.
use super::*;
#[test]
fn emitted_model_policy_retains_approved_trained_catalog_and_refusals() -> Result<()> {
    let catalog = inspect_catalog()?.catalog;
    catalog.validate()?;
    let value = serde_json::to_value(&catalog)?;
    for (pointer, replacement) in [
        (
            "/profiles/0/manifest/model_sha256",
            serde_json::json!("00".repeat(32)),
        ),
        (
            "/profiles/0/manifest/tokenizer_sha256",
            serde_json::json!("01".repeat(32)),
        ),
        ("/profiles/0/id", serde_json::json!("minilm-l6-v2-u8-v1")),
        (
            "/profiles/1/quantization/signed_lowering/maximum_inner_dimension",
            serde_json::json!(65536),
        ),
        (
            "/profiles/0/quantization/fp32_embeddings_retained",
            serde_json::json!(false),
        ),
    ] {
        let mut invalid = value.clone();
        // Each independent unsupported mutation must fail the owning native
        // validator before any build consumer accepts the captured catalog.
        *invalid
            .pointer_mut(pointer)
            .context("independent policy mutation missing")? = replacement;
        assert!(
            serde_json::from_value::<ModelCatalog>(invalid)?
                .validate()
                .is_err()
        );
    }
    let mut fake_url = value.clone();
    fake_url["profiles"][0]["assets"][0]["url"] = "https://example.invalid/trained-model".into();
    assert!(
        serde_json::from_value::<ModelCatalog>(fake_url)?
            .validate()
            .is_err()
    );
    let mut false_precision = value.clone();
    false_precision["profiles"][0]["quantization"]["quantized_linear_parameters"] =
        10_616_832.into();
    assert!(
        serde_json::from_value::<ModelCatalog>(false_precision)?
            .validate()
            .is_err()
    );
    let old = legacy_catalog()?;
    assert!(old.validate().is_err());
    for profile in &old.profiles {
        assert!(approved_manifest(&profile.manifest).is_err());
    }
    let capture = serde_json::json!({"schema":"airk.minilm.native-policy/2","catalog":catalog,"scope":"candidate build policy; human quality and installed platform qualification are not inferred"});
    let path = std::env::var_os("AIRK_EMIT_MODEL_POLICY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("airk-minilm-policy-observed.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&capture)?)?;
    let retained = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models/model-policy.json");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&retained).unwrap_or_default())
            .ok(),
        Some(capture),
        "native policy capture differs: {}",
        path.display()
    );
    Ok(())
}
