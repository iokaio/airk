# SPDX-License-Identifier: Apache-2.0
"""Qualify explicitly supplied local providers through AIRK's public CLI; no installs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import zlib


def original_images():
    """Create an original block-letter raster and a two-page image-only PDF."""
    glyphs = [[14, 17, 17, 31, 17, 17, 17], [31, 4, 4, 4, 4, 4, 31],
              [30, 17, 17, 30, 20, 18, 17], [17, 18, 20, 24, 20, 18, 17], [0]*7,
              [14, 17, 1, 2, 4, 8, 31], [14, 17, 17, 17, 17, 17, 14],
              [14, 17, 17, 17, 17, 17, 14], [0]*7,
              [17, 27, 21, 21, 17, 17, 17], [15, 16, 16, 14, 1, 1, 30]]
    width, height = 850, 170
    rgb = bytearray([255] * (width * height * 3))
    for index, glyph in enumerate(glyphs):
        for y, bits in enumerate(glyph):
            for x in range(5):
                if bits & (1 << (4 - x)):
                    for dy in range(10):
                        for dx in range(10):
                            start = ((45 + y * 10 + dy) * width + 40 + index * 70 + x * 10 + dx) * 3
                            rgb[start:start+3] = b"\0\0\0"
    row = (width * 3 + 3) & ~3
    bmp = bytearray(54)
    bmp[:2] = b"BM"
    struct.pack_into("<I", bmp, 2, 54 + row * height)
    struct.pack_into("<I", bmp, 10, 54)
    struct.pack_into("<IiiHH", bmp, 14, 40, width, height, 1, 24)
    for y in reversed(range(height)):
        bmp.extend(rgb[y * width * 3:(y + 1) * width * 3])
        bmp.extend(bytes(row - width * 3))
    compressed = zlib.compress(bytes(rgb))
    # Keep the original raster scale after the adapter's 2,400-pixel page cap.
    # Enlarging these deliberately block-shaped glyphs threefold changes OCR
    # segmentation; this fixture checks integration/provenance, not font recall.
    drawing = b"q 850 0 0 170 775 365 cm /Image Do Q"
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>",
               b"<< /Type /Pages /Kids [5 0 R 6 0 R] /Count 2 >>",
               f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {len(compressed)} >>\nstream\n".encode() + compressed + b"\nendstream",
               f"<< /Length {len(drawing)} >>\nstream\n".encode() + drawing + b"\nendstream"]
    objects.extend([b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 2400 900] /Resources << /XObject << /Image 3 0 R >> >> /Contents 4 0 R >>"] * 2)
    pdf = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for index, content in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf.extend(f"{index} 0 obj\n".encode() + content + b"\nendobj\n")
    xref = len(pdf)
    pdf.extend(f"xref\n0 {len(objects)+1}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        pdf.extend(f"{offset:010} 00000 n \n".encode())
    pdf.extend(f"trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
    return bytes(bmp), bytes(pdf)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["binary", "lsp", "ctags", "tesseract", "tessdata", "pdftoppm"]:
        parser.add_argument(f"--{name}", type=Path, required=name == "binary")
    args = parser.parse_args()
    assert args.lsp or args.ctags or args.tesseract, "Supply at least one explicit local provider"
    binary = args.binary.resolve(strict=True)
    qualified = []
    environment = {**os.environ, "AIRK_USAGE_MODE": "disabled"}
    with tempfile.TemporaryDirectory(prefix="airk-provider-cli-") as temporary:
        base = Path(temporary)
        root = base / "sources"
        (root / "src").mkdir(parents=True)
        source = root / "src/lib.rs"
        source.write_text("pub fn one() -> u32 { 1 }\npub fn two() -> u32 { one() }\n", encoding="utf-8")
        (root / "Cargo.toml").write_text('[package]\nname="airk_provider_smoke"\nversion="0.1.0"\nedition="2021"\n', encoding="utf-8")
        original_source = source.read_bytes()

        def call(*command, succeeds=True):
            result = subprocess.run([str(binary), "--root", str(root), "--data-dir", str(base / "state"),
                                     "--collection", "providers", *map(str, command)], env=environment,
                                    capture_output=True, timeout=180)
            assert (result.returncode == 0) == succeeds, (command, result.stderr.decode(errors="replace"))
            return json.loads(result.stdout) if succeeds else None

        call("index")
        for adapter in ["lsp", "ctags"]:
            program = getattr(args, adapter)
            if not program:
                continue
            request = {"command": {"program": str(program.resolve(strict=True)), "args": [], "timeout_ms": 120000},
                       "producer": f"fixture-{adapter}", "producer_version": "explicit-local-test",
                       "build_inputs": ["Cargo.toml"], "paths": ["src/lib.rs"]}
            request_path = base / f"{adapter}.json"
            request_path.write_text(json.dumps(request), encoding="utf-8")
            published = call("analysis", adapter, "--request", request_path)
            facts = call("outline", "src/lib.rs", "--max-bytes", "65536")["facts"]
            imported = [fact for fact in facts if fact.get("code", {}).get("producer") == request["producer"]]
            assert {"one", "two"} <= {fact["name"] for fact in imported}, (adapter, facts)
            assert any(fact.get("code", {}).get("basis") == "syntax" for fact in facts), "Native evidence lost"
            symbol = next(fact["code"]["symbol_id"] for fact in imported if fact["name"] == "one")
            calls = call("relationships", symbol, "--kind", "call")["relationships"]
            assert bool(calls) == (adapter == "lsp"), calls
            assert source.read_bytes() == original_source
            assert not (root / "target").exists() and not (root / "Cargo.lock").exists()
            source.write_bytes(original_source + b"// changed source\n")
            call("analysis", adapter, "--request", request_path, succeeds=False)
            assert call("status")["generation"] == published["status"]["generation"]
            call("index")
            assert not any(fact.get("code", {}).get("basis") == "imported" for fact in call("outline", "src/lib.rs")["facts"])
            source.write_bytes(original_source)
            call("index")
            qualified.append(adapter)

        if args.tesseract:
            assert args.tessdata, "Tesseract requires explicit --tessdata"
            bmp, pdf = original_images()
            for name, content, page in [("fixture.bmp", bmp, 1)] + ([("fixture.pdf", pdf, 2)] if args.pdftoppm else []):
                (root / name).write_bytes(content)
                request = {"tesseract": {"program": str(args.tesseract.resolve(strict=True)), "args": [], "timeout_ms": 120000},
                           "pdf_renderer": {"program": str(args.pdftoppm.resolve(strict=True)), "args": [], "timeout_ms": 30000} if args.pdftoppm else None,
                           "tessdata_dir": str(args.tessdata.resolve(strict=True)), "language": "eng",
                           "first_page": page, "last_page": page, "dpi": 150}
                request_path = base / "ocr.json"
                request_path.write_text(json.dumps(request), encoding="utf-8")
                published = call("ocr", name, "--request", request_path)
                assert published["provenance"]["first_page"] == page
                hits = call("search", "200", "--path-prefix", name)["hits"]
                assert hits and all(hit["citation"]["location"] == {"Page": {"number": page}} for hit in hits)
                assert all(hit["citation"]["content_hash"] == hashlib.sha256(content).hexdigest() for hit in hits)
                call("index")
                assert call("search", "200", "--path-prefix", name)["hits"], "Unchanged OCR extraction was lost"
                assert (root / name).read_bytes() == content
                qualified.append("ocr-" + Path(name).suffix[1:])
    print("Local provider CLI qualification passed: " + ", ".join(qualified) + "; native evidence, provenance, original sources, stale rejection and reindex invalidation.")


if __name__ == "__main__":
    main()
