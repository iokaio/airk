// SPDX-License-Identifier: Apache-2.0
//! Explicit instrumented helper test target; never an installed/release binary.
#[cfg(not(test))]
compile_error!("the guarded provider fixture must be built as its explicit test target");

#[path = "../../src/embed_request.rs"]
mod embed_request;
use embed_request::handle;
mod embed_pool {
    include!("../../src/embed_pool_legacy_context.rs");
    include!("../../src/embed_pool_core.rs");
    include!("../../src/embed_pool_legacy_entry.rs");
    mod retirement {
        include!("../../src/embed_pool_retirement.rs");
    }
}
mod gate;
#[path = "guarded_provider_gate_protocol.rs"]
mod gate_protocol;

fn main() -> anyhow::Result<()> {
    if std::env::args().skip(1).eq(["--version"]) {
        // Package/platform identity is real; the qualification receipt binds
        // these different actual bytes and explicitly labels cfg(test).
        println!("airk-embed {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    anyhow::ensure!(
        std::env::args().skip(1).eq(["pool-guarded"]),
        "this fixture accepts only the native fixed guarded role or --version"
    );
    let bootstrap = airk_cli::embedding_pool_bootstrap::accept_guarded_bootstrap()?;
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(embed_pool::run_guarded_with_engine(
            Some(bootstrap),
            |init, closing| {
                Ok(embed_pool::Engine::new(&init.runtime)?
                    .with_compute_hook(gate::hook(init, closing)?))
            },
        ))
}
