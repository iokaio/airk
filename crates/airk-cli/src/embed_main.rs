// SPDX-License-Identifier: Apache-2.0
//! Dedicated inference executable. No project publication or service management commands.
use airk_cli::service_ipc::{read_frame_limited, write_frame_limited};
use airk_retrieval::embedding_worker::{MAX_FRAME_BYTES, Response};
use anyhow::{Result, ensure};
use std::io;

mod embed_pool;
mod embed_request;
use embed_request::handle;
fn main() -> Result<()> {
    if std::env::args().skip(1).eq(["--verify-bundled-models"]) {
        println!("{}", airk_cli::bundled_assets::verify_inventory()?);
        return Ok(());
    }
    if std::env::args().skip(1).eq(["pool"]) {
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(embed_pool::run());
    }
    if std::env::args().skip(1).eq(["pool-guarded"]) {
        let bootstrap = airk_cli::embedding_pool_bootstrap::accept_guarded_bootstrap()?;
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(embed_pool::run_guarded(Some(bootstrap)));
    }
    if std::env::args().skip(1).eq(["--version"]) {
        println!("airk-embed {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    ensure!(
        std::env::args().skip(1).eq(["worker"]),
        "usage: airk-embed worker | --version | --verify-bundled-models"
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
