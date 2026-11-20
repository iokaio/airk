# SPDX-License-Identifier: Apache-2.0
"""Independently re-open both actual Windows reference runs and the R3 Cargo/PE event."""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path

from maintained_helpers import private_root, worker


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(path, sha):
    raw = worker.regular(path).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == sha, f"changed original record {path}")
    return json.loads(raw)


def digest(path):
    with worker.regular(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def distance(actual, reference):
    def masses(xs):
        exp = [math.exp(x - max(xs)) for x in xs]
        return [x / sum(exp) for x in exp]
    require(len(actual) == len(reference) == 2 and all(map(math.isfinite, actual + reference)), "invalid binary scores")
    return max(abs(x-y) for x,y in zip(actual, reference)), max(abs(x-y) for x,y in zip(masses(actual), masses(reference)))


def audit(base):
    record = lambda name, sha: read(base / name, sha)
    producer = record("assembled/reference-producer.json", "e58817db68b4785574b7d239a97257a02e1246a367b947140a55745ef06c5ae2")
    fixture = record("assembled/numeric-fixture.json", "be431f9c65d59823ba70af1524cecfbe9fcaa4ce55bd835105d221d30ba2dd0d")
    actual = record("owned-numeric/native-result.json", "77964dcfa1b7e6258152c10dad9813b992f530e1b5e4bc54c13030fddcb1fa20")
    require(producer["rows"] == fixture["rows"] and len(actual["rows"]) == len(fixture["rows"]) == 14, "row roster")
    kinds = set()
    seals = {}
    results = {}
    for pin in producer["reference_executions"]:
        run = read(Path(pin["path"]), pin["sha256"])
        kind = run["reference_kind"]
        require(kind in {"original_fp32","same_artifact"} and kind not in kinds, "reference type")
        kinds.add(kind)
        require(run["complete"] and not run["post_input_drift"] and run["native_airk_nn_invocations"] == 0 and
                run["torch_forward_invocations"] == 14 and run["owned"]["exit_code"] == 0 and
                run["owned"]["owned_descendants_settled"] and not run["owned"]["owned_job_forced_termination"] and
                run["owned"]["owned_job_final_active_processes"] == 0 and run["owned"]["owned_job_final_member_pids"] == [], "reference closure")
        require(run["source_assets"] == producer["original_reference_assets" if kind == "original_fp32" else "candidate_assets"], "source asset roster")
        child = read(Path(run["raw_output"]["path"]), run["raw_output"]["sha256"])
        require(len(child["rows"]) == 14 and child["rows"] == run["rows"], "child score roster")
        errors = []
        for r, f, n in zip(run["rows"], fixture["rows"], actual["rows"], strict=True):
            require(r["name"] == f["name"] == n["name"] and r["input"] == f["input"] and
                    r["input"]["input_digest"] == n["input_digest"] and len(r["input"]["input_ids"]) == 1024 and
                    r["scores"] == f["reference_" + kind], "original native inputs or score vector")
            require(max(range(2), key=n["raw_scores"].__getitem__) == max(range(2), key=r["scores"].__getitem__), "decision drift")
            errors.append(distance(n["raw_scores"], r["scores"]))
        for path, sha in run["input_seals"].items():
            require(seals.setdefault(path,sha) == sha, "inconsistent source seal")
        results[kind] = {"raw":max(x[0] for x in errors),"mass":max(x[1] for x in errors),"rows":len(errors)}
    require(kinds == {"original_fp32","same_artifact"}, "missing reference kind")
    for path, sha in seals.items():
        require(digest(Path(path)) == sha, "post-measurement source drift " + path)
    packet = record("native-image-inventory.json","3642c1cdf881ef8aa58a0cbaa152d3c451d8b54bf1bd34357c452c5850834593")
    build = record("focus1024-cal-image.json","28bd37f1a683f77d8982b3a78956c8aa14dac795d8c399b4ad534f4ca82cd4d0")
    require(build["passed"] and build["inputs_unchanged"] and build["groups"]["native"]["sha256"] == packet["native_group"],"build source")
    events=[json.loads(line) for line in (base / build["log"]).read_bytes().splitlines() if line.startswith(b"{")]
    env_path=Path("C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/cu15/environment.json")
    env=read(env_path,"05c074577ed80c5f2ac733a5775f71732ff631c890a78932083fddde9a7b4d5c")
    core=env["build"]["build_receipt"]
    archive=env["request"]["aggregate_receipt"]
    compile_record=read(Path(core["path"]),core["sha256"])
    aggregate_record=read(Path(archive["path"]),archive["sha256"])
    lib=env["aggregate"]["output"]
    linked_archive = Path(env["environment"]["ORT_LIB_LOCATION"]) / "onnxruntime.lib"
    require(env_path.parent / lib["path"] == linked_archive and linked_archive.stat().st_size == lib["bytes"] and
            digest(linked_archive)==lib["sha256"] and aggregate_record["output"]==lib,
            "actual aggregate differs from recorded bytes")
    library=env["environment"]["ORT_LIB_LOCATION"]
    require(sum(e.get("reason")=="build-script-executed" and e.get("package_id","").endswith("#ort-sys@2.0.0-rc.10") and
                e["linked_libs"]==["static=onnxruntime"] and e["linked_paths"]==["native="+library] for e in events)==1,"static linker row")
    for image in packet["images"]:
        file=Path(image["path"])
        require(digest(file)==image["sha256"], "image changed")
        with file.open("rb") as pe:
            require(pe.read(2)==b"MZ","DOS header")
            pe.seek(60); pe.seek(struct.unpack("<I",pe.read(4))[0]); hdr=pe.read(26)
        require(hdr[:6]==b"PE\0\0\x64\x86" and hdr[24:26]==b"\x0b\x02","expected Windows x64 PE32+")
        require(sum(e.get("reason")=="compiler-artifact" and e.get("executable")==image["original_path"]
                    and e["profile"]["test"] for e in events)==1,"matching Cargo PE row")
    return {"references":results,"unique_reopened_source_seals":len(seals),
            "target":"x86_64-pc-windows-msvc","archive":lib,"test_images":len(packet["images"]),
            "observed_compile_schema":compile_record.get("schema"),"observed_aggregate_schema":aggregate_record.get("schema"),
            "legacy_ort_root_literals_match":compile_record.get("result")=="libraries_built_not_yet_model_or_static_link_qualified" and
                 aggregate_record.get("result")=="genuine_aggregate_not_yet_final_link_or_model_qualified",
            "scope":"Independent technical replay of measured inputs, arrays and native artifacts."}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    args=parser.parse_args()
    print(json.dumps(audit(private_root(args.work.resolve(strict=True))),indent=2))
