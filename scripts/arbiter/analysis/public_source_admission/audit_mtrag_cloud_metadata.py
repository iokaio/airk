# SPDX-License-Identifier: Apache-2.0
"""Collect a private identifier-only screening roster from frozen IBM Cloud qrels."""

from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import re
import runpy
import sys
import time
from urllib.request import Request, urlopen


REVISION = "e0970de1b1207c2a9506adf238e6e1a22037e38a"
BASE = f"https://raw.githubusercontent.com/IBM/mt-rag-benchmark/{REVISION}/"
READER = Path(__file__).with_name("audit_techqa_validation_metadata.py")
READER_SHA = "1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d"
LICENSE = ("LICENSE", 11_357, "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4")
RELEASES = (
    ("mtragun-human", 507, 131, 86,
     ("mtragun-human/retrieval_tasks/qrels/cloud.tsv", 15_719,
      "4c9e5903b0c1a702ade78f3f3bb826712614d31095f40c91121a214de6d248bf"),
     ("mtragun-human/generation_tasks/reference.jsonl", 3_836_136,
      "68053f709c928570718fd2c88455eccf974746f5bfa2206d7fd39b969acb21d4")),
    ("mtrag-human", 842, 205, 188,
     ("mtrag-human/retrieval_tasks/cloud/qrels/dev.tsv", 30_951,
      "925635635bf0e60b20d8ea0b18160cff878897acf8e0bf54e7096cc0ade8bd3b"),
     ("mtrag-human/generation_tasks/reference.jsonl", 7_608_375,
      "b3dde4076f9f692a99513e424a0a39f57591dbe45600a4c1017aaa7a7edcfaac")),
)
CLOUD = {"ibmcloud", "mt-rag-ibmcloud-elser-512-100-20240502"}
TASK = re.compile(r"([a-f0-9]{32})<::>([1-9][0-9]?)")
PASSAGE = re.compile(r"ibmcld_[0-9]+-([0-9]+)-([0-9]+)")
LIMIT = 90


def require(condition, message):
    if not condition:
        raise ValueError(message)


def fetch(spec, check):
    name, extent, digest = spec
    url = BASE + name
    check()
    request = Request(url, headers={"User-Agent": "AIRK-source-metadata-audit"})
    with urlopen(request, timeout=20) as response:
        require(response.geturl() == url, "immutable official release URL changed")
        blob = response.read(extent + 1)
    check()
    require(len(blob) == extent and hashlib.sha256(blob).hexdigest() == digest,
            "official release member bytes differ")
    return blob, {"url": url, "bytes": extent, "sha256": digest}


def object_projection(reader, handlers):
    def field():
        key = reader.string()
        reader.expect(58)
        return key

    projected = {}
    for key in reader.collection(123, 125, field):
        if key in handlers:
            require(key not in projected, "duplicate selected JSON metadata field")
            projected[key] = handlers[key]()
        else:
            reader.skip_opaque_value()
    return projected


def utterance_digest(reader):
    reader.expect(34)
    digest = hashlib.sha256()
    token = bytearray(b'"')
    escaped = False
    while True:
        value = reader.take()
        require(value >= 32, "invalid opaque question string")
        token.append(value)
        if not escaped and value == 34:
            break
        escaped = not escaped and value == 92
        if len(token) == 4096:
            digest.update(token)
            token.clear()
    digest.update(token)
    return digest.hexdigest()


def reference_metadata(blob, reader_type, check):
    result = {}
    for raw in io.BytesIO(blob):
        require(0 < len(raw) <= 1 << 18, "reference JSONL line exceeds bound")
        reader = reader_type(io.BytesIO(raw), check, maximum_bytes=1 << 18)

        def marks():
            return tuple(reader.collection(91, 93, reader.string))

        def inputs():
            def utterance():
                return object_projection(reader, {
                    "speaker": reader.string,
                    "text": lambda: utterance_digest(reader),
                    "metadata": lambda: object_projection(reader, {"author_type": reader.string}),
                })
            rows = list(reader.collection(91, 93, utterance))
            require(1 <= len(rows) <= 40 and all(
                set(row) == {"speaker", "text", "metadata"} and
                set(row["metadata"]) == {"author_type"} for row in rows),
                "published utterance metadata shape differs")
            last = rows[-1]
            return len(rows), last["speaker"].casefold(), last["metadata"]["author_type"], last["text"]

        row = object_projection(reader, {
            "task_id": reader.string,
            "conversation_id": reader.string,
            "turn": reader.string,
            "Collection": reader.string,
            "Answerability": marks,
            "answerability": marks,
            "Multi-Turn": marks,
            "Question Type": marks,
            "Standalone Type": marks,
            "input": inputs,
        })
        reader.finish(len(raw))
        identifier = row["task_id"]
        task = TASK.fullmatch(identifier)
        require(task is not None and row["conversation_id"] == task[1] and row["turn"] == task[2]
                and identifier not in result and ("Answerability" in row) != ("answerability" in row),
                "published task identity or answerability side differs")
        result[identifier] = row
    return result


def qrel_metadata(blob):
    lines = blob.decode("ascii").splitlines()
    require(lines and lines[0] == "query-id\tcorpus-id\tscore", "Cloud qrel TSV header differs")
    results = {}
    for line in lines[1:]:
        identifier, pointer, score = line.split("\t")
        match = PASSAGE.fullmatch(pointer)
        require(TASK.fullmatch(identifier) is not None and match is not None
                and int(match[1]) < int(match[2]) and score == "1",
                "expected positive Cloud qrel and publisher pointer")
        pointers = results.setdefault(identifier, [])
        require(pointer not in pointers, "duplicate published qrel pointer")
        pointers.append(pointer)
    return results


def classified(references, qrels, release):
    require(qrels.keys() <= references.keys(), "Cloud qrel misses its published reference task")
    observed, candidates = Counter(), []
    for identifier, pointers in qrels.items():
        row = references[identifier]
        require(row["Collection"] in CLOUD, "qrel joined a different corpus")
        length, speaker, author, digest = row["input"]
        marks = row.get("answerability", row.get("Answerability"))
        standalone = row.get("Standalone Type", ())
        require(speaker == "user" and author in {"human", "model"}
                and len(marks) == 1 and marks[0] in {"ANSWERABLE", "PARTIAL"},
                "published Cloud qrels require supported question provenance")
        if marks != ("ANSWERABLE",):
            observed["partially_answerable"] += 1
        elif author != "human":
            observed["nonhuman_last_user"] += 1
        elif standalone == ("Standalone",) or (row["turn"] == "1" and length == 1
                                                  and row["Multi-Turn"] == ("N/A",)):
            evidence_digest = hashlib.sha256("\n".join(sorted(pointers)).encode("ascii")).hexdigest()
            candidates.append({
                "release": release, "task_id": identifier, "conversation_id": row["conversation_id"],
                "turn": int(row["turn"]), "question_types": row["Question Type"],
                "standalone_basis": "publisher_marked" if standalone else "first_user_turn",
                "question_json_token_sha256": digest, "positive_passage_roster_sha256": evidence_digest,
                "publisher_positive_passage_pointers": pointers,
            })
        else:
            observed["standalone_not_established"] += 1
    return candidates, dict(observed)


def cross_release(new_references, new_qrels, old_references, old_qrels):
    def indexes(references, qrels):
        return {
            "task_ids": set(qrels),
            "conversation_ids": {references[key]["conversation_id"] for key in qrels},
            "exact_question_json_tokens": {references[key]["input"][3] for key in qrels},
            "exact_positive_passage_rosters": {tuple(sorted(pointers)) for pointers in qrels.values()},
            "exact_positive_passage_pointers": {pointer for pointers in qrels.values() for pointer in pointers},
        }

    newer = indexes(new_references, new_qrels)
    older = indexes(old_references, old_qrels)
    return {key: len(newer[key] & older[key]) for key in newer}


def select(candidates):
    fields = ("task_id", "question_json_token_sha256", "positive_passage_roster_sha256")
    seen = {key: set() for key in fields}
    roster, repeats = [], Counter()
    for row in candidates:
        matches = [key for key in fields if row[key] in seen[key]]
        if matches:
            repeats.update(matches)
            continue
        for key in fields:
            seen[key].add(row[key])
        roster.append(row)
        if len(roster) == LIMIT:
            break
    return roster, dict(repeats)


def audit():
    started = time.monotonic()

    def check():
        if time.monotonic() - started >= 120:
            raise TimeoutError("original IBM Cloud metadata allowance expired")

    require(hashlib.sha256(READER.read_bytes()).hexdigest() == READER_SHA, "reviewed metadata reader changed")
    reader_type = runpy.run_path(str(READER))["MetadataReader"]
    _, license_pin = fetch(LICENSE, check)
    archive_pins = {"repository_license": license_pin}
    metadata, cases, counts = [], [], {}
    for name, total, cloud_total, query_count, qrel_input, reference_input in RELEASES:
        qrel_raw, qrel_pin = fetch(qrel_input, check)
        reference_raw, reference_pin = fetch(reference_input, check)
        qrels = qrel_metadata(qrel_raw)
        refs = reference_metadata(reference_raw, reader_type, check)
        require(len(refs) == total and len(qrels) == query_count
                and sum(row["Collection"] in CLOUD for row in refs.values()) == cloud_total,
                "official frozen row census differs")
        cloud_cases, rejected = classified(refs, qrels, name)
        metadata.extend((refs, qrels))
        cases.extend(cloud_cases)
        counts[name] = {
            "released_reference_tasks": total, "cloud_reference_tasks": cloud_total,
            "cloud_qrel_tasks": query_count, "cloud_positive_qrels": sum(map(len, qrels.values())),
            "answerable_human_standalone_candidates_before_deduplication": len(cloud_cases),
            "not_selected_by_metadata": rejected,
        }
        archive_pins[name] = {"cloud_qrels": qrel_pin, "published_reference_metadata": reference_pin}
    newer_refs, newer_qrels, older_refs, older_qrels = metadata
    older_questions = {row["input"][3] for row in older_refs.values()}
    older_evidence = {tuple(sorted(pointers)) for pointers in older_qrels.values()}
    preferred = []
    newer_repeats = 0
    for row in cases:
        if row["release"] == RELEASES[0][0]:
            if row["task_id"] in older_refs or row["question_json_token_sha256"] in older_questions:
                newer_repeats += 1
                continue
            row["exact_original_release_qrel_roster_reuse"] = (
                tuple(sorted(row["publisher_positive_passage_pointers"])) in older_evidence)
        preferred.append(row)
    roster, repeats = select(preferred)
    source = Path(__file__).resolve()
    check()
    return {
        "schema": "airk.private.ibm-mtrag-cloud-technical-support-screen/1",
        "repository_revision": REVISION,
        "collector": {"path": str(source), "sha256": hashlib.sha256(source.read_bytes()).hexdigest()},
        "sources": archive_pins,
        "metadata_reader_sha256": READER_SHA,
        "maximum_candidates": LIMIT,
        "counts": counts,
        "cross_release_exact_overlap": cross_release(newer_refs, newer_qrels, older_refs, older_qrels),
        "newer_candidates_with_exact_original_release_query_rejected": newer_repeats,
        "duplicate_candidate_matches": repeats,
        "selected_by_release": dict(Counter(row["release"] for row in roster)),
        "selected_conversations": len({row["conversation_id"] for row in roster}),
        "screening_candidates": roster,
        "task_text_or_answer_decoded": False,
        "corpus_downloaded": False,
        "judgment_role": "publisher positive passage relevance; complete acceptable evidence set remains unverified",
        "pending": {
            "rights": "repository Apache-2.0 pinned; underlying Cloud document rights need per-source verification",
            "exposure": "model and AIRK exposure, source-family overlap, cloning and paraphrases unverified",
            "original_release_chronology": "original MTRAG was public before the pinned Qwen3 release",
            "native_visibility": "passage offsets and citations not yet joined to exact native-visible documents",
            "menus": "no complete candidate menu furnished by qrels; freeze using an independently pinned corpus",
            "human_sufficiency": "native-visible sufficient evidence and alternative support sets not adjudicated",
        },
        "qualification": False,
        "elapsed_seconds": time.monotonic() - started,
    }


if __name__ == "__main__":
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and len(sys.argv) == 1, "use checked Windows Python -I -B -S with no arguments")
    print(json.dumps(audit(), allow_nan=False, separators=(",", ":")))
