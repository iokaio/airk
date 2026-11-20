// SPDX-License-Identifier: Apache-2.0
//! Data-only build assembly. Live module and device authority belongs to the native loader.
use super::*;

pub fn provenance_build(raw: &[u8]) -> Result<Sha256Digest> {
    ensure!(raw.len() <= 1 << 20, "Linux provenance bound");
    let p: Provenance = serde_json::from_slice(raw)?;
    ensure!(
        p.schema == "airk.native.linux-cuda-build-provenance/1"
            && p.target == TARGET
            && p.static_core_build != ZERO,
        "unknown Linux build evidence"
    );
    Ok(p.static_core_build)
}

pub fn assemble_scientific(
    static_core_build: Sha256Digest,
    provenance: Asset,
    files: Vec<ElfFile>,
    load_order: Vec<String>,
    system_imports: Vec<ElfFile>,
) -> Result<Vec<u8>> {
    let closure = Closure {
        target: TARGET.into(),
        ort_version: "1.30.0".into(),
        static_core_build,
        provenance,
        files,
        load_order,
        system_imports,
    };
    closure.validate()?;
    let raw = serde_json::to_vec(&Descriptor::Scientific {
        closure,
        fits: Vec::new(),
    })?;
    ensure!(raw.len() <= 128 << 10, "Linux CUDA descriptor byte cap");
    Ok(raw)
}

/// Reuse a checked scientific ELF closure for packaging without adding fitted variants.
pub fn encode_empty_product(scientific: &[u8], expected: Sha256Digest) -> Result<Vec<u8>> {
    ensure!(
        digest(scientific) == expected,
        "scientific descriptor digest differs"
    );
    let Descriptor::Scientific { closure, fits } = Descriptor::parse(scientific)? else {
        anyhow::bail!("original scientific ELF closure required");
    };
    ensure!(
        fits.is_empty(),
        "package metadata requires an unqualified scientific closure"
    );
    let result = serde_json::to_vec(&Descriptor::Product {
        closure,
        requirements: Vec::new(),
    })?;
    ensure!(
        Descriptor::parse(&result)?.is_product(),
        "invalid product ELF closure"
    );
    Ok(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoticeInventory {
    originals: Vec<NoticeFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoticeFile {
    component: String,
    archive_sha256: Sha256Digest,
    original_path: String,
    staged_name: String,
    bytes: u64,
    sha256: Sha256Digest,
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn pin(doc: &serde_json::Value, path: &str) -> Result<Sha256Digest> {
    let hash: Sha256Digest = doc
        .pointer(path)
        .and_then(|s| s.as_str())
        .context("missing Linux producer digest")?
        .parse()?;
    ensure!(hash != ZERO, "empty Linux producer digest");
    Ok(hash)
}

/// Encode the public provenance from preserved native producer receipts and original notice inventory.
/// The seven slices are the completed static aggregate, provider ELF build, source configure, protected
/// bootstrap, frozen ORT archives, NVIDIA archives, and component notice inventory respectively.
pub fn encode_build_provenance(receipts: [&[u8]; 7]) -> Result<Vec<u8>> {
    ensure!(
        receipts.iter().all(|r| r.len() <= 8 << 20),
        "Linux producer receipt capacity"
    );
    let [
        aggregate,
        elf,
        configure,
        bootstrap,
        archives,
        distributions,
        notices,
    ] = receipts;
    let core: serde_json::Value = serde_json::from_slice(aggregate)?;
    let provider: serde_json::Value = serde_json::from_slice(elf)?;
    let cfg: serde_json::Value = serde_json::from_slice(configure)?;
    let boot: serde_json::Value = serde_json::from_slice(bootstrap)?;
    let source: serde_json::Value = serde_json::from_slice(archives)?;
    let nv: serde_json::Value = serde_json::from_slice(distributions)?;
    ensure!(
        core["success"] == true
            && provider["success"] == true
            && cfg["configured"] == true
            && boot["built"] == true
            && pin(&core, "/graph")? == pin(&provider, "/graph")?
            && pin(&core, "/bootstrap")? == digest(bootstrap)
            && pin(&core, "/configure")? == digest(configure)
            && pin(&provider, "/configure")? == digest(configure)
            && pin(&core, "/graph")? == pin(&cfg, "/ninja_sha256")?
            && pin(&boot, "/bootstrap")? == pin(&cfg, "/source/bootstrap_sha256")?
            && pin(&boot, "/original_env")? == pin(&cfg, "/source/unprotected_env_sha256")?,
        "Linux producer stages are incomplete or have different consumed pins"
    );
    ensure!(
        cfg["command"]["argv"]
            .as_array()
            .context("missing CMake arguments")?
            .iter()
            .any(|arg| arg.as_str() == Some("-DCMAKE_CUDA_ARCHITECTURES=86")),
        "Linux producer is not the SM86 recipe"
    );
    ensure!(
        core["symbols"]
            .as_array()
            .is_some_and(|list| list.iter().any(|v| v == "airk_ort_bootstrap_linux_cuda_v1"))
            && boot["symbols"]
                .as_array()
                .is_some_and(|list| list.iter().any(|v| v == "airk_ort_bootstrap_linux_cuda_v1")),
        "native bootstrap export absent"
    );
    let ort = pin(&source, "/source_archive/sha256")?;
    ensure!(
        ort == pin(&cfg, "/source/archive_sha256")?,
        "Linux ORT source differs"
    );
    let mut components = BTreeMap::from([("onnxruntime".to_owned(), ort)]);
    for (name, row) in source["dependency_archives"]
        .as_object()
        .context("missing ORT archives")?
    {
        ensure!(
            components
                .insert(name.clone(), pin(row, "/sha256")?)
                .is_none(),
            "duplicate ORT component"
        );
    }
    for row in nv["packages"]
        .as_array()
        .context("missing NVIDIA archives")?
    {
        let package = row["name"]
            .as_str()
            .and_then(|n| n.split_once("-linux-x86_64-"))
            .context("invalid NVIDIA archive name")?
            .0;
        ensure!(
            components
                .insert(package.to_owned(), pin(row, "/sha256")?)
                .is_none(),
            "duplicate CUDA component"
        );
    }
    let notices: NoticeInventory = serde_json::from_slice(notices)?;
    ensure!(
        (1..=512).contains(&notices.originals.len()),
        "invalid original notice count"
    );
    let mut original_legal = Vec::with_capacity(notices.originals.len());
    for row in notices.originals {
        ensure!(
            components.get(&row.component) == Some(&row.archive_sha256)
                && !row.original_path.is_empty()
                && row.original_path.len() <= 1024
                && !row.original_path.starts_with('/')
                && !row.original_path.contains('\\')
                && row
                    .original_path
                    .split('/')
                    .all(|c| !c.is_empty() && c != "." && c != ".."),
            "original legal source does not belong to a frozen dependency"
        );
        original_legal.push(OriginalNotice {
            component: row.component,
            source_sha256: row.archive_sha256,
            asset: Asset {
                name: row.staged_name,
                role: AssetRole::Notice,
                bytes: row.bytes,
                sha256: row.sha256,
            },
        });
    }
    let projection: BTreeMap<String, Sha256Digest> =
        serde_json::from_value(cfg["source"]["linux_sources"].clone())?;
    let posix_env = "source/onnxruntime/core/platform/posix/env.cc";
    ensure!(
        projection.len() == 6
            && projection.values().all(|v| *v != ZERO)
            && projection.get(posix_env) == Some(&pin(&boot, "/projected_env")?),
        "Linux bootstrap source differs from configured Env"
    );
    let stable_source_bytes = serde_json::to_vec(&projection)?;
    let value = Provenance {
        schema: "airk.native.linux-cuda-build-provenance/1".into(),
        target: TARGET.into(),
        static_core_build: digest(aggregate),
        ort_commit: "f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7".into(),
        source_archive_sha256: ort,
        source_projection_sha256: airk_core::identity::framed_digest(
            b"airk.native.linux-cuda-source-projection/1",
            &[&stable_source_bytes],
        ),
        cuda_version: "12.8.1".into(),
        cudnn_version: "9.10.2.21".into(),
        architectures: vec![[8, 6]],
        provider_shared_sha256: pin(&provider, "/shared/sha256")?,
        provider_cuda_sha256: pin(&provider, "/provider/sha256")?,
        aggregate_static_sha256: pin(&core, "/static/sha256")?,
        components,
        legal: original_legal,
    };
    let raw = serde_json::to_vec(&value)?;
    ensure!(raw.len() <= 1 << 20, "Linux public provenance capacity");
    Ok(raw)
}
