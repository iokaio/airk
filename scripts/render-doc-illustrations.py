# SPDX-License-Identifier: Apache-2.0
"""Render AIRK's original explanatory diagrams (optional Pillow documentation tool)."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "docs/images"
INK = "#142D39"
MUTED = "#49626D"
TEAL = "#076D70"
BACKGROUND = "#F3F7F7"


def font(size, bold=False):
    candidates = (["C:/Windows/Fonts/segoeuib.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"]
                  if bold else ["C:/Windows/Fonts/segoeui.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"])
    for candidate in candidates:
        if Path(candidate).is_file():
            return ImageFont.truetype(candidate, size)
    return ImageFont.load_default(size=size)


def canvas(kicker, title, subtitle):
    image = Image.new("RGB", (1440, 820), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((56, 48, 155, 87), radius=10, fill=TEAL)
    draw.text((72, 52), "AIRK", font=font(26, True), fill="white")
    draw.text((177, 56), kicker.upper(), font=font(19, True), fill=MUTED)
    draw.text((56, 113), title, font=font(43, True), fill=INK)
    draw.text((58, 180), subtitle, font=font(24), fill=MUTED)
    return image, draw


def card(draw, box, title, lines, accent=False):
    draw.rounded_rectangle(box, radius=20, fill=TEAL if accent else "white", outline=TEAL if accent else "#CBDCDE", width=2)
    x, y, _, _ = box
    draw.text((x + 23, y + 22), title, font=font(26, True), fill="white" if accent else INK)
    for ordinal, line in enumerate(lines):
        draw.text((x + 23, y + 66 + ordinal * 32), line, font=font(21), fill="#D8F4F0" if accent else MUTED)


def arrow(draw, start, end):
    draw.line((start, end), fill=TEAL, width=4)
    x, y = end
    draw.polygon(((x, y), (x - 12, y - 7), (x - 12, y + 7)), fill=TEAL)


def main():
    OUTPUT.mkdir(exist_ok=True)
    image, draw = canvas("Local evidence for coding agents", "One workspace. A shared source of evidence.",
                         "AIRK indexes your files; your chosen agent decides how to use the results.")
    card(draw, (56, 279, 405, 466), "Your workspace", ["Source code and configuration", "Markdown, PDF, DOCX, PPTX", "XLSX requirements and tables"])
    card(draw, (495, 279, 880, 466), "AIRK on your machine", ["Native extraction and indexing", "Isolated workspace collection", "Bounded retrieval with citations"], True)
    card(draw, (970, 279, 1384, 466), "Your coding agents", ["Claude Code  /  Codex", "Gemini CLI  /  VS Code", "Other local stdio MCP clients"])
    arrow(draw, (413, 370), (483, 370))
    arrow(draw, (889, 370), (958, 370))
    draw.text((893, 329), "MCP", font=font(17, True), fill=TEAL)
    card(draw, (56, 518, 1384, 688), "A result you can inspect", ["src/auth.py  |  lines 12-24  |  source hash  |  indexed revision",
                                                               "Documents retain original pages, paragraphs, slides, table rows or worksheet cells."])
    draw.text((58, 731), "Each agent needs its own setup and approval. Its data-handling policy governs returned evidence.", font=font(22), fill=MUTED)
    image.save(OUTPUT / "local-evidence.png", optimize=True)

    image, draw = canvas("Freshness and control", "From a saved edit to cited context", "Indexing and retrieval remain separate, so each response refers to one published revision.")
    stages = [("1  Save files", ["Source remains", "authoritative"]),
              ("2  Index", ["Coalesce edits", "Extract and reuse"]),
              ("3  Publish", ["One complete", "collection revision"]),
              ("4  Retrieve", ["Agent requests", "bounded evidence"])]
    for ordinal, (title, lines) in enumerate(stages):
        x = 56 + ordinal * 343
        card(draw, (x, 282, x + 299, 476), title, lines, ordinal == 2)
        if ordinal < 3:
            arrow(draw, (x + 308, 380), (x + 331, 380))
    card(draw, (56, 533, 473, 703), "Coverage", ["Freshness at inventory time", "Partial and unknown stay visible"])
    card(draw, (492, 533, 909, 703), "Controls", ["Index Now, pause and resume", "Choose mode and storage"])
    card(draw, (928, 533, 1384, 703), "Traceability", ["Path, hash and original location", "Reindex to include later changes"])
    draw.text((58, 748), "Explanatory diagram, not an editor screenshot. Coverage is separate from retrieval quality.", font=font(22), fill=MUTED)
    image.save(OUTPUT / "index-to-context.png", optimize=True)


if __name__ == "__main__":
    main()
