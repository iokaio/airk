# SPDX-License-Identifier: Apache-2.0
"""Reconstruct the first post-edit verification patch and validate the replay against the final archive."""
import argparse
import difflib
from pathlib import Path

import common


def diff(baseline, files):
    output = []
    for name, new in sorted(files.items()):
        original = baseline / name
        old = original.read_bytes().decode("utf-8") if original.is_file() else ""
        lines = difflib.unified_diff(old.splitlines(True), new.splitlines(True), f"a/{name}", f"b/{name}")
        output.extend(line if line.endswith("\n") else line + "\n" for line in lines)
    return "".join(output)


def reconstruct(folder):
    record = common.read_json(folder / "session.json")
    baseline = Path(record["prepared_workspace"]["baseline"]).resolve(strict=True)
    work = (folder / "work").resolve()
    events = common.read_jsonl(folder / "stream.jsonl")
    state, tools, usage = {}, {}, {}
    first, first_usage = None, None
    for event in events:
        message = event.get("message", {})
        blocks = message.get("content", [])
        if not isinstance(blocks, list):
            continue
        if event.get("type") == "assistant":
            if message.get("id") and message.get("usage"):
                usage[message["id"]] = message["usage"]
            for block in blocks:
                if block.get("type") != "tool_use":
                    continue
                tools[block["id"]] = block["name"]
                if block["name"] == "mcp__verification__run_tests" and state and first is None:
                    first, first_usage = dict(state), dict(usage)
        if event.get("type") != "user":
            continue
        for block in blocks:
            if block.get("type") != "tool_result" or block.get("is_error"):
                continue
            tool = tools.get(block.get("tool_use_id"))
            if tool not in ("Edit", "Write"):
                continue
            result = event["tool_use_result"]
            path = Path(result["filePath"])
            name = (work / path).resolve().relative_to(work).as_posix()
            original = (baseline / name).resolve()
            common.require(original.is_relative_to(baseline), "Unexpected baseline path")
            previous = state.get(name)
            if previous is None:
                previous = original.read_bytes().decode("utf-8") if original.is_file() else ""
            if tool == "Write":
                state[name] = result["content"]
            else:
                common.require(not result.get("userModified") and result["originalFile"] == previous,
                               f"Original file differs in successful edit receipt: {name}")
                old, new = result["oldString"], result["newString"]
                common.require(old and old in previous and (result["replaceAll"] or previous.count(old) == 1),
                               "Ambiguous edit cannot establish first-attempt bytes")
                state[name] = previous.replace(old, new, -1 if result["replaceAll"] else 1)
    final = diff(baseline, state)
    common.require(final.encode() == (folder / "changes.patch").read_bytes(), "Edits do not reproduce the final archived patch")
    basis = "first_post_edit_verification" if first is not None else "final_no_post_edit_verification"
    first_cost_usage = (first_usage if first_usage is not None else usage).values()
    totals = {key: sum(row.get(key, 0) for row in first_cost_usage) for key in (
        "input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")}
    keys = {"input_tokens": "input", "output_tokens": "output", "cache_creation_input_tokens": "cache_write",
            "cache_read_input_tokens": "cache_read"}
    complete = {k: sum(row.get(k, 0) for row in usage.values()) for k in keys}
    reconciled = all(complete[key] == record["tokens"][field] for key, field in keys.items())
    first_cost_evidence = {"matches_final_invoice_tokens": reconciled,
                           "first_tokens": totals if reconciled else None,
                           "final_streamed": complete, "final_invoice": record["tokens"]}
    return diff(baseline, first if first is not None else state), basis, first_cost_evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("first_patch", type=Path)
    args = parser.parse_args()
    text, basis, tokens = reconstruct(args.folder.resolve(strict=True))
    with args.first_patch.open("xb") as out:
        out.write(text.encode())
    print(basis, len(text.encode()), tokens)


if __name__ == "__main__":
    main()