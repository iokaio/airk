// SPDX-License-Identifier: Apache-2.0
'use strict';
const {isDeepStrictEqual: equal} = require('node:util');
function need(ok, message) { if (!ok) throw Error(message); }

function windowStatus(status, binding, grant) {
    const obs = status.scope_observation, original = binding.scope_observation;
    need(obs?.schema === 1 && obs.qualifier === original.qualifier && /^[a-f0-9]{64}$/.test(obs.fingerprint) &&
        obs.selection?.owner === 'window' && obs.selection.scope_identity === grant.scope_identity &&
        obs.selection.catalog_revision === original.selection.catalog_revision &&
        status.collection === binding.publication.collection && status.publication?.token === binding.publication.token &&
        obs.members?.length === 1 && original.members.length === 1, 'window scope, original publication or revision differs');
    const before = original.members[0], after = obs.members[0];
    for (const key of ['index_id','source_root_ids','source_revision','policy_epoch','policy_digest']) {
        need(equal(after[key],before[key]), 'window source or policy changed: ' + key);
    }
    need(!Object.hasOwn(status,'read_scope') &&
        equal(after.memberships?.map(m=>m.project_id),binding.window_selection.project_ids) &&
        after.memberships.length===1 && Number.isSafeInteger(after.memberships[0].membership_revision) &&
        after.memberships[0].membership_revision > 0 && status.files===binding.files &&
        status.indexed_vectors===binding.indexed_vectors && status.semantic_readiness?.state==='ready',
        'window membership or source coverage differs');
    return obs;
}

async function acquire(session, selection, instance, retain) {
    const result = await session.scopeSelect(selection);
    retain({kind:'window_grant',selection,result});
    need(session.hello?.serviceInstance === instance && result.status === 'scope' &&
        result.catalog_revision === selection.expected_catalog_revision && /^[a-f0-9]{64}$/.test(result.scope_identity) &&
        Number.isSafeInteger(result.expires_at_ms), 'original native window grant unavailable');
    return result;
}

async function release(session, retain) {
    const result = await session.scopeRelease();
    retain({kind:'window_release',result});
    need(result.status === 'scope_released', 'native window did not release its completed question');
}

module.exports = {windowStatus, acquire, release};
