// SPDX-License-Identifier: Apache-2.0
//! Metadata-only projection of exact pinned maintained-owner outputs.
use super::*;
use airk_core::arbiter::resource_policy::ResourceFilePin;
use serde_json::Value;
mod strict;

pub(in crate::arbiter::runtime::cuda) trait Source {
    /// The actual native caller retains no-follow/owner/held-object custody.
    fn read(&self, pin: &ResourceFilePin, maximum: u64) -> Result<Vec<u8>>;
    /// Stream-authenticate large DLL bytes without allocating an entire DLL.
    fn authenticate(&self, pin: &ResourceFilePin, bytes: u64, maximum: u64) -> Result<()>;
    /// Recheck every held source under the original caller deadline before return.
    fn recheck(&self) -> Result<()>;
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::arbiter::runtime::cuda) struct PayloadInput {
    pub file: manifest::RuntimeFile,
    pub source: ResourceFilePin,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::arbiter::runtime::cuda) struct NoticeInput {
    pub component: String,
    pub package_name: String,
    pub source: ResourceFilePin,
    pub original_source_sha256: Sha256Digest,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::arbiter::runtime::cuda) struct Input {
    pub schema: String,
    pub build_proof: ResourceFilePin,
    pub producer_receipt: ResourceFilePin,
    pub aggregate_proof: ResourceFilePin,
    pub aggregate_receipt: ResourceFilePin,
    pub source_lock: ResourceFilePin,
    pub cpu_lock: ResourceFilePin,
    pub cuda_inventory: ResourceFilePin,
    pub cuda_version: ResourceFilePin,
    pub cudnn_inventory: ResourceFilePin,
    pub verifier: ResourceFilePin,
    pub aggregate_archive: ResourceFilePin,
    pub legal_inventory: ResourceFilePin,
    pub legal_owner: ResourceFilePin,
    pub payloads: Vec<PayloadInput>,
    pub notices: Vec<NoticeInput>,
}
const INPUT_SCHEMA: &str = "airk.native.cuda-provenance-projection-input/1";
/// Opaque native metadata request only; it grants no runtime/device permit.
pub(in crate::arbiter::runtime::cuda) fn decode_input(raw: &[u8]) -> Result<Input> {
    ensure!(raw.len() <= 16 << 20, "projection request byte capacity");
    strict::parse(raw)?; // Reject all duplicate object keys before typed decode.
    let input: Input = serde_json::from_slice(raw)?;
    ensure!(
        input.schema == INPUT_SCHEMA
            && (2..=64).contains(&input.payloads.len())
            && (3..=512).contains(&input.notices.len()),
        "unknown projection input scope"
    );
    for pin in [
        &input.build_proof,
        &input.producer_receipt,
        &input.aggregate_proof,
        &input.aggregate_receipt,
        &input.source_lock,
        &input.cpu_lock,
        &input.cuda_inventory,
        &input.cuda_version,
        &input.cudnn_inventory,
        &input.verifier,
        &input.aggregate_archive,
        &input.legal_inventory,
        &input.legal_owner,
    ] {
        pin.validate()?;
        ensure!(nonzero(pin.sha256), "zero projection pin");
    }
    for payload in &input.payloads {
        payload.source.validate()?;
        ensure!(
            input_payload_valid(&payload.file)
                && payload.source.sha256 == payload.file.sha256
                && payload.file.imports.len() <= 256
                && payload.file.delay_imports.len() <= 256,
            "invalid projection payload input"
        );
    }
    for notice in &input.notices {
        notice.source.validate()?;
        ensure!(
            nonzero(notice.source.sha256)
                && nonzero(notice.original_source_sha256)
                && notice.bytes > 0
                && notice.bytes <= 2 << 20
                && token(&notice.component, 64)
                && basename(&notice.package_name),
            "invalid projection legal input"
        );
    }
    Ok(input)
}

#[derive(Serialize)]
pub(in crate::arbiter::runtime::cuda) struct Prepared {
    pub provenance: Asset,
    pub bytes: Vec<u8>,
    pub package_files: Vec<PublicFile>,
}
fn read(source: &dyn Source, pin: &ResourceFilePin, maximum: u64) -> Result<Vec<u8>> {
    pin.validate()?;
    let bytes = source.read(pin, maximum)?;
    ensure!(
        bytes.len() as u64 <= maximum
            && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == pin.sha256,
        "pinned private projection input differs"
    );
    Ok(bytes)
}
fn json(source: &dyn Source, pin: &ResourceFilePin) -> Result<Value> {
    strict::parse(&read(source, pin, 16 << 20)?)
}
fn sha(value: &Value) -> Result<Sha256Digest> {
    let v: Sha256Digest = value.as_str().context("SHA field missing")?.parse()?;
    ensure!(nonzero(v), "zero source SHA");
    Ok(v)
}
fn text(value: &Value) -> Result<String> {
    let v = value.as_str().context("public source token missing")?;
    ensure!(token(v, 128), "source text is not a safe public token");
    Ok(v.into())
}
fn size(value: &Value) -> Result<u64> {
    let v = value.as_u64().context("source length missing")?;
    ensure!(v > 0, "zero source length");
    Ok(v)
}
fn rows(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .context("source array missing")
}
fn canonical_sha(value: &Value) -> Result<Sha256Digest> {
    fn ordered(value: &Value) -> Result<Value> {
        Ok(match value {
            Value::Object(map) => {
                let ordered: BTreeMap<_, _> = map
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), ordered(v)?)))
                    .collect::<Result<_>>()?;
                Value::Object(ordered.into_iter().collect())
            }
            Value::Array(v) => Value::Array(v.iter().map(ordered).collect::<Result<_>>()?),
            Value::Number(v) => {
                ensure!(
                    v.is_i64() || v.is_u64(),
                    "nonintegral private canonical proof refused"
                );
                value.clone()
            }
            _ => value.clone(),
        })
    }
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&ordered(value)?)?).into(),
    ))
}
fn successful(value: &Value) -> bool {
    value["exit_code"] == 0
        && value["timed_out"] == false
        && value["log_oversized"] == false
        && value["owned_descendants_settled"] == true
        && value["owned_job_forced_termination"] == false
        && value.get("cleanup_refusal").is_none()
}
fn checked_rows(inventory: &Value, name: &str, bytes: u64, hash: Sha256Digest) -> Result<()> {
    ensure!(
        rows(inventory)?
            .iter()
            .filter(|r| r["path"].as_str().is_some_and(|p| p
                .rsplit(['/', '\\'])
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case(name)))
                && r["bytes"].as_u64() == Some(bytes)
                && r["sha256"].as_str() == Some(hash.to_string().as_str()))
            .count()
            == 1,
        "payload not uniquely bound to authenticated producer/dependency inventory"
    );
    Ok(())
}
const PATH_OPTIONS: &[&str] = &[
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
];
fn options(value: &Value) -> Result<Vec<BuildOption>> {
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    let mut private = BTreeSet::new();
    for arg in rows(value)? {
        let arg = arg.as_str().context("configure argument not text")?;
        if let Some(def) = arg.strip_prefix("-D") {
            let (name, v) = def
                .split_once('=')
                .context("configure definition missing value")?;
            ensure!(seen.insert(name), "duplicate configure definition");
            if PATH_OPTIONS.contains(&name) {
                private.insert(name);
                continue;
            }
            ensure!(
                token(name, 128) && (v.is_empty() || token(v, 256)),
                "unsafe semantic build option"
            );
            output.push(BuildOption {
                name: name.into(),
                value: v.into(),
            });
        }
    }
    ensure!(
        private.len() == PATH_OPTIONS.len(),
        "private path option roster differs"
    );
    output.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(output)
}
/// Output is descriptive and unapproved. The existing maintained verifier owns
/// full source/Ninja/COFF/build proof, and exact compiled manifest SHA remains
/// the separate runtime activation authority. No serialized bool grants it.
pub(in crate::arbiter::runtime::cuda) fn project(
    source: &dyn Source,
    input: Input,
) -> Result<Prepared> {
    ensure!(
        input.schema == INPUT_SCHEMA
            && (2..=64).contains(&input.payloads.len())
            && (3..=512).contains(&input.notices.len()),
        "projection input capacity"
    );
    let build = json(source, &input.build_proof)?;
    let producer = json(source, &input.producer_receipt)?;
    let aggregate = json(source, &input.aggregate_proof)?;
    let command = json(source, &input.aggregate_receipt)?;
    let lock = json(source, &input.source_lock)?;
    let cpu = json(source, &input.cpu_lock)?;
    let cuda = json(source, &input.cuda_inventory)?;
    let cuda_version = json(source, &input.cuda_version)?;
    let cudnn = json(source, &input.cudnn_inventory)?;
    read(source, &input.verifier, 1 << 20)?;
    read(source, &input.legal_owner, 1 << 20)?;
    let legal_inventory = json(source, &input.legal_inventory)?;
    ensure!(
        rows(&legal_inventory)?.len() == input.notices.len(),
        "exact legal owner roster required"
    );
    ensure!(
        build["schema"] == "airk.cuda-static-build-proof/1"
            && build["target"] == "x86_64-pc-windows-msvc"
            && build["ort_version"] == "1.30.0"
            && build["qualification"] == false
            && sha(&build["build_receipt"]["sha256"])? == input.producer_receipt.sha256
            && producer["status"] == "PASS_PRODUCER_ONLY_UNQUALIFIED",
        "successful same-build producer proof required"
    );
    let commands = rows(&producer["commands"])?;
    ensure!(
        !commands.is_empty() && commands.len() <= 16 && commands.iter().all(successful),
        "producer did not naturally settle successfully"
    );
    ensure!(
        producer["policy"]["one_static_core"] == true
            && producer["policy"]["optional_shared_cuda_provider"] == true
            && producer["policy"]["runtime_version"] == "1.30.0",
        "producer core policy differs"
    );
    ensure!(
        aggregate["schema"] == "airk.cuda-static-aggregate-proof/1"
            && aggregate["qualification"] == false
            && sha(&aggregate["producer_proof_sha256"])? == canonical_sha(&build)?
            && sha(&aggregate["receipt_sha256"])? == input.aggregate_receipt.sha256
            && command["status"] == "PASS_GPU_AGGREGATE_ONLY_UNQUALIFIED"
            && successful(&command["command"]),
        "matching naturally settled static aggregate required"
    );
    ensure!(
        build["toolchain"] == producer["toolchain"]
            && build["provider_dlls"] == producer["provider_dlls"]
            && aggregate["output"] == command["output"]
            && sha(&aggregate["plan_sha256"])? == canonical_sha(&command["plan"])?
            && sha(&command["plan"]["producer_proof_sha256"])? == canonical_sha(&build)?
            && sha(&lock["cpu_lock_sha256"])? == input.cpu_lock.sha256
            && sha(&build["cuda_inventory_sha256"])? == input.cuda_inventory.sha256
            && sha(&lock["cudnn"]["extraction_receipt_sha256"])? == input.cudnn_inventory.sha256,
        "proof/source/aggregate closure mismatch"
    );
    ensure!(
        input.aggregate_archive.sha256 == sha(&aggregate["output"]["sha256"])?,
        "aggregate artifact pin differs"
    );
    source.authenticate(
        &input.aggregate_archive,
        size(&aggregate["output"]["bytes"])?,
        2u64 << 30,
    )?;
    let original_lock = build
        .get("ancestry")
        .and_then(|a| a.get("original_r8_lock"))
        .unwrap_or(&build["producer_lock_sha256"]);
    ensure!(
        sha(original_lock)? == input.source_lock.sha256
            && lock["ort_version"] == "1.30.0"
            && cpu["ort_version"] == "1.30.0"
            && lock["upstream_commit"] == cpu["upstream_commit"],
        "source lock ancestry differs"
    );
    let mut components = vec![Component {
        name: "onnxruntime".into(),
        version: Some("1.30.0".into()),
        upstream_sha1: None,
        source_revision: Some(text(&cpu["upstream_commit"])?),
        original_url: cpu["source"]
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_owned),
        source_kind: SourceKind::Archive,
        source_sha256: sha(&cpu["source"]["sha256"])?,
        inventory_sha256: Some(sha(&build["source_inventory_sha256"])?),
        source_changes: vec![
            sha(&lock["loader_patch_sha256"])?,
            sha(&lock["offline_helper"]["patch_sha256"])?,
            sha(&lock["final_placement"]["patch_sha256"])?,
        ],
    }];
    if let Some(change) = producer.get("changed_source_after_sha256") {
        components[0].source_changes.push(sha(change)?);
    }
    ensure!(
        cuda_version["cuda"]["version"] == "12.8.1",
        "CUDA version differs"
    );
    checked_rows(
        &cuda,
        "version.json",
        read(source, &input.cuda_version, 1 << 20)?.len() as u64,
        input.cuda_version.sha256,
    )?;
    components.push(Component {
        name: "cuda".into(),
        version: Some("12.8.1".into()),
        upstream_sha1: None,
        source_revision: None,
        original_url: None,
        source_kind: SourceKind::InstalledToolkitInventory,
        source_sha256: input.cuda_inventory.sha256,
        inventory_sha256: Some(input.cuda_inventory.sha256),
        source_changes: Vec::new(),
    });
    ensure!(
        lock["cudnn"]["sha256"]
            == "c1a4567d822ebda7373fa1f19255dff4942302de741f830160b6c7d1fb31af23",
        "cuDNN source archive differs"
    );
    components.push(Component {
        name: "cudnn".into(),
        version: Some("9.10.2.21".into()),
        upstream_sha1: None,
        source_revision: None,
        original_url: None,
        source_kind: SourceKind::Archive,
        source_sha256: sha(&lock["cudnn"]["sha256"])?,
        inventory_sha256: Some(input.cudnn_inventory.sha256),
        source_changes: Vec::new(),
    });
    for dependency in rows(&cpu["dependencies"])?
        .iter()
        .chain(rows(&lock["dependencies"])?)
    {
        components.push(Component {
            name: text(&dependency["name"])?,
            version: dependency.get("version").map(text).transpose()?,
            upstream_sha1: dependency.get("upstream_sha1").map(text).transpose()?,
            source_revision: None,
            original_url: dependency
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_owned),
            source_kind: SourceKind::Archive,
            source_sha256: sha(&dependency["sha256"])?,
            inventory_sha256: None,
            source_changes: Vec::new(),
        });
    }
    let mut tools = Vec::new();
    for name in ["compiler", "cmake", "ninja", "python", "patch", "vcvars"] {
        let t = &build["toolchain"][name];
        let version = if name == "compiler" {
            Some(text(&build["toolchain"]["msvc_toolset"])?)
        } else {
            t.get("version").map(text).transpose()?
        };
        let version_kind = version.as_ref().map(|_| {
            if name == "compiler" {
                "msvc-toolset".into()
            } else {
                "reported-tool-version".into()
            }
        });
        let pin = ResourceFilePin {
            path: t["path"].as_str().context("tool path absent")?.into(),
            sha256: sha(&t["sha256"])?,
        };
        source.authenticate(&pin, size(&t["bytes"])?, 1u64 << 30)?;
        tools.push(Tool {
            name: name.into(),
            version,
            version_kind,
            bytes: size(&t["bytes"])?,
            sha256: sha(&t["sha256"])?,
        });
    }
    let archive_tool = &command["tool"];
    tools.push(Tool {
        name: "archiver".into(),
        version: None,
        version_kind: None,
        bytes: size(&archive_tool["bytes"])?,
        sha256: sha(&archive_tool["sha256"])?,
    });
    let archive_pin = ResourceFilePin {
        path: archive_tool["path"]
            .as_str()
            .context("archiver path absent")?
            .into(),
        sha256: sha(&archive_tool["sha256"])?,
    };
    source.authenticate(&archive_pin, size(&archive_tool["bytes"])?, 1u64 << 30)?;
    let nvcc = rows(&cuda)?
        .iter()
        .find(|v| v["path"] == "bin/nvcc.exe")
        .context("actual CUDA compiler inventory missing")?;
    let nvcc_path = rows(&producer["configure_argv"])?
        .iter()
        .filter_map(Value::as_str)
        .find_map(|v| v.strip_prefix("-DCMAKE_CUDA_COMPILER="))
        .context("CUDA compiler binding missing")?;
    let nvcc_pin = ResourceFilePin {
        path: nvcc_path.into(),
        sha256: sha(&nvcc["sha256"])?,
    };
    source.authenticate(&nvcc_pin, size(&nvcc["bytes"])?, 1u64 << 30)?;
    tools.push(Tool {
        name: "nvcc".into(),
        version: None,
        version_kind: None,
        bytes: size(&nvcc["bytes"])?,
        sha256: nvcc_pin.sha256,
    });
    let mut files = Vec::new();
    let mut native_files = Vec::new();
    for payload in input.payloads {
        let f = payload.file;
        ensure!(
            input_payload_valid(&f) && payload.source.sha256 == f.sha256,
            "unsafe payload input"
        );
        let inventory = match component_for(&f.name) {
            Some("onnxruntime") => &producer["provider_dlls"],
            Some("cudnn") => &cudnn["files"],
            Some("cuda") => &cuda,
            _ => anyhow::bail!("unknown payload"),
        };
        checked_rows(inventory, &f.name, f.bytes, f.sha256)?;
        source.authenticate(&payload.source, f.bytes, 2u64 << 30)?;
        files.push(PublicFile {
            name: f.name.clone(),
            bytes: f.bytes,
            sha256: f.sha256,
        });
        native_files.push(f);
    }
    let mut legal = Vec::new();
    for notice in input.notices {
        ensure!(
            notice.bytes > 0 && notice.bytes <= 2 << 20,
            "notice byte capacity"
        );
        let raw = read(source, &notice.source, 2 << 20)?;
        ensure!(raw.len() as u64 == notice.bytes, "notice bytes differ");
        ensure!(
            components
                .iter()
                .any(|c| c.name == notice.component
                    && c.source_sha256 == notice.original_source_sha256),
            "notice source archive not in exact selected component closure"
        );
        ensure!(
            rows(&legal_inventory)?
                .iter()
                .filter(
                    |v| v["component"].as_str() == Some(notice.component.as_str())
                        && v["package_name"].as_str() == Some(notice.package_name.as_str())
                        && v["bytes"].as_u64() == Some(notice.bytes)
                        && v["sha256"].as_str() == Some(notice.source.sha256.to_string().as_str())
                        && v["source_artifact_sha256"].as_str()
                            == Some(notice.original_source_sha256.to_string().as_str())
                )
                .count()
                == 1,
            "notice is not uniquely bound to the exact legal-owner roster"
        );
        legal.push(LegalNotice {
            component: notice.component,
            file: PublicFile {
                name: notice.package_name,
                bytes: notice.bytes,
                sha256: notice.source.sha256,
            },
            original_source_sha256: notice.original_source_sha256,
        });
    }
    let value = PublicRuntimeProvenance {
        schema: "airk.native.cuda-runtime-provenance/1".into(),
        target: "x86_64-pc-windows-msvc".into(),
        ort_version: "1.30.0".into(),
        static_core_build: input.producer_receipt.sha256,
        producer_script_sha256: sha(&build["producer_script_sha256"])?,
        producer_lock_sha256: sha(&build["producer_lock_sha256"])?,
        verifier_sha256: input.verifier.sha256,
        build_proof_sha256: input.build_proof.sha256,
        legal_inventory_sha256: input.legal_inventory.sha256,
        legal_owner_sha256: input.legal_owner.sha256,
        configure_argv_sha256: Sha256Digest::from_bytes(
            Sha256::digest(serde_json::to_vec(&producer["configure_argv"])?).into(),
        ),
        historical_failed_parent: producer
            .get("parent_failed_receipt")
            .map(|v| sha(&v["sha256"]))
            .transpose()?,
        aggregate: Aggregate {
            file: PublicFile {
                name: "onnxruntime.lib".into(),
                bytes: size(&aggregate["output"]["bytes"])?,
                sha256: sha(&aggregate["output"]["sha256"])?,
            },
            proof_sha256: input.aggregate_proof.sha256,
            receipt_sha256: input.aggregate_receipt.sha256,
            plan_sha256: sha(&aggregate["plan_sha256"])?,
        },
        components,
        tools,
        options: options(&producer["configure_argv"])?,
        files,
        legal,
    };
    validate_description(&value, value.static_core_build, &native_files)?;
    let bytes = serde_json::to_vec(&value)?;
    ensure!(bytes.len() <= MAX_JSON, "public projection byte capacity");
    let provenance = Asset {
        name: NAME.into(),
        role: AssetRole::Notice,
        bytes: bytes.len() as u64,
        sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
    };
    validate(&bytes, &provenance, value.static_core_build, &native_files)?;
    let mut package_files = value.files.clone();
    package_files.extend(value.legal.into_iter().map(|v| v.file));
    package_files.push(PublicFile {
        name: NAME.into(),
        bytes: provenance.bytes,
        sha256: provenance.sha256,
    });
    source.recheck()?;
    Ok(Prepared {
        provenance,
        bytes,
        package_files,
    })
}
fn input_payload_valid(value: &manifest::RuntimeFile) -> bool {
    basename(&value.name)
        && component_for(&value.name).is_some()
        && value.bytes > 0
        && value.bytes <= 2u64 << 30
        && nonzero(value.sha256)
}

#[cfg(test)]
mod tests;
