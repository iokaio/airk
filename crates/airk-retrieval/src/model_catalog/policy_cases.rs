// SPDX-License-Identifier: Apache-2.0
//! Native policy capture: original publisher bytes under the current provider.
use super::*;
#[test]
fn emitted_model_policy_retains_approved_trained_catalog_and_refusals() -> Result<()> {
    // Retained test identity; the capture carries no platform or task qualification.
    let catalog = inspect_catalog()?.catalog;
    catalog.validate_fp16()?;
    assert_eq!(catalog.default_profile, "minilm-l6-v2-fp16-v1");
    let [profile] = catalog.profiles.as_slice() else {
        anyhow::bail!("expected exactly one shared FP16 profile");
    };
    assert_eq!(profile.id, "minilm-l6-v2-fp16-v1");
    assert_eq!(profile.precision, "float16");
    assert_eq!(
        profile.runtime,
        "onnxruntime/1.30.0/airk-minilm-fp16-shared/1"
    );
    assert_eq!(
        profile
            .assets
            .iter()
            .map(|asset| (asset.name.as_str(), asset.bytes, asset.sha256.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                "model.onnx",
                45_469_884,
                "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a",
            ),
            (
                "tokenizer.json",
                466_247,
                "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
            ),
            (
                "LICENSE",
                11_358,
                "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
            ),
        ]
    );
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
        ("/profiles/0/id", serde_json::json!("minilm-l6-v2-u8-v2")),
        (
            "/profiles/0/manifest/license_sha256",
            serde_json::json!("02".repeat(32)),
        ),
        ("/profiles/0/precision", serde_json::json!("mixed-u8")),
    ] {
        let mut changed = value.clone();
        *changed
            .pointer_mut(pointer)
            .context("independent policy mutation missing")? = replacement;
        assert!(
            serde_json::from_value::<ModelCatalog>(changed)?
                .validate_fp16()
                .is_err()
        );
    }
    let mut fake_url = value.clone();
    fake_url["profiles"][0]["assets"][0]["url"] = "https://example.invalid/model".into();
    assert!(
        serde_json::from_value::<ModelCatalog>(fake_url)?
            .validate_fp16()
            .is_err()
    );
    // The previous mixed policy is independently well-formed but cannot be
    // attached to the shared FP16 profile. Keep its historical math checker exercised.
    let previous = QuantizationPolicy {
        schema: "airk.minilm.quantization/2".into(),
        activation_precision: "unsigned asymmetric INT8".into(),
        signed_lowering: None,
        fp32_embeddings_retained: true,
        protected_fp32_nodes: (0..6)
            .map(|layer| format!("/inner/encoder/layer.{layer}/output/dense/MatMul"))
            .collect(),
        constant_linear_parameters: 10_616_832,
        quantized_linear_parameters: 7_077_888,
        trained_checkpoint_sha256:
            "51517cd30b9318cfabfffcf844d6312486a4d84a5027ff88e32ce73da523b906".into(),
    };
    previous.validate(false)?;
    let mut false_training = value.clone();
    false_training["profiles"][0]["quantization"] = serde_json::to_value(previous)?;
    assert!(
        serde_json::from_value::<ModelCatalog>(false_training)?
            .validate_fp16()
            .is_err()
    );
    let old = legacy_catalog()?;
    assert!(old.validate().is_err());
    assert!(old.validate_fp16().is_err());
    approved_manifest(&profile.manifest)?;
    assert!(profile.quantization.is_none());
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
