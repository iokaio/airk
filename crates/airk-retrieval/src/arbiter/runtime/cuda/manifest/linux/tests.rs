// SPDX-License-Identifier: Apache-2.0
use super::*;

fn pin() -> String {
    "1".repeat(64)
}
fn file(name: &str, needed: &[&str]) -> serde_json::Value {
    serde_json::json!({"name":name,"soname":name,"bytes":17,"sha256":pin(),"needed":needed})
}

pub(super) fn descriptor(product: bool) -> serde_json::Value {
    let vendor = [
        "libcudart.so.12",
        "libcublasLt.so.12",
        "libcublas.so.12",
        "libcufft.so.11",
        "libcurand.so.10",
        "libnvJitLink.so.12",
        "libnvrtc-builtins.so.12.8",
        "libnvrtc.so.12",
        "libcudnn.so.9",
        "libcudnn_graph.so.9",
        "libcudnn_ops.so.9",
        "libcudnn_adv.so.9",
        "libcudnn_cnn.so.9",
        "libcudnn_heuristic.so.9",
        "libcudnn_engines_precompiled.so.9",
        "libcudnn_engines_runtime_compiled.so.9",
    ];
    let mut files = vec![file(SHARED, &[]), file(PROVIDER, &[SHARED, DRIVER])];
    files.extend(vendor.iter().map(|name| file(name, &[])));
    let mut order = vendor.to_vec();
    order.extend([SHARED, PROVIDER]);
    let closure = serde_json::json!({
       "target":"x86_64-unknown-linux-gnu","ort_version":"1.30.0","static_core_build":pin(),
       "provenance":{"name":"airk-linux-cuda-build-provenance.json","role":"notice","bytes":200,"sha256":pin()},
       "files":files,
       "system_imports":[file(DRIVER,&["libc.so.6"]),file("libc.so.6",&[])],
       "load_order":order
    });
    if product {
        serde_json::json!({"schema":"airk.native.linux-cuda-runtime-manifest/2","closure":closure,"requirements":[]})
    } else {
        serde_json::json!({"schema":"airk.native.linux-cuda-runtime-manifest/1","closure":closure,"fits":[]})
    }
}

fn parse(v: &serde_json::Value) -> Result<Descriptor> {
    Descriptor::parse(&serde_json::to_vec(v)?)
}

#[test]
fn separates_elf_scientific_and_product_from_windows_and_each_other() -> Result<()> {
    assert!(parse(&descriptor(false))?.fits()?.is_empty());
    assert!(parse(&descriptor(true))?.fits().is_err());
    let mut wrong = descriptor(false);
    wrong["schema"] = "airk.native.cuda-runtime-manifest/1".into();
    assert!(parse(&wrong).is_err());
    let mut wrong = descriptor(true);
    wrong["fits"] = serde_json::json!([]);
    assert!(parse(&wrong).is_err());
    let mut wrong = descriptor(false);
    wrong["closure"]["target"] = "x86_64-pc-windows-msvc".into();
    assert!(parse(&wrong).is_err());
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn encodes_unchanged_scientific_closure_with_zero_product_requirements() -> Result<()> {
    let scientific = descriptor(false);
    let raw = serde_json::to_vec(&scientific)?;
    let hash = Sha256Digest::from_bytes(Sha256::digest(&raw).into());
    let product = encode_empty_product(&raw, hash)?;
    let parsed: serde_json::Value = serde_json::from_slice(&product)?;
    assert_eq!(parsed, descriptor(true));
    assert!(encode_empty_product(&raw, ZERO).is_err());
    let hash = Sha256Digest::from_bytes(Sha256::digest(&product).into());
    assert!(encode_empty_product(&product, hash).is_err());
    Ok(())
}

#[test]
fn validates_elf_pins_names_and_dependency_order() {
    for (pointer, bad) in [
        (
            "/closure/files/1/name",
            serde_json::json!("onnxruntime_providers_cuda.dll"),
        ),
        ("/closure/files/1/sha256", serde_json::json!("0".repeat(64))),
        ("/closure/files/1/soname", serde_json::Value::Null),
        ("/closure/files/0/name", serde_json::json!(DRIVER)),
        (
            "/closure/system_imports/0/name",
            serde_json::json!("../../libcuda.so.1"),
        ),
        (
            "/closure/files/1/needed",
            serde_json::json!(["libmystery.so.1"]),
        ),
        ("/closure/load_order", serde_json::json!([PROVIDER, SHARED])),
        ("/closure/load_order", serde_json::json!([SHARED, SHARED])),
    ] {
        let mut value = descriptor(false);
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(parse(&value).is_err(), "accepted invalid {pointer}");
    }
}

#[test]
fn rejects_wrong_provenance_and_cross_platform_compatibility() -> Result<()> {
    let product = parse(&descriptor(true))?;
    assert!(product.verify_provenance(b"{}").is_err());
    assert_ne!(
        compatibility_policy_digest(),
        super::super::compatibility_policy_digest()
    );
    assert_ne!(
        product_compatibility_policy_digest(),
        super::super::product_compatibility_policy_digest()
    );
    Ok(())
}

#[test]
fn accepts_authenticated_system_cxx_and_compression_dependencies() -> Result<()> {
    let mut v = descriptor(false);
    let system = v["closure"]["system_imports"].as_array_mut().unwrap();
    system.push(file("libstdc++.so.6", &["libc.so.6"]));
    system.push(file("libz.so.1", &["libc.so.6"]));
    v["closure"]["files"][1]["needed"] =
        serde_json::json!([SHARED, DRIVER, "libstdc++.so.6", "libz.so.1"]);
    parse(&v)?;
    Ok(())
}

#[test]
fn missing_delayed_cuda_library_refuses_before_driver_initialization() {
    let mut v = descriptor(false);
    let files = v["closure"]["files"].as_array_mut().unwrap();
    files.retain(|row| row["name"] != "libnvrtc-builtins.so.12.8");
    v["closure"]["load_order"]
        .as_array_mut()
        .unwrap()
        .retain(|name| name != "libnvrtc-builtins.so.12.8");
    assert!(parse(&v).is_err());
}
