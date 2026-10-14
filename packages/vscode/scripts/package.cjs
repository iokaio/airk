// SPDX-License-Identifier: Apache-2.0
// The Python driver constructs/verifies the stage; VSCE alone creates the VSIX.
const { createVSIX } = require('@vscode/vsce');
const [stage, target, output, channel = 'preview'] = process.argv.slice(2);
if (!stage || !target || !output) { throw new Error('Expected stage, VSIX platform and output path.'); }
if (!['preview', 'stable'].includes(channel)) { throw new Error('Expected preview or stable channel.'); }
createVSIX({ cwd: stage, target, packagePath: output, dependencies: false,
    followSymlinks: false, rewriteRelativeLinks: false, preRelease: channel === 'preview' })
    .catch(error => { console.error(error); process.exitCode = 1; });
