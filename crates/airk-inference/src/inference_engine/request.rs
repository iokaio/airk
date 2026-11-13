// SPDX-License-Identifier: Apache-2.0
//! Canonical model request execution for the in-process inference pool.
use airk_retrieval::{
    embedding_worker::{ModelLocation, PROTOCOL, Request, Response},
    model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel, MAX_INPUT_BYTES},
};
use anyhow::{Context, Result, ensure};
pub(crate) fn handle(
    request: Request,
    model: &mut Option<EmbeddingModel>,
    tokenizer: &mut Option<airk_kernel::usage::OfflineTokenizer>,
) -> Result<Response> {
    Ok(match request {
        Request::ProvisionModel { .. } | Request::ProvisionBundled => {
            anyhow::bail!("model provisioning requires the shared service role")
        }
        Request::TokenizerOpen {
            protocol,
            version,
            path,
        } => {
            ensure!(
                model.is_none()
                    && tokenizer.is_none()
                    && protocol == PROTOCOL
                    && version == env!("CARGO_PKG_VERSION"),
                "invalid tokenizer handshake"
            );
            let opened = airk_kernel::usage::OfflineTokenizer::open(&path)?;
            let response = Response::TokenizerOpened {
                identity: opened.identity().clone(),
                description: opened.description(),
            };
            *tokenizer = Some(opened);
            response
        }
        Request::TokenCount { text } => Response::TokenCount {
            count: tokenizer
                .as_ref()
                .context("tokenizer not opened")?
                .count(&text)?,
        },
        Request::Extract { directory } => {
            ensure!(
                model.is_none() && tokenizer.is_none(),
                "asset extraction cannot run in a model session"
            );
            airk_host::bundled_assets::extract(&directory)?;
            Response::Extracted
        }
        Request::Open {
            protocol,
            version,
            location,
            identity,
        } => {
            ensure!(
                model.is_none()
                    && tokenizer.is_none()
                    && protocol == PROTOCOL
                    && version == env!("CARGO_PKG_VERSION"),
                "invalid embedding handshake"
            );
            let opened = match location {
                ModelLocation::Shared { .. } => {
                    anyhow::bail!("shared model location requires the service pool")
                }
                ModelLocation::Directory { path } => EmbeddingModel::open(&path)?,
                ModelLocation::Retained {
                    collection,
                    objects,
                } => EmbeddingModel::open_objects(&collection, &objects)?,
            };
            ensure!(
                opened.identity() == identity,
                "embedding model identity mismatch"
            );
            *model = Some(opened);
            Response::Opened {
                protocol,
                version,
                identity,
            }
        }
        Request::Plan { text } => {
            ensure!(
                text.len() <= MAX_INPUT_BYTES,
                "embedding planning input exceeds bound"
            );
            Response::Planned {
                tokens: model
                    .as_ref()
                    .context("embedding session not opened")?
                    .planning_tokens(&text)?,
            }
        }
        Request::Embed {
            texts,
            workers,
            memory_bytes,
        } => {
            ensure!(
                !texts.is_empty()
                    && texts.len() <= 32
                    && texts.iter().all(|t| t.len() <= MAX_INPUT_BYTES)
                    && (1..=8).contains(&workers)
                    && (8 << 20..=1 << 30).contains(&memory_bytes),
                "embedding batch exceeds admission bounds"
            );
            let model = model.as_ref().context("embedding session not opened")?;
            let inputs = texts.iter().map(String::as_str).collect::<Vec<_>>();
            let mut execution = EmbeddingCacheStats::default();
            let vectors = model.embed_batch(
                &inputs,
                &mut EmbeddingCacheContext {
                    cache: None,
                    bypass: true,
                    stats: &mut execution,
                    max_workers: workers,
                    working_memory_bytes: memory_bytes,
                },
            )?;
            Response::Embedded {
                vectors,
                execution: Box::new(execution),
            }
        }
        Request::Peak => Response::Peak {
            bytes: model
                .as_ref()
                .context("embedding session not opened")?
                .plan_cache_peak_bytes()?,
        },
    })
}
