# SPDX-License-Identifier: Apache-2.0
"""Bind the exact private IBM Cloud screen to publisher passages and TechQA metadata.

Run under checked Windows Python with -I -B -S and no arguments. The script
reads the frozen candidate receipt and original TechQA archive, fetches the
official IBM benchmark ZIP and reference metadata, and emits one private JSON
result to stdout. No source archive, evidence text or question text is written.
"""

from collections import Counter
from datetime import datetime, timezone
import hashlib
import io
from pathlib import Path
import runpy
import sys
import tarfile
import time
from urllib.parse import urlsplit
import zipfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
SCREEN = CAMPAIGN / "techqa-style-ibm-mtrag-cloud-screen-r1.json"
SCREEN_SHA = "779c51e74f6e73c6ff6c59749c1978b47f1ee2894f0044007e2d184e785c75f0"
COLLECTOR = HERE / "audit_mtrag_cloud_metadata.py"
COLLECTOR_SHA = "757541d9d3be7fa6af3ee2ff9e4cb3d750f787bde13efb214c114e3bab5f220a"
SOURCE_ROOT = ROOT / "artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1"
TRAIN_MANIFEST = SOURCE_ROOT / "techqa-source-disjoint-positive-pairs-r5/research-source-manifest.json"
TRAIN_MANIFEST_SHA = "3e9506f1e6c3b5223d1d77f9289c941402b5a41ab385cbc0cefdc9a02f770642"
TECH_MEMBER = ("TechQA/training_and_dev/training_dev_technotes.json", 291_957_707,
               "69261b5f4436d70379fd7172a8d97062c93f5d433c386d196937d198c66bf75a")
CLOUD_ZIP = ("corpora/passage_level/cloud.jsonl.zip", 21_310_415,
             "a0c5427013c65556aeddfdc246dd6068f3a7fad08ce6ae7adddaa9e87aae246d")
CLOUD_JSONL = ("cloud.jsonl", 126_307_701, "34e375cdcfda3efe96c603a7820eae0eb3df3e33dabddd566fca89288bd245f6")
CLOUD_BLOB_SHA = "1a97d1c8fa9bdef184edaf669680b1b5f162eb07"
QWEN3_PUBLICATION_DATE = "2025-04-29"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def source_url(value):
    if not isinstance(value, str):
        return None
    try:
        url = urlsplit(value.strip())
        if url.scheme not in {"http", "https"} or url.hostname is None or url.username is not None:
            return None
        return url.netloc.lower(), url.path or "/", url.query
    except ValueError:
        return None


def cloud_passages(blob, selected, reader_type, project, check):
    matches, ids = {}, set()
    digest = hashlib.sha256()
    with zipfile.ZipFile(io.BytesIO(blob)) as archive:
        members = archive.infolist()
        require(len(members) == 1 and (members[0].filename, members[0].file_size) == CLOUD_JSONL[:2],
                "exact official Cloud inner artifact required")
        expanded = 0
        with archive.open(members[0]) as member:
            while raw := member.readline((512 << 10) + 1):
                check()
                require(len(raw) <= 512 << 10, "Cloud passage exceeds record bound")
                expanded += len(raw)
                digest.update(raw)
                reader = reader_type(io.BytesIO(raw), check, maximum_bytes=512 << 10)
                row = project(reader, {"_id": reader.string, "id": reader.string, "url": reader.string})
                reader.finish(len(raw))
                pointer = row["_id"]
                require(pointer not in ids, "duplicate official Cloud passage pointer")
                ids.add(pointer)
                if pointer in selected:
                    require(pointer == row["id"] and source_url(row["url"]) is not None,
                            "selected publisher pointer or public URL differs")
                    # The publisher repeats the passage pointer in both ID fields.
                    # The first portion is the literal document grouping only.
                    matches[pointer] = {"document_id": pointer.rsplit("-", 2)[0], "url": row["url"]}
    require(expanded == CLOUD_JSONL[1] and digest.hexdigest() == CLOUD_JSONL[2] and len(ids) == 72_442
            and set(matches) == selected, "official Cloud passage closure differs")
    return matches, len(ids)


def annotation_dates(blob, selected, reader_type, project, check):
    found = {}
    for raw in io.BytesIO(blob):
        require(len(raw) <= 1 << 18, "publisher reference record exceeds bound")
        reader = reader_type(io.BytesIO(raw), check, maximum_bytes=1 << 18)

        def input_metadata():
            def utterance():
                return project(reader, {
                    "speaker": reader.string,
                    "metadata": lambda: project(reader, {"author_type": reader.string, "created_at": reader.atom}),
                })

            last = None
            for last in reader.collection(91, 93, utterance):
                pass
            return last

        row = project(reader, {"task_id": reader.string, "input": input_metadata})
        reader.finish(len(raw))
        key = row["task_id"]
        if key in selected:
            last = row["input"]
            stamp = last["metadata"]["created_at"]
            require(last["speaker"].casefold() == "user" and last["metadata"]["author_type"] == "human"
                    and type(stamp) in (float, int) and 1_600_000_000 <= stamp < 1_850_000_000
                    and key not in found, "expected epoch-seconds timestamp for the exact human question")
            found[key] = datetime.fromtimestamp(stamp, timezone.utc).isoformat()
    require(set(found) == selected, "published annotation date census differs")
    return found


def techqa_exposure(parser, project, check):
    archive_path = parser["ARCHIVE"]
    fence_path = parser["ARCHIVE_ROOT"] / "full-original-DEV-source-fence-r5.json"
    fence, fence_pin = parser["read_checked_json"](fence_path, parser["DEV_FENCE_SHA"])
    train = {row["original_gold_document_id"] for row in fence["records"] if row["answerable"] is True}
    dev = set(fence["entire_DEV_source_id_closure"])
    require(len(train) == 388 and len(dev) == 12_355 and len(train & dev) == 134,
            "exact TechQA exposure fences required")
    selected = train | dev
    metadata = {}
    require(archive_path.resolve(strict=True) == archive_path
            and not any(part.is_symlink() or part.is_junction() for part in (archive_path, *archive_path.parents)),
            "canonical frozen TechQA archive path required")
    sha = hashlib.sha256()
    with archive_path.open("rb") as source:
        before = parser["archive_state"](source)
        require(before[2] == parser["ARCHIVE_BYTES"], "exact original TechQA archive byte extent required")
        while block := source.read(16 << 20):
            check()
            sha.update(block)
        require(sha.hexdigest() == parser["ARCHIVE_SHA"], "exact original TechQA archive digest required")
        source.seek(0)
        with tarfile.open(fileobj=parser["DeadlineStream"](source, check), mode="r|gz") as archive:
            for entry in archive:
                if entry.name != TECH_MEMBER[0]:
                    continue
                require(entry.isfile() and entry.size == TECH_MEMBER[1], "exact shared TechQA member required")
                with archive.extractfile(entry) as source_member:
                    reader = parser["MetadataReader"](source_member, check, maximum_bytes=TECH_MEMBER[1])

                    def selected_text():
                        if reader.nonspace() == 34:
                            return reader.string()
                        reader.skip_opaque_value()
                        return None

                    def item():
                        identifier = reader.string()
                        reader.expect(58)
                        if identifier not in selected:
                            reader.skip_opaque_value()
                            return
                        attrs = ("sourceDocumentId", "date", "productName", "productId", "canonicalUrl")
                        def source_metadata():
                            if reader.nonspace() == 123:
                                return project(reader, {key: selected_text for key in attrs})
                            reader.skip_opaque_value()
                            return {}

                        row = project(reader, {"id": reader.string, "metadata": source_metadata})
                        require(identifier not in metadata and row.get("id") == identifier,
                                "TechQA identifier differs from the exact map")
                        metadata[identifier] = {key: row.get("metadata", {}).get(key) for key in attrs}

                    count = sum(1 for _ in reader.collection(123, 125, item))
                    member_pin = reader.finish(TECH_MEMBER[1])
                    require(member_pin["sha256"] == TECH_MEMBER[2] and count == 28_482,
                            "exact shared TechQA member bytes or map census differ")
                break
        require(parser["archive_state"](source) == before and set(metadata) == selected,
                "exact exposed TechQA source closure differs")
    return metadata, train, dev, {
        "fence": fence_pin,
        "archive": {"bytes": parser["ARCHIVE_BYTES"], "sha256": parser["ARCHIVE_SHA"]},
        "member": {"name": TECH_MEMBER[0], "bytes": TECH_MEMBER[1], "sha256": TECH_MEMBER[2]},
    }


def retained_train_metadata(reader_type, project, check):
    raw = TRAIN_MANIFEST.read_bytes()
    require(len(raw) == 1_190_517 and hashlib.sha256(raw).hexdigest() == TRAIN_MANIFEST_SHA,
            "retained TechQA training manifest differs")
    reader = reader_type(io.BytesIO(raw), check, maximum_bytes=len(raw))
    fields = ("sourceDocumentId", "date", "productName", "productId", "canonicalUrl")

    def document():
        return project(reader, {
            "original_document_id": reader.string,
            "original_metadata": lambda: project(reader, {key: reader.string for key in fields}),
        })

    result = project(reader, {"documents": lambda: list(reader.collection(91, 93, document))})
    reader.finish(len(raw))
    return result["documents"], {"path": str(TRAIN_MANIFEST), "bytes": len(raw), "sha256": TRAIN_MANIFEST_SHA}


def exposure_comparison(cloud, metadata, identifiers):
    selected = [metadata[identifier] for identifier in identifiers]
    cloud_ids = {row["document_id"] for row in cloud.values()}
    cloud_urls = {source_url(row["url"]) for row in cloud.values()}
    urls = {source_url(row["canonicalUrl"]) for row in selected} - {None}
    dates = sorted(row["date"] for row in selected if row["date"])
    return {
        "documents": len(identifiers),
        "documents_with_usable_canonical_url": sum(source_url(row["canonicalUrl"]) is not None for row in selected),
        "distinct_usable_canonical_urls": len(urls),
        "exact_document_id_matches": len(cloud_ids & (identifiers | {row["sourceDocumentId"] for row in selected})),
        "canonical_url_matches_ignoring_scheme_and_fragment": len(cloud_urls & urls),
        "shared_host_count": len({url[0] for url in cloud_urls} & {url[0] for url in urls}),
        "distinct_literal_product_id_values": len({row["productId"] for row in selected if row["productId"]}),
        "published_date_metadata_range": [dates[0], dates[-1]] if dates else [],
    }


def audit():
    started = time.monotonic()

    def check():
        if time.monotonic() - started >= 300:
            raise TimeoutError("original metadata evidence deadline expired")

    require(hashlib.sha256(COLLECTOR.read_bytes()).hexdigest() == COLLECTOR_SHA, "screening collector pin differs")
    collector = runpy.run_path(str(COLLECTOR))
    parser_path = collector["READER"]
    require(hashlib.sha256(parser_path.read_bytes()).hexdigest() == collector["READER_SHA"], "opaque reader pin differs")
    parser = runpy.run_path(str(parser_path))
    project = collector["object_projection"]
    reader = parser["MetadataReader"]
    screen, screen_pin = parser["read_checked_json"](SCREEN, SCREEN_SHA)
    cases = screen["screening_candidates"]
    require(screen["repository_revision"] == collector["REVISION"]
            and screen["qualification"] is False and len(cases) == 25, "exact private screen required")
    selected = {pointer for row in cases for pointer in row["publisher_positive_passage_pointers"]}
    require(len(selected) == 60, "exact private passage roster differs")
    blob, cloud_pin = collector["fetch"](CLOUD_ZIP, check)
    git_digest = hashlib.sha1(f"blob {len(blob)}\0".encode("ascii"))
    git_digest.update(blob)
    require(git_digest.hexdigest() == CLOUD_BLOB_SHA, "official immutable Git blob differs")
    cloud, passage_count = cloud_passages(blob, selected, reader, project, check)
    dates, reference_pins = {}, {}
    for release, _, _, _, _, spec in collector["RELEASES"]:
        blob, reference_pins[release] = collector["fetch"](spec, check)
        date_ids = {case["task_id"] for case in cases if case["release"] == release}
        dates.update(annotation_dates(blob, date_ids, reader, project, check))

    metadata, train, dev, techqa_pins = techqa_exposure(parser, project, check)
    training, training_pin = retained_train_metadata(reader, project, check)
    retained_train = {row["original_document_id"] for row in training}
    require(len(training) == len(retained_train) == 254 and retained_train <= (train - dev)
            and all(row["original_metadata"] == metadata[row["original_document_id"]] for row in training),
            "the separately retained TRAIN source manifest no longer matches the exact archive")
    date_counts = Counter((row["release"], dates[row["task_id"]][:10] < QWEN3_PUBLICATION_DATE) for row in cases)
    check()
    return {
        "schema": "airk.private.ibm-mtrag-cloud-source-metadata-bindings/1",
        "complete": True, "qualification": False,
        "screening_receipt": screen_pin, "collector": {"path": str(COLLECTOR), "sha256": COLLECTOR_SHA},
        "source": {"path": str(Path(__file__).resolve()), "sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},
        "official_cloud_archive": {**cloud_pin, "git_blob_sha": CLOUD_BLOB_SHA, "member": CLOUD_JSONL[0],
                                   "member_bytes": CLOUD_JSONL[1], "member_sha256": CLOUD_JSONL[2]},
        "published_cloud_passages": passage_count, "bound_passages": len(cloud),
        "bound_cases": len(cases), "distinct_literal_cloud_documents": len({row["document_id"] for row in cloud.values()}),
        "cloud_host_counts": dict(Counter(source_url(row["url"])[0] for row in cloud.values())),
        "publisher_reference_pins": reference_pins, "qwen3_publication_date": QWEN3_PUBLICATION_DATE,
        "annotation_dates_before_qwen3_publication": {release: count for (release, before), count in date_counts.items() if before},
        "techqa_exposure_pins": {**techqa_pins, "retained_train_manifest": training_pin},
        "techqa_exact_metadata_comparison": {
            "original_train_gold": exposure_comparison(cloud, metadata, train),
            "original_dev_menu": exposure_comparison(cloud, metadata, dev),
            "retained_train_candidate_documents": exposure_comparison(cloud, metadata, retained_train),
        },
        "cases": [{"task_id": case["task_id"], "release": case["release"], "conversation_id": case["conversation_id"],
                   "human_question_created_at": dates[case["task_id"]],
                   "published_positive_passage_pointers": case["publisher_positive_passage_pointers"]} for case in cases],
        "published_passage_bindings": cloud,
        "scope": {
            "document_or_question_text_decoded": False, "native_visibility_observed": False,
            "repository_license_scope": "Official IBM corpus usage instructions and Apache-2.0 repository release; per-source notice review pending.",
            "family_and_clone_overlap": "Equal source IDs and canonical URLs checked. Alias, product family and text-clone exposure remain open.",
            "chronology": "Annotator timestamps establish authoring dates only. Publisher release chronology and old model exposure remain distinct.",
            "human_judgment": "Positive relevance pointers joined. Complete ordered menus and acceptable-evidence-set sufficiency remain unverified.",
        },
        "elapsed_seconds": time.monotonic() - started,
    }


if __name__ == "__main__":
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and len(sys.argv) == 1, "use checked Windows Python -I -B -S with no arguments")
    import json

    print(json.dumps(audit(), allow_nan=False, separators=(",", ":")))
