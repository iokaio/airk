# SPDX-License-Identifier: Apache-2.0
"""Exercise original DOCX/XLSX table evidence through the built public CLI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def write_package(path, parts):
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, text in parts.items():
            archive.writestr(name, text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-table-smoke-") as temporary:
        base = Path(temporary)
        root = base / "sources"
        root.mkdir()
        xlsx = root / "requirements.xlsx"
        docx = root / "requirements.docx"
        write_package(xlsx, {
            "xl/workbook.xml": '<workbook xmlns:r="r"><sheets><sheet name="Requirements" r:id="r1"/></sheets></workbook>',
            "xl/_rels/workbook.xml.rels": '<Relationships><Relationship Id="r1" Type="o/worksheet" Target="worksheets/spec.xml"/></Relationships>',
            "xl/worksheets/spec.xml": '<worksheet><sheetData><row r="1"><c t="inlineStr"><is><t>ID</t></is></c><c t="inlineStr"><is><t>Requirement</t></is></c></row><row r="14"><c r="A14" t="inlineStr"><is><t>REQ-9</t></is></c><c r="B14" t="inlineStr"><is><t>The API must respond in at most 200 ms.</t></is></c><c r="C14"><f>1+1</f><v>2</v></c><c r="D14" t="str"><f>""</f><v/></c><c r="E14" t="b"><f>1=1</f></c><c r="F14"><f>2+2</f><v/></c></row><row r="20"><c r="A20" t="inlineStr"><is><t>Merged anchor</t></is></c></row><row r="21"><c r="B21" t="inlineStr"><is><t>Stored continuation</t></is></c></row></sheetData><mergeCells><mergeCell ref="A20:C21"/></mergeCells></worksheet>',
        })
        write_package(docx, {
            "word/document.xml": '<w:document xmlns:w="w"><w:body><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>ID</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Requirement</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>REQ-8</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>The cache must hold at least 8 MiB.</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Ordinary background information.</w:t></w:r></w:p></w:body></w:document>',
        })
        originals = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in [xlsx, docx]}
        environment = dict(os.environ)
        environment.pop("AIRK_USAGE_MODE", None)

        def run(*command):
            completed = subprocess.run([str(binary), "--root", str(root), "--data-dir", str(base / "state"),
                                        "--collection", "tables", *command], capture_output=True, check=True, timeout=120, env=environment)
            return json.loads(completed.stdout)

        indexed = run("index")
        assert indexed["indexed"] == 2, indexed
        for filename, location in [
            (xlsx.name, {"WorksheetRow": {"sheet": "Requirements", "row": 14}}),
            (docx.name, {"TableRow": {"table": 1, "row": 2}}),
        ]:
            facts = run("facts", filename, "--kind", "requirement")["facts"]
            table_fact = next(fact for fact in facts if fact["citation"]["location"] == location)
            assert table_fact["citation"]["content_hash"] == originals[filename]
            assert table_fact["document"]["table"]["cells"][1]["header"] == "Requirement"
            assert table_fact["document"]["quantities"]
        formula = run("facts", xlsx.name, "--kind", "table_cell", "--name", "C14")["facts"][0]
        assert formula["document"]["table"]["cells"][0]["formula"] == "1+1"
        assert formula["document"]["table"]["cells"][0]["cached_value"] is True
        for address, present in [("D14", True), ("E14", False), ("F14", False)]:
            cell = run("facts", xlsx.name, "--kind", "table_cell", "--name", address)["facts"][0]
            assert cell["document"]["table"]["cells"][0]["cached_value"] is present
        continuation = run("facts", xlsx.name, "--kind", "table_cell", "--name", "B21")["facts"][0]
        assert continuation["document"]["table"]["cells"][0]["address"] == "B21"
        assert continuation["document"]["table"]["cells"][0]["column_span"] == 1
        assert continuation["document"]["table"]["cells"][0]["merged_range"] == "A20:C21"
        docx_requirements = run("facts", docx.name, "--kind", "requirement")["facts"]
        assert len(docx_requirements) == 1, "Header/body paragraphs must not become duplicate requirements"
        header = next(fact for fact in run("facts", docx.name, "--kind", "table_row")["facts"]
                      if fact["citation"]["location"]["TableRow"]["row"] == 1)
        assert header["document"]["table"]["is_header"] is True
        search = run("search", "--mode", "lexical", "200", "--path-prefix", xlsx.name)
        assert search["hits"][0]["citation"]["location"]["WorksheetRow"]["row"] == 14
        assert sum(len(hit["text"].encode()) for hit in run("search", "--mode", "lexical", "200", "--max-bytes", "17")["hits"]) <= 17
        for path in [xlsx, docx]:
            assert hashlib.sha256(path.read_bytes()).hexdigest() == originals[path.name], "Source file was modified"
        # Retrieval stays on indexed bytes until a successful new generation.
        xlsx.write_bytes(b"not a valid workbook")
        for name, worksheet in [
            ("wrong-part.xlsx", '<comments><sheetData><row><c t="inlineStr"><is><t>Fabricated wrong-part evidence</t></is></c></row></sheetData></comments>'),
            ("duplicate-values.xlsx", '<worksheet><sheetData><row><c><v>20</v><v>0</v></c></row></sheetData></worksheet>'),
        ]:
            write_package(root / name, {
                "xl/workbook.xml": '<workbook xmlns:r="r"><sheets><sheet name="Invalid" r:id="r1"/></sheets></workbook>',
                "xl/_rels/workbook.xml.rels": '<Relationships><Relationship Id="r1" Type="o/worksheet" Target="worksheets/spec.xml"/></Relationships>',
                "xl/worksheets/spec.xml": worksheet,
            })
        assert run("search", "--mode", "lexical", "200", "--path-prefix", xlsx.name)["generation"] == search["generation"]
        changed = run("index")
        assert changed["generation"] > indexed["generation"]
        assert not run("facts", xlsx.name)["facts"], "Failed replacement must not retain stale evidence"
        for name in ("wrong-part.xlsx", "duplicate-values.xlsx"):
            assert not run("facts", name)["facts"], "Malformed input must not publish table evidence"
            assert any(item["path"] == name for item in changed["skipped"]), changed
        assert run("facts", docx.name, "--kind", "table_row")["facts"], "Unrelated document lost"
        report = run("usage", "report")["report"]
        assert report["missing_baseline_calls"] == 0, report
        assert report["comparisons"]["full_returned_sources"]["compared_calls"] == report["eligible_evidence_calls"]
        assert report["content_kinds"]["docx"] > 0 and report["content_kinds"]["xlsx"] > 0
    print("DOCX/XLSX public CLI smoke passed: facts, headers, original coordinates/hashes, formula caches, merges, malformed inputs, budgets and retained revision.")


if __name__ == "__main__":
    main()
