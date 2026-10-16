// SPDX-License-Identifier: Apache-2.0
// Publish only the prebuilt paths emitted by the release verifier; never rebuild here.
const fs = require('node:fs');
const { publishVSIX } = require('@vscode/vsce');
const [inventory, channel] = process.argv.slice(2);
if (!inventory || !['preview', 'stable'].includes(channel)) { throw new Error('Expected verified inventory and channel.'); }
const paths = JSON.parse(fs.readFileSync(inventory, 'utf8'));
if (!Array.isArray(paths) || paths.length !== 4 || paths.some(p => typeof p !== 'string' || !p.endsWith('.vsix'))) {
    throw new Error('Expected four verified platform packages.');
}
publishVSIX(paths, { azureCredential: true, preRelease: channel === 'preview' })
    .catch(error => { console.error(error); process.exitCode = 1; });
