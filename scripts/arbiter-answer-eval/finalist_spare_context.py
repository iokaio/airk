# SPDX-License-Identifier: Apache-2.0
"""Extend complete source lines using spare space in an already fitted finalist Gather response."""
import json
from dataclasses import dataclass

import finalist_gather_contract as contract


@dataclass(frozen=True)
class Widened:
    original: object
    at: str
    text: str

    def __getattr__(self, name):
        # Keep file-score metadata, need ownership, and the model-visible source extents unchanged.
        return getattr(self.original, name)


def escaped_content_size(text):
    """UTF-8 bytes of a string's content in a JSON string nested inside the JSON-RPC text result."""
    inner = json.dumps(text, ensure_ascii=False)[1:-1]
    return len(json.dumps(inner, ensure_ascii=False)[1:-1].encode())


def frame_size(payload, framing):
    return payload + (len(f"Content-Length: {payload}\r\n\r\n") if framing == "header" else 1)


def widen(arguments, repository, index, metrics, total, request_id, framing, base_wire, original,
          *, passage_limit=14000, **render_options):
    """Preserve baseline passages and provenance, then fit at most 16 additional lines on each side."""
    if not original:
        return base_wire, original
    if framing not in ("header", "line"):
        raise ValueError("Unknown MCP framing")
    payload = base_wire.split(b"\r\n\r\n",1)[1] if framing=="header" else base_wire[:-1]
    payload_bytes = len(payload)
    cap = contract.reply_cap(arguments)
    body = json.loads(json.loads(payload)["result"]["content"][0]["text"])
    emitted = body["passages"]
    if len(emitted)!=len(original):
        raise ValueError("Fitted passages differ from wire")
    spent = sum(len(p.text.encode()) for p in original)
    if spent>passage_limit or len(base_wire)>cap:
        raise ValueError("Baseline exceeds its source or transport budget")
    span, used, multipliers = [], set(), []
    for row,p in zip(emitted,original,strict=True):
        if (row["at"],row["text"])!=(p.at,p.text):
            raise ValueError("Wire source range mismatch")
        path,extent = p.at.rsplit(":",1)
        a,b = map(int,extent.split("-"))
        span.append([path,a,b])
        used.update((path,n) for n in range(a,b+1))
        # Only the returned location and its per-need 'where' entries move.
        # The scored range and file-card locations remain original citations.
        multipliers.append(1+sum(x["at"]==p.at for need in body["needs"] for x in need.get("where",[])))
    line_costs = {}
    for _ in range(16):
        changed = False
        for i,extent in enumerate(span):
            path,a,b = extent
            lines = repository.texts[path]
            for left in (True,False):
                n = a-1 if left else b+1
                if not 1<=n<=len(lines) or (path,n) in used:
                    continue
                line = lines[n-1]+"\n"
                if line not in line_costs:
                    line_costs[line] = len(line.encode()), escaped_content_size(line)
                extra, quoted = line_costs[line]
                digits = len(str(n)) - len(str(a if left else b))
                increment = quoted + digits*multipliers[i]
                if spent+extra>passage_limit or frame_size(payload_bytes+increment,framing)>cap:
                    continue
                a,b = min(a,n),max(b,n)
                extent[1:] = [a,b]
                used.add((path,n))
                spent += extra
                payload_bytes += increment
                changed = True
        if not changed:
            break
    result = []
    for p,(path,a,b) in zip(original,span,strict=True):
        at = f"{path}:{a}-{b}"
        result.append(p if p.at==at else Widened(p,at,"\n".join(repository.texts[path][a-1:b])+"\n"))
    if all(a is b for a,b in zip(original,result,strict=True)):
        return base_wire, original
    blob,selected = contract.render(arguments,repository,index,metrics,result,total,request_id,framing,**render_options)
    if selected!=result or len(blob)!=frame_size(payload_bytes,framing):
        raise ValueError("Finalist frame differs from exact spare-byte estimate")
    return blob,selected


def render(arguments, repository, index, metrics, passages, total, request_id, framing,
           *, passage_limit=14000, **render_options):
    """Drop-in finalist renderer for an explicitly selected spare-context treatment."""
    wire,kept = contract.render(arguments,repository,index,metrics,passages,total,request_id,framing,**render_options)
    return widen(arguments,repository,index,metrics,total,request_id,framing,wire,kept,
                 passage_limit=passage_limit,**render_options)
