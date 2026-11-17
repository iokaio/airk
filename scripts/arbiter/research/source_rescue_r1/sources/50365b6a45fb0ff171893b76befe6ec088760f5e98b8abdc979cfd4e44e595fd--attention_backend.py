# SPDX-License-Identifier: Apache-2.0
"""Private stock-ORT dispatch observation only; no execution or qualification."""
import hashlib
import re

MAX_LOG_BYTES = 64 << 20
FIELDS = re.compile(
    r"Operator=(GroupQueryAttention|MultiHeadAttention) Node=([^\s]+) "
    r"DataType=(fp16|fp32|bf16) SdpaKernel=([A-Z_]+)"
    r"(?: (?:NumSplits|GqaGroupSize|EffectiveKvLengthBound)=[0-9]+)*"
    r"(?: XqaPageTable=(?:expanded|native))?"
)

def observe(raw, *, nodes, forward_roster, completed_forwards, operator, backend):
    """Consume caller-frozen graph/forward rosters, never construct them from logs.

    Caller authenticates source/image/graph/input/rosters, native completion+sync,
    operation journal, peaks and natural Job settlement under its original limits.
    Each forward is single-model/single-lane. Node scheduling order may vary.
    """
    if not isinstance(raw, bytes) or len(raw) > MAX_LOG_BYTES:
        raise ValueError("original raw-log type/64MiB bound differs")
    if (operator, backend) not in {
        ("GroupQueryAttention", "FLASH_ATTENTION"),
        ("MultiHeadAttention", "EFFICIENT_ATTENTION"),
    }:
        raise ValueError("unsupported frozen G5 operator/backend pair")
    if (not isinstance(nodes, tuple) or not nodes or len(nodes) > 128
            or any(not isinstance(n, str) or not n or any(c.isspace() for c in n)
                   or any(ord(c) < 32 for c in n) for n in nodes)
            or len(set(nodes)) != len(nodes)):
        raise ValueError("complete unique frozen graph-node roster required")
    if (not isinstance(forward_roster, tuple) or not forward_roster
            or len(forward_roster) > 4096
            or any(not isinstance(f, str) or not f for f in forward_roster)
            or len(set(forward_roster)) != len(forward_roster)):
        raise ValueError("complete unique native forward roster required")
    if not isinstance(completed_forwards, tuple) or completed_forwards != forward_roster:
        raise ValueError("actual completed native forwards differ from original scheduled denominator")
    text = raw.decode("utf-8", errors="strict")
    required = set(nodes)
    seen = set()
    groups = []
    records = []
    for number, original in enumerate(text.splitlines(), 1):
        # Remove only the exact stock observer color wrappers in the derived view.
        line = original
        if line.startswith("\x1b[36m") and line.endswith("\x1b[0m"):
            line = line[5:-4]
        if "Operator=" not in line and "SdpaKernel=" not in line:
            continue  # Option configuration, libtest and other native logs retained raw.
        match = FIELDS.fullmatch(line)
        if match is None:
            raise ValueError(f"malformed/interleaved attention observation at line {number}")
        actual_operator, node, dtype, actual_backend = match.groups()
        if (actual_operator, dtype, actual_backend) != (operator, "fp16", backend):
            raise ValueError(f"wrong operator/dtype/backend at line {number}")
        if node not in required or node in seen:
            raise ValueError(f"unknown/duplicate graph node at line {number}")
        if len(groups) >= len(forward_roster):
            raise ValueError("attention observation exceeds original forward denominator")
        seen.add(node)
        records.append({"raw_line": number, "node": node})
        if seen == required:
            groups.append({"forward": forward_roster[len(groups)], "observations": records})
            seen = set()
            records = []
    if seen or len(groups) != len(forward_roster):
        raise ValueError("missing/truncated full graph or original forward denominator")
    return {
        "scope": "stock-ORT selected attention backend observations only",
        "raw_bytes": len(raw), "raw_sha256": hashlib.sha256(raw).hexdigest(),
        "operator": operator, "dtype": "fp16", "backend": backend,
        "nodes": list(nodes), "forward_roster": list(forward_roster),
        "groups": groups, "qualification_granted": False,
    }
