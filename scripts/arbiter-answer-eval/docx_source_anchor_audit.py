# SPDX-License-Identifier: Apache-2.0
"""Check retained DOCX Focus windows against pinned OOXML source anchors."""

import argparse
from collections import Counter
from difflib import SequenceMatcher
from hashlib import sha256
from io import BytesIO
import json
from pathlib import Path, PurePosixPath
import xml.etree.ElementTree as ET
from zipfile import ZipFile

def require(condition, message):
    if not condition:
        raise ValueError(message)


W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def pinned(path, digest):
    data = path.read_bytes()
    require(sha256(data).hexdigest() == digest, "input digest differs")
    return json.loads(data)


def paragraph(node):
    return "".join(
        item.text or "" if item.tag == W + "t" else
        "\t" if item.tag == W + "tab" else
        "\n" if item.tag in {W + "br", W + "cr"} else ""
        for item in node.iter()
    )


def compact(text):
    return " ".join(text.split())


def anchors(data):
    with ZipFile(BytesIO(data)) as archive:
        body = ET.fromstring(archive.read("word/document.xml")).find(W + "body")
    require(body is not None, "missing DOCX body")
    paragraphs = [paragraph(node) for node in body.iter(W + "p")]
    body_paragraphs = [paragraph(node) for node in body.findall(W + "p")]
    blocks = {
        "all_paragraphs": paragraphs,
        "nonblank_paragraphs": [text for text in paragraphs if text.strip()],
        "body_paragraphs": body_paragraphs,
        "nonblank_body_paragraphs": [text for text in body_paragraphs if text.strip()],
    }
    tables = []
    for table in body.iter(W + "tbl"):
        rows = []
        for row in table.findall(W + "tr"):
            rows.append(" ".join(paragraph(p) for cell in row.findall(W + "tc")
                                 for p in cell.iter(W + "p")))
        tables.append(rows)
    return blocks, tables


def locate(text, kind, coordinate, document):
    original, tables = document
    needle = compact(text)
    if not needle:
        return "empty_window", [], None
    if kind == "TableRow":
        table, row = coordinate["table"], coordinate["row"]
        if not (1 <= table <= len(tables) and 1 <= row <= len(tables[table - 1])):
            return "unmapped_table_row", [], None
        row_text = compact(tables[table - 1][row - 1])
        if needle in row_text:
            return "anchored_table_row", ["physical_table_row"], None
        # A pipe between displayed cells does not occur in the original XML.
        without_separators = compact(text.replace(" | ", " "))
        if without_separators and without_separators in row_text:
            return "anchored_table_row", ["physical_table_row_without_display_separators"], None
        # A formatted row can repeat earlier header text. Accept it only if
        # the entire original row is recovered verbatim in order, ignoring
        # whitespace, and every inserted label occurs in this same table.
        alignment = SequenceMatcher(None, row_text, needle, autojunk=False)
        edits = alignment.get_opcodes()
        earlier_rows = [compact(value) for value in tables[table - 1][:row - 1]]
        if all(tag in {"equal", "insert"} for tag, *_ in edits):
            labels = [needle[first:last].strip(" |:") for tag, _, _, first, last in edits
                      if tag == "insert"]
            if all(not label or any(label in value for value in earlier_rows) for label in labels):
                return "anchored_table_row_with_source_labels", ["physical_table_row"], None
        return "table_row_text_unmatched", [], None
    ordinal = coordinate["number"]
    matches = [name for name, texts in original.items()
               if 1 <= ordinal <= len(texts) and needle in compact(texts[ordinal - 1])]
    if matches:
        return "anchored_block", matches, None
    anywhere = sum(needle in compact(text) for text in original["all_paragraphs"])
    return ("ambiguous_block_coordinate" if anywhere else "block_text_unmatched",
            [], anywhere)


def token(publication):
    return f"airk-r1:{publication['incarnation']}:{publication['sequence']}:{publication['digest']}"


def audit(args):
    data = {name: pinned(getattr(args, name), getattr(args, name + "_sha256"))
            for name in ("input", "binding", "off", "fidelity", "handle")}
    model, binding, off = (data[name] for name in ("input", "binding", "off"))
    fidelity, handle = data["fidelity"], data["handle"]
    require(model["schema"] == "airk.private.focus-shortlist-input/1"
            and binding["schema"] == "airk.private.focus-shortlist-binding/1"
            and binding["scorer_input_sha256"] == args.input_sha256
            and binding["input_sha256"]["off"] == args.off_sha256,
            "unbound Focus capture")
    require(fidelity["schema"] == "airk.private.census-source-fidelity/1"
            and fidelity["handle_sha256"] == args.handle_sha256
            and handle["state"] == "closed", "unbound source fidelity receipt")
    record = handle["indexes"]["bookbuilder-v2"]
    source = Path(record["root"]).resolve(strict=True)
    receipt, = [entry for entry in fidelity["repositories"] if entry["repository"] == "bookbuilder-v2"]
    captured = off["identity"]["indexes"]["bookbuilder"]
    require(captured["source_commit"] == record["source_commit"] == receipt["source_commit"]
            and captured["pipeline_digest"] == record["pipeline_digest"]
            and receipt["publication_token"] == record["publication_token"],
            "candidate source revision, pipeline or closed source publication differs")
    omitted = set().union(*(receipt[key] for key in ("other_absent", "intentionally_denied",
                           "indexed_denies", "source_hash_mismatches", "outside_export")))

    counts, conventions, documents, failures = Counter(), Counter(), {}, []
    for measured, bound in zip(model["rows"], binding["rows"], strict=True):
        repo = bound["repo"]
        require(measured["id"] == bound["id"]
                and token(bound["publication"]) == off["identity"]["indexes"][repo]["publication_token"],
                "capture row identity or native publication differs")
        for window, item in zip(measured["candidates"], bound["candidates"], strict=True):
            citation = item["citation"]
            relative = PurePosixPath(citation["path"])
            if relative.suffix.lower() != ".docx":
                continue
            require(repo == "bookbuilder" and not relative.is_absolute() and ".." not in relative.parts
                    and relative.as_posix() not in omitted, "DOCX outside authenticated source selection")
            text = window["text"]
            require(window["id"] == item["id"]
                    and sha256(text.encode("utf-8")).hexdigest() == item["window_text_sha256"],
                    "DOCX scoring window integrity differs")
            key = relative.as_posix(), citation["content_hash"]
            if key not in documents:
                path = (source / relative).resolve(strict=True)
                require(path.is_relative_to(source), "DOCX path escapes the pinned source")
                content = path.read_bytes()
                require(sha256(content).hexdigest() == key[1], "DOCX source bytes differ")
                documents[key] = anchors(content)
            kind, = citation["location"]
            require(kind in {"TableRow", "Block"}, "unexpected DOCX location kind")
            outcome, schemes, occurrences = locate(text, kind, citation["location"][kind], documents[key])
            counts[outcome] += 1
            conventions.update(schemes)
            if not outcome.startswith("anchored_"):
                failures.append({"candidate_id": item["id"], "kind": kind, "outcome": outcome,
                                 "matching_original_paragraphs": occurrences})
    return {"schema": "airk.private.docx-source-anchor-audit/1",
            "pins": {name: getattr(args, name + "_sha256") for name in data},
            "capture_publication": captured["publication_token"],
            "source_publication": record["publication_token"],
            "source_commit": record["source_commit"], "documents": len(documents),
            "candidates": sum(counts.values()), "results": counts,
            "matching_ordinal_conventions": conventions, "unverified": failures,
            "scope": ("Captured windows checked against original OOXML after whitespace normalization. "
                      "For source-labeled table renderings, checked complete ordered row text and "
                      "the presence of inserted labels in earlier rows of the same table. "
                      "The capture contains model windows, not necessarily full Search hit text.")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("input", "binding", "off", "fidelity", "handle"):
        parser.add_argument("--" + name, type=Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = audit(args)
    with args.output.open("x", encoding="utf-8", newline="\n") as out:
        json.dump(report, out, indent=2, sort_keys=True)
        out.write("\n")


if __name__ == "__main__":
    main()
