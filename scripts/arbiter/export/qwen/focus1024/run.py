# SPDX-License-Identifier: Apache-2.0
"""Own the same-human, bounded offline Focus1024 graph and CPU numerical run."""
import argparse
import hashlib
import os
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
VENV = ROOT / "artifacts/advanced-split/models/training-env/venv"
sys.path.insert(0, str(HERE))
from export_graph import export, save, sha, require

wrapper = ROOT / "scripts/arbiter/evaluation/run_compact_answer.py"
sha(wrapper, "9625bb14ca85eaf5057b3b272d92eb99589fa13e47ea1eeef56d35b0fcb09496")
native = runpy.run_path(str(wrapper))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    require(out.parent == CAMPAIGN and out.name.startswith("focus1024-cpu-export-r") and out.name.split("-r")[-1].isdigit(),
            "fresh private campaign root required")
    if args.worker:
        require(os.environ.get("AIRK_FOCUS1024_OWNER") == str(out) and not os.environ.get("CUDA_VISIBLE_DEVICES"),
                "bounded CPU owner required")
        sys.path.append(str(VENV/"Lib/site-packages"))
        with native["capped_worker_job"](out):
            graph = export(out)
            save(out / "graph.private.json", graph)
            from verify_publisher import verify
            verify(out, graph)
        return
    require(not out.exists(), "original output may not be overwritten")
    out.mkdir()
    cache = out / "cache"
    cache.mkdir()
    codes = [HERE/name for name in ("export_graph.py","verify_publisher.py","run.py")]
    seals = {p.name:sha(p) for p in codes}
    sha(ROOT/"scripts/provision-static-onnx-runtime.py", "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044")
    owner = native["owner_module"]()
    env = os.environ | dict(AIRK_FOCUS1024_OWNER=str(out), CUDA_VISIBLE_DEVICES="", CUDA_CACHE_DISABLE="1",
                            CUBLAS_WORKSPACE_CONFIG=":4096:8", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                            TOKENIZERS_PARALLELISM="false", HF_HOME=str(cache), TORCH_HOME=str(cache),
                            TEMP=str(cache), TMP=str(cache), TORCHINDUCTOR_CACHE_DIR=str(cache),
                            OMP_NUM_THREADS="4", MKL_NUM_THREADS="4", OPENBLAS_NUM_THREADS="1")
    result = owner.run_evaluation_fixture([VENV/"Scripts/python.exe","-I","-B","-S",HERE/"run.py","--output",out,"--worker"],
                                          ROOT, out/"output.log",600,env)
    result["source_sha256"] = seals
    result["source_unchanged"] = all(sha(p) == seals[p.name] for p in codes)
    save(out/"owned.json", result)
    require(owner.successful(result) and result["source_unchanged"] and not result["owned_job_final_member_pids"]
            and result["owned_job_final_active_processes"] == 0, "original export/verification owner failed")


if __name__ == "__main__":
    main()
