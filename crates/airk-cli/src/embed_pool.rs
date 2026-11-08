// SPDX-License-Identifier: Apache-2.0
//! Production shared-helper entry and its normal unit harness. The explicit
//! qualification image includes the same core without importing unit tests.
include!("embed_pool_legacy_context.rs");
include!("embed_pool_core.rs");
include!("embed_pool_legacy_entry.rs");

mod retirement {
    include!("embed_pool_retirement.rs");
    #[cfg(test)]
    mod tests {
        include!("embed_pool_retirement_tests.rs");
    }
}
#[cfg(test)]
impl Engine {
    fn request(
        &mut self,
        grant: &protocol::Grant,
        session: Incarnation,
        request: Request,
    ) -> Result<Response> {
        self.request_guarded(grant, session, request, None)
    }
}

pub async fn run() -> Result<()> {
    run_guarded(None).await
}

pub async fn run_guarded(
    bootstrap: Option<airk_cli::embedding_pool_bootstrap::GuardedBootstrap>,
) -> Result<()> {
    run_guarded_with_engine(bootstrap, |init, _| Engine::new(&init.runtime)).await
}

#[cfg(test)]
mod tests {
    include!("embed_pool_tests.rs");
}
