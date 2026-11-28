// SPDX-License-Identifier: Apache-2.0
// Run named public contract examples against the checkout's dependency-free TypeScript module.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';

const { module, exported, cases } = JSON.parse(fs.readFileSync(0, 'utf8'));
const source = stripTypeScriptTypes(fs.readFileSync(module, 'utf8'), { mode: 'strip' });
let failures = 0;
for (const example of cases) {
  try {
    const context = vm.createContext(Object.create(null), { codeGeneration: { strings: false, wasm: false } });
    const subject = new vm.SourceTextModule(source, { context, identifier: module });
    await subject.link(() => { throw Error('This public contract module must be dependency-free'); });
    await subject.evaluate({ timeout: 5000 });
    const probe = new vm.SourceTextModule(
      `import { ${exported} as check } from "subject"; export const result = JSON.stringify(check(...${JSON.stringify(example.args)}));`,
      { context }
    );
    await probe.link(() => subject);
    await probe.evaluate({ timeout: 5000 });
    assert.deepEqual(JSON.parse(probe.namespace.result), example.expected);
    console.log(`PASS ${example.name}`);
  } catch (error) {
    failures++;
    console.log(`FAIL ${example.name}\n${error.stack ?? error}`);
  }
}
console.log(`${cases.length - failures} passed, ${failures} failed`);
process.exitCode = failures ? 1 : 0;