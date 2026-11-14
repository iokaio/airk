// SPDX-License-Identifier: Apache-2.0
// Parse a restricted literal grammar; never execute a generated shell command.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
const src = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const bundle = await build({ stdin: { contents: `export {setEnvCommand} from './agents';`, resolveDir: src, sourcefile: 'r23-observation.ts' },
  bundle: true, write: false, platform: 'node', format: 'esm', jsx: 'automatic', jsxImportSource: 'preact',
  plugins: [{ name: 'export-observed-formatter', setup(plugin) {
    plugin.onLoad({ filter: /agents\.tsx$/ }, async args => ({ contents: `${await readFile(args.path, 'utf8')}\nexport {setEnvCommand};`, loader: 'tsx', resolveDir: src }));
  } }],
});
const { setEnvCommand } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
function literal(value, windows) {
  if (windows) {
    assert.match(value, /^'(?:[^']|'')*'$/u, 'PowerShell single-quoted string grammar');
    return value.slice(1, -1).replaceAll("''", "'");
  }
  assert.match(value, /^'[^']*'(?:"'"'[^']*')*$/u, 'POSIX concatenated single-quoted literal grammar');
  return value.slice(1, -1).replaceAll(`'"'"'`, "'");
}
const values = ['/home/Zoë Nguyen/CA.pem', "C:\\Users\\O'Brien\\AIRK CA.pem", '/tmp/$HOME/`literal`/$(never-run).pem', `/tmp/"quoted"/安全.pem`];
for (const windows of [true, false]) {
  test(`R23 ${windows ? 'PowerShell' : 'POSIX'} certificate paths round-trip as literal data`, () => {
    for (const name of ['NODE_EXTRA_CA_CERTS', 'CODEX_CA_CERTIFICATE']) {
      for (const value of values) {
        const command = setEnvCommand(name, value, windows);
        const prefix = windows ? `$env:${name} = ` : `export ${name}=`;
        assert.ok(command.startsWith(prefix));
        assert.equal(literal(command.slice(prefix.length), windows), value);
      }
    }
  });
}
