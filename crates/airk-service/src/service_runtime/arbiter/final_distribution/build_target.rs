// SPDX-License-Identifier: Apache-2.0
//! Actual retained native target evidence. No architecture inference from filenames.
use super::*;
use serde_json::{Value, json};
use std::io::{Seek, SeekFrom};
fn pin(value: &Value) -> Result<PinnedFile> {
    Ok(PinnedFile {
        path: PathBuf::from(value["path"].as_str().context("target evidence path")?),
        bytes: value["bytes"].as_u64().context("target evidence bytes")?,
        sha256: serde_json::from_value(value["sha256"].clone())?,
    })
}
fn bytes(value: &Value) -> Result<Vec<u8>> {
    pinned(&pin(value)?)
}
fn object(value: &Value) -> Result<Value> {
    Ok(serde_json::from_slice(&bytes(value)?)?)
}
fn machine(reader: &mut (impl Read + Seek)) -> Result<(u16, u16)> {
    let mut dos = [0u8; 64];
    reader.read_exact(&mut dos)?;
    ensure!(&dos[..2] == b"MZ", "actual DOS signature absent");
    let offset = u32::from_le_bytes(dos[60..64].try_into()?);
    ensure!((64..=(1 << 20)).contains(&offset), "actual PE offset bound");
    reader.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut header = [0u8; 26];
    reader.read_exact(&mut header)?;
    ensure!(&header[..4] == b"PE\0\0", "actual PE signature absent");
    Ok((
        u16::from_le_bytes(header[4..6].try_into()?),
        u16::from_le_bytes(header[24..26].try_into()?),
    ))
}
fn actual_image(value: &Value, expected: Sha256Digest) -> Result<()> {
    let input = pin(value)?;
    ensure!(
        input.sha256 == expected && input.bytes > 0 && input.bytes <= (4u64 << 30),
        "actual preserved image identity/bound differs"
    );
    let map = serde_json::Map::from_iter([(
        input.path.to_str().context("image UTF8 path")?.into(),
        json!(input.sha256),
    )]);
    verify_named_seals(&map)?;
    let mut file = fs::File::open(&input.path)?;
    ensure!(
        file.metadata()?.len() == input.bytes,
        "actual preserved image length differs"
    );
    ensure!(
        machine(&mut file)? == (0x8664, 0x020b),
        "actual preserved PE target differs from x64/PE32+"
    );
    Ok(())
}
fn build(value: &Value, source: &Value) -> Result<()> {
    ensure!(
        value["schema"] == "airk-readiness-command/1"
            && value["exit_code"] == 0
            && value["inputs_unchanged"] == true
            && value["changed_groups"] == json!([])
            && value["runner_unchanged"] == true
            && value["runtime_unchanged"] == true
            && value["command_inputs_unchanged"] == true
            && value["groups"]["native"]["sha256"] == *source,
        "actual successful unchanged native build absent"
    );
    let command = value["command"]
        .as_array()
        .context("actual native Cargo command absent")?;
    ensure!(
        !command
            .iter()
            .filter_map(Value::as_str)
            .any(|value| value == "--target" || value.starts_with("--target=")),
        "target proof does not cover a cross-target command"
    );
    Ok(())
}
fn cargo(value: &Value, actual: &Value, library: &str) -> Result<()> {
    let raw = bytes(value)?;
    let text = std::str::from_utf8(&raw)?;
    let rows = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    ensure!(
        rows.iter().filter(|row| *row == actual).count() == 1
            && actual["reason"] == "compiler-artifact"
            && actual["profile"]["test"] == true
            && actual["executable"].is_string(),
        "actual Cargo image selector absent/ambiguous"
    );
    ensure!(
        rows.iter()
            .any(|row| row["reason"] == "build-script-executed"
                && row["package_id"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("#ort-sys@2.0.0-rc.10"))
                && row["linked_libs"] == json!(["static=onnxruntime"])
                && row["linked_paths"] == json!([format!("native={library}")])),
        "actual Cargo image lacks recorded MSVC static ORT linkage"
    );
    Ok(())
}
fn validate_inner(
    input: &PinnedFile,
    decision: &RecordedDecision,
    target: &str,
    depth: usize,
) -> Result<(BTreeMap<Sha256Digest, String>, String)> {
    ensure!(depth < 8, "native target evidence ancestry bound");
    ensure!(
        decision
            .approved_component_target_provenances
            .contains(&input.sha256),
        "actual build target record not separately Root-reviewed"
    );
    let evidence: Value = serde_json::from_slice(&pinned(input)?)?;
    ensure!(
        evidence["schema"] == "airk.private-native-build-target-evidence/1"
            && evidence["complete"] == true
            && evidence["target"] == target
            && target == "x86_64-pc-windows-msvc",
        "unknown/incomplete/foreign actual target evidence"
    );
    let basis = &evidence["target_basis"];
    let (mut targets, library) = if let Some(parent) =
        basis.get("retained_original_target_evidence")
    {
        let path = PathBuf::from(
            parent["path"]
                .as_str()
                .context("parent target record path")?,
        );
        let parent = PinnedFile {
            bytes: fs::symlink_metadata(&path)?.len(),
            path,
            sha256: serde_json::from_value(parent["sha256"].clone())?,
        };
        let parent = validate_inner(&parent, decision, target, depth + 1)?;
        let cached = object(&basis["current_cached_rustc_target"])?;
        ensure!(
            basis["no_explicit_cross_target_argument_in_actual_build_commands"] == true
                && cached["outputs"]
                    .as_object()
                    .context("current rustc cached cfg")?
                    .values()
                    .any(|value| value["success"] == true
                        && value["code"] == 0
                        && value["stdout"].as_str().is_some_and(|text| [
                            "target_arch=\"x86_64\"",
                            "target_env=\"msvc\"",
                            "target_os=\"windows\""
                        ]
                        .iter()
                        .all(|key| text.lines().any(|line| line == *key)))),
            "actual additive host target proof differs"
        );
        parent
    } else {
        ensure!(
            basis["static_ORT_target"] == target
                && basis["no_explicit_cross_target_argument_in_retained_R92_R95_commands"] == true
                && basis["historical_ambient_CARGO_BUILD_TARGET_was_not_recorded"] == true,
            "target evidence historical limitation or ABI basis misrepresented"
        );
        let probe = bytes(&basis["rustc_probe"])?;
        let probe = std::str::from_utf8(&probe)?;
        ensure!(
            basis["rustc_probe"]["exit_code"] == 0
                && probe
                    .lines()
                    .any(|line| line.strip_prefix("host: ") == Some(target)),
            "actual rustc host proof absent"
        );
        let cached = object(&basis["cached_rustc_target"])?;
        ensure!(
            cached["outputs"]
                .as_object()
                .context("recorded rustc target cache absent")?
                .values()
                .any(|value| value["success"] == true
                    && value["code"] == 0
                    && value["stdout"].as_str().is_some_and(|text| [
                        "target_arch=\"x86_64\"",
                        "target_env=\"msvc\"",
                        "target_os=\"windows\""
                    ]
                    .iter()
                    .all(|key| text.lines().any(|line| line == *key)))),
            "recorded native rustc target cfg differs"
        );
        let ort = object(&basis["static_ORT_build_receipt"])?;
        let aggregate = object(&basis["static_ORT_aggregate_receipt"])?;
        ensure!(
            ort["target"] == target
                && ort["result"] == "libraries_built_not_yet_model_or_static_link_qualified"
                && aggregate["result"] == "genuine_aggregate_not_yet_final_link_or_model_qualified"
                && aggregate["output"] == basis["static_library_recorded_identity"],
            "actual original static ORT build/aggregate differs"
        );
        let env = object(&basis["static_ORT_cargo_env"])?;
        let library = env["environment"]["ORT_LIB_LOCATION"]
            .as_str()
            .context("actual static ORT lib location absent")?
            .to_owned();
        ensure!(
            env["environment"]["ORT_SKIP_DOWNLOAD"] == "1"
                && env["environment"]["ORT_PREFER_DYNAMIC_LINK"] == "0",
            "actual static-only ORT build environment differs"
        );
        for record in basis["build_configuration_records"]
            .as_array()
            .context("actual native build configuration absent")?
        {
            bytes(record)?;
        }
        (BTreeMap::new(), library)
    };
    let images = evidence["images"]
        .as_array()
        .context("actual image target roster absent")?;
    ensure!(!images.is_empty(), "empty image target roster");
    let mut local_images = BTreeSet::new();
    for image in images {
        let digest: Sha256Digest = serde_json::from_value(image["image_sha256"].clone())?;
        ensure!(
            image["target"] == target && local_images.insert(digest),
            "foreign/duplicate image target"
        );
        match image["evidence_kind"]
            .as_str()
            .context("image target evidence kind")?
        {
            "materialized_current_PE_and_recorded_build" => {
                actual_image(&image["materialized_image"], digest)?;
                ensure!(
                    image["pe"]["machine_u16"] == 0x8664
                        && image["pe"]["optional_header_magic_u16"] == 0x020b,
                    "reported actual PE fields differ"
                );
                cargo(
                    &image["cargo_json"],
                    &image["actual_cargo_artifact"],
                    &library,
                )?;
                build(
                    &object(&image["build_receipt"])?,
                    &image["native_source_group"],
                )?;
                ensure!(
                    image["materialized_image"]["original_path"]
                        == image["actual_cargo_artifact"]["executable"],
                    "preserved actual image original Cargo identity differs"
                );
            }
            "historical_recorded_build_static_ABI_and_Windows_owned_image" => {
                ensure!(
                    image.get("materialized_image") == Some(&Value::Null)
                        && image.get("pe") == Some(&Value::Null)
                        && image["historical_image_bytes_overwritten"] == true,
                    "historical image pretends current bytes or PE evidence"
                );
                let identity = object(&image["original_image_identity"])?;
                ensure!(
                    identity["sha256"] == json!(digest)
                        && identity["source_group"] == image["native_source_group"]
                        && identity["source_receipt_sha256"]
                            == image["original_build_receipt"]["sha256"]
                        && identity["actual_cargo_artifact"] == image["actual_cargo_artifact"],
                    "historical actual image/build binding differs"
                );
                cargo(
                    &image["original_cargo_json"],
                    &image["actual_cargo_artifact"],
                    &library,
                )?;
                build(
                    &object(&image["original_build_receipt"])?,
                    &image["native_source_group"],
                )?;
                let proofs = image["owned_Windows_image_proofs"]
                    .as_array()
                    .context("original Windows owned image proofs absent")?;
                ensure!(
                    !proofs.is_empty(),
                    "historical image lacks real execution proof"
                );
                for proof in proofs {
                    let execution = object(&proof["execution"])?;
                    settled_owner(&execution)?;
                    let request = object(&proof["request"])?;
                    ensure!(
                        proof["image_json_pointer"] == "/owned/suspended_assignment/image"
                            && execution["owned"]["suspended_assignment"]["image"]
                                == proof["captured_image"]
                            && proof["captured_image"]["sha256"] == json!(digest)
                            && request["packet_sha256"] == execution["packet_sha256"],
                        "historical Windows execution actual image differs"
                    );
                }
            }
            _ => anyhow::bail!("unknown native target evidence selector"),
        }
        if let Some(prior) = targets.insert(digest, target.into()) {
            ensure!(prior == target, "conflicting ancestor target evidence");
        }
    }
    Ok((targets, library))
}
pub(super) fn validate(
    input: &PinnedFile,
    decision: &RecordedDecision,
    target: &str,
) -> Result<BTreeMap<Sha256Digest, String>> {
    Ok(validate_inner(input, decision, target, 0)?.0)
}
#[cfg(test)]
pub(super) fn refusal_cases() -> Result<Vec<Value>> {
    let mut bytes = vec![0u8; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&0x020bu16.to_le_bytes());
    ensure!(
        machine(&mut std::io::Cursor::new(&bytes))? == (0x8664, 0x020b),
        "synthetic PE control"
    );
    let mut cases = Vec::new();
    for index in [0usize, 64usize] {
        let mut changed = bytes.clone();
        changed[index] = 0;
        ensure!(
            machine(&mut std::io::Cursor::new(changed)).is_err(),
            "malformed PE header accepted"
        );
        cases.push(json!({"id":format!("target_PE_signature_{index}_refused"),"accepted":false}));
    }
    Ok(cases)
}
