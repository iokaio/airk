# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,os
from pathlib import Path
root=Path.cwd(); base=root/'artifacts/readiness-1817'; os.environ.update(TEMP=str(base/'tls-host-fixtures'),TMP=str(base/'tls-host-fixtures'))
(base/'tls-host-fixtures').mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('tls_diag',root/'scripts/tests/test_local_https_examples.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
cls=m.LocalHttpsExamplesTests;cls.setUpClass()
try:
    case=cls('test_real_tls_complete_get_post_refusal_and_tool_error_without_replay')
    source=(root/'examples/local_https_client.ps1').read_text()
    original="[Console]::Error.WriteLine('AIRK local HTTPS read failed; obtain current verified connection metadata and inspect native status.')"
    replacement="$fault=$_;$chain=[Collections.Generic.List[object]]::new();$cause=$fault.Exception;while($null -ne $cause){$chain.Add(@{type=$cause.GetType().FullName;hresult=$cause.HResult;message=$cause.Message});$cause=$cause.InnerException};[Console]::Error.WriteLine((@{line=$fault.InvocationInfo.ScriptLineNumber;errors=$chain}|ConvertTo-Json -Compress -Depth 8))"
    assert source.count(original)==1
    copied=cls.root/'fixture-only-diagnostic.ps1';copied.write_text(source.replace(original,replacement))
    client=next(value for value in case.clients() if value[0]=='pwsh')
    client=(client[0],[*client[1][:-1],str(copied)],client[2])
    with case.server() as (connection,observed):
        result=case.run_client(client,connection,{'query':'fictional diagnostic','response_mode':'full'})
        receipt={'returncode':result.returncode,'observed_requests':len(observed),'diagnostic':result.stderr.strip()}
        (base/'https-pwsh-host-diagnostic.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
finally:
    cls.tearDownClass()
