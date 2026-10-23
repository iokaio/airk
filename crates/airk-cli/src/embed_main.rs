// SPDX-License-Identifier: Apache-2.0
//! Dedicated inference executable. No project publication or service management commands.
use airk_cli::service_ipc::{read_frame_limited, write_frame_limited};
use airk_retrieval::{
    embedding_worker::{MAX_FRAME_BYTES, ModelLocation, PROTOCOL, Request, Response},
    model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel, MAX_INPUT_BYTES},
};
use anyhow::{Context, Result, ensure};
use std::io;

mod embed_pool;

fn handle(
    request: Request,
    model: &mut Option<EmbeddingModel>,
    tokenizer: &mut Option<airk_kernel::usage::OfflineTokenizer>,
) -> Result<Response> {
    Ok(match request {
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
            airk_cli::bundled_assets::extract(&directory)?;
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

fn main() -> Result<()> {
    if std::env::args().skip(1).eq(["pool"]) {
        return tokio::runtime::Runtime::new()?.block_on(embed_pool::run());
    }
    if std::env::args().skip(1).eq(["--version"]) {
        println!("airk-embed {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    ensure!(
        std::env::args().skip(1).eq(["worker"]),
        "usage: airk-embed worker | --version"
    );
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut model = None;
    let mut tokenizer = None;
    while let Some(request) = read_frame_limited(&mut input, MAX_FRAME_BYTES)? {
        let response = match handle(request, &mut model, &mut tokenizer) {
            Ok(response) => response,
            Err(_) => Response::Failed {
                reason: "invalid request or unavailable verified model".into(),
            },
        };
        write_frame_limited(&mut output, &response, MAX_FRAME_BYTES)?;
    }
    Ok(())
}
