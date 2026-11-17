# SPDX-License-Identifier: Apache-2.0
"""Private complete synchronous-control bridge batch; Root owns public mount/Cargo."""
from pathlib import Path
import difflib,hashlib,json
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
    name="crates/airk-supervisor/src/runtime_metadata/arbiter.rs";old=(ROOT/name).read_text();text=old
    helper=(HERE/"bridge.rs.fragment").read_text()
    if text.count('struct Controller(PathBuf);')!=1:raise ValueError("exact callback owner required")
    text=text.replace('struct Controller(PathBuf);',helper+'\nstruct Controller(PathBuf);',1)
    if text.count('tokio::runtime::Handle::current().block_on(async {')!=3:raise ValueError("three bounded scoring callback siblings required")
    text=text.replace('tokio::runtime::Handle::current().block_on(async {','drive_control(|| async {')
    for method,next_method in [('focus_application','focus_rank'),('application','next_read')]:
        start=text.index('    fn '+method+'(');end=text.index('    fn '+next_method+'(',start)
        part=text[start:end]
        if part.count('tokio::runtime::Handle::current().block_on(')!=1:raise ValueError("one metadata callback sibling required")
        part=part.replace('tokio::runtime::Handle::current().block_on(\n','drive_control(|| async {\n',1)
        tail='''            ),
        )?;'''
        if part.count(tail)!=1:raise ValueError("exact metadata request future required")
        part=part.replace(tail,'''            ).await
        })?;''',1)
        text=text[:start]+part+text[end:]
    if 'Handle::current().block_on' in text:raise ValueError("unbridged callback remains")
    text+='\n#[cfg(test)]\nmod tests;\n'
    rows={name:text,"crates/airk-supervisor/src/runtime_metadata/arbiter/tests.rs":(HERE/"tests.rs.proposed").read_text()}
    contract="crates/airk-supervisor/src/project_worker/CONTRACT.md"
    rows[contract]=(ROOT/contract).read_text()+(HERE/"CONTRACT-addition.md").read_text()
    process="docs/process-model.md";before=(ROOT/process).read_text()
    anchor='Project owners, cleanup owners and the source watcher run on service threads.'
    if before.count(anchor)!=1:raise ValueError("exact owning process-model paragraph required")
    addition='''Synchronous optional-model control callbacks use the project owner's existing
multithread runtime through its blocking boundary. Outside-runtime and
current-thread callers use local or joined scoped IO drivers; they do not add
resident processes, permanent per-session runtimes or detached work. All callback
requests preserve their original authority, attempt and absolute deadline.

'''
    rows[process]=before.replace(anchor,addition+anchor,1)
    # Contract decision is retained before dependent source in the mount order.
    order=[contract,process,name,"crates/airk-supervisor/src/runtime_metadata/arbiter/tests.rs"]
    patch=[];files=[]
    for path in order:
        target=ROOT/path;original=target.read_text() if target.exists() else "";content=rows[path]
        output=HERE/"source"/path;output.parent.mkdir(parents=True,exist_ok=True);output.write_text(content)
        patch.extend(difflib.unified_diff(original.splitlines(True),content.splitlines(True),fromfile=path,tofile=path))
        files.append({"source":str(output),"target":path,"source_sha256":sha(output),"target_before_sha256":sha(target) if target.exists() else None})
    (HERE/"sync-control-bridge.patch").write_text("".join(patch))
    manifest={"schema":"airk.private-supervisor-sync-control-bridge/1","files":files,
        "root_decision":"Same multithread handle via block_in_place; outside/current-thread driver-owned local/joined fallback; factory entered inside driver; no permanent threads/queues/retry/new budget",
        "coverage":"All five NativeController control callbacks; existing embedding pool client and async bundle/work sibling classes inspected and unchanged",
        "focused_tests":6,"selector":"test(runtime_metadata::arbiter::tests::sync_control_)",
        "public_writes":False,"Cargo_or_tests_or_NN_executed":False}
    output=HERE/"mount-manifest.json";output.write_text(json.dumps(manifest,indent=2)+"\n")
    print(json.dumps({"manifest":str(output),"sha256":sha(output),"files":len(files),"focused_tests_authored":6}))
if __name__=="__main__":main()
