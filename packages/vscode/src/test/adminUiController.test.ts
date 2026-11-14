// SPDX-License-Identifier: Apache-2.0
// Actual controller generation/control decisions; no native transport claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import Module from 'node:module';

function fixture() {
    const loader = Module as unknown as { _load(name: string, ...rest: any[]): any }, prior = loader._load;
    loader._load = function(name, ...rest) { return name === 'vscode' ? {} : prior.call(this, name, ...rest); };
    let Controller: typeof import('../explorerController').ExplorerController;
    try { ({ ExplorerController: Controller } = require('../explorerController')); } finally { loader._load = prior; }
    let admin = {}, pilot = {}, trusted = true, native = true, revoked = 0;
    const calls: unknown[] = [];
    const owner = Object.create(Controller.prototype) as any;
    Object.assign(owner, { disposed: false, adminEpoch: 0, pilotEpoch: 0, panel: { generation: 1 },
        host: { adminManagementIdentity: () => admin, pilotIdentity: () => pilot, trusted: () => trusted,
            nativeExecutionEnabled: () => native, revokeAdminManagement: async () => { revoked++; }, log() {},
            adminManagement: async (input: unknown) => { calls.push(input); return { closed: true }; } }, publish() {} });
    return { owner, calls, revoked: () => revoked,
        rotateProbe: () => { pilot = {}; }, rotateNativeOwner: () => { admin = {}; },
        trust: (value: boolean) => { trusted = value; }, native: (value: boolean) => { native = value; } };
}
test('Admin controller retains native epoch across repeated metadata probes and evidence selection changes', () => {
    const f = fixture(), original = f.owner.adminContext().epoch;
    f.rotateProbe(); f.owner.selectionEpoch = 9;
    assert.equal(f.owner.adminContext().epoch, original);
    f.rotateNativeOwner(); assert.equal(f.owner.adminContext().epoch, original + 1);
});
test('Admin controller trust/native revocation closes captured host owner without opening another', async () => {
    for (const change of ['trust', 'native'] as const) {
        const f = fixture(), original = f.owner.adminContext().epoch;
        f[change](false); const ended = f.owner.adminContext();
        assert.equal(ended.epoch, original + 1); await Promise.resolve();
        assert.equal(f.revoked(), 1); assert.deepEqual(f.calls, []);
    }
});
test('Admin controller close advances view epoch so an old close/401 cannot address the replacement', async () => {
    const f = fixture(), epoch = f.owner.adminContext().epoch;
    await f.owner.dispatch({ v: 1, type: 'admin.close', requestId: 'w1', epoch });
    assert.equal(f.owner.adminContext().epoch, epoch + 1);
    assert.deepEqual(f.calls, [{ v: 1, type: 'admin.close', requestId: 'w1', epoch }]);
});
