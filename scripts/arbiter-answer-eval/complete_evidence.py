# SPDX-License-Identifier: Apache-2.0
"""Controller-only first Answer treatment for the frozen development ceiling."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path, PurePosixPath, PureWindowsPath

import common
import loop_reply


@dataclass(frozen=True)
class Passage:
    path: str
    start: int
    end: int
    text: str

    def hit(self):
        return {"citation": {"path": self.path, "start_line": self.start, "end_line": self.end},
                "text": self.text}


def source_passage(root: Path, span: dict) -> Passage:
    name = span["path"]
    common.require(isinstance(name, str) and name and "\\" not in name and ":" not in name
                   and not PureWindowsPath(name).is_absolute()
                   and all(part not in ("", ".", "..") for part in name.split("/")),
                   "invalid relative source path")
    path = (root / PurePosixPath(name)).resolve(strict=True)
    common.require(path.is_relative_to(root) and path.is_file(), "source outside frozen export")
    data = path.read_bytes()
    common.require(common.sha256_bytes(data) == span["source_sha256"], "source digest differs")
    common.require(b"\0" not in data, "extracted source needs authenticated line mapping")
    data.decode("utf-8", errors="strict")
    lines = data.split(b"\n")
    first, last = span["start_line"], span["end_line"]
    total = len(lines) - int(not lines[-1])
    common.require(type(first) is int and type(last) is int and 1 <= first <= last <= total,
                   "invalid frozen source range")
    excerpt = b"\n".join(lines[first - 1:last])
    if last < len(lines):
        excerpt += b"\n"
    common.require(common.sha256_bytes(excerpt) == span["excerpt_sha256"], "excerpt digest differs")
    return Passage(name, first, last, excerpt.decode("utf-8"))


@dataclass(frozen=True)
class CompleteEvidence:
    repo: str
    publication_token: str
    manifest_sha256: str
    passages: tuple[Passage, ...]
    essential_ids: tuple[str, ...]

    @classmethod
    def from_manifest(cls, manifest: bytes, *, expected_sha256: str, source_root: Path,
                      repo: str, slot: str, scenario_sha256: str, publication_token: str, essential_ids):
        common.require(common.sha256_bytes(manifest) == expected_sha256, "manifest digest differs")
        doc = json.loads(manifest)
        common.require(doc["schema"] == "airk.private.complete-evidence-input/1"
                       and (doc["repo"], doc["slot"], doc["scenario_sha256"], doc["publication_token"])
                       == (repo, slot, scenario_sha256, publication_token), "crossed case manifest")
        expected = tuple(essential_ids)
        common.require(expected and all(isinstance(i, str) and i for i in expected)
                       and len(set(expected)) == len(expected), "invalid expected essentials")
        root = Path(source_root).resolve(strict=True)
        passages, members = [], set()
        common.require(isinstance(doc["spans"], list), "invalid span roster")
        for span in doc["spans"]:
            ids = span["essential_ids"]
            common.require(isinstance(ids, list) and ids and all(i in expected for i in ids),
                           "invalid span membership")
            members.update(ids)
            passages.append(source_passage(root, span))
        common.require(members == set(expected), "incomplete essential membership")
        ordered = tuple(sorted(set(passages), key=lambda p: (p.path, p.start, p.end)))
        return cls(repo, publication_token, expected_sha256, ordered, expected)

    def augment(self, reply: dict) -> dict:
        blocks = reply["content"]
        common.require(not reply.get("isError") and len(blocks) == 1 and blocks[0]["type"] == "text",
                       "expected one successful Answer text block")
        body = json.loads(blocks[0]["text"])
        common.require(loop_reply.is_v3(body) and not loop_reply.is_ticket(body)
                       and "source_passages" not in body, "expected a final ordinary Answer")
        observed = [publication["token"] for publication in
                    (body.get("publication"), (body.get("scope") or {}).get("publication")) if publication]
        common.require(observed and all(token == self.publication_token for token in observed),
                       "Answer publication differs")
        body["source_passages"] = [p.hit() for p in self.passages]
        return {"content": [{"type": "text", "text": json.dumps(body, ensure_ascii=False, separators=(",", ":"))}]}

    def verify_dispatch(self, delivered_envelope: dict, recorded_output_sha256: str) -> dict:
        """Bind coverage to the first successful dynamic attempt's contentItems digest."""
        common.require(set(delivered_envelope) == {"content"}, "unexpected delivered envelope")
        blocks = delivered_envelope["content"]
        common.require(len(blocks) == 1 and blocks[0]["type"] == "text", "invalid delivered text")
        text = blocks[0]["text"]
        items = [{"type": "inputText", "text": text}]
        digest = common.sha256_bytes(common.canonical_json(items))
        common.require(digest == recorded_output_sha256, "dispatch content digest differs")
        body = json.loads(text)
        common.require(body.get("source_passages") == [p.hit() for p in self.passages],
                       "dispatched citation or excerpt differs")
        return {"manifest_sha256": self.manifest_sha256, "output_sha256": digest,
                "text_bytes": len(text.encode("utf-8")), "essential_ids": list(self.essential_ids)}
