// SPDX-License-Identifier: Apache-2.0
//! Embedding route choice on the original lease-owning inference lane.
use super::{Binding, Engine, RequestOwner};
use airk_core::inference_provider::{EmbeddingFallback, EmbeddingRoute, EmbeddingRouteObservation};
use airk_retrieval::embedding_worker::{Request, Response};
use anyhow::Result;
use std::sync::atomic::Ordering;

impl Engine {
    pub(super) fn retire_model(&mut self) -> Result<()> {
        #[cfg(feature = "gpu-cuda")]
        self.retire_embedding_cuda()?;
        self.model = None;
        self.model_lease = None;
        self.model_key = None;
        Ok(())
    }

    #[cfg(feature = "gpu-cuda")]
    pub(super) fn retire_embedding_cuda(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.embedding_unsettled.load(Ordering::Acquire),
            "CUDA embedding has unsettled native work"
        );
        if let Some(resident) = self.gpu_resident.take()
            && let Err(error) = resident.close()
        {
            self.embedding_unsettled.store(true, Ordering::Release);
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn handle_model_request(
        &mut self,
        binding: &Binding,
        owner: Option<RequestOwner<'_>>,
        request: Request,
    ) -> Result<Response> {
        let Request::Embed { texts, .. } = &request else {
            return super::super::handle(request, &mut self.model, &mut None);
        };
        let reason = if !self.allow_embedding_gpu.load(Ordering::Acquire) {
            #[cfg(feature = "gpu-cuda")]
            self.retire_embedding_cuda()?;
            EmbeddingFallback::Disabled
        } else {
            #[cfg(feature = "gpu-cuda")]
            match self.embed_on_cuda(binding, owner, texts, &request)? {
                Ok(embedded) => {
                    self.observe_embedding(&embedded, EmbeddingRoute::Cuda, None);
                    return Ok(embedded);
                }
                Err(reason) => reason,
            }
            #[cfg(not(feature = "gpu-cuda"))]
            EmbeddingFallback::RuntimeUnavailable
        };
        let _ = (binding, owner, texts);
        super::require_live(owner)?;
        let result = super::super::handle(request, &mut self.model, &mut None)?;
        self.observe_embedding(&result, EmbeddingRoute::Cpu, Some(reason));
        Ok(result)
    }

    fn observe_embedding(
        &self,
        response: &Response,
        route: EmbeddingRoute,
        fallback: Option<EmbeddingFallback>,
    ) {
        if matches!(response, Response::Embedded { vectors, .. } if vectors.iter().any(Option::is_some))
            && let Ok(mut observation) = self.embedding_observation.lock()
        {
            *observation = Some(EmbeddingRouteObservation { route, fallback });
        }
    }
}

#[cfg(feature = "gpu-cuda")]
mod cuda {
    use super::*;
    use airk_core::identity::Sha256Digest;
    use airk_core::model_objects::ModelAssetName;
    use airk_retrieval::{
        arbiter::runtime::cuda::process_poisoned,
        embedding_batch::{MAX_BATCH_CHUNKS, MAX_INFERENCE_WORKERS, pooled_model_bytes},
        embedding_worker::ModelLocation,
        minilm_fp16::{CudaBatchFailure, MiniLmCudaModel, MiniLmCudaSession},
        model::MAX_INPUT_BYTES,
        model_catalog::FP16_MODEL_SHA256,
    };
    use anyhow::{Context, ensure};

    pub(in crate::inference_engine::pool) struct Resident {
        key: Sha256Digest,
        session: MiniLmCudaSession,
        model: MiniLmCudaModel,
    }

    impl Resident {
        pub(in crate::inference_engine::pool) fn close(self) -> Result<()> {
            self.session.close()
        }
    }

    impl Engine {
        fn require_cuda_settlement(
            &self,
            settled: bool,
            details: std::fmt::Arguments<'_>,
        ) -> Result<()> {
            if !settled || process_poisoned() {
                self.embedding_unsettled.store(true, Ordering::Release);
                anyhow::bail!("CUDA embedding settlement unknown: {details}");
            }
            Ok(())
        }

        pub(super) fn embed_on_cuda(
            &mut self,
            binding: &Binding,
            owner: Option<RequestOwner<'_>>,
            texts: &[String],
            request: &Request,
        ) -> Result<std::result::Result<Response, EmbeddingFallback>> {
            let Request::Embed {
                workers,
                memory_bytes,
                ..
            } = request
            else {
                return Err(anyhow::anyhow!("embedding GPU request mismatch"));
            };
            ensure!(
                !texts.is_empty()
                    && texts.len() <= MAX_BATCH_CHUNKS
                    && texts.iter().all(|text| text.len() <= MAX_INPUT_BYTES)
                    && (1..=MAX_INFERENCE_WORKERS).contains(workers)
                    && (8 << 20..=pooled_model_bytes(MAX_INFERENCE_WORKERS) as usize)
                        .contains(memory_bytes),
                "embedding batch exceeds admission bounds"
            );
            let Some(admission) = MiniLmCudaModel::compiled_cuda_admission()? else {
                return Ok(Err(EmbeddingFallback::AdmissionRefused));
            };
            if *memory_bytes < admission.host_bytes() {
                return Ok(Err(EmbeddingFallback::AdmissionRefused));
            }
            // The CUDA batch has no blank-row ABI. Let the canonical CPU adapter
            // preserve sparse result slots for a request containing blank text.
            if texts.iter().any(|text| text.trim().is_empty()) {
                return Ok(Err(EmbeddingFallback::ModelUnsupported));
            }
            let Some((state, grant)) = owner.and_then(|owner| owner.authorization) else {
                return Ok(Err(EmbeddingFallback::AdmissionRefused));
            };
            let deadline = state.embedding_deadline(grant)?;
            let Binding::Model { location, key, .. } = binding else {
                return Ok(Err(EmbeddingFallback::ModelUnsupported));
            };
            let Some(lease) = self.gpu_lease.clone() else {
                return Ok(Err(EmbeddingFallback::AdmissionRefused));
            };
            if !self.allow_embedding_gpu.load(Ordering::Acquire) {
                return Ok(Err(EmbeddingFallback::Disabled));
            }
            if self
                .gpu_resident
                .as_ref()
                .is_some_and(|resident| resident.key != *key)
            {
                self.retire_embedding_cuda()?;
            }
            if self.gpu_resident.is_none() {
                let model = match location.as_ref() {
                    ModelLocation::Shared { objects }
                        if objects
                            .assets
                            .get(&ModelAssetName::ModelOnnx)
                            .is_some_and(|asset| asset.sha256.to_string() == FP16_MODEL_SHA256) =>
                    {
                        let source = self.embedded.as_ref().context("embedded source missing")?;
                        let bytes = airk_core::model_objects::ModelAssetSource::read_asset(
                            source,
                            ModelAssetName::ModelJson,
                            ModelAssetName::ModelJson.maximum_bytes(),
                        )?;
                        MiniLmCudaModel::open_assets(source, &bytes)
                    }
                    ModelLocation::Directory { path } => MiniLmCudaModel::open_directory(path),
                    ModelLocation::Retained {
                        collection,
                        objects,
                    } => MiniLmCudaModel::open_retained(collection, objects),
                    ModelLocation::Shared { .. } => {
                        return Ok(Err(EmbeddingFallback::ModelUnsupported));
                    }
                };
                let Ok(model) = model else {
                    return Ok(Err(EmbeddingFallback::ModelUnsupported));
                };
                let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
                let runtime = match airk_host::gpu_runtime::admit(lease, &root, &manifest, deadline)
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        ensure!(
                            !process_poisoned(),
                            "CUDA runtime settlement unknown: {error:#}"
                        );
                        return Ok(Err(EmbeddingFallback::RuntimeUnavailable));
                    }
                };
                super::super::require_live(owner)?;
                if !self.allow_embedding_gpu.load(Ordering::Acquire) {
                    return Ok(Err(EmbeddingFallback::Disabled));
                }
                let session = match model.open_cuda(&runtime, admission.adapter_uuid(), deadline) {
                    Ok(session) => session,
                    Err(error) => {
                        let settled = error
                            .downcast_ref::<CudaBatchFailure>()
                            .is_none_or(CudaBatchFailure::settled);
                        self.require_cuda_settlement(settled, format_args!("{error:#}"))?;
                        return Ok(Err(EmbeddingFallback::AdmissionRefused));
                    }
                };
                self.gpu_resident = Some(Resident {
                    key: *key,
                    session,
                    model,
                });
            }
            super::super::require_live(owner)?;
            if !self.allow_embedding_gpu.load(Ordering::Acquire) {
                return Ok(Err(EmbeddingFallback::Disabled));
            }
            let resident = self
                .gpu_resident
                .as_mut()
                .context("CUDA resident missing")?;
            let inputs = texts.iter().map(String::as_str).collect::<Vec<_>>();
            match resident
                .model
                .embed_cuda(&mut resident.session, &inputs, *memory_bytes, deadline)
            {
                Ok((vectors, stats)) => {
                    super::super::require_live(owner)?;
                    Ok(Ok(Response::Embedded {
                        vectors: vectors.into_iter().map(Some).collect(),
                        execution: Box::new(stats),
                    }))
                }
                Err(error) => {
                    let retired = self.retire_embedding_cuda();
                    self.require_cuda_settlement(
                        error.settled() && retired.is_ok(),
                        format_args!("{error}"),
                    )?;
                    Ok(Err(EmbeddingFallback::ExecutionFailed))
                }
            }
        }
    }
}

#[cfg(feature = "gpu-cuda")]
pub(super) use cuda::Resident;
