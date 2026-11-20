# SPDX-License-Identifier: Apache-2.0
"""Authenticate the completed stock Windows CPU pair after its original bulk ACL check timed out."""
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
product = runpy.run_path(str(HERE/"windows_cpu_product.py"))
ROOT, E, OUT, require = (product[k] for k in ("ROOT","E","OUT","require"))
FINAL = E/"windows-stock-r2-custody-complete.json"
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"


def close():
    c,b,_,_,guard = runpy.run_path(str(HERE/"windows_cpu_ort.py"))["runtime"]()
    def opened(path, sha):
        rec=b.pin(path)
        require(rec["sha256"]==sha, f"changed original {path.name}")
        return rec,b.read(path)

    source,prior=opened(OUT/"execution.json","5f16089950823bc14e6443b666b82ba42a6fce77788cc1938420d718eee6bfdd")
    require(not prior["passed"] and "TimeoutExpired" in prior["error"] and "Assert-AirkOutputBatch" in prior["error"],
            "original stage did not stop in its post-run owner check")
    job=prior["job"]
    require(job["exit_code"]==0 and not job["timed_out"] and not job["owned_job_forced_termination"]
            and job["owned_descendants_settled"] and job["owned_job_final_active_processes"]==0
            and job["owned_job_final_member_pids"]==[] and prior["c_cache_before"]==prior["c_cache_after"],
            "stock product did not finish in the owned job")
    for entry in [*prior["inputs"],prior["log"]]:
        b.pinned(entry)
    runtime=OUT/"runtime"
    install,payload=opened(runtime/"airk-install.json","20ea74f79a7416ce268c0de8d29be950aaf4c199df614444692ea37c84564302")
    _,build=opened(runtime/"airk-build.json","f7b44faae5e2dc74ac73f7c242ce585ed05a7c4fbd85742657ae1daf7a1e6e24")
    _,pair=opened(runtime/"airk-runtime.json","1d257a194e9b7aae1cebc6b4a8e5a92f41f337135eb3b9d21e1031850abc272a")
    require(build["source"]==prior["source_identity"] and build["profile"]=="debug" and pair["schema"]==3 and
            (payload["version"],payload["os"],payload["arch"])==("1.8.207","windows","x86_64")
            and payload["files"]["airk.exe"]==pair["airk_sha256"] and payload["files"]["airkctl.exe"]==pair["airkctl_sha256"],
            "native product identity differs")
    roots=[]
    for name,sha,expected in [
        ("target","e03b72ae9be8c5ba65974cfaad8428be6bc2737fb5ee2b5a3eed985ff123eaad",ROOT/"target"),
        ("product","436588cf5af9c29979b5ded47613329812d3fce485a2ac387062d60e17019aca",OUT),
    ]:
        rec,acl=opened(E/f"windows-stock-r2-{name}-post-custody.json",sha)
        require(Path(acl["root"])==expected and acl["owner"]==SID and acl["dirs"]>0 and acl["files"]>0,
                "original checked owner audit missing")
        roots.append(rec)
    require(acl["allocated_upper_bytes"]<=prior["remaining_storage"],"private stage cap exceeded")
    actual=[b.pin(path) for path in sorted(runtime.rglob("*")) if path.is_file()]
    hashes={Path(row["path"]).relative_to(runtime).as_posix():row["sha256"] for row in actual}
    require(hashes == payload["files"]|{"airk-install.json":install["sha256"]},"original complete offline payload differs")
    c["write_new"](guard,FINAL,{"passed":True,"original":source,"job":job,"source_identity":build["source"],
            "provenance":"stock native product completed; original post-audit timed out; separate full custody measured",
            "custody_after":roots,"files":actual,"product_allocated_upper_bytes":acl["allocated_upper_bytes"],
            "remaining_storage":prior["remaining_storage"],"source":b.pin(Path(__file__).resolve())})


if __name__=="__main__":
    close()

