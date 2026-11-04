// SPDX-License-Identifier: Apache-2.0
//! Instrumentation exists only in the explicit cfg(test) helper image. The
//! production helper retains its original handler and contains no latch.
#![cfg(test)]
use super::{embed_pool::ComputeContext, gate_protocol as protocol};
use airk_cli::{embedding_pool_protocol::Init, local_ipc};
use airk_retrieval::embedding_worker::Request;
use anyhow::{Context as _, Result, ensure};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

type Hook = Box<dyn FnMut(Option<&ComputeContext>, &Request) + Send>;

pub(crate) fn hook(init: &Init, closing: Arc<AtomicBool>) -> Result<Hook> {
    let endpoint = local_ipc::endpoint(&init.runtime, protocol::ROLE)?;
    let service = init.service;
    let helper = init.pool;
    let mut executor = None;
    Ok(Box::new(move |observed, request| {
        if !matches!(request, Request::Embed { .. }) {
            return;
        }
        let outcome = (|| -> Result<()> {
            let observed =
                observed.context("qualification compute lacks native call observation")?;
            let worker = observed
                .worker
                .context("qualification compute lacks actual native worker identity")?;
            ensure!(
                worker.service == service && observed.helper == helper,
                "qualification compute belongs to another native owner"
            );
            let context = protocol::Context {
                worker,
                helper,
                request_id: observed.request_id,
                session: observed.session,
            };
            // Constructed lazily on the EXISTING engine thread, never nested
            // inside the helper's Tokio runtime and never spawning a thread.
            if executor.is_none() {
                executor = Some(
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?,
                );
            }
            executor.as_ref().expect("fixture executor initialized").block_on(async {
                let budget = Duration::from_secs(protocol::DEADLINE_SECONDS);
                // One deadline covers connect, every frame, and the held call.
                tokio::time::timeout(budget, async {
                    let mut peer = local_ipc::connect(&endpoint).await?;
                    local_ipc::write_message_limited(&mut peer, &protocol::Event::Reached { context },
                        budget, protocol::MAX_FRAME_BYTES).await?;
                    let action: protocol::Action = local_ipc::read_message_limited(&mut peer,
                        budget, protocol::MAX_FRAME_BYTES).await?.context("qualification owner closed before decision")?;
                    match action {
                        protocol::Action::Resume { context: actual, nonce: None } if actual == context => Ok(()),
                        protocol::Action::Hold { context: actual, nonce } if actual == context => {
                            local_ipc::write_message_limited(&mut peer,
                                &protocol::Event::Entered { context, nonce }, budget, protocol::MAX_FRAME_BYTES).await?;
                            let action: protocol::Action = local_ipc::read_message_limited(&mut peer,
                                budget, protocol::MAX_FRAME_BYTES).await?.context("qualification owner closed held call")?;
                            ensure!(matches!(action, protocol::Action::Resume { context: actual, nonce: Some(actual_nonce) }
                                if actual == context && actual_nonce == nonce), "qualification resume identity mismatch");
                            Ok(())
                        }
                        _ => anyhow::bail!("qualification decision identity mismatch"),
                    }
                }).await.context("qualification compute gate deadline exhausted")?
            })
        })();
        if let Err(error) = outcome {
            // Never execute this call or synthesize a drain acknowledgment when
            // its fixture owner disappears. The ordinary helper failure path
            // joins the failed engine and retains native containment until exit.
            closing.store(true, Ordering::Release);
            panic!("native qualification compute gate failed: {error:#}");
        }
    }))
}
