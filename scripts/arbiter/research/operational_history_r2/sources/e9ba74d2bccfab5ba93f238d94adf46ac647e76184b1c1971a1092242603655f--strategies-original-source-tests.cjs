// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs'),path=require('node:path'),ts=require(path.resolve('packages/vscode/node_modules/typescript'));
require.extensions['.ts']=(module,file)=>{const source=fs.readFileSync(file,'utf8');const output=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}});module._compile(output.outputText,file);};
require(path.resolve('packages/vscode/src/test/explorerStrategies.test.ts'));
require(path.resolve('packages/vscode/src/test/explorerStrategyRunner.test.ts'));
require(path.resolve('packages/vscode/src/test/explorerPlanner.test.ts'));

