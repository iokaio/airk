# SPDX-License-Identifier: Apache-2.0
"""Stage the fifteen sealed development menus in separate native source roots."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import tarfile
import time


HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
INPUT = CAMPAIGN / "compact-development-margin-census-r1/inputs.private.json"
OUTPUT = CAMPAIGN / "compact-development-source-menus-r2"
CENSUS_SHA = "eef3f7c896e5d147236402941ed56d7e4c118c1f4ea8f9477b8b585bf6d9c56c"
MEMBER = "TechQA/training_and_dev/training_dev_technotes.json"
MEMBER_SHA = "69261b5f4436d70379fd7172a8d97062c93f5d433c386d196937d198c66bf75a"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked_reader(name, sha):
    path = HERE.parent / "public_source_admission" / name
    require(hashlib.sha256(path.read_bytes()).hexdigest() == sha, "source reader differs")
    return runpy.run_path(str(path))


def layout(cases):
    paths = {}
    for case in cases:
        key, menu = case["id"], case["menu_ids"]
        require(re.fullmatch("TRAIN_Q[0-9]{3}", key) and key not in paths, "invalid case identity")
        require(len(menu) == 50 and len(set(menu)) == len(menu), "expected a full publisher menu")
        require(all(re.fullmatch("[A-Za-z0-9_-]+", doc) for doc in menu)
                and len({doc.casefold() for doc in menu}) == len(menu), "unsafe publisher path")
        paths[key] = [(doc, f"{key}/{doc}.txt") for doc in menu]
    return paths


def project(reader, selected, decode):
    bodies = {}

    def document():
        key = reader.string()
        reader.expect(58)
        if key not in selected:
            reader.skip_opaque_value()
            return

        def field():
            name = reader.string()
            reader.expect(58)
            if name == "id":
                return name, reader.string()
            if name == "text":
                return name, decode(reader)
            reader.skip_opaque_value()
            return None

        entries = [x for x in reader.collection(123, 125, field) if x is not None]
        fields = dict(entries)
        require(key not in bodies and len(entries) == 2 and fields.get("id") == key and "text" in fields,
                "duplicate or missing document field")
        bodies[key] = fields["text"].encode("utf-8")

    require(sum(1 for _ in reader.collection(123, 125, document)) == 28482 and bodies.keys() == selected,
            "incomplete source document projection")
    return bodies


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layout", action="store_true")
    args = parser.parse_args()
    raw = INPUT.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == CENSUS_SHA, "development input differs")
    paths = layout(json.loads(raw)["train_source_menus"])
    require(len(paths) == 15, "development case census differs")
    if args.layout:
        print(json.dumps({"directories": list(paths), "files": [
            p for rows in paths.values() for _, p in rows] + ["manifest.private.json"]}))
        return

    require(OUTPUT.resolve(strict=True) == OUTPUT and {p.name for p in OUTPUT.iterdir()} == set(paths)
            and all(not any((OUTPUT / key).iterdir()) for key in paths), "expected fresh isolated source roots")
    reader = checked_reader("audit_techqa_validation_metadata.py",
                            "1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d")
    spans = checked_reader("audit_techqa_dev_source_spans.py",
                           "7f45cb47d897154cdf50505a6ca6b6e99b9acbe9baf83c3f6e33e0c3bf5f8958")
    docs = {doc for rows in paths.values() for doc, _ in rows}
    require(len(docs) == 698, "development document census differs")
    start = time.monotonic()

    def deadline():
        require(time.monotonic() - start < 300, "source projection deadline expired")

    with reader["ARCHIVE"].open("rb") as stream:
        before = reader["archive_state"](stream)
        require(before[2] == reader["ARCHIVE_BYTES"] and hashlib.file_digest(stream, "sha256").hexdigest()
                == reader["ARCHIVE_SHA"], "publisher archive differs")
        stream.seek(0)
        with tarfile.open(fileobj=reader["DeadlineStream"](stream, deadline), mode="r|gz") as archive:
            for entry in archive:
                deadline()
                if entry.name == MEMBER:
                    require(entry.isfile() and entry.size == reader["EXACT_DEV_CORPUS_BYTES"], "member size differs")
                    with archive.extractfile(entry) as member:
                        data = reader["MetadataReader"](member, deadline, maximum_bytes=entry.size)
                        bodies = project(data, docs, spans["read_document_text"])
                        require(data.finish(entry.size)["sha256"] == MEMBER_SHA, "corpus member differs")
                    break
            else:
                raise ValueError("corpus member missing")
        require(reader["archive_state"](stream) == before, "archive changed during projection")

    files = []
    for case, rows in paths.items():
        for document, relative in rows:
            deadline()
            body = bodies[document]
            with (OUTPUT / relative).open("xb") as dst:
                dst.write(body)
            files.append({"case": case, "id": document, "path": relative, "bytes": len(body),
                          "sha256": hashlib.sha256(body).hexdigest()})
    manifest = {"source_census_sha256": CENSUS_SHA, "source_member_sha256": MEMBER_SHA,
                "archive_sha256": reader["ARCHIVE_SHA"], "files": files}
    with (OUTPUT / "manifest.private.json").open("xb") as dst:
        dst.write((json.dumps(manifest, indent=2) + "\n").encode())
    print(json.dumps({"cases": len(paths), "files": len(files), "unique_docs": len(docs),
                      "bytes": sum(f["bytes"] for f in files), "elapsed_seconds": time.monotonic() - start}))


if __name__ == "__main__":
    main()
