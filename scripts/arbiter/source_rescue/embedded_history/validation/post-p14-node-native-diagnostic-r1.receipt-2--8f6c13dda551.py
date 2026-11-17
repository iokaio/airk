# SPDX-License-Identifier: Apache-2.0
import os,subprocess,sys
os.environ['AIRK_TEST_BINARY']='D:/code/Github/airk/artifacts/v190/p10-worktree/artifacts/post-p14-final-windows-r1/runtime/airk.exe'
command=['node','--test','--test-reporter=tap','--test-concurrency=1','--test-name-pattern','one installed endpoint serves evidence|extension native clients index, inspect','packages/vscode/out/test/explorerAssistanceNative.test.js','packages/vscode/out/test/native.test.js']
sys.exit(subprocess.run(command).returncode)