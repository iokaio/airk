// SPDX-License-Identifier: Apache-2.0
// Verify the segment contract against the original strings without choosing a particular LCS tie-break.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';

const source = stripTypeScriptTypes(readFileSync(process.argv[2], 'utf8'), { mode: 'strip' });
const context = vm.createContext(Object.create(null), { codeGeneration: { strings: false, wasm: false } });
const mod = new vm.SourceTextModule(source, { context });
await mod.link(() => { throw Error('Unexpected dependency in the frozen pure diff utility'); });
await mod.evaluate({ timeout: 5000 });
const texts = ['', ' ', '\t', '\n\n', 'alpha', 'alpha alpha', '  alpha\t', 'βeta\n\n',
               ' 👩🏽‍💻\tmañana ', 'a\r\nb', 'one two three', 'one blue three'];
const failures = [];
for (const oldText of texts) {
  for (const newText of texts) {
    try {
      const segs = JSON.parse(JSON.stringify(mod.namespace.computeWordDiff(oldText, newText)));
      assert.ok(Array.isArray(segs));
      assert.ok(segs.every(s => ['removed', 'equal', 'added'].includes(s.type) && typeof s.text === 'string' && s.text.length));
      assert.ok(segs.every((s, i) => !i || s.type !== segs[i - 1].type));
      assert.equal(segs.filter(s => s.type !== 'added').map(s => s.text).join(''), oldText);
      assert.equal(segs.filter(s => s.type !== 'removed').map(s => s.text).join(''), newText);
      if (!oldText && !newText) assert.deepEqual(segs, []);
      else if (!oldText) assert.deepEqual(segs, [{ type: 'added', text: newText }]);
      else if (!newText) assert.deepEqual(segs, [{ type: 'removed', text: oldText }]);
      else if (oldText === newText) assert.deepEqual(segs, [{ type: 'equal', text: oldText }]);
      if (oldText === 'one two three' && newText === 'one blue three') {
        assert.ok(segs.some(s => s.type === 'equal' && s.text.includes('one')));
        assert.ok(segs.some(s => s.type === 'equal' && s.text.includes('three')));
        assert.ok(segs.some(s => s.type === 'removed' && s.text.includes('two')));
        assert.ok(segs.some(s => s.type === 'added' && s.text.includes('blue')));
      }
    } catch (e) {
      failures.push({ oldText, newText, error: String(e) });
    }
  }
}
console.log(JSON.stringify({ cases: texts.length ** 2, failed: failures.length, firstFailures: failures.slice(0, 3) }));
process.exitCode = Number(failures.length > 0);
