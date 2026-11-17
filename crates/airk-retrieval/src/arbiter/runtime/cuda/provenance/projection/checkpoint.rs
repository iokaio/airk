// SPDX-License-Identifier: Apache-2.0
//! Exact metadata bridge for the reviewed warm checkpoint composition.
use super::*;
const PLAN: &str = "6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97";
const FAILED: &str = "3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026";
const FIRST_RUNNER: &str = "b382a216d1a46a1245b6fc3f78289e9abb61a027eed5f86699713c0b4db0d619";
const FIRST_INPUT: &str = "68a5f3109e3ad78a5453a966fd1d00db4217096536f7b7f7253ef309d9a396eb";
const NEXT_RUNNER: &str = "797c6d2e1a97865f5a8b95adf4d68ad0d91e4cfcf72f8f43d0329a2bba352b1b";
const NEXT_INPUT: &str = "c118835f41e95820fed5645c7de59606f583ff403a32f13422d5bb6120168c55";
pub(super) const SOURCE_LOCK: &str =
    "549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d";
const IDS: [&str; 3] = [
    "flash-remainder",
    "llm-remainder",
    "final-provider-all-closure",
];
const MAX_METADATA: u64 = 16 << 20;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePin {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}
impl FilePin {
    fn resource(&self) -> Result<ResourceFilePin> {
        let p = ResourceFilePin {
            path: self.path.clone(),
            sha256: self.sha256,
        };
        p.validate()?;
        ensure!(
            self.bytes > 0 && self.bytes <= 2u64 << 30 && nonzero(self.sha256),
            "invalid checkpoint pin"
        );
        Ok(p)
    }
    fn read(&self, source: &dyn Source) -> Result<Vec<u8>> {
        ensure!(self.bytes <= MAX_METADATA, "checkpoint metadata capacity");
        let b = super::read(source, &self.resource()?, MAX_METADATA)?;
        ensure!(
            b.len() as u64 == self.bytes,
            "checkpoint metadata length differs"
        );
        Ok(b)
    }
    fn json(&self, source: &dyn Source) -> Result<Value> {
        strict::parse(&self.read(source)?)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NinjaCustody {
    before_log: FilePin,
    before_deps: FilePin,
    after_log: FilePin,
    after_deps: FilePin,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    section_id: String,
    section_index: u64,
    receipt: FilePin,
    ninja_state_custody: NinjaCustody,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetClosure {
    sha256: Sha256Digest,
    count: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    schema: String,
    target: String,
    ort_version: String,
    build_receipt: FilePin,
    producer_source: FilePin,
    approved_plan_sha256: Sha256Digest,
    #[serde(rename = "original_failed_R12")]
    original_failed_r12: FilePin,
    failed_ancestry: Vec<FilePin>,
    sections: Vec<Section>,
    source_manifest: FilePin,
    source_inventory_sha256: Sha256Digest,
    frontend_inventory_sha256: Sha256Digest,
    cuda_inventory_sha256: Sha256Digest,
    frozen_input_closure_sha256: Sha256Digest,
    ninja: FilePin,
    static_target_closure: TargetClosure,
    libraries: Vec<Output>,
    provider_dlls: Vec<Output>,
    toolchain_pins: Vec<FilePin>,
    qualification: bool,
    activation_pins_available: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Composite {
    schema: String,
    status: String,
    mode: String,
    qualification: bool,
    cold_build_qualified: bool,
    plan: FilePin,
    producer_source: FilePin,
    input_contract: FilePin,
    original_failed_build: FilePin,
    failed_ancestry: Vec<FilePin>,
    warm_baseline: FilePin,
    warm_final: FilePin,
    sections: Vec<Section>,
    source_manifest: FilePin,
    actual_build_root: String,
    source_inventory_sha256: Sha256Digest,
    frontend_inventory_sha256: Sha256Digest,
    cuda_inventory_sha256: Sha256Digest,
    frozen_original_inputs_sha256: Sha256Digest,
    ninja_sha256: Sha256Digest,
    target_closure_sha256: Sha256Digest,
    configure_argv: Vec<String>,
    configured_preflight: Value,
    toolchain: Value,
    policy: Value,
    libraries: Vec<Output>,
    provider_dlls: Vec<Output>,
    original_attempt_started_utc: String,
    completed_utc: String,
    global_campaign_cutoff_utc: String,
    total_active_section_seconds: f64,
    wall_since_first_attempt_seconds: f64,
    output_policy: Value,
    metadata_composition_seconds: f64,
}
pub(in crate::arbiter::runtime::cuda) struct CheckpointedProducer {
    pub static_core_build: Sha256Digest,
    pub provider_dlls: Vec<PublicFile>,
    pub(super) history: CheckpointHistory,
    pub(super) corrected_source: Sha256Digest,
    pub(super) producer: Value,
    pub(super) build: Value,
}
fn typed<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
    Ok(serde_json::from_value(value.clone())?)
}
fn pin(value: &Value) -> Result<FilePin> {
    let p: FilePin = typed(value)?;
    p.resource()?;
    Ok(p)
}
fn finite(value: &Value) -> Result<f64> {
    let n = value.as_f64().context("checkpoint timing missing")?;
    ensure!(n.is_finite() && n >= 0.0, "invalid checkpoint timing");
    Ok(n)
}
// Strict bounded RFC3339 UTC, preserving fractional seconds rather than discarding them.
fn utc(value: &str) -> Result<i128> {
    let v = value
        .strip_suffix("+00:00")
        .or_else(|| value.strip_suffix('Z'))
        .context("checkpoint UTC offset refused")?;
    ensure!(
        v.len() >= 19
            && v.len() <= 29
            && v.as_bytes()[4] == b'-'
            && v.as_bytes()[7] == b'-'
            && v.as_bytes()[10] == b'T'
            && v.as_bytes()[13] == b':'
            && v.as_bytes()[16] == b':',
        "checkpoint UTC shape refused"
    );
    fn number(v: &str) -> Result<u32> {
        ensure!(
            v.bytes().all(|x| x.is_ascii_digit()),
            "checkpoint UTC digit refused"
        );
        Ok(v.parse()?)
    }
    ensure!(v.is_ascii(), "checkpoint UTC not ASCII");
    let year = number(&v[0..4])?;
    let month = number(&v[5..7])?;
    let day = number(&v[8..10])?;
    let hour = number(&v[11..13])?;
    let minute = number(&v[14..16])?;
    let second = number(&v[17..19])?;
    ensure!(
        (2000..=2100).contains(&year)
            && (1..=12).contains(&month)
            && hour < 24
            && minute < 60
            && second < 60,
        "checkpoint UTC range refused"
    );
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(
        day > 0 && day <= months[month as usize - 1],
        "checkpoint UTC day refused"
    );
    let mut days: i128 = (2000..year).map(|y| if leap(y) { 366 } else { 365 }).sum();
    days += months[..month as usize - 1]
        .iter()
        .map(|x| i128::from(*x))
        .sum::<i128>()
        + i128::from(day)
        - 1;
    let fractional = if v.len() == 19 {
        0
    } else {
        ensure!(
            v.as_bytes()[19] == b'.' && v.len() > 20,
            "checkpoint fractional UTC refused"
        );
        let f = &v[20..];
        i128::from(number(f)?) * 10i128.pow(9 - f.len() as u32)
    };
    Ok(
        (days * 86400 + i128::from(hour) * 3600 + i128::from(minute) * 60 + i128::from(second))
            * 1_000_000_000
            + fractional,
    )
}
fn natural(command: &Value) -> Result<()> {
    let tree = &command["actual_tree_result"];
    ensure!(
        successful(command)
            && command["owned_job_final_active_processes"] == 0
            && command["owned_job_final_member_pids"] == serde_json::json!([])
            && tree["schema"] == "airk.private.process-tree-result/1"
            && tree["exit_code"] == 0
            && tree["wait"] == "Start-Process-Wait-process-and-descendants"
            && tree["command_pid"].as_u64().is_some_and(|x| x > 0)
            && tree["wrapper_pid"].as_u64().is_some_and(|x| x > 0),
        "checkpoint original whole tree/Job not naturally settled"
    );
    Ok(())
}
fn clocks(v: &Value) -> Result<(i128, i128, f64)> {
    let text = |key: &str| -> Result<&str> { v[key].as_str().context("checkpoint clock missing") };
    let started = utc(text("started_utc")?)?;
    let finished = utc(text("completed_utc")?)?;
    let cutoff = utc(text("absolute_cutoff_utc")?)?;
    let campaign = utc(text("global_campaign_cutoff_utc")?)?;
    let fixed = utc("2026-11-19T05:29:42Z")?;
    let elapsed = finite(&v["elapsed_seconds"])?;
    let observed = finite(&v["observed_monotonic_seconds"])?;
    let converted = finite(&v["converted_monotonic_deadline"])?;
    ensure!(
        v["original_section_seconds"] == 10800
            && campaign == fixed
            && cutoff == (started + 10_800_000_000_000).min(campaign)
            && started <= finished
            && finished <= cutoff
            && elapsed <= 10800.0
            && elapsed <= (cutoff - started) as f64 / 1e9
            && converted > observed
            && converted - observed <= 10800.0,
        "checkpoint original section/campaign cutoff refused"
    );
    let tree = &v["command"]["actual_tree_result"];
    let tree_start = utc(tree["start_utc"]
        .as_str()
        .context("actual tree start absent")?)?;
    let tree_end = utc(tree["finished_utc"]
        .as_str()
        .context("actual tree finish absent")?)?;
    ensure!(
        started <= tree_start && tree_start <= tree_end && tree_end <= finished,
        "checkpoint actual tree outside original phase"
    );
    Ok((started, finished, elapsed))
}
fn output_rows(rows: &[Output], providers: bool) -> Result<Vec<PublicFile>> {
    ensure!(
        !rows.is_empty() && rows.len() <= 128,
        "checkpoint output capacity"
    );
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for r in rows {
        ensure!(
            !r.path.is_empty()
                && r.path.len() <= 512
                && !r.path.contains('\\')
                && !r.path.starts_with('/')
                && !r.path.contains(':')
                && r.path
                    .split('/')
                    .all(|x| !x.is_empty() && x != "." && x != "..")
                && r.bytes > 0
                && r.bytes <= 2u64 << 30
                && nonzero(r.sha256)
                && seen.insert(r.path.to_ascii_lowercase()),
            "unsafe checkpoint output path/identity"
        );
        let name = r
            .path
            .rsplit('/')
            .next()
            .context("output basename absent")?
            .to_ascii_lowercase();
        if providers {
            ensure!(
                matches!(
                    name.as_str(),
                    "onnxruntime_providers_shared.dll" | "onnxruntime_providers_cuda.dll"
                ),
                "checkpoint unexpected provider DLL"
            );
        } else {
            ensure!(name.ends_with(".lib"), "checkpoint static library expected");
        }
        result.push(PublicFile {
            name,
            bytes: r.bytes,
            sha256: r.sha256,
        });
    }
    if providers {
        ensure!(
            result.len() == 2 && result[0].name != result[1].name,
            "checkpoint exact provider pair required"
        );
    }
    Ok(result)
}
fn same_bytes(a: &FilePin, b: &FilePin) -> bool {
    a.bytes == b.bytes && a.sha256 == b.sha256
}
fn custody(
    source: &dyn Source,
    v: &Value,
    row: &Section,
    index: usize,
    prior: Option<&Section>,
) -> Result<()> {
    let c = &row.ninja_state_custody;
    for (key, copy) in [
        ("ninja_log_before", &c.before_log),
        ("ninja_deps_before", &c.before_deps),
        ("ninja_log_after", &c.after_log),
        ("ninja_deps_after", &c.after_deps),
    ] {
        let original = pin(&v[key])?;
        ensure!(
            same_bytes(&original, copy),
            "checkpoint immutable Ninja bytes differ"
        );
        if index > 0 {
            ensure!(
                original == *copy,
                "checkpoint successor Ninja custody path differs"
            );
        }
        copy.read(source)?;
    }
    if let Some(p) = prior {
        ensure!(
            same_bytes(&p.ninja_state_custody.after_log, &c.before_log)
                && same_bytes(&p.ninja_state_custody.after_deps, &c.before_deps),
            "checkpoint Ninja continuity differs"
        );
    }
    Ok(())
}
/// Shared by scientific assembly and the distributable projector. No capability is returned.
struct Authority {
    plan: Sha256Digest,
    failed: Sha256Digest,
    failed_ancestry: Vec<Sha256Digest>,
    first_runner: Sha256Digest,
    first_input: Sha256Digest,
    next_runner: Sha256Digest,
    next_input: Sha256Digest,
    composer: Sha256Digest,
    composer_input: Sha256Digest,
}
pub(in crate::arbiter::runtime::cuda) fn validate_checkpointed_producer(
    source: &dyn Source,
    raw_composite: &ResourceFilePin,
    verified_proof: &ResourceFilePin,
) -> Result<CheckpointedProducer> {
    let authority = Authority {
        plan: PLAN.parse()?,
        failed: FAILED.parse()?,
        failed_ancestry: [
            "451852dda5ca840c58a84cec7e5352600a185d8a1cd6d72f2ecf059b01e3b1c4",
            "1c46afdfaf67626d08045e9744d941f40a88208ca33234ef8a72b9b2aa6fee1d",
            "cfec05100c3be0857f27e5fa928ca594d9ca8929e1e637d5f16a00ef79560c95",
            "8cd15ca9c79394a41e9234fdabbacd3abbfc0ce73978b11fade7b5c775321c6e",
            "213cbb2a8b98ec2bb560dcd923225e69d62d66c74c44ce1cf66db3fee3eba9a7",
            "b6f4035cfff70ef53c713ab762cd926875b3fc5993004c1c75b67c30797fc4df",
            FAILED,
        ]
        .iter()
        .map(|x| x.parse())
        .collect::<Result<_, _>>()?,
        first_runner: FIRST_RUNNER.parse()?,
        first_input: FIRST_INPUT.parse()?,
        next_runner: NEXT_RUNNER.parse()?,
        next_input: NEXT_INPUT.parse()?,
        composer: "096a69ca0500b240f5f0b54a9951decee26cff917be707702fb76861a3ee5199".parse()?,
        composer_input: "e610c60b7dc684a84e43d962888bf2a4ebf5bde165cad379bbba43b9191824ba"
            .parse()?,
    };
    validate_with_authority(source, raw_composite, verified_proof, &authority)
}
fn validate_with_authority(
    source: &dyn Source,
    raw_composite: &ResourceFilePin,
    verified_proof: &ResourceFilePin,
    authority: &Authority,
) -> Result<CheckpointedProducer> {
    let raw = super::json(source, raw_composite)?;
    let verified = super::json(source, verified_proof)?;
    // The strict duplicate-key pass has already covered all nested objects.
    canonical_sha(&verified)?;
    let b: Proof = typed(&verified)?;
    let p: Composite = typed(&raw)?;
    ensure!(
        b.schema == "airk.cuda-static-build-proof/2"
            && b.target == "x86_64-pc-windows-msvc"
            && b.ort_version == "1.30.0"
            && !b.qualification
            && !b.activation_pins_available
            && p.schema == "airk.private.cuda-checkpointed-build/1"
            && p.status == "PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED"
            && p.mode == "checkpointed-warm"
            && !p.qualification
            && !p.cold_build_qualified,
        "exact checkpoint producer/proof form required"
    );
    ensure!(
        b.build_receipt.sha256 == raw_composite.sha256
            && b.build_receipt.bytes
                == super::read(source, raw_composite, MAX_METADATA)?.len() as u64
            && b.producer_source == p.producer_source
            && b.approved_plan_sha256 == authority.plan
            && p.plan.sha256 == b.approved_plan_sha256
            && b.original_failed_r12 == p.original_failed_build
            && p.original_failed_build.sha256 == authority.failed
            && b.failed_ancestry == p.failed_ancestry
            && b.sections == p.sections
            && b.source_manifest == p.source_manifest
            && b.source_inventory_sha256 == p.source_inventory_sha256
            && b.frontend_inventory_sha256 == p.frontend_inventory_sha256
            && b.cuda_inventory_sha256 == p.cuda_inventory_sha256
            && b.frozen_input_closure_sha256 == p.frozen_original_inputs_sha256
            && b.static_target_closure.sha256 == p.target_closure_sha256
            && b.static_target_closure.count == 96
            && b.libraries == p.libraries
            && b.provider_dlls == p.provider_dlls,
        "checkpoint proof/composite closure differs"
    );
    let plan = p.plan.json(source)?;
    ensure!(
        plan["schema"] == "airk.private.cuda-checkpointed-build-plan/1"
            && plan["actual_build_root"] == p.actual_build_root
            && pin(&plan["previous_failed_R12"])? == p.original_failed_build
            && plan["configured_preflight"] == p.configured_preflight
            && sha(&plan["configured_preflight"]["ninja_sha256"])? == p.ninja_sha256
            && b.ninja.sha256 == p.ninja_sha256,
        "checkpoint approved plan/source identity differs"
    );
    ensure!(
        p.producer_source.sha256 == authority.composer
            && p.input_contract.sha256 == authority.composer_input,
        "checkpoint composer/input is not reviewed authority"
    );
    p.producer_source.read(source)?;
    p.input_contract.read(source)?;
    let failed = p.original_failed_build.json(source)?;
    let last = rows(&failed["commands"])?
        .last()
        .context("R12 failed command absent")?;
    ensure!(
        failed["schema"] == "airk.private.cuda-runtime-continuation/2"
            && failed["status"] == "FAILED_PRESERVED_NO_RETRY"
            && last["exit_code"] == 77
            && last["timed_out"] == true
            && last["owned_descendants_settled"] == true
            && last["owned_job_forced_termination"] == true
            && last["owned_job_final_active_processes"] == 0
            && failed["toolchain"] == p.toolchain
            && failed["configure_argv"] == serde_json::to_value(&p.configure_argv)?
            && failed["configured_preflight"] == p.configured_preflight
            && failed["policy"] == p.policy,
        "original R12 failure/identity was not preserved"
    );
    ensure!(
        p.failed_ancestry
            .iter()
            .map(|x| x.sha256)
            .collect::<Vec<_>>()
            == authority.failed_ancestry
            && p.failed_ancestry.contains(&p.original_failed_build),
        "checkpoint failed ancestry missing"
    );
    let mut failures = BTreeSet::new();
    for f in &p.failed_ancestry {
        ensure!(
            failures.insert(f.sha256),
            "duplicate checkpoint failed ancestor"
        );
        let v = f.json(source)?;
        ensure!(
            v["status"] == "FAILED_PRESERVED_NO_RETRY" || v["status"] == "FAILED",
            "checkpoint ancestor relabelled"
        );
    }
    let manifest = p.source_manifest.json(source)?;
    ensure!(
        manifest["schema"] == "airk.private.cuda-materialized-source/1"
            && manifest["actual_build_root"] == p.actual_build_root
            && sha(&manifest["source_inventory_sha256"])? == p.source_inventory_sha256
            && sha(&manifest["frontend_inventory_sha256"])? == p.frontend_inventory_sha256
            && sha(&manifest["cuda_inventory_sha256"])? == p.cuda_inventory_sha256
            && sha(&manifest["frozen_original_inputs_sha256"])? == p.frozen_original_inputs_sha256
            && manifest["tools"] == p.toolchain
            && manifest["configured_preflight"] == p.configured_preflight
            && manifest["configure_argv"] == serde_json::to_value(&p.configure_argv)?
            && manifest["policy"] == p.policy
            && sha(&manifest["failed_r12_sha256"])? == p.original_failed_build.sha256,
        "checkpoint materialized source manifest differs"
    );
    ensure!(
        b.toolchain_pins.len() >= 6 && b.toolchain_pins.len() <= 16,
        "checkpoint tool capacity"
    );
    let toolrows = p
        .toolchain
        .as_object()
        .context("checkpoint toolchain missing")?;
    let mut actual_tools = BTreeSet::new();
    for (name, tool) in toolrows.iter().filter(|(_, x)| x.is_object()) {
        let t = FilePin {
            path: tool["path"].as_str().context("tool path missing")?.into(),
            bytes: size(&tool["bytes"])?,
            sha256: sha(&tool["sha256"])?,
        };
        t.resource()?;
        ensure!(
            b.toolchain_pins.contains(&t) && actual_tools.insert(t.sha256),
            "checkpoint actual tool pin absent/duplicate"
        );
        source.authenticate_historical_tool(name, &t.resource()?, t.bytes, 1u64 << 30)?;
    }
    ensure!(
        actual_tools.len() == b.toolchain_pins.len(),
        "checkpoint extra tool pin"
    );
    b.ninja.read(source)?;
    let source_inventory = pin(&plan["current_source_inventory"])?;
    ensure!(
        source_inventory.sha256 == p.source_inventory_sha256,
        "checkpoint source inventory pin differs"
    );
    source_inventory.read(source)?;
    for (name, hash) in [
        ("frontend-source-files.json", p.frontend_inventory_sha256),
        ("cuda-toolkit-inputs.json", p.cuda_inventory_sha256),
        ("frozen-input-closure.json", p.frozen_original_inputs_sha256),
    ] {
        super::read(
            source,
            &ResourceFilePin {
                path: format!(
                    "{}/{}",
                    p.actual_build_root.trim_end_matches(['/', '\\']),
                    name
                ),
                sha256: hash,
            },
            MAX_METADATA,
        )?;
    }
    ensure!(
        manifest["corrected_capi_sha256"]
            == plan["configured_preflight"]["final_capi_source_sha256"],
        "checkpoint corrected source identity differs"
    );
    ensure!(
        p.sections.len() == 3 && rows(&plan["sections"])?.len() == 3,
        "checkpoint exact ordered three sections required"
    );
    let mut history = Vec::new();
    let mut elapsed = 0.0;
    let mut previous_end = None;
    let mut previous_receipt = None;
    let mut previous_warm = None;
    for (index, row) in p.sections.iter().enumerate() {
        ensure!(
            row.section_index == index as u64 && row.section_id == IDS[index],
            "checkpoint section ordering refused"
        );
        let v = row.receipt.json(source)?;
        ensure!(
            v["schema"] == "airk.private.cuda-checkpoint-section/1"
                && v["status"] == "PASS_TARGET_SECTION_ONLY_UNQUALIFIED"
                && v["qualification"] == false
                && v["cold_build_qualified"] == false
                && v["section_id"] == IDS[index]
                && v["section_index"] == index as u64
                && pin(&v["plan"])? == p.plan,
            "checkpoint actual successful section required"
        );
        let runner = pin(&v["runner"])?;
        let input = pin(&v["input_contract"])?;
        ensure!(
            runner.sha256
                == if index == 0 {
                    authority.first_runner
                } else {
                    authority.next_runner
                }
                && input.sha256
                    == if index == 0 {
                        authority.first_input
                    } else {
                        authority.next_input
                    },
            "checkpoint runner/input is not reviewed authority"
        );
        runner.read(source)?;
        input.read(source)?;
        let ancestors = rows(&v["failed_ancestry"])?;
        ensure!(
            ancestors.len() == p.failed_ancestry.len(),
            "checkpoint section failed ancestry missing"
        );
        for (declared, expected) in ancestors.iter().zip(&p.failed_ancestry) {
            let held = FilePin {
                path: declared["path"]
                    .as_str()
                    .context("ancestor path absent")?
                    .into(),
                bytes: size(&declared["bytes"])?,
                sha256: sha(&declared["sha256"])?,
            };
            ensure!(
                held == *expected
                    && declared["status"] == "FAILED_PRESERVED_NO_RETRY"
                    && declared["never_relabelled"] == true
                    && declared["scope"]
                        .as_str()
                        .is_some_and(|x| !x.is_empty() && x.len() <= 512),
                "checkpoint section failed ancestor changed"
            );
        }
        let expected = &plan["sections"][index];
        ensure!(
            v["native_argv"] == expected["argv"]
                && expected["id"] == IDS[index]
                && expected["parallel"] == 2
                && expected["fixed_original_seconds"] == 10800,
            "checkpoint native target/parallel changed"
        );
        ensure!(
            rows(&v["original_command_argv"])?.len() >= 2,
            "checkpoint actual original command absent"
        );
        natural(&v["command"])?;
        let (started, finished, seconds) = clocks(&v)?;
        ensure!(
            previous_end.is_none_or(|end| end <= started),
            "checkpoint sections overlap or reorder"
        );
        if let Some(prior) = previous_receipt.as_ref() {
            ensure!(
                pin(&v["predecessor"])? == *prior,
                "checkpoint predecessor differs"
            );
        } else {
            ensure!(
                v["predecessor"].is_null(),
                "first checkpoint predecessor must be null"
            );
        }
        let before = pin(&v["warm_checkpoint_before"])?;
        let after = pin(&v["warm_checkpoint_after"])?;
        ensure!(
            previous_warm
                .as_ref()
                .map_or(before == p.warm_baseline, |prior| prior == &before),
            "checkpoint warm continuity differs"
        );
        before.read(source)?;
        after.read(source)?;
        ensure!(
            pin(&v["source_manifest_before"])?.json(source)? == manifest
                && pin(&v["source_manifest_after"])?.json(source)? == manifest,
            "checkpoint section source/config changed"
        );
        custody(
            source,
            &v,
            row,
            index,
            index.checked_sub(1).map(|i| &p.sections[i]),
        )?;
        if index == 0 {
            ensure!(
                v["flash_outputs_completed"] == 48,
                "checkpoint Flash48 closure missing"
            );
        }
        if index == 1 {
            ensure!(
                v["target_outputs_completed"] == 43,
                "checkpoint LLM43 closure missing"
            );
        }
        if index == 2 {
            ensure!(
                v["target_outputs_completed"]
                    .as_u64()
                    .is_some_and(|x| x >= 96),
                "checkpoint final target closure missing"
            );
        }
        previous_end = Some(finished);
        previous_receipt = Some(row.receipt.clone());
        previous_warm = Some(after);
        elapsed += seconds;
        history.push(CheckpointSectionHistory {
            section_id: row.section_id.clone(),
            section_index: row.section_index,
            receipt_sha256: row.receipt.sha256,
        });
    }
    ensure!(
        previous_warm.as_ref() == Some(&p.warm_final),
        "checkpoint final warm identity differs"
    );
    let first = utc(&p.original_attempt_started_utc)?;
    let complete = utc(&p.completed_utc)?;
    let campaign = utc(&p.global_campaign_cutoff_utc)?;
    ensure!(
        first
            == utc(failed["original_start_utc"]
                .as_str()
                .context("original attempt start absent")?)?
            && campaign == utc("2026-11-19T05:29:42Z")?
            && first <= previous_end.context("last section absent")?
            && previous_end.is_some_and(|end| end <= complete)
            && complete <= campaign
            && p.total_active_section_seconds.is_finite()
            && p.wall_since_first_attempt_seconds.is_finite()
            && p.metadata_composition_seconds.is_finite()
            && (0.0..=300.0).contains(&p.metadata_composition_seconds)
            && p.output_policy.as_object().is_some_and(|x| !x.is_empty())
            && p.total_active_section_seconds >= elapsed
            && p.total_active_section_seconds <= 32400.0
            && p.wall_since_first_attempt_seconds >= elapsed
            && p.wall_since_first_attempt_seconds <= (campaign - first) as f64 / 1e9,
        "checkpoint composition timing refused"
    );
    output_rows(&p.libraries, false)?;
    let provider_dlls = output_rows(&p.provider_dlls, true)?;
    for r in &p.provider_dlls {
        let path = format!(
            "{}/{}",
            p.actual_build_root.trim_end_matches(['/', '\\']),
            r.path
        );
        source.authenticate(
            &ResourceFilePin {
                path,
                sha256: r.sha256,
            },
            r.bytes,
            2u64 << 30,
        )?;
    }
    source.recheck()?;
    Ok(CheckpointedProducer {
        static_core_build: raw_composite.sha256,
        provider_dlls,
        corrected_source: sha(&manifest["corrected_capi_sha256"])?,
        history: CheckpointHistory {
            mode: "checkpointed-warm".into(),
            cold_build_qualified: false,
            qualification: false,
            approved_plan_sha256: b.approved_plan_sha256,
            failed_ancestry: p.failed_ancestry.iter().map(|x| x.sha256).collect(),
            sections: history,
        },
        producer: raw,
        build: verified,
    })
}
#[cfg(test)]
mod tests;
