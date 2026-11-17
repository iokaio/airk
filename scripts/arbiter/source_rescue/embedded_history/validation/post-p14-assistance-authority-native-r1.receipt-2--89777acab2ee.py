# SPDX-License-Identifier: Apache-2.0
import os,subprocess,sys
os.environ['AIRK_TEST_BINARY']='D:/code/Github/airk/artifacts/v190/p10-worktree/artifacts/post-p14-final-windows-r1/runtime/airk.exe'
sys.exit(subprocess.run(['node','--test','--test-reporter=tap','packages/vscode/out/test/explorerAssistanceNative.test.js']).returncode)