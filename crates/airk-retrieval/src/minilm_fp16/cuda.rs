// SPDX-License-Identifier: Apache-2.0
use super::{BatchInputs, MiniLmCudaModel, placement, pool};
use crate::arbiter::runtime::cuda::{
    CudaDevice, CudaRuntime, PlacementObservation, process_poisoned,
};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{mem::ManuallyDrop, sync::Arc, time::Instant};

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capacity {
    cold_peak_bytes: u64,
    warm_extra_bytes: u64,
    headroom_bytes: u64,
    host_bytes: usize,
}

impl Capacity {
    fn validate(self) -> Result<Self> {
        ensure!(
            self.cold_peak_bytes > 0
                && self.headroom_bytes > 0
                && self.warm_extra_bytes <= self.cold_peak_bytes
                && self.host_bytes > 0,
            "invalid MiniLM CUDA capacity evidence"
        );
        self.cold_peak_bytes
            .checked_add(self.headroom_bytes)
            .context("MiniLM CUDA capacity overflow")?;
        Ok(self)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Qualification {
    asset_sha256: Sha256Digest,
    graph_sha256: Sha256Digest,
    runtime_manifest_sha256: Sha256Digest,
    placement_sha256: Sha256Digest,
    adapter_uuid: [u8; 16],
    driver_sha256: Sha256Digest,
    driver_api_version: i32,
    capacity: Capacity,
}

pub struct MiniLmCudaAdmission {
    row: Qualification,
    native_development: bool,
}

impl MiniLmCudaAdmission {
    pub fn adapter_uuid(&self) -> [u8; 16] {
        self.row.adapter_uuid
    }

    pub fn host_bytes(&self) -> usize {
        self.row.capacity.host_bytes
    }

    pub fn native_development(&self) -> bool {
        self.native_development
    }
}

#[cfg(feature = "test-support")]
pub struct MiniLmCudaProbe {
    pub asset_sha256: Sha256Digest,
    pub graph_sha256: Sha256Digest,
    pub runtime_manifest_sha256: Sha256Digest,
    pub placement_sha256: Sha256Digest,
    pub adapter_uuid: [u8; 16],
    pub cold_peak_bytes: u64,
    pub warm_extra_bytes: u64,
    pub headroom_bytes: u64,
    pub host_bytes: usize,
}

#[derive(Debug)]
pub struct CudaBatchFailure {
    error: anyhow::Error,
    settled: bool,
}

impl CudaBatchFailure {
    pub fn settled(&self) -> bool {
        self.settled
    }
}

impl std::fmt::Display for CudaBatchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:#}", self.error)
    }
}

impl std::error::Error for CudaBatchFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error.as_ref())
    }
}

pub struct MiniLmCudaSession {
    native: NativeSession,
    capacity: Capacity,
    asset_sha256: Sha256Digest,
    placement: PlacementObservation,
}

struct NativeSession {
    session: ManuallyDrop<Session>,
    device: Arc<CudaDevice>,
    load_deadline: Instant,
}

impl MiniLmCudaSession {
    pub fn placement(&self) -> &PlacementObservation {
        &self.placement
    }

    pub fn placement_sha256(&self) -> Result<Sha256Digest> {
        placement::identity(&self.placement)
    }

    pub fn close(self) -> Result<()> {
        let device = self.native.device.clone();
        drop(self);
        let settled = !device.poisoned();
        drop(device);
        ensure!(
            settled && !process_poisoned(),
            "MiniLM CUDA teardown settlement is unknown"
        );
        Ok(())
    }
}

impl Drop for NativeSession {
    fn drop(&mut self) {
        let _ = self.device.synchronize(self.load_deadline);
        if self.device.poisoned() {
            std::mem::forget(self.device.clone());
            return;
        }
        // SAFETY: the exact held device completed pending native operations.
        unsafe { ManuallyDrop::drop(&mut self.session) };
        let _ = self.device.synchronize(self.load_deadline);
        if self.device.poisoned() {
            std::mem::forget(self.device.clone());
        }
    }
}

impl MiniLmCudaModel {
    pub fn compiled_cuda_admission() -> Result<Option<MiniLmCudaAdmission>> {
        let product = option_env!("AIRK_APPROVED_MINILM_CUDA_QUALIFICATION");
        #[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
        let development = option_env!("AIRK_APPROVED_MINILM_CUDA_DEVELOPMENT");
        #[cfg(not(all(feature = "test-support", windows, target_arch = "x86_64")))]
        let development: Option<&str> = None;
        let (encoded, native_development) = match (product, development) {
            (Some(product), None) => (product, false),
            (None, Some(development)) => (development, true),
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => anyhow::bail!("ambiguous compiled MiniLM CUDA qualification"),
        };
        let row: Qualification =
            serde_json::from_str(encoded).context("invalid compiled MiniLM CUDA qualification")?;
        row.capacity.validate()?;
        ensure!(
            row.adapter_uuid != [0; 16],
            "invalid compiled MiniLM adapter UUID"
        );
        Ok(Some(MiniLmCudaAdmission {
            row,
            native_development,
        }))
    }

    pub fn open_cuda(
        &self,
        runtime: &Arc<CudaRuntime>,
        adapter_uuid: [u8; 16],
        deadline: Instant,
    ) -> Result<MiniLmCudaSession> {
        let admission =
            Self::compiled_cuda_admission()?.context("no compiled MiniLM CUDA qualification")?;
        let row = admission.row;
        let result = (|| -> Result<MiniLmCudaSession> {
            ensure!(
                runtime.is_product() != admission.native_development,
                "MiniLM CUDA runtime mode differs from qualification scope"
            );
            self.verify_pins(
                runtime,
                row.asset_sha256,
                row.graph_sha256,
                row.runtime_manifest_sha256,
            )?;
            ensure!(
                adapter_uuid == row.adapter_uuid,
                "MiniLM CUDA adapter differs from qualification"
            );
            let device = runtime.device(adapter_uuid, deadline)?;
            ensure!(
                device.driver_image_sha256() == row.driver_sha256
                    && device.driver_api_version() == row.driver_api_version,
                "MiniLM CUDA driver differs from qualification"
            );
            self.open_device(device, row.capacity, Some(row.placement_sha256), deadline)
        })();
        result.map_err(|error| {
            CudaBatchFailure {
                error,
                settled: runtime.minilm_device_settled(adapter_uuid),
            }
            .into()
        })
    }

    #[cfg(feature = "test-support")]
    pub fn probe_cuda(
        &self,
        runtime: &Arc<CudaRuntime>,
        probe: &MiniLmCudaProbe,
        deadline: Instant,
    ) -> Result<MiniLmCudaSession> {
        self.probe_device(runtime, probe, Some(probe.placement_sha256), deadline)
    }

    #[cfg(feature = "test-support")]
    pub fn observe_cuda_placement(
        &self,
        runtime: &Arc<CudaRuntime>,
        probe: &MiniLmCudaProbe,
        deadline: Instant,
    ) -> Result<PlacementObservation> {
        // This deliberately cannot issue a scoring handle. It observes actual
        // CPU staging even if the reviewed assignment rejects the graph.
        let session = self.probe_device(runtime, probe, None, deadline)?;
        let placement = session.placement.clone();
        session.close()?;
        Ok(placement)
    }

    #[cfg(feature = "test-support")]
    fn probe_device(
        &self,
        runtime: &Arc<CudaRuntime>,
        probe: &MiniLmCudaProbe,
        expected_placement: Option<Sha256Digest>,
        deadline: Instant,
    ) -> Result<MiniLmCudaSession> {
        self.verify_pins(
            runtime,
            probe.asset_sha256,
            probe.graph_sha256,
            probe.runtime_manifest_sha256,
        )?;
        let device = runtime.device(probe.adapter_uuid, deadline)?;
        self.open_device(
            device,
            Capacity {
                cold_peak_bytes: probe.cold_peak_bytes,
                warm_extra_bytes: probe.warm_extra_bytes,
                headroom_bytes: probe.headroom_bytes,
                host_bytes: probe.host_bytes,
            },
            expected_placement,
            deadline,
        )
    }

    fn verify_pins(
        &self,
        runtime: &CudaRuntime,
        asset_sha256: Sha256Digest,
        graph_sha256: Sha256Digest,
        runtime_manifest_sha256: Sha256Digest,
    ) -> Result<()> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            self.model_sha256.parse::<Sha256Digest>()? == asset_sha256
                && Sha256Digest::from_bytes(Sha256::digest(&self.cuda_graph).into())
                    == graph_sha256
                && graph_sha256 == placement::REVIEWED_GRAPH_SHA256.parse::<Sha256Digest>()?
                && runtime.manifest_identity() == runtime_manifest_sha256,
            "MiniLM CUDA graph, asset or runtime differs from required pin"
        );
        Ok(())
    }

    fn open_device(
        &self,
        device: Arc<CudaDevice>,
        capacity: Capacity,
        expected_placement: Option<Sha256Digest>,
        deadline: Instant,
    ) -> Result<MiniLmCudaSession> {
        let capacity = capacity.validate()?;
        let builder = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .with_parallel_execution(false)?
            .with_memory_pattern(false)?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;
        let claim = device.reserve_cold_load(
            capacity.cold_peak_bytes,
            capacity.headroom_bytes,
            deadline,
        )?;
        let result = device
            .configure_minilm_cuda(
                builder,
                &self.graph,
                &self.cuda_graph,
                usize::try_from(capacity.cold_peak_bytes)?,
                deadline,
            )
            .and_then(|builder| Ok(builder.commit_from_memory(&self.cuda_graph)?));
        let settlement = claim.settle();
        let session = match (result, settlement) {
            (Ok(session), Ok(())) => session,
            (Ok(session), Err(error)) => {
                if device.poisoned() {
                    std::mem::forget(session);
                    std::mem::forget(device);
                }
                return Err(error);
            }
            (Err(error), Ok(())) => return Err(error),
            (Err(error), Err(wait)) => {
                return Err(
                    error.context(format!("MiniLM CUDA load settlement also failed: {wait:#}"))
                );
            }
        };
        let native = NativeSession {
            session: ManuallyDrop::new(session),
            device,
            load_deadline: deadline,
        };
        validate_abi(&native.session)?;
        let placement = native.device.observe_minilm_placement(&native.session)?;
        if let Some(expected) = expected_placement {
            ensure!(
                placement::identity(&placement)? == expected,
                "actual MiniLM CUDA placement differs from qualification"
            );
        }
        ensure!(
            Instant::now() < deadline,
            "MiniLM CUDA admission deadline expired"
        );
        Ok(MiniLmCudaSession {
            native,
            capacity,
            asset_sha256: self.model_sha256.parse()?,
            placement,
        })
    }

    pub fn embed_cuda(
        &self,
        session: &mut MiniLmCudaSession,
        texts: &[&str],
        memory_bytes: usize,
        deadline: Instant,
    ) -> std::result::Result<(Vec<Vec<f32>>, crate::model::EmbeddingCacheStats), CudaBatchFailure>
    {
        let result = self.embed_cuda_inner(session, texts, memory_bytes, deadline);
        result.map_err(|error| CudaBatchFailure {
            error,
            settled: !session.native.device.poisoned() && !process_poisoned(),
        })
    }

    fn embed_cuda_inner(
        &self,
        owner: &mut MiniLmCudaSession,
        texts: &[&str],
        memory_bytes: usize,
        deadline: Instant,
    ) -> Result<(Vec<Vec<f32>>, crate::model::EmbeddingCacheStats)> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            owner.asset_sha256 == self.model_sha256.parse::<Sha256Digest>()?
                && memory_bytes >= owner.capacity.host_bytes,
            "MiniLM CUDA batch exceeds the admitted model or host budget"
        );
        let BatchInputs {
            ids,
            mask,
            types,
            width,
            count,
            mut stats,
        } = self.inputs(texts)?;
        let inputs = vec![
            (
                "input_ids",
                Tensor::from_array(([count, width], ids))?.into_dyn(),
            ),
            (
                "attention_mask",
                Tensor::from_array(([count, width], mask.clone()))?.into_dyn(),
            ),
            (
                "token_type_ids",
                Tensor::from_array(([count, width], types))?.into_dyn(),
            ),
        ];
        let output = owner.native.device.run_minilm_cuda(
            &mut owner.native.session,
            inputs,
            [count, width, 384],
            owner.capacity.warm_extra_bytes,
            owner.capacity.headroom_bytes,
            deadline,
        )?;
        let vectors = pool(&output, &mask, count, width)?;
        ensure!(
            Instant::now() < deadline,
            "MiniLM CUDA batch deadline expired"
        );
        stats.plan_cache_peak_bytes = u64::try_from(owner.capacity.host_bytes)?;
        Ok((vectors, stats))
    }
}

fn validate_abi(session: &Session) -> Result<()> {
    let expected = ["attention_mask", "input_ids", "token_type_ids"];
    let mut inputs = session
        .inputs
        .iter()
        .map(|input| input.name.as_str())
        .collect::<Vec<_>>();
    inputs.sort_unstable();
    ensure!(
        inputs == expected
            && session.outputs.len() == 1
            && session.outputs[0].name == "last_hidden_state",
        "MiniLM CUDA session input or output differs from the authenticated ABI"
    );
    Ok(())
}
