# SPDX-License-Identifier: Apache-2.0
"""Pure Focus text helpers that mirror the evaluation build's Rust sources.

Ports, kept byte-for-byte in behaviour:
- `model_view` and the pair template of
  `crates/airk-retrieval/src/arbiter/compiler/focus.rs` (`compile_focus_instruction`);
- the SoftwareEvidenceV1 instruction text of
  `crates/airk-retrieval/src/arbiter/compiler/focus/experiment.rs`;
- `identifiers` and `focus_window` of `crates/airk-kernel/src/arbiter/software.rs`.

No model, runtime or tokenizer is imported here; `compile_live_ids` takes any
object with `encode(text) -> list[int]` (the tokenizers library adapter in
`build_examples.py`, a fake in tests).
"""
from __future__ import annotations

import hashlib
import re

PUBLISHER = "Given a web search query, retrieve relevant passages that answer the query"
SOFTWARE_EVIDENCE_V1 = (
    "Given a software question, retrieve relevant code, documentation, or diagnostic "
    "evidence that helps answer the question."
)
INSTRUCTIONS = {"publisher": PUBLISHER, "software_evidence_v1": SOFTWARE_EVIDENCE_V1}
PREFIX = (
    "<|im_start|>system\nJudge whether the Document meets the requirements based on the "
    "Query and the Instruct provided. Note that the answer can only be \"yes\" or \"no\"."
    "<|im_end|>\n<|im_start|>user\n"
)
SUFFIX = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
PAD_ID = 151643
NO_ID = 2152
YES_ID = 9693
MAX_QUESTION_BYTES = 16_384
MAX_DOCUMENT_BYTES = 8192

STOP = {
    "GitHub", "JavaScript", "TypeScript", "PowerShell", "MacOS", "OpenAI", "WebAssembly",
    "YouTube", "iPhone", "VSCode", "NuGet", "DevOps", "PostgreSQL", "MySQL", "SQLite",
    "OAuth", "README", "TODO", "self_",
}


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def model_view(text: str) -> str:
    """Injective quoting of `\\`, `<`, `>` exactly as the Rust compiler does."""
    return text.replace("\\", "\\\\").replace("<", "\\u003c").replace(">", "\\u003e")


def pair_body(instruction: str, question: str, document: str) -> str:
    return f"<Instruct>: {instruction}\n<Query>: {model_view(question)}\n<Document>: {model_view(document)}"


def compile_live_ids(tokenizer, instruction: str, question: str, document: str, width: int):
    """Live (unpadded) token ids, or None when the pair exceeds `width` (never cropped)."""
    if not question.strip() or not document.strip():
        raise ValueError("Focus requires nonempty question and document")
    if len(question.encode("utf-8")) > MAX_QUESTION_BYTES or len(document.encode("utf-8")) > MAX_DOCUMENT_BYTES:
        return None
    ids = (list(tokenizer.encode(PREFIX)) + list(tokenizer.encode(pair_body(instruction, question, document)))
           + list(tokenizer.encode(SUFFIX)))
    if not ids or len(ids) > width:
        return None
    return ids


def padded(ids, width: int):
    """Left padding, position ids 0..width-1, decision at width-1 (the evaluation ABI)."""
    pad = width - len(ids)
    if pad < 0:
        raise ValueError("over-width pair")
    return [PAD_ID] * pad + list(ids), [0] * pad + [1] * len(ids)


def _ascii_alnum(ch: str) -> bool:
    return ("a" <= ch <= "z") or ("A" <= ch <= "Z") or ("0" <= ch <= "9")


def _identifier_like(word: str) -> bool:
    if not 3 <= len(word.encode("utf-8")) <= 96:
        return False
    first = word[0]
    if not (("a" <= first <= "z") or ("A" <= first <= "Z") or first == "_") or word in STOP:
        return False
    lower = any("a" <= c <= "z" for c in word)
    inner_capital = any("A" <= c <= "Z" for c in word[1:])
    snake = "_" in word and any(_ascii_alnum(c) for c in word)
    return (lower and inner_capital) or snake


def identifiers(text: str, limit: int):
    found: list[str] = []

    def push(candidate: str, quoted: bool) -> None:
        segments = [part.strip("".join(sorted({c for c in part if not (_ascii_alnum(c) or c == "_")})))
                    for part in candidate.split("::")]
        path = len(segments) > 1
        for index, name in enumerate(segments):
            plain = (3 <= len(name.encode("utf-8")) <= 96
                     and all(_ascii_alnum(c) or c == "_" for c in name)
                     and (("a" <= name[0] <= "z") or ("A" <= name[0] <= "Z"))
                     and name not in STOP)
            if path:
                accepted = plain and (index + 1 == len(segments) or ("A" <= name[0] <= "Z"))
            elif quoted:
                accepted = plain
            else:
                accepted = _identifier_like(name)
            if accepted and len(found) < limit and name not in found:
                found.append(name)

    rest = text
    while "`" in rest:
        start = rest.index("`")
        after = rest[start + 1:]
        end = after.find("`")
        if end < 0:
            break
        quoted = after[:end]
        if not any(c.isspace() for c in quoted):
            while quoted.endswith("()"):
                quoted = quoted[:-2]
            push(quoted, True)
        rest = after[end + 1:]
    for word in re.split(r"[^A-Za-z0-9_:]", text):
        word = word.strip(":")
        if word:
            push(word, False)
    return found


def focus_window(text: str, question: str, pair_tokens: int):
    """UTF-8 byte range (start, end) of the window the kernel gives Focus."""
    data = text.encode("utf-8")
    budget = max(max(pair_tokens * 2 - len(question.encode("utf-8")) * 2, 0) - 320, 0)
    budget = max(budget, 256)
    names = identifiers(question, 8)
    start = 0
    if len(data) > budget:
        hits = [data.find(name.encode("utf-8")) for name in names]
        hits = [h for h in hits if h >= 0]
        if hits:
            found = min(hits)
            line = data.rfind(b"\n", 0, found)
            start = 0 if line < 0 else line + 1

    def bounded(begin: int) -> int:
        end = min(begin + budget, len(data))
        while 0 < end < len(data) and (data[end] & 0xC0) == 0x80:
            end -= 1
        return end

    end = bounded(start)
    if end <= start:
        return 0, bounded(0)
    return start, end


def focus_document(text: str, question: str, pair_tokens: int) -> str:
    start, end = focus_window(text, question, pair_tokens)
    return text.encode("utf-8")[start:end].decode("utf-8")


def window_around(lines, first: int, last: int, target: int = 1200, cap: int = 4096):
    """Hit-like text: 1-based inclusive line range grown alternately until `target` bytes.

    A range longer than `cap` bytes is cut at a line boundary. Returns (text, first, last).
    """
    n = len(lines)
    first = max(1, min(first, n))
    last = max(first, min(last, n))
    size = sum(len(lines[i - 1].encode("utf-8")) + 1 for i in range(first, last + 1))
    if size > cap:
        total, end = 0, first - 1
        while end < last and total + len(lines[end].encode("utf-8")) + 1 <= cap:
            total += len(lines[end].encode("utf-8")) + 1
            end += 1
        last = max(first, end)
    else:
        up = True
        while size < target and (first > 1 or last < n):
            if (up and first > 1) or last >= n:
                first -= 1
                size += len(lines[first - 1].encode("utf-8")) + 1
            else:
                last += 1
                size += len(lines[last - 1].encode("utf-8")) + 1
            up = not up
    return "\n".join(lines[first - 1:last]), first, last
