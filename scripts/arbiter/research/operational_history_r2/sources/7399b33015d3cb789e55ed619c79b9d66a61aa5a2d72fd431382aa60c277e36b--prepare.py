# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd()
target=root/'crates/airk-cli/src/unified_mcp.rs'
old=target.read_bytes().decode('utf-8')
ending='\r\n' if '\r\n' in old else '\n'
replacement=Path(__file__).with_name('readiness-owner.rs').read_text()
start=old.index('/// A request deadline must not restart authenticated installation setup.')
end=old.index('struct RootNative {',start)
new=old[:start]+replacement+old[end:]
new=new.replace('airk_mcp::remote::serve(Arc::new(backend)).await','''let served = airk_mcp::remote::serve(Arc::new(backend)).await;
    let cleanup = ready.shutdown().await;
    served?;
    cleanup''')
extra=Path(__file__).with_name('readiness-tests.rs').read_text()
position=new.rfind('}')
new=new[:position]+extra+'\n'+new[position:]
destination=Path(__file__).parent/'source/crates/airk-cli/src/unified_mcp.rs'
destination.parent.mkdir(parents=True,exist_ok=True)
new=new.replace('\r\n','\n').replace('\n',ending)
destination.write_bytes(new.encode('utf-8'))
patch=''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/crates/airk-cli/src/unified_mcp.rs',tofile='b/crates/airk-cli/src/unified_mcp.rs'))
Path(__file__).with_name('readiness-owner.patch').write_bytes(patch.encode('utf-8'))
