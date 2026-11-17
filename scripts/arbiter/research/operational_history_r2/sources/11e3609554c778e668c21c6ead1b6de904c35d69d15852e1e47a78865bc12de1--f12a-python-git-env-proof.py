# SPDX-License-Identifier: Apache-2.0
import os,subprocess,sys,json
from pathlib import Path
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy();env.update(GIT_CONFIG_COUNT='2',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix(),GIT_CONFIG_KEY_1='safe.directory',GIT_CONFIG_VALUE_1=root.as_posix()+'/*')
command=[sys.executable,'-m','unittest','discover','-s','scripts/tests','-p','test_package_extension.py','-v']
with (a/'f12a-python-git-env-proof.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
print(json.dumps({'command':command,'exit':r.returncode,'git_config':{k:env[k] for k in ['GIT_CONFIG_COUNT','GIT_CONFIG_KEY_0','GIT_CONFIG_VALUE_0','GIT_CONFIG_KEY_1','GIT_CONFIG_VALUE_1']}}));raise SystemExit(r.returncode)
