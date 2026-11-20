# SPDX-License-Identifier: Apache-2.0
"""Seal one original TechQA question, complete source menu and native observer inputs.

This preparation reads no CAL case data and runs no model or fixture.
The checked caller supplies a fresh empty campaign child and preflights the
three private JSON outputs. Preserve the original caller's runtime template.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import runpy
import tarfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
MENU = "34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821"
INVENTORY = "120e9b270f8fb5a0accc7ed31f33ba99f52e3016736409d7720592b84e1633ff"
PLAN = "e85a33d6322a2a38814bbb282afd339f23ae3461b43be8a9461bcc2f7b68a608"
READER = "1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d"
DEV = "TechQA/training_and_dev/dev_Q_A.json"
DEV_SHA = "df62a05619b07b386159bcbaaf3e0e50be93eaf983aea8a5f8ed1513a3c2ec6f"
PIPELINE = "cb2166f2f648aa44cb4ee87f6b58b1e5c8007720d24fa4426234b65058e2b638"
FOCUS = "c8c5918be0f9e6a7d74e797250d0f8e7a32fd63e226d1deb662f44240651f632"
CONTEXT = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
SELECTOR = "service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture_software_v2"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def check_file(path, expected):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file()
            and not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents)), "plain input required")
    with path.open("rb") as stream:
        require(hashlib.file_digest(stream, "sha256").hexdigest() == expected, "pinned input differs: " + path.name)


def json_bytes(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()


def selected_question(reader, raw, case_id, ordered):
    def field():
        key = reader.string()
        reader.expect(58)
        if key == "QUESTION_ID":
            return key, reader.string()
        if key == "DOC_IDS":
            return key, list(reader.collection(91, 93, reader.string))
        if key in ("QUESTION_TITLE", "QUESTION_TEXT"):
            reader.nonspace()
            start = reader.bytes - len(reader.buffer) + reader.index
            reader.skip_opaque_string()
            return key, (start, reader.bytes - len(reader.buffer) + reader.index)
        reader.skip_opaque_value()
        return None

    selected = []
    for fields in reader.collection(91, 93, lambda: list(reader.collection(123, 125, field))):
        pairs = [item for item in fields if item is not None]
        row = dict(pairs)
        require(len(row) == len(pairs), "duplicate publisher question field")
        if row.get("QUESTION_ID") == case_id:
            require(row.get("DOC_IDS") == ordered, "pilot menu order changed in original DEV member")
            parts = []
            for key in ("QUESTION_TITLE", "QUESTION_TEXT"):
                start, end = row[key]
                require(0 < end - start <= 65536, "publisher question field exceeds bound")
                value = json.loads(raw[start:end])
                require(isinstance(value, str), "invalid publisher question field")
                parts.append(value)
            question = "\n\n".join(parts)
            require(question.strip(), "empty publisher question")
            selected.append(question)
    require(len(selected) == 1, "unique pilot question required")
    return selected[0]


def original_question(owner, case_id, ordered):
    started = time.monotonic()

    def deadline():
        require(time.monotonic() - started < 300, "original source read expired")

    archive = owner["ARCHIVE"]
    check_file(archive, owner["ARCHIVE_SHA"])
    with archive.open("rb") as stream:
        with tarfile.open(fileobj=owner["DeadlineStream"](stream, deadline), mode="r|gz") as tar:
            for member in tar:
                if member.name != DEV:
                    continue
                require(member.isfile() and 0 < member.size <= owner["MAX_MEMBER_BYTES"], "invalid DEV member")
                with tar.extractfile(member) as data:
                    raw = data.read(member.size + 1)
                require(len(raw) == member.size and sha(raw) == DEV_SHA, "publisher DEV member differs")
                reader = owner["MetadataReader"](io.BytesIO(raw), deadline)
                question = selected_question(reader, raw, case_id, ordered)
                require(reader.finish(len(raw))["sha256"] == DEV_SHA, "publisher parser did not consume exact member")
                return question
    raise ValueError("publisher DEV member missing")


def prepare(destination):
    require(destination.parent == CAMPAIGN and re.fullmatch(r"techqa-predispatch-pilot-inputs-r[1-9][0-9]*", destination.name)
            and destination.resolve(strict=True) == destination and not any(destination.iterdir()),
            "fresh checked pilot destination required")
    full = CAMPAIGN / "techqa-full-menu-corpus-r1/manifest.private.json"
    native = CAMPAIGN / "techqa-menu-native-source-reads-r1/native-reads.private.json"
    plan_path = CAMPAIGN / "fp32-paired-answer-r1/cpu-launch-current-roster.json"
    for path, expected in ((full, MENU), (native, INVENTORY), (plan_path, PLAN)):
        check_file(path, expected)
    menu = json.loads(full.read_bytes())
    case_id = menu["pilot_case"]
    require(case_id == min(menu["ordered_menus"]) == "DEV_Q002", "pilot selector differs")
    ordered = menu["ordered_menus"][case_id]
    view = full.parent / menu["pilot_source_root"]
    observed = json.loads(native.read_bytes())
    require(observed["corpus_manifest_sha256"] == MENU and not observed["answer_scored"]
            and len(observed["cases"]) == 1, "prior native source view differs")
    row = observed["cases"][0]
    require(row["selector"] == {"question_id": case_id} and len(ordered) == len(set(ordered)) == 50
            and [doc["publisher_id"] for doc in row["ordered_documents"]] == ordered, "native/publisher menu differs")
    for doc in row["ordered_documents"]:
        expected = menu["documents"][doc["publisher_id"]]
        require(doc["path"] == doc["publisher_id"] + ".txt" and doc["sha256"] == expected["sha256"],
                "native document identity differs")
        check_file(view / doc["path"], doc["sha256"])
    require({p.name for p in view.iterdir()} == set(row["native_inventory_order"]), "source directory closure differs")
    reader_path = HERE / "audit_techqa_validation_metadata.py"
    check_file(reader_path, READER)
    owner = runpy.run_path(str(reader_path))
    framing_sha = "f348fad09baf0fbbce6438dc00afa25289d7dcd605c54a7c551047ab645032a9"
    check_file(owner["ARCHIVE_ROOT"] / "original600-comparison-r4.json", framing_sha)
    question = original_question(owner, case_id, ordered)
    plan = json.loads(plan_path.read_bytes())
    assets = {k: plan[k] for k in ("service", "cli", "inputs", "answer", "minilm")}
    for asset in assets.values():
        check_file(Path(asset["path"]), asset["sha256"])
    require(assets["service"]["sha256"] == "f3a317c196196340d45dea90cfd9b743b32e299028651f1b75fc38a5174168fd"
            and assets["inputs"]["sha256"] == "62f01d429a0268477b500a462eec86df887090a81a8358d4d001f160e2945608",
            "private service or compiler roster differs")
    for asset in plan["minilm_assets"]:
        path = Path(assets["minilm"]["path"]).parent / asset["name"]
        check_file(path, asset["sha256"])
    unmodified = json.loads(Path(assets["answer"]["path"]).read_bytes())
    require(unmodified["response_mode"] == "full" and unmodified["include_status"] is True
            and unmodified["budget"]["deadline_ms"] == 120000 and unmodified["budget"]["input_tokens"] == 8192,
            "native Answer limits differ")
    request = dict(unmodified, question=question)
    inputs = {"request.private.json": request, "source-scopes.private.json": {"source_roots": [str(view)]}}
    descriptor = {
        "schema": "airk.private.techqa-predispatch-pilot-inputs/1", "question_id": case_id,
        "question_framing": "title_double_newline_text", "framing_comparison_sha256": framing_sha,
        "question_sha256": sha(question.encode()), "archive_sha256": owner["ARCHIVE_SHA"], "dev_member_sha256": DEV_SHA,
        "full_menu_manifest_sha256": MENU, "prior_native_inventory_sha256": INVENTORY,
        "publisher_order": ordered, "source_root": str(view), "runtime_plan_sha256": PLAN, "runtime_inputs": assets,
        "minilm_assets": plan["minilm_assets"], "original_answer_budget": request["budget"],
        "cpu_focus_manifest": FOCUS, "cpu_context_bootstrap_manifest": CONTEXT, "required_minilm_pipeline": PIPELINE,
        "native_selector": SELECTOR, "source_sha256": sha(Path(__file__).read_bytes()),
        "required_environment": {"AIRK_SELECTED_CONTEXT_CAL_OBSERVER": "true", "AIRK_SELECTED_CONTEXT_MINILM_PIPELINE": PIPELINE,
                                 "AIRK_NATIVE_WARM_OBSERVER": "true", "AIRK_NATIVE_DISPATCH_OBSERVER": "true"},
        "private_files": {name: sha(json_bytes(data)) for name, data in inputs.items()},
    }
    inputs["pilot.private.json"] = descriptor
    for name, value in inputs.items():
        with (destination / name).open("xb") as stream:
            stream.write(json_bytes(value))
    print(json.dumps({"case": case_id, "documents": len(ordered), "descriptor_sha256": sha(json_bytes(descriptor)),
                      "request_sha256": descriptor["private_files"]["request.private.json"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    prepare(parser.parse_args().output)
