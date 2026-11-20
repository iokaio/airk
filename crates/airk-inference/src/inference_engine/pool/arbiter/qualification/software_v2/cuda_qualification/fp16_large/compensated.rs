// SPDX-License-Identifier: Apache-2.0
//! Three separate original processes for the compensated Windows Large control, graph and score.
use super::*;

enum Profile {
    Operand,
    Rounded,
    Direct,
}
fn observe(profile: Profile, scored: bool) -> Result<()> {
    let operand = matches!(profile, Profile::Operand);
    let deadline = Instant::now() + Duration::from_secs(120);
    let host = if operand { 4 << 30 } else { 22 << 30 };
    let job = job_limits(host, host)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let campaign = root
        .parent()
        .context("scientific provider staging parent absent")?;
    let gate = if scored {
        let value = std::env::var("AIRK_LARGE_CUTLASS_TYPED_GRAPH")?;
        ensure!(
            value.len() == 64 && value.bytes().all(|v| v.is_ascii_hexdigit()),
            "invalid typed graph gate"
        );
        Some((
            std::env::var("AIRK_LARGE_CUTLASS_WIDTH")?.parse::<usize>()?,
            value,
        ))
    } else {
        ensure!(
            std::env::var_os("AIRK_LARGE_CUTLASS_TYPED_GRAPH").is_none()
                && std::env::var_os("AIRK_LARGE_CUTLASS_WIDTH").is_none(),
            "unexpected scoring gate"
        );
        None
    };
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let result = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity().to_string()
                == "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
            "original scientific CU15 manifest required"
        );
        let selected = gate
            .as_ref()
            .map(|(width, digest)| (*width, digest.as_str()));
        if matches!(profile, Profile::Direct) {
            runtime.observe_large_direct(campaign, selected, deadline)
        } else {
            runtime.observe_large_compensated(campaign, operand, selected, deadline)
        }
    })??;
    drop(owner);
    drop(lease);
    drop(airk_store::service::ServiceLease::acquire(&control)?);
    println!(
        "AIRK_LARGE_COMPENSATED={}",
        json!({"job":job,"native":result})
    );
    Ok(())
}

#[test]
#[ignore = "owned Windows A6000 operand control"]
fn large_compensated_operand() -> Result<()> {
    observe(Profile::Operand, false)
}

#[test]
#[ignore = "owned Windows A6000 graph measurement"]
fn large_compensated_graph() -> Result<()> {
    observe(Profile::Rounded, false)
}

#[test]
#[ignore = "owned Windows A6000 numerical measurement"]
fn large_compensated_score() -> Result<()> {
    observe(Profile::Rounded, true)
}

#[test]
#[ignore = "owned Windows A6000 direct FP32 graph measurement"]
fn large_direct_graph() -> Result<()> {
    observe(Profile::Direct, false)
}

#[test]
#[ignore = "owned Windows A6000 direct FP32 score measurement"]
fn large_direct_score() -> Result<()> {
    observe(Profile::Direct, true)
}
