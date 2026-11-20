# SPDX-License-Identifier: Apache-2.0
"""Bind a rebuilt scientific service and project its audit into the two frozen CAL source owners."""
import copy
import hashlib
import json
from pathlib import Path

from r6_preflight import CAMPAIGN, ROOT, original, require

FOLDER = CAMPAIGN / "cal100-fused-candidate-launch-r1"
GROUPED = Path(__file__).resolve().parent
OLD_AUDIT = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
OLD_WORKER = "22216d4262e25e24f2c5c53138dbc77a486f8373b6e55d70c850f45fb4d40b51"
OLD_GROUP = "d4c6acb3dc9b091127c97ba13932fcf3ad8bb95282cac92778a2b1c3970b9533"


def equivalent(base, candidate, image):
    expected = copy.deepcopy(base)
    require(base["inputs"]["service"]["sha256"] != image["sha256"]
            and base["selector_check"][0] == base["inputs"]["service"]["path"], "expected a distinct service build")
    expected["inputs"]["service"] = image
    expected["selector_check"][0] = image["path"]
    require(candidate == expected, "candidate changed an input outside the service image and selector path")


def checked_epoch(digest, pin):
    proof = pin(FOLDER / "provenance.json", digest)
    row = original(Path(proof["path"]), proof["sha256"])
    require(row["schema"] == "airk.private.cal-fused-service-source/1" and row["qualified"] is False,
            "expected genuine scientific source stage")
    expected_paths = {"historical": CAMPAIGN / "cal100-focus1024-launch-r1/cal100-launch-audit.json",
                      "audit": FOLDER / "cal100-launch-audit.json", "image": FOLDER / "airk-service-fused-search.exe",
                      "selector": FOLDER / "selector.stdout"}
    for name, path in expected_paths.items():
        require(pin(path, row[name]["sha256"]) == row[name], "incorrect native producer pin: " + name)
    require(row["historical"]["sha256"] == OLD_AUDIT, "different historical audit")
    base = original(expected_paths["historical"], OLD_AUDIT)
    audit = original(expected_paths["audit"], row["audit"]["sha256"])
    image = row["image"]
    equivalent(base, audit, image)
    compiler = row["compiler_image"]
    require(Path(compiler["path"]).is_relative_to(ROOT / "target")
            and (compiler["bytes"], compiler["sha256"]) == (image["bytes"], image["sha256"]),
            "service image differs from the recorded compiler output")
    build = Path(row["build"]["path"])
    require(build.name == "fused-service-image.json" and build.parent.parent == CAMPAIGN / "cal100-store-candidate-timing-r1"
            and pin(build, row["build"]["sha256"]) == row["build"]
            and pin(build.with_suffix(".log"), row["compiler_log"]["sha256"]) == row["compiler_log"],
            "original build or compiler log changed")
    receipt = original(build, row["build"]["sha256"])
    require(receipt["exit_code"] == 0 and receipt["inputs_unchanged"] is True and receipt["command"] == row["cargo_scope"],
            "incomplete native compiler scope")
    compiled = []
    with build.with_suffix(".log").open("rb") as stream:
        for line in stream:
            if line.startswith(b"{"):
                artifact = json.loads(line)
                if (artifact.get("reason") == "compiler-artifact" and artifact["target"]["name"] == "airk_service"
                        and artifact["profile"].get("test") and artifact.get("executable")):
                    compiled.append(Path(artifact["executable"]))
    require(compiled == [Path(compiler["path"])], "compiler log did not emit the recorded service test image")
    selector = audit["selector_check"][-1]
    require(expected_paths["selector"].read_text().splitlines().count(selector + ": test") == 1,
            "candidate native selector not observed")
    return {"provenance": proof, "audit": row["audit"], "image": image, "build": row["build"],
            "compiler_log": row["compiler_log"], "selector": row["selector"]}


def replace_bytes(source, before, after):
    old = before.encode()
    require(source.count(old) == 1, "ambiguous historical source edit: " + before)
    return source.replace(old, after.encode())


def projected(epoch, output):
    capture = GROUPED.parent
    worker = (capture / "worker.cjs").read_bytes()
    group = (GROUPED / "group_service.py").read_bytes()
    require(hashlib.sha256(worker).hexdigest() == OLD_WORKER and hashlib.sha256(group).hexdigest() == OLD_GROUP,
            "historical CAL source changed")
    digest = epoch["audit"]["sha256"]
    for before, after in [
        ("require('./phase.cjs')", "require(" + json.dumps(str(capture / "phase.cjs")) + ")"),
        ("require('./window_scope.cjs')", "require(" + json.dumps(str(capture / "window_scope.cjs")) + ")"),
        ("path.resolve(__dirname,'../../../../..')", json.dumps(str(ROOT))),
        (OLD_AUDIT, digest), ("'cal100-focus1024-launch-r1'", "'cal100-fused-candidate-launch-r1'"),
        ("path.join(__dirname, 'controller.py')", json.dumps(str(capture / "controller.py"))),
    ]:
        worker = replace_bytes(worker, before, after)
    worker_name = repr(str(output / "worker.cjs"))
    base = original(CAMPAIGN / "cal100-focus1024-launch-r1/cal100-launch-audit.json", OLD_AUDIT)
    original_directory = str(Path(base["inputs"]["service"]["path"]).parent)
    for before, after in [
        ("from controller import AUDIT_SHA, ", "from controller import "),
        ("from prepare import SELECTOR, pin", "from prepare import SELECTOR, pin\n\nAUDIT_SHA = " + repr(digest)
         + "\nWORKER = Path(" + worker_name + ")"),
        ('link(HERE / n) for n in ("worker.cjs",', 'link(WORKER if n == "worker.cjs" else HERE / n) for n in ("worker.cjs",'),
        ('[node, HERE/"worker.cjs",', '[node, WORKER,'),
        ('env.update(audit["fixture_environment"])', 'env.update(audit["fixture_environment"])\n    env["PATH"] = '
         + repr(original_directory) + ' + os.pathsep + env["PATH"]'),
    ]:
        group = replace_bytes(group, before, after)
    return {"worker.cjs": worker, "group_service.py": group}


def snapshots(epoch, output, pin, create=False):
    sources = projected(epoch, output)
    if create:
        output.mkdir()
    result = {}
    for name, data in sources.items():
        path = output / name
        if create:
            with path.open("xb") as stream:
                stream.write(data)
        result[name] = pin(path, hashlib.sha256(data).hexdigest())
    require({p.name for p in output.iterdir()} == set(sources), "candidate source snapshot includes extra files")
    return result
