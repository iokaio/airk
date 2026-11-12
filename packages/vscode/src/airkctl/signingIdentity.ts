// SPDX-License-Identifier: Apache-2.0
//! The public signing identities the extension requires of `airkctl` (plan
//! §8.2, contracts §7 C10). The owner is scripts/signing-identity.json;
//! src/test/airkctlDiscovery.test.ts fails when these values drift from it.
export const SIGNING_IDENTITY = Object.freeze({
    windows: Object.freeze({
        publisher: 'CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US',
        identity_eku: '1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669',
    }),
    macos: Object.freeze({ team_id: '5PX7D99PXA' }),
});
