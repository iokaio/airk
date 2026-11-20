// SPDX-License-Identifier: Apache-2.0
//! Original GNU parent custody and graph-only inspection of the protected Compact candidate.
use super::*;
use std::os::unix::fs::MetadataExt;

const SCIENTIFIC: &str = "3c123dc7db0f0bd80ad2d35b68ca8d7bdbbf3380db7dfaad3513fadbec0f48cc";
#[derive(Clone, Copy)]
enum GraphModel {
    Compact,
    Large,
}

fn custody(host: u64, backing: u64) -> Result<u64> {
    let available = host
        .checked_sub(backing)
        .context("invalid sealed ELF reservation")?;
    let uid = unsafe { libc::getuid() };
    ensure!(
        uid == 1000 && unsafe { libc::geteuid() } == uid,
        "unexpected original GNU user"
    );
    let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: the native kernel fills one complete rlimit on success.
    ensure!(
        unsafe { libc::getrlimit(libc::RLIMIT_AS, limit.as_mut_ptr()) } == 0,
        "missing GNU address-space cap"
    );
    let limit = unsafe { limit.assume_init() };
    let status = std::fs::read_to_string("/proc/self/status")?;
    let mut sig = 0;
    // SAFETY: PR_GET_PDEATHSIG writes the configured signal to a valid integer.
    ensure!(
        limit.rlim_cur == available
            && limit.rlim_max == available
            && unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut sig) } == 0
            && sig == libc::SIGKILL
            && status
                .lines()
                .any(|s| s.split_whitespace().eq(["Seccomp:", "2"]))
            && status
                .lines()
                .any(|s| s.split_whitespace().eq(["NoNewPrivs:", "1"])),
        "bounded independent GNU parent custody required"
    );
    Ok(available)
}

#[test]
#[ignore = "owned GNU Compact protected CUTLASS graph observation; no scoring"]
fn compact_gnu_cutlass_graph() -> Result<()> {
    owned_graph(GraphModel::Compact)
}

#[test]
#[ignore = "owned GNU Large direct FP32 protected CUTLASS graph observation; no scoring"]
fn large_gnu_cutlass_graph() -> Result<()> {
    owned_graph(GraphModel::Large)
}

fn owned_graph(model: GraphModel) -> Result<()> {
    let (host, clock) = match model {
        GraphModel::Compact => (32 << 30, "AIRK_COMPACT_CUTLASS_EXPIRES_NS"),
        GraphModel::Large => (48 << 30, "AIRK_LARGE_CUTLASS_EXPIRES_NS"),
    };
    let expiry: u64 = std::env::var(clock)?.parse()?;
    let remaining = expiry
        .checked_sub(airk_process::monotonic::now_ns()?)
        .context("GNU graph deadline elapsed")?;
    ensure!(
        (1..=120_000_000_000).contains(&remaining),
        "invalid absolute GNU graph deadline"
    );
    let deadline = Instant::now() + Duration::from_nanos(remaining);
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let metadata = std::fs::symlink_metadata(&manifest)?;
    ensure!(
        metadata.is_file() && metadata.uid() == 1000 && metadata.len() <= 1 << 20,
        "unexpected native ELF descriptor"
    );
    let raw = std::fs::read(&manifest)?;
    ensure!(
        sha(&raw).to_string() == SCIENTIFIC,
        "original GNU scientific descriptor required"
    );
    let backing = cuda::linux_cuda_private_backing(&raw)?;
    let address_space = custody(host, backing)?;
    let campaign =
        Path::new("/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z");
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let graph = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity().to_string() == SCIENTIFIC,
            "GNU static/ELF identity differs"
        );
        match model {
            GraphModel::Compact => {
                runtime.observe_protected_cutlass_model(campaign, None, deadline)
            }
            GraphModel::Large => runtime.observe_large_direct_gnu_graph(campaign, deadline),
        }
    })??;
    drop(owner);
    drop(lease);
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    let result = match model {
        GraphModel::Compact => {
            ensure!(
                graph["timing"].is_null()
                    && graph["cal"].is_null()
                    && graph["cal_native_forwards"] == 0,
                "Compact graph observer dispatched extra work"
            );
            &graph
        }
        GraphModel::Large => &graph["result"],
    };
    ensure!(
        result["score"].is_null() && result["graph"].is_object() && Instant::now() < deadline,
        "expected a load-only native graph within original deadline"
    );
    write_json(
        &output()?,
        &serde_json::json!({"native":graph,"pid":std::process::id(),"uid":1000,
        "descriptor_sha256":SCIENTIFIC,"private_elf_bytes":backing,"address_space":address_space,
        "host_limit":host,"control_reacquired":true,"native_forwards":0,"qualification":false}),
    )?;
    Ok(())
}
