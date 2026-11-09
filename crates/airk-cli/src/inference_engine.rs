// SPDX-License-Identifier: Apache-2.0
//! Service-owned inference execution, sharing the canonical model handlers.
#[path = "embed_request.rs"]
mod request;
use request::handle;
mod pool {
    // This library owns the native in-process completion barrier.
    macro_rules! provider_context {
        ($definition:ident, shared_tests) => {
            $definition!(cfg(all()), cfg(all()));
        };
        ($definition:ident) => {
            $definition!(cfg(all()));
        };
    }
    include!("embed_pool_core.rs");
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
    #[cfg(test)]
    mod tests {
        include!("embed_pool_tests.rs");
    }
    include!("inference_owner.rs");
    #[cfg(test)]
    mod owner_tests {
        include!("inference_owner_tests.rs");
    }
}
#[cfg(test)]
pub(crate) use pool::ComputeHook;
pub(crate) use pool::{InferenceOwner, NativeCompletionWitness};
