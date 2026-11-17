# SPDX-License-Identifier: Apache-2.0
import os,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[2]
env=dict(os.environ,AIRK_TEST_EXTENSION_ROOT=str(root/'artifacts/one-process/bounded-extension-1823/stage'))
sys.exit(subprocess.run(['node','--test','--test-concurrency=1','--test-reporter=tap','packages/vscode/out/test/explorerAssistanceNative.test.js','packages/vscode/out/test/f09NativeQualification.test.js'],cwd=root,env=env).returncode)
