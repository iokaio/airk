// SPDX-License-Identifier: Apache-2.0
//! Synthetic metadata only; these inputs are never a genuine build/runtime proof.
use super::*;
use serde_json::json;
#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
}
impl Memory {
    fn pin(&mut self, name: &str, bytes: Vec<u8>) -> ResourceFilePin {
        let path = std::env::temp_dir()
            .join("airk-provenance-synthetic")
            .join(name)
            .to_string_lossy()
            .into_owned();
        let sha256 = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        self.files.insert(path.clone(), bytes);
        ResourceFilePin { path, sha256 }
    }
    fn json(&mut self, name: &str, v: Value) -> ResourceFilePin {
        self.pin(name, serde_json::to_vec(&v).unwrap())
    }
}
impl Source for Memory {
    fn recheck(&self) -> Result<()> {
        Ok(())
    }

    fn read(&self, p: &ResourceFilePin, max: u64) -> Result<Vec<u8>> {
        let b = self.files.get(&p.path).context("synthetic input missing")?;
        ensure!(b.len() as u64 <= max, "synthetic bound");
        Ok(b.clone())
    }
    fn authenticate(&self, p: &ResourceFilePin, n: u64, max: u64) -> Result<()> {
        let b = self.read(p, max)?;
        ensure!(
            b.len() as u64 == n && Sha256Digest::from_bytes(Sha256::digest(&b).into()) == p.sha256,
            "synthetic bytes changed"
        );
        Ok(())
    }
}
fn row(pin: &ResourceFilePin, bytes: u64) -> Value {
    json!({"path":pin.path,"bytes":bytes,"sha256":pin.sha256})
}
fn fixture() -> Result<(Memory, Input)> {
    let mut m = Memory::default();
    let version = m.json("version", json!({"cuda":{"version":"12.8.1"}}));
    let nvcc = m.pin("nvcc", b"synthetic compiler".to_vec());
    let cuda=m.json("cuda-inventory",json!([{"path":"version.json","bytes":m.files[&version.path].len(),"sha256":version.sha256},{"path":"bin/nvcc.exe","bytes":m.files[&nvcc.path].len(),"sha256":nvcc.sha256}]));
    let cudnn = m.json("cudnn-inventory", json!({"files":[]}));
    let archive = Sha256Digest::from_bytes([41; 32]);
    let cpu=m.json("cpu-lock",json!({"ort_version":"1.30.0","upstream_commit":"synthetic-pinned-revision","source":{"sha256":archive},"dependencies":[]}));
    let source_lock=m.json("gpu-lock",json!({"ort_version":"1.30.0","upstream_commit":"synthetic-pinned-revision","cpu_lock_sha256":cpu.sha256,"cudnn":{"extraction_receipt_sha256":cudnn.sha256,"sha256":"c1a4567d822ebda7373fa1f19255dff4942302de741f830160b6c7d1fb31af23"},"loader_patch_sha256":Sha256Digest::from_bytes([42;32]),"offline_helper":{"patch_sha256":Sha256Digest::from_bytes([43;32])},"final_placement":{"patch_sha256":Sha256Digest::from_bytes([44;32])},"dependencies":[]}));
    let mut tools = serde_json::Map::new();
    for name in ["compiler", "cmake", "ninja", "python", "patch", "vcvars"] {
        let p = m.pin(name, format!("synthetic {name}").into_bytes());
        let mut r = row(&p, m.files[&p.path].len() as u64);
        if matches!(name, "cmake" | "ninja" | "python") {
            r["version"] = json!("1.0.0");
        }
        tools.insert(name.into(), r);
    }
    tools.insert("msvc_toolset".into(), json!("14.44.35207"));
    let tools = Value::Object(tools);
    let mut configure = vec![
        json!("cmake"),
        json!("-Donnxruntime_USE_CUDA=ON"),
        json!("-Donnxruntime_BUILD_SHARED_LIB=OFF"),
        json!("-Donnxruntime_BUILD_CUDA_EP_AS_PLUGIN=OFF"),
        json!("-DCMAKE_CUDA_ARCHITECTURES=86"),
    ];
    for name in &[
        "CMAKE_MAKE_PROGRAM",
        "Patch_EXECUTABLE",
        "PYTHON_EXECUTABLE",
        "Python3_EXECUTABLE",
        "CMAKE_CUDA_COMPILER",
        "onnxruntime_CUDA_HOME",
        "CUDAToolkit_ROOT",
        "onnxruntime_CUDNN_HOME",
        "CUDNN_PATH",
        "FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND",
    ] {
        configure.push(json!(format!(
            "-D{name}={}",
            if *name == "CMAKE_CUDA_COMPILER" {
                nvcc.path.clone()
            } else {
                std::env::temp_dir().to_string_lossy().into_owned()
            }
        )));
    }
    let ok = json!({"exit_code":0,"timed_out":false,"log_oversized":false,"owned_descendants_settled":true,"owned_job_forced_termination":false});
    let mut payloads = Vec::new();
    let mut providers = Vec::new();
    for name in [
        "onnxruntime_providers_shared.dll",
        "onnxruntime_providers_cuda.dll",
    ] {
        let pin = m.pin(name, vec![7]);
        providers.push(json!({"path":format!("b/Release/{name}"),"bytes":1,"sha256":pin.sha256}));
        payloads.push(PayloadInput {
            file: manifest::RuntimeFile {
                name: name.into(),
                bytes: 1,
                sha256: pin.sha256,
                imports: BTreeSet::new(),
                delay_imports: BTreeSet::new(),
            },
            source: pin,
        });
    }
    let receipt=m.json("producer",json!({"status":"PASS_PRODUCER_ONLY_UNQUALIFIED","commands":[ok.clone()],"policy":{"one_static_core":true,"optional_shared_cuda_provider":true,"runtime_version":"1.30.0"},"toolchain":tools,"provider_dlls":providers,"configure_argv":configure}));
    let build_value = json!({"schema":"airk.cuda-static-build-proof/1","target":"x86_64-pc-windows-msvc","ort_version":"1.30.0","qualification":false,"build_receipt":{"sha256":receipt.sha256},"toolchain":tools,"provider_dlls":providers,"producer_script_sha256":Sha256Digest::from_bytes([45;32]),"producer_lock_sha256":source_lock.sha256,"source_inventory_sha256":Sha256Digest::from_bytes([46;32]),"cuda_inventory_sha256":cuda.sha256});
    let build_sha = canonical_sha(&build_value)?;
    let build = m.json("build-proof", build_value);
    let aggregate_archive = m.pin("onnxruntime.lib", vec![3; 70_000]);
    let out = json!({"path":"gpu-aggregate/onnxruntime.lib","bytes":70_000,"sha256":aggregate_archive.sha256});
    let plan = json!({"synthetic":true,"inputs":[],"producer_proof_sha256":build_sha});
    let plan_sha = canonical_sha(&plan)?;
    let lib = m.pin("lib.exe", b"synthetic archiver".to_vec());
    let aggregate_receipt=m.json("aggregate-receipt",json!({"status":"PASS_GPU_AGGREGATE_ONLY_UNQUALIFIED","command":ok,"output":out,"plan":plan,"tool":row(&lib,m.files[&lib.path].len() as u64)}));
    let aggregate_proof=m.json("aggregate-proof",json!({"schema":"airk.cuda-static-aggregate-proof/1","qualification":false,"producer_proof_sha256":build_sha,"receipt_sha256":aggregate_receipt.sha256,"output":out,"plan_sha256":plan_sha}));
    let verifier = m.pin("verifier.py", b"synthetic verifier; no authority".to_vec());
    let legal_owner = m.pin(
        "legal.py",
        b"synthetic legal owner; no rights inference".to_vec(),
    );
    let mut notices = Vec::new();
    let mut legal = Vec::new();
    for (name, source_hash) in [
        ("onnxruntime", archive),
        ("cuda", cuda.sha256),
        (
            "cudnn",
            "c1a4567d822ebda7373fa1f19255dff4942302de741f830160b6c7d1fb31af23".parse()?,
        ),
    ] {
        let pin = m.pin(&format!("{name}-license"), b"synthetic license".to_vec());
        let package = format!("airk-cuda-notice-{name}-license.txt");
        let bytes = m.files[&pin.path].len() as u64;
        legal.push(json!({"component":name,"package_name":package,"bytes":bytes,"sha256":pin.sha256,"source_artifact_sha256":source_hash}));
        notices.push(NoticeInput {
            component: name.into(),
            package_name: package,
            source: pin,
            original_source_sha256: source_hash,
            bytes,
        });
    }
    let legal_inventory = m.json("legal-inventory", Value::Array(legal));
    Ok((
        m,
        Input {
            schema: "airk.native.cuda-provenance-projection-input/1".into(),
            build_proof: build,
            producer_receipt: receipt,
            aggregate_proof,
            aggregate_receipt,
            source_lock,
            cpu_lock: cpu,
            cuda_inventory: cuda,
            cuda_version: version,
            cudnn_inventory: cudnn,
            verifier,
            aggregate_archive,
            legal_inventory,
            legal_owner,
            payloads,
            notices,
            custody_copies: Vec::new(),
            historical_tool_copies: Vec::new(),
        },
    ))
}
#[test]
fn synthetic_projector_emits_only_exact_public_roster() -> Result<()> {
    let (m, input) = fixture()?;
    let value = project(&m, input)?;
    assert_eq!(value.package_files.len(), 6);
    assert_eq!(value.provenance.name, "airk-cuda-build-provenance.json");
    assert_eq!(value.provenance.role, AssetRole::Notice);
    let raw = String::from_utf8(value.bytes)?;
    assert!(!raw.contains("airk-provenance-synthetic"));
    assert!(!raw.contains("\"configure_argv\":"));
    assert!(!raw.contains("commands"));
    Ok(())
}
fn reseal_producer_chain(m: &mut Memory, input: &mut Input, producer: Value) -> Result<()> {
    input.producer_receipt = m.json("changed-producer", producer);
    let mut build: Value = serde_json::from_slice(&m.files[&input.build_proof.path])?;
    build["build_receipt"]["sha256"] = json!(input.producer_receipt.sha256);
    let build_sha = canonical_sha(&build)?;
    input.build_proof = m.json("changed-build", build);
    let mut receipt: Value = serde_json::from_slice(&m.files[&input.aggregate_receipt.path])?;
    receipt["plan"]["producer_proof_sha256"] = json!(build_sha);
    let plan_sha = canonical_sha(&receipt["plan"])?;
    input.aggregate_receipt = m.json("changed-aggregate-receipt", receipt);
    let mut aggregate: Value = serde_json::from_slice(&m.files[&input.aggregate_proof.path])?;
    aggregate["producer_proof_sha256"] = json!(build_sha);
    aggregate["receipt_sha256"] = json!(input.aggregate_receipt.sha256);
    aggregate["plan_sha256"] = json!(plan_sha);
    input.aggregate_proof = m.json("changed-aggregate-proof", aggregate);
    Ok(())
}
#[test]
fn synthetic_projector_refuses_failed_or_unsettled_producer() -> Result<()> {
    for (field, expected) in [
        ("status", "successful same-build producer proof required"),
        (
            "owned_job_forced_termination",
            "producer did not naturally settle successfully",
        ),
    ] {
        let (mut m, mut input) = fixture()?;
        let mut value: Value = serde_json::from_slice(&m.files[&input.producer_receipt.path])?;
        if field == "status" {
            value["status"] = json!("FAILED_PRESERVED_NO_RETRY");
        } else {
            value["commands"][0][field] = json!(true);
        }
        reseal_producer_chain(&mut m, &mut input, value)?;
        let error = project(&m, input)
            .err()
            .context("invalid producer accepted")?;
        assert_eq!(error.to_string(), expected);
    }
    Ok(())
}
#[test]
fn aggregate_plan_refuses_different_build_even_with_resealed_plan_sha() -> Result<()> {
    let (mut m, mut input) = fixture()?;
    let mut receipt: Value = serde_json::from_slice(&m.files[&input.aggregate_receipt.path])?;
    receipt["plan"]["producer_proof_sha256"] = json!(Sha256Digest::from_bytes([0x99; 32]));
    let plan_sha = canonical_sha(&receipt["plan"])?;
    input.aggregate_receipt = m.json("cross-build-aggregate-receipt", receipt);
    let mut proof: Value = serde_json::from_slice(&m.files[&input.aggregate_proof.path])?;
    proof["receipt_sha256"] = json!(input.aggregate_receipt.sha256);
    proof["plan_sha256"] = json!(plan_sha);
    input.aggregate_proof = m.json("cross-build-aggregate-proof", proof);
    let error = project(&m, input)
        .err()
        .context("cross-build plan accepted")?;
    assert_eq!(error.to_string(), "proof/source/aggregate closure mismatch");
    Ok(())
}
#[test]
fn private_json_duplicates_floats_and_unlisted_path_options_refuse() -> Result<()> {
    assert!(strict::parse(br#"{"a":1,"a":2}"#).is_err());
    assert!(strict::parse(br#"{"nested":{"b":1,"b":2}}"#).is_err());
    assert!(canonical_sha(&json!({"elapsed":0.1})).is_err());
    let mut valid_paths: Vec<Value> = [
        "CMAKE_MAKE_PROGRAM",
        "Patch_EXECUTABLE",
        "PYTHON_EXECUTABLE",
        "Python3_EXECUTABLE",
        "CMAKE_CUDA_COMPILER",
        "onnxruntime_CUDA_HOME",
        "CUDAToolkit_ROOT",
        "onnxruntime_CUDNN_HOME",
        "CUDNN_PATH",
        "FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND",
    ]
    .into_iter()
    .map(|name| json!(format!("-D{name}=C:/synthetic")))
    .collect();
    valid_paths.push(json!("-DCMAKE_CUDA_ARCHITECTURES=86;89;120"));
    let projected = options(&Value::Array(valid_paths.clone()))?;
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].name, "CMAKE_CUDA_ARCHITECTURES");
    assert_eq!(projected[0].value, "86;89;120");

    let mut fresh = valid_paths.clone();
    for name in [
        "CMAKE_C_COMPILER",
        "CMAKE_CXX_COMPILER",
        "CMAKE_CUDA_HOST_COMPILER",
    ] {
        fresh.push(json!(format!("-D{name}=C:/Tools/cl.exe")));
    }
    let compiler = r"C:\Tools\cl.exe";
    let observed = product_fresh::public_options(&json!(fresh), compiler)?;
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].name, "CMAKE_CUDA_ARCHITECTURES");
    assert!(product_fresh::public_options(&json!(fresh), r"C:\Other\cl.exe").is_err());
    let mut one_foreign = fresh.clone();
    *one_foreign
        .last_mut()
        .context("missing compiler option in test")? =
        json!("-DCMAKE_CUDA_HOST_COMPILER=C:/Other/cl.exe");
    assert!(product_fresh::public_options(&json!(one_foreign), compiler).is_err());
    let mut one_missing = fresh.clone();
    one_missing.pop();
    assert!(product_fresh::public_options(&json!(one_missing), compiler).is_err());
    let mut unlisted = fresh.clone();
    unlisted.push(json!("-DFOREIGN=C:/Tools/cl.exe"));
    assert!(product_fresh::public_options(&json!(unlisted), compiler).is_err());
    fresh.push(json!("-DCMAKE_C_COMPILER=C:/Tools/cl.exe"));
    assert!(product_fresh::public_options(&json!(fresh), compiler).is_err());
    assert!(product_fresh::public_options(&json!(valid_paths), compiler).is_err());
    valid_paths.push(json!("-DUNKNOWN=C:/Users/private"));
    let error = options(&Value::Array(valid_paths))
        .err()
        .context("unsafe semantic option accepted")?;
    assert_eq!(error.to_string(), "unsafe semantic build option");
    Ok(())
}
#[test]
fn synthetic_projector_refuses_missing_notice_or_changed_blob() -> Result<()> {
    let (mut m, input) = fixture()?;
    m.files
        .get_mut(&input.notices[0].source.path)
        .unwrap()
        .push(b'X');
    assert!(project(&m, input).is_err());
    let (m, mut input) = fixture()?;
    input.notices.pop();
    assert!(project(&m, input).is_err());
    Ok(())
}

#[test]
fn final_source_custody_failure_refuses_projection_before_publication() -> Result<()> {
    struct Lost(Memory);
    impl Source for Lost {
        fn read(&self, p: &ResourceFilePin, max: u64) -> Result<Vec<u8>> {
            self.0.read(p, max)
        }
        fn authenticate(&self, p: &ResourceFilePin, n: u64, max: u64) -> Result<()> {
            self.0.authenticate(p, n, max)
        }
        fn recheck(&self) -> Result<()> {
            anyhow::bail!("synthetic original held object drift")
        }
    }
    let (source, input) = fixture()?;
    assert!(project(&Lost(source), input).is_err());
    Ok(())
}

#[test]
fn opaque_native_projection_input_roundtrips_and_refuses_unknown_duplicate_or_zero_pins()
-> Result<()> {
    let (_, input) = fixture()?;
    let raw = serde_json::to_vec(&input)?;
    let decoded = decode_input(&raw)?;
    assert_eq!(serde_json::to_vec(&decoded)?, raw);
    let mut unknown = serde_json::to_value(&input)?;
    unknown["available"] = json!(true);
    assert!(decode_input(&serde_json::to_vec(&unknown)?).is_err());
    let text = String::from_utf8(raw)?;
    let duplicate = text.replacen(
        "{",
        "{\"schema\":\"airk.native.cuda-provenance-projection-input/1\",",
        1,
    );
    assert!(decode_input(duplicate.as_bytes()).is_err());
    let mut zero = serde_json::to_value(&input)?;
    zero["build_proof"]["sha256"] = json!(Sha256Digest::from_bytes([0; 32]));
    assert!(decode_input(&serde_json::to_vec(&zero)?).is_err());
    Ok(())
}

#[test]
fn legacy_projection_input_omits_empty_custody_and_rejects_unknown_copy_fields() -> Result<()> {
    let (_, input) = fixture()?;
    let raw = serde_json::to_vec(&input)?;
    assert!(!String::from_utf8_lossy(&raw).contains("custody_copies"));
    let decoded = decode_input(&raw)?;
    assert!(decoded.custody_copies.is_empty());
    assert!(decoded.historical_tool_copies.is_empty());
    assert!(!String::from_utf8_lossy(&raw).contains("historical_tool_copies"));
    let mut bad: Value = serde_json::from_slice(&raw)?;
    bad["custody_copies"] = json!([{"original":{"path":input.build_proof.path,"sha256":input.build_proof.sha256},"copy":{"path":input.build_proof.path,"sha256":input.build_proof.sha256},"bytes":1,"unknown":true}]);
    assert!(decode_input(&serde_json::to_vec(&bad)?).is_err());
    Ok(())
}

#[test]
fn only_named_historical_tools_use_inert_evidence_method() -> Result<()> {
    use std::cell::RefCell;
    struct Observed {
        memory: Memory,
        historical: RefCell<Vec<String>>,
        direct: RefCell<Vec<String>>,
    }
    impl Source for Observed {
        fn read(&self, p: &ResourceFilePin, max: u64) -> Result<Vec<u8>> {
            self.memory.read(p, max)
        }
        fn authenticate(&self, p: &ResourceFilePin, n: u64, max: u64) -> Result<()> {
            self.direct.borrow_mut().push(p.path.clone());
            self.memory.authenticate(p, n, max)
        }
        fn authenticate_historical_tool(
            &self,
            name: &str,
            p: &ResourceFilePin,
            n: u64,
            max: u64,
        ) -> Result<()> {
            self.historical.borrow_mut().push(name.into());
            self.memory.authenticate(p, n, max)
        }
        fn recheck(&self) -> Result<()> {
            self.memory.recheck()
        }
    }
    let (memory, input) = fixture()?;
    let aggregate = input.aggregate_archive.path.clone();
    let payloads = input
        .payloads
        .iter()
        .map(|p| p.source.path.clone())
        .collect::<Vec<_>>();
    let source = Observed {
        memory,
        historical: RefCell::new(Vec::new()),
        direct: RefCell::new(Vec::new()),
    };
    project(&source, input)?;
    assert_eq!(
        *source.historical.borrow(),
        [
            "compiler", "cmake", "ninja", "python", "patch", "vcvars", "archiver", "nvcc"
        ]
    );
    let direct = source.direct.borrow();
    assert_eq!(direct.len(), 3);
    assert!(direct.contains(&aggregate));
    assert!(payloads.iter().all(|p| direct.contains(p)));
    Ok(())
}
