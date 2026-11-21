// SPDX-License-Identifier: Apache-2.0
//! Admission evidence for an explicitly requested scientific availability tier.
use super::{Result, Sha256, Value, ensure, held, json};
use serde::Deserialize;
use sha2::Digest;
use std::{
    fs::OpenOptions,
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::Instant,
};

const GIB: u64 = 1 << 30;
const TIERS: [u64; 5] = [6, 8, 12, 16, 24];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Availability {
    nvidia_free: u64,
    wddm_free: u64,
    cuda_free_after_touch: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    schema: String,
    nonce: String,
    candidate_pid: u32,
    tier_gib: u64,
    adapter_uuid: String,
    adapter_luid: String,
    native_image_sha256: String,
    typed_graph_sha256: String,
    available_bytes: Availability,
    observed_monotonic_ns: u64,
    expires_monotonic_ns: u64,
}

fn uuid4(value: &str) -> bool {
    let raw = value.as_bytes();
    raw.len() == 36
        && [8, 13, 18, 23].into_iter().all(|i| raw[i] == b'-')
        && raw[14] == b'4'
        && matches!(raw[19], b'8' | b'9' | b'a' | b'b')
        && raw.iter().enumerate().all(|(i, ch)| {
            [8, 13, 18, 23].contains(&i) || ch.is_ascii_digit() || (b'a'..=b'f').contains(ch)
        })
}

fn gpu_uuid(bytes: &[u8; 16]) -> String {
    let digits = bytes.iter().map(|v| format!("{v:02x}")).collect::<String>();
    format!(
        "GPU-{}-{}-{}-{}-{}",
        &digits[..8],
        &digits[8..12],
        &digits[12..16],
        &digits[16..20],
        &digits[20..]
    )
}

fn pre_resume(available: &Availability, tier: u64) -> Result<u64> {
    ensure!(TIERS.contains(&tier), "unregistered pressure tier");
    let floor = tier * GIB;
    let local = available.nvidia_free.min(available.wddm_free);
    ensure!(
        local >= floor && available.cuda_free_after_touch >= floor && local <= floor + (128 << 20),
        "pre-resume CUDA/NVIDIA/WDDM availability outside registered tier"
    );
    Ok(floor)
}

pub(in super::super) enum Model {
    Compact,
    Large,
}

impl Model {
    fn namespace(&self) -> &'static str {
        match self {
            Self::Compact => "AIRK_COMPACT",
            Self::Large => "AIRK_LARGE",
        }
    }
}

pub(in super::super) fn admit(
    model: Model,
    tier: u64,
    expected_graph: &str,
    uuid: &[u8; 16],
    luid: &str,
    current_free: impl FnOnce() -> Result<u64>,
    deadline: Instant,
) -> Result<(u64, Value)> {
    let prefix = model.namespace();
    let name = std::env::var(format!("{prefix}_PRESSURE_NONCE"))?;
    let original_expiry = std::env::var(format!("{prefix}_CUTLASS_EXPIRES_NS"))?.parse::<u64>()?;
    let image = std::env::var(format!("{prefix}_CUTLASS_TEST_IMAGE_SHA"))?;
    let base = PathBuf::from(std::env::var("AIRK_PRIVATE_CENSUS_BASE")?);
    ensure!(
        base.is_absolute() && std::fs::symlink_metadata(&base)?.file_attributes() & 0x400 == 0,
        "invalid pressure custody directory"
    );
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(base.join("pressure-admission.json"))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && (1..=16384).contains(&metadata.len())
            && metadata.file_attributes() & 0x400 == 0,
        "invalid pressure grant"
    );
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let grant: Grant = serde_json::from_slice(&raw)?;
    let now = airk_process::monotonic::now_ns()?;
    ensure!(
        grant.schema == "airk.private.cuda-pressure-admission/1"
            && grant.nonce == name
            && uuid4(&name)
            && grant.candidate_pid == std::process::id()
            && grant.tier_gib == tier
            && grant.adapter_uuid == gpu_uuid(uuid)
            && grant.adapter_luid == luid
            && grant.typed_graph_sha256 == expected_graph
            && grant.native_image_sha256 == image
            && grant.observed_monotonic_ns <= now
            && now < grant.expires_monotonic_ns
            && grant.expires_monotonic_ns == original_expiry
            && original_expiry - now <= 120_000_000_000,
        "pressure grant process, artifact, physical adapter, graph or original expiry mismatch"
    );
    let executable = std::env::current_exe()?;
    let image_bytes = executable.metadata()?.len();
    ensure!(
        (1..=512 << 20).contains(&image_bytes),
        "unexpected pressure executable size"
    );
    let _image = held(&executable, image_bytes, &image, deadline)?;
    let floor = pre_resume(&grant.available_bytes, tier)?;
    let free = current_free()?;
    ensure!(
        airk_process::monotonic::now_ns()? < original_expiry,
        "pressure grant expired before memory check"
    );
    let evidence = json!({"grant_sha256":format!("{:x}",Sha256::digest(&raw)),"arena_limit_bytes":floor,
        "native_free_bytes":free,"tier_gib":tier,"adapter_uuid":grant.adapter_uuid,"adapter_luid":grant.adapter_luid,
        "pre_resume_available_bytes":{"nvidia_free":grant.available_bytes.nvidia_free,"wddm_free":grant.available_bytes.wddm_free,
            "cuda_free_after_touch":grant.available_bytes.cuda_free_after_touch},
        "observed_monotonic_ns":grant.observed_monotonic_ns,"expires_monotonic_ns":original_expiry});
    println!("{prefix}_PRESSURE_GO={evidence}");
    ensure!(
        free >= floor,
        "native pre-load CUDA free {free} below grant floor {floor} after primary-context initialization"
    );
    Ok((floor, evidence))
}

#[test]
fn protected_pressure_requires_preregistered_tier_and_three_independent_floors() {
    let tier = Availability {
        nvidia_free: 8 * GIB + 1,
        wddm_free: 8 * GIB,
        cuda_free_after_touch: 8 * GIB,
    };
    assert_eq!(pre_resume(&tier, 8).unwrap(), 8 * GIB);
    assert!(pre_resume(&tier, 7).is_err());
    assert!(
        pre_resume(
            &Availability {
                cuda_free_after_touch: 8 * GIB - 1,
                ..tier
            },
            8
        )
        .is_err()
    );
    assert!(
        pre_resume(
            &Availability {
                nvidia_free: 9 * GIB,
                wddm_free: 9 * GIB,
                ..tier
            },
            8
        )
        .is_err()
    );
    assert!(uuid4("98765432-1234-4abc-b012-000000000001"));
    assert!(!uuid4("98765432-1234-4abc-7012-000000000001"));
}
