# SPDX-License-Identifier: Apache-2.0
"""Check native file paths and hashes in isolated original TechQA menus, without questions or labels."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import sys
import time


HERE = Path(__file__).resolve().parent
AWS = HERE / "aws_normal_service.py"
AWS_SHA = "bf2850980a395de97a9cae888f910fe7e95f1f9f679a72641c32940a8ac497f1"
DEPENDENCIES = {
    "aws_normal_service.py": AWS_SHA,
    "verify_aws_natural_menu.py": "0d779cb66f28416c6e1b3d23f88b8026b3b3f07bff0be89aaa325160981f6618",
    "aws_publication_setup.py": "7b87f3d242ff50d183603ef90dd4d80e195e37bef35aee01e723f2310a42bb2d",
    "native_cohort_source_reads.py": "a81e3ef514edb17b9c41dcd39aa0df8bc349a0f452109110d41ffd7785986a42",
    "../../evaluation/run_compact_answer.py": "9625bb14ca85eaf5057b3b272d92eb99589fa13e47ea1eeef56d35b0fcb09496",
    "../../../provision-static-onnx-runtime.py": "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044",
}
MANIFEST = "34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821"
MENU = "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7"
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
CORPUS = CAMPAIGN / "techqa-full-menu-corpus-r1"
WALL = {"pilot": 600, "full": 4 * 3600}
HOST = 2 << 30


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def check_dependencies():
    for name, seal in DEPENDENCIES.items():
        require(digest((HERE / name).read_bytes()) == seal, "normal service dependency changed: " + name)


def selected(mode):
    raw = (CORPUS / "manifest.private.json").read_bytes()
    require(digest(raw) == MANIFEST, "source-only membership manifest differs")
    census = json.loads(raw)
    menus = census["ordered_menus"]
    docs = census["documents"]
    require(census["ordered_menu_freeze_sha256"] == MENU and census["pilot_case"] == "DEV_Q002"
            and len(menus) == 76 and len(docs) == 3526, "publisher menu census differs")
    cases = {}
    for case in sorted(menus):
        if (case == "DEV_Q002") != (mode == "pilot"):
            continue
        ids = menus[case]
        require(re.fullmatch(r"DEV_Q[0-9]{3}", case) and len(ids) == len(set(ids)) == 50, "isolated publisher menu differs")
        entries = []
        for identity in ids:
            doc = docs[identity]
            name = identity + ".txt"
            rel = Path(doc["path"])
            require(re.fullmatch(r"[A-Za-z0-9_.-]+", identity) and len(rel.parts) == 2
                    and rel.parts[0] in ("pilot-menu", "remaining-documents") and rel.name == name, "foreign source member")
            path = CORPUS / rel
            require(path.resolve(strict=True) == path and path.is_file(), "noncanonical publisher body")
            body = path.read_bytes()
            require(len(body) == doc["bytes"] and digest(body) == doc["sha256"], "source member differs")
            entries.append({"name": name, "publisher_id": identity, **doc})
        cases[case] = entries
    require(len(cases) == (1 if mode == "pilot" else 75), "selected population differs")
    return cases


def materialize(output, case, docs):
    tree = output / "sources" / case
    tree.mkdir()
    for doc in docs:
        raw = (CORPUS / doc["path"]).read_bytes()
        require(digest(raw) == doc["sha256"], "publisher body changed")
        dest = tree / doc["name"]
        with dest.open("xb") as stream:
            stream.write(raw)
        require(digest(dest.read_bytes()) == doc["sha256"], "isolated materialization differs")
    require({p.name for p in tree.iterdir()} == {d["name"] for d in docs}, "foreign file in menu root")
    return tree


def inspect_menu(docs, status, index, tool):
    require(status["read_scope"] == {"source": "named", "index_ids": [index]} and status["discovery_readiness"]["state"] == "ready"
            and status["semantic_readiness"]["state"] == "disabled", "unready or unexpected index scope")
    native_publication = None
    def same(label, name, payload):
        nonlocal native_publication
        body = tool(label, name, dict(payload, include_status=True, response_mode="full"))
        publication = body["publication"]
        require(all(publication.get(k) == v for k, v in status["publication"].items())
                and (native_publication is None or native_publication == publication)
                and body["scope_observation"] == status["scope_observation"],
                "source publication or native grant changed")
        native_publication = publication
        return body
    listing = same("list", "airk_list_files", {"prefix": "", "limit": 100, "max_bytes": 65536})
    paths = listing["paths"]
    require(not listing["truncated"] and not listing.get("cursor") and len(paths) == len(set(paths)) == status["files"]
            and set(paths) <= {d["name"] for d in docs}, "partial, duplicated or foreign native inventory")
    observed = []
    for pos, doc in enumerate(docs):
        row = {"publisher_position_zero_based": pos, "publisher_id": doc["publisher_id"], "path": doc["name"],
               "original_bytes": doc["bytes"], "original_sha256": doc["sha256"], "visible": doc["name"] in paths}
        if row["visible"]:
            info = same("file-" + str(pos), "airk_file_info", {"path": doc["name"]})
            require(info["path"] == doc["name"] and info["source_bytes"] == doc["bytes"] and info["content_hash"] == doc["sha256"],
                    "published document bytes or identity differ")
            row.update(native_sha256=info["content_hash"], native_bytes=info["source_bytes"])
        observed.append(row)
    return {"native_publication": native_publication, "native_order": paths, "ordered_members": observed, "visible": len(paths), "omitted": len(docs) - len(paths)}


def allocation_bytes(paths, unit):
    total = 0
    for path in paths:
        require(not path.is_symlink() and not path.is_junction(), "unexpected reparse point in native output")
        if path.is_dir():
            total += unit + allocation_bytes(path.iterdir(), unit)
        else:
            size = path.stat().st_size
            total += ((size + unit - 1) // unit) * unit
    return total


def cluster_size(output):
    import ctypes
    from ctypes import wintypes
    sectors, size, free, count = (wintypes.DWORD() for _ in range(4))
    ok = ctypes.windll.kernel32.GetDiskFreeSpaceW(str(output.anchor), ctypes.byref(sectors),
                                                  ctypes.byref(size), ctypes.byref(free), ctypes.byref(count))
    require(ok and sectors.value * size.value > 0, "output volume allocation unit unavailable")
    return sectors.value * size.value


def execute(normal, w, owner, output, mode, deadline, member):
    n = normal["n"]
    source_sha = digest(Path(__file__).read_bytes())
    cases = selected(mode)
    paths, pins = normal["images"]()
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG", "PYTHON"))}
    for name in ("commands", "sources", "stores", "observations"):
        (output / name).mkdir()
    control = output / "control"
    runs = output / "commands"
    off = output / "vectors-off.json"
    w["write_json"](off, {"operation": "vectors", "enabled": False})
    def command(label, *args, closing=False):
        return w["run_logged"]([paths["airkctl"], "--output", "json", "--service-runtime-dir", control, *args],
                               runs, label, env, deadline if closing else deadline - 25)[0]
    def cli(label, *args):
        return w["read_json"](command(label, *args))
    def job(label, expected):
        for i in range(240):
            item = cli(f"{label}-{i}", "jobs", "get", expected["id"])["job"]
            require((item["id"], item["kind"]) == (expected["id"], expected["kind"]), "native job identity changed")
            if item["state"] == "succeeded":
                require(item["outcome"]["state"] == "succeeded", "native setup failed")
                return item
            require(item["state"] in ("pending", "running"), "native setup job failed")
            time.sleep(.5)
        raise ValueError("240 native job observations exhausted")
    def tool(case, index, label, name, payload):
        return w["tool_body"](n["publication_read"](cli, w["read_json"], runs, case + "-" + label,
            ["tools", "call", name, "--index", index, "--args", json.dumps(payload)], deadline - 25))
    result = cli("start", "service", "start", "--runtime-dir", control)
    summaries = []
    indices = set()
    unit = cluster_size(output)
    completed_storage = 0
    high_water = 0
    try:
        health = result["observation"]
        require(result["status"] == "health" and health["executable_sha256"] == pins["airk"]["sha256"]
                and health["user"] == "S-1-5-21-3678370870-3858642417-826968105-1002", "service identity differs")
        member(health["process_id"])
        for case, docs in cases.items():
            tree = materialize(output, case, docs)
            store = output / "stores" / case
            store.mkdir()
            setup = command(case + "-register", "--root", tree, "--data-dir", store, "library", "setup",
                            "--name", "techqa-source-" + case, "--watch", "off", "--apply")
            with setup.open("rb") as stream:
                draft, applied = map(json.loads, stream)
            reg = applied["receipt"]
            project, index = reg["request"]["registration"]["binding"], reg["index_id"]
            require("draft" in draft and index not in indices, "registration or isolation differs")
            indices.add(index)
            job(case + "-policy-job", cli(case + "-configure", "--service-project", project, "configure", "--file", off)["job"])
            indexed = job(case + "-index-job", cli(case + "-index", "--service-project", project, "index")["job"])
            status = tool(case, index, "status", "airk_status", {"response_mode": "full", "include_status": True})
            require(indexed["outcome"]["revision"] == status["publication"]["token"], "published index revision differs")
            census = inspect_menu(docs, status, index, lambda *args: tool(case, index, *args))
            report = {"case_id": case, "source_root": str(tree), "index_id": index, "registration": reg,
                      "publication": status["publication"], "scope": status["scope_observation"], **census}
            file = output / "observations" / (case + ".json")
            w["write_json"](file, report)
            completed_storage += allocation_bytes([tree, store, file, *runs.glob(case + "-*")], unit)
            shared = allocation_bytes([control, off, *runs.glob("start.*")], unit)
            high_water = max(high_water, completed_storage + shared)
            require(high_water + (32 << 20) <= 2 << 30, "D campaign reservation exhausted at case boundary")
            summaries.append({"case_id": case, "index_id": index, "publication": status["publication"]["token"],
                              "receipt_sha256": digest(file.read_bytes()), "visible": census["visible"], "omitted": census["omitted"]})
    finally:
        command("stop", "service", "stop", "--runtime-dir", control, closing=True)
    check_dependencies()
    require(normal["images"]()[1] == pins and selected(mode) == cases and digest(Path(__file__).read_bytes()) == source_sha,
            "source, program, or installed image epoch changed")
    w["write_json"](output / "census.private.json", {"mode": mode, "publisher_manifest_sha256": MANIFEST, "menu_freeze_sha256": MENU,
                  "installed_record_sha256": normal["RECORD_SHA"], "images": pins, "service": health, "cases": summaries,
                  "source_sha256": source_sha, "dependency_sha256": DEPENDENCIES, "normal_owner_sha256": AWS_SHA, "allocation_unit": unit,
                  "largest_measured_at_case_boundary_bytes": high_water, "disk_reservation_bytes": 2 << 30,
                  "count": len(summaries), "answer_requests": 0, "ranker_requests": 0})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=WALL, required=True)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    check_dependencies()
    normal = runpy.run_path(str(AWS))
    n = normal["n"]
    n["checked"](n["WRAPPER"], n["WRAPPER_SHA"])
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    if args.check_only:
        require(args.output is None and not args.worker, "metadata preflight requires no output")
        normal["images"]()
        cases = selected(args.mode)
        print(json.dumps({"cases": len(cases), "memberships": sum(map(len, cases.values())),
                          "source_bytes": sum(d["bytes"] for docs in cases.values() for d in docs)}))
    else:
        output = owner.plain(args.output)
        require(output.parent == CAMPAIGN and re.fullmatch("techqa-normal-source-" + args.mode + r"-r[1-9][0-9]*", output.name),
                "fresh D campaign target required")
        if args.worker:
            require(os.environ["AIRK_TECHQA_SOURCE_OWNER"] == str(output), "owned worker required")
            with normal["capped"](w, output, HOST) as (member, _sample):
                execute(normal, w, owner, output, args.mode, float(os.environ["AIRK_TECHQA_SOURCE_DEADLINE"]), member)
        else:
            require(not output.exists(), "fresh output required")
            output.mkdir()
            expiry = time.monotonic() + WALL[args.mode]
            proof = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"],
                n["ROOT"], output / "outer.log", WALL[args.mode], os.environ | {"AIRK_TECHQA_SOURCE_OWNER": str(output),
                "AIRK_TECHQA_SOURCE_DEADLINE": str(expiry)})
            w["write_json"](output / "owned.json", proof)
            normal["stage"]["require_settlement"](w, proof)
            print(owner.digest(output / "census.private.json"))
