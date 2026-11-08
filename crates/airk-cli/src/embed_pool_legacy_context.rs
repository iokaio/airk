// SPDX-License-Identifier: Apache-2.0
// Historical process helpers retain shared worker/charge behavior. They never
// compile the modern service's in-process owner-completion implementation.
macro_rules! provider_context {
    ($definition:ident, shared_tests) => {
        $definition!(cfg(any()), cfg(test));
    };
    ($definition:ident) => {
        $definition!(cfg(any()));
    };
}
