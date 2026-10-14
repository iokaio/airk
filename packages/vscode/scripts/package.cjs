// SPDX-License-Identifier: Apache-2.0
// The Python driver constructs/verifies the stage; VSCE alone creates the VSIX.
const { createVSIX } = require('@vscode/vsce');
const [stage, target, output] = process.argv.slice(2);
if (!stage || !target || !output) { throw new Error('Expected stage, VSIX platform and output path.'); }
createVSIX({ cwd: stage, target, packagePath: output, dependencies: false,
    followSymlinks: false, rewriteRelativeLinks: false, preRelease: true })
    .catch(error => { console.error(error); process.exitCode = 1; });
