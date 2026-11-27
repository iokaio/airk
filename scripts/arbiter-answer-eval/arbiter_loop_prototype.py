# SPDX-License-Identifier: Apache-2.0
"""Prototype of the single-stage Arbiter loop, outside the AIRK runtime.

Evaluation tooling for a development diagnostic, never product and never a
judged pass. It answers one question before the loop is built in the kernel:
with one judge model doing every model job, do the fixture's own read tools
reach the passages a task needs?

For each chosen case it takes the task and the ``needs`` and ``anchors`` a
calling agent wrote, and runs, against a ready fixture lifetime:

1. **Fetch.** One hybrid search per need, one per anchor, and one for the
   whole task. Anchor and task hits are offered to every need.
2. **Judge.** The model scores every (need, candidate) pair.
3. **Follow up**, for ``--rounds`` rounds, on each need's best passages:
   read the lines around them and judge the windows; and let the model
   choose which names found in them are worth a lookup, then search those
   names and judge the hits.
4. **Deliver.** Each need keeps its best-scored passages above ``--keep``,
   inside one byte budget shared in need order.

The judge is Qwen3-Reranker-4B in FP16 through PyTorch, the same prompt as
``arbiter_pair_probe.py``. The scenario's notes are never read here; scoring
the delivered passages against them is ``gather_context_loss_stage.py``'s
job, on the ``<case>.reply.json`` files this script writes.

    python -B arbiter_loop_prototype.py --handle <lifetime>/handle.json --scenarios <dir> \
        --needs <caller-needs.json> --model <checkpoint dir> --output <new dir> --cases munarium-C3 ...

``--arm task`` drops the caller inputs and uses the task as its only need.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import re
import sys
import time

import common
import evidence
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
from arbiter_probe_cases import ACTION, PASSAGE

SCHEMA = "airk.private.arbiter-loop-prototype/1"
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]{3,}")
WINDOW_LINES, WINDOW_STEP = 40, 25
MUST_READ = ("Given one need of a software development task, judge whether the Document is one of the few source "
             "passages a developer must read to meet that need, not merely a passage on the same topic.")
GUIDANCE = ("CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "README.md")
"""Files a project keeps its own working rules in; read at the root and beside the best passages."""
PLAIN_WORDS = frozenset("""this that with from have will would should could there their which where when what into
    than then them they were been being also only each every other some such more most must does none true false
    self return async await const static public private function class struct impl enum type string number""".split())


def names_in(text: str, limit: int) -> list[str]:
    """Identifier-like words of a passage: an underscore, an inner capital, or all capitals."""
    counts: dict[str, int] = {}
    for word in NAME.findall(text):
        shaped = "_" in word.strip("_") or word.isupper() or any(c.isupper() for c in word[1:])
        if shaped and len(word) >= 5 and word.lower() not in PLAIN_WORDS:
            counts[word] = counts.get(word, 0) + 1
    return [word for word, _ in sorted(counts.items(), key=lambda item: (-item[1], item[0]))[:limit]]


def windows(path: str, start: int, text: str) -> list[dict]:
    """Overlapping line windows of one read, each a candidate with its own lines."""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    found = []
    for offset in range(0, max(len(lines) - WINDOW_LINES + WINDOW_STEP, 1), WINDOW_STEP):
        part = lines[offset:offset + WINDOW_LINES]
        if part:
            found.append({"path": path, "start": start + offset, "end": start + offset + len(part) - 1,
                          "text": "\n".join(part)})
    return found


def merge(passages: list[dict]) -> list[dict]:
    """Passages of one file whose lines touch or overlap become one, keeping the best score."""
    merged: list[dict] = []
    for passage in sorted(passages, key=lambda item: (item["path"], item["start"], item["end"])):
        last = merged[-1] if merged else None
        if last and last["path"] == passage["path"] and passage["start"] <= last["end"] + 1:
            if passage["end"] > last["end"]:
                extra = passage["text"].split("\n")[last["end"] - passage["start"] + 1:]
                last["text"] = "\n".join([last["text"], *extra]) if extra else last["text"]
                last["end"] = passage["end"]
            last["score"] = max(last["score"], passage["score"])
            last["needs"] = sorted(set(last["needs"]) | set(passage["needs"]))
        else:
            merged.append({**passage, "needs": list(passage["needs"])})
    return merged


def deliver(per_need: list[list[dict]], keep: float, per_need_limit: int, budget: int, floor: int = 0,
            relative: float = 0.0, shares: bool = False) -> list[dict]:
    """Each need's best passages above ``keep``, taken in rounds across needs inside one byte budget. A
    need's first ``floor`` passages are delivered whatever they score, so no need comes back empty.
    With ``shares`` each need first spends its own part of the budget: a quarter of the budget is split
    evenly, the rest by how many passages each need kept. What a need leaves unspent goes to the others in
    a second sweep."""
    chosen, used, seen = [], 0, set()
    kept_all = []
    for ranked in per_need:
        if relative:
            # Dynamic: a passage stays while it scores at least ``relative`` of this need's best.
            best = ranked[0]["score"] if ranked else 0.0
            kept = [item for item in ranked if item["score"] >= relative * best and item["score"] > 0.0]
            kept_all.append((kept or ranked[:1])[:per_need_limit])
        else:
            kept_all.append((ranked[:floor] + [item for item in ranked[floor:] if item["score"] >= keep])
                            [:per_need_limit])
    total = sum(len(kept) for kept in kept_all) or 1
    allowance = [budget // (4 * len(per_need)) + (budget * 3 // 4) * len(kept) // total for kept in kept_all]
    spent = [0] * len(per_need)
    sweeps = (True, False) if shares else (False,)
    for limited, rank, index in ((limited, rank, index) for limited in sweeps for rank in range(per_need_limit)
                                 for index in range(len(per_need))):
        if True:
            kept = kept_all[index]
            if rank >= len(kept):
                continue
            item = kept[rank]
            key = (item["path"], item["start"], item["end"])
            size = len(item["text"].encode())
            if key in seen:
                row = next(row for row in chosen if (row["path"], row["start"], row["end"]) == key)
                if index not in row["needs"]:
                    row["needs"].append(index)
                continue
            if limited and spent[index] and spent[index] + size > allowance[index]:
                continue
            lines = set(range(item["start"], item["end"] + 1))
            held = {line for row in chosen if row["path"] == item["path"]
                    for line in range(row["start"], row["end"] + 1)}
            if len(lines & held) * 5 >= len(lines) * 4:
                continue  # at least four fifths of it is already delivered
            if used + size > budget:
                continue
            seen.add(key)
            used += size
            spent[index] += size
            chosen.append({"path": item["path"], "start": item["start"], "end": item["end"], "text": item["text"],
                           "score": item["score"], "needs": [index]})
    return merge(chosen)


class Judge:
    """Qwen3-Reranker yes/no pair scoring, FP16 on CUDA."""

    def __init__(self, model_dir: pathlib.Path, max_tokens: int, batch: int):
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer
        if not torch.cuda.is_available():
            raise RuntimeError("CUDA is not available; the prototype does not fall back to CPU")
        self.torch = torch
        self.tokenizer = AutoTokenizer.from_pretrained(model_dir, padding_side="left", local_files_only=True)
        self.model = AutoModelForCausalLM.from_pretrained(
            model_dir, local_files_only=True, device_map={"": 0}, torch_dtype=torch.float16).eval()
        self.yes = self.tokenizer.convert_tokens_to_ids("yes")
        self.no = self.tokenizer.convert_tokens_to_ids("no")
        self.prefix = self.tokenizer.encode(PREFIX, add_special_tokens=False)
        self.suffix = self.tokenizer.encode(SUFFIX, add_special_tokens=False)
        self.room = max_tokens - len(self.prefix) - len(self.suffix)
        self.batch, self.pairs, self.seconds, self.clipped = batch, 0, 0.0, 0

    def score(self, instruction: str, pairs: list[tuple[str, str]]) -> list[float]:
        torch, started = self.torch, time.monotonic()
        encoded = []
        for query, document in pairs:
            ids = self.tokenizer.encode(pair_text(instruction, query, document), add_special_tokens=False)
            self.clipped += len(ids) > self.room
            encoded.append(self.prefix + ids[:self.room] + self.suffix)
        order = sorted(range(len(encoded)), key=lambda index: len(encoded[index]))
        scores = [0.0] * len(encoded)
        for offset in range(0, len(order), self.batch):
            chunk = order[offset:offset + self.batch]
            inputs = self.tokenizer.pad([{"input_ids": encoded[index]} for index in chunk], padding=True,
                                        return_tensors="pt").to(self.model.device)
            with torch.no_grad():
                logits = self.model(**inputs).logits[:, -1, :]
            both = torch.stack([logits[:, self.no], logits[:, self.yes]], dim=1).float()
            for index, value in zip(chunk, torch.nn.functional.softmax(both, dim=1)[:, 1].tolist()):
                scores[index] = round(float(value), 6)
        self.pairs += len(pairs)
        self.seconds += time.monotonic() - started
        return scores


class Tools:
    """The fixture's read tools, with a count of calls."""

    def __init__(self, handle, repository: str, timeout: float, workers: int):
        self.handle, self.repository, self.timeout, self.calls, self.failed = handle, repository, timeout, 0, 0
        self.errors: list[dict] = []
        self.lengths: dict[str, int] = {}
        self.pool = concurrent.futures.ThreadPoolExecutor(workers)

    def _spans(self, tool: str, arguments: dict) -> list[dict]:
        outcome = common.call_tool_retrying(self.handle, tool, self.repository,
                                            {**arguments, "response_mode": "full"}, self.timeout)
        outcome = outcome[0] if isinstance(outcome, tuple) else outcome
        self.calls += 1
        if not outcome.ok:
            self.failed += 1
            self.errors.append({"tool": tool, "arguments": arguments, "error": outcome.error,
                                "text": (common.envelope_text(outcome.envelope) or "")[:300]})
            return []
        return [{"path": span.path, "start": span.start, "end": span.end, "text": span.text}
                for span in evidence.extract(common.envelope_body(outcome.envelope))
                if span.start is not None and span.end is not None and span.text]

    def _body(self, tool: str, arguments: dict):
        outcome, _ = common.call_tool_retrying(self.handle, tool, self.repository,
                                               {**arguments, "response_mode": "full"}, self.timeout)
        self.calls += 1
        if not outcome.ok:
            self.failed += 1
            self.errors.append({"tool": tool, "arguments": arguments, "error": outcome.error,
                                "text": (common.envelope_text(outcome.envelope) or "")[:300]})
            return None
        return common.envelope_body(outcome.envelope)

    def search_many(self, queries: list[str], limit: int, lexical: bool = False) -> list[list[dict]]:
        mode = {"mode": "lexical"} if lexical else {}
        return list(self.pool.map(lambda query: self._spans(
            "search", {"query": query, "limit": limit, "max_bytes": 65536, "full_chunks": True, **mode}), queries))

    def length(self, path: str) -> int:
        """Indexed lines of a file, 0 when it cannot be read by line."""
        if path not in self.lengths:
            body = self._body("file_info", {"path": path})
            self.lengths[path] = int((body or {}).get("indexed_lines") or 0)
        return self.lengths[path]

    def read_many(self, ranges: list[tuple[str, int, int]]) -> list[list[dict]]:
        """Bounded reads, each clamped to its file's own length."""
        def read(item):
            path, start, end = item
            last = self.length(path)
            if last == 0 or start > last:
                return []
            return self._spans("read_file", {"path": path, "start_line": max(start, 1), "end_line": min(end, last)})
        return list(self.pool.map(read, ranges))

    def around_many(self, spots: list[dict], margin: int) -> list[list[dict]]:
        return self.read_many([(spot["path"], spot["start"] - margin, spot["end"] + margin) for spot in spots])

    def list_many(self, prefixes: list[str]) -> list[list[str]]:
        def listing(prefix):
            body = self._body("list_files", {"prefix": prefix, "limit": 200} if prefix else {"limit": 200})
            return [path for path in (body or {}).get("paths", []) if isinstance(path, str)]
        return list(self.pool.map(listing, prefixes))


def add(pool: dict, items: list[dict]) -> list[tuple]:
    keys = []
    for item in items:
        key = (item["path"], item["start"], item["end"])
        pool.setdefault(key, item)
        keys.append(key)
    return keys


def run_case(tools: Tools, judge: Judge, task: str, needs: list[str], anchors: list[str], args) -> dict:
    pool: dict[tuple, dict] = {}
    scores: list[dict[tuple, float]] = [{} for _ in needs]
    trace = []

    def judge_new(offered: list[list[tuple]]) -> None:
        pairs, where = [], []
        for index, keys in enumerate(offered):
            for key in dict.fromkeys(keys):
                if key not in scores[index]:
                    pairs.append((needs[index], pool[key]["text"]))
                    where.append((index, key))
        for (index, key), value in zip(where, judge.score(PASSAGE, pairs)):
            scores[index][key] = value

    strict: list[dict[tuple, float]] = [{} for _ in needs]

    def final(index: int, key: tuple) -> float:
        """Relevance alone, or relevance times the stricter second judgment when that is on. Relevance
        saturates (many candidates of a need score above 0.98), so alone its order is noise."""
        if not args.second_pass:
            return scores[index][key]
        return round(scores[index][key] * strict[index].get(key, 0.0), 6)

    def ranked(index: int) -> list[tuple]:
        return sorted(scores[index], key=lambda key: (-final(index, key), -scores[index][key], key))

    def settle() -> None:
        """Give each need's most relevant candidates the stricter second judgment, once each."""
        if not args.second_pass:
            return
        where = []
        for index in range(len(needs)):
            by_relevance = sorted(scores[index], key=lambda key: (-scores[index][key], key))[:args.second_pass]
            where += [(index, key) for key in by_relevance if key not in strict[index]]
        values = judge.score(MUST_READ, [(needs[index], pool[key]["text"]) for index, key in where])
        for (index, key), value in zip(where, values):
            strict[index][key] = value

    def best(index: int) -> float:
        return max((final(index, key) for key in scores[index]), default=0.0)

    def open_tops() -> list[list[dict]]:
        """The best passages of every need still open. A need whose best passage already scores
        ``--settled`` stops fetching, so the rounds go to the needs that lack evidence."""
        return [[] if args.settled and best(index) >= args.settled
                else [pool[key] for key in ranked(index)[:args.follow]] for index in range(len(needs))]

    found = tools.search_many([*needs, *anchors, task], args.search_limit)
    shared = [key for hits in found[len(needs):] for key in add(pool, hits)]
    judge_new([add(pool, found[index]) + shared for index in range(len(needs))])
    settle()
    trace.append({"round": 0, "candidates": len(pool), "best": [round(best(index), 3) for index in range(len(needs))]})
    for round_number in range(1, args.rounds + 1):
        tops = open_tops()
        if not any(tops):
            break
        if round_number == 1:
            # Structure: the project's own guidance files, and the files beside each need's best passages.
            folders = [sorted({item["path"].rpartition("/")[0] for item in row}) for row in tops]
            every = sorted({folder for row in folders for folder in row})
            roots = sorted({""} | {folder.split("/")[0] for folder in every} | set(every))
            guides = [f"{root}/{name}" if root else name for root in roots for name in GUIDANCE]
            guide_keys = [key for spans in tools.read_many([(path, 1, args.head * 3) for path in guides])
                          for span in spans for key in add(pool, windows(span["path"], span["start"], span["text"]))]
            listings = dict(zip(every, tools.list_many([folder + "/" if folder else "" for folder in every])))
            known = {key[0] for key in pool}
            beside = [[path for folder in row for path in listings[folder] if path not in known][:args.beside_offer]
                      for row in folders]
            values = iter(judge.score(ACTION, [(needs[index], f"Read the file `{path}`.")
                                               for index, paths in enumerate(beside) for path in paths]))
            picked = []
            for paths in beside:
                scored = sorted(((next(values), path) for path in paths), reverse=True)
                picked.append([path for value, path in scored[:args.beside] if value >= args.lookup_keep])
            wanted_files = sorted({path for row in picked for path in row})
            heads = dict(zip(wanted_files, tools.read_many([(path, 1, args.head) for path in wanted_files])))
            judge_new([guide_keys + [key for path in row for span in heads[path]
                                     for key in add(pool, windows(span["path"], span["start"], span["text"]))]
                       for row in picked])
            trace.append({"round": "structure", "candidates": len(pool), "guides": sorted({key[0] for key in guide_keys}),
                          "beside": picked})
            settle()
            tops = open_tops()
        spots = list({(item["path"], item["start"], item["end"]): item for row in tops for item in row}.values())
        reads = dict(zip([(spot["path"], spot["start"], spot["end"]) for spot in spots],
                         tools.around_many(spots, args.margin)))
        offered = []
        for row in tops:
            keys = []
            for item in row:
                for span in reads[(item["path"], item["start"], item["end"])]:
                    keys += add(pool, windows(span["path"], span["start"], span["text"]))
            offered.append(keys)
        judge_new(offered)
        # The model chooses which names in a need's best passages are worth a lookup.
        candidates = [names_in("\n".join(item["text"] for item in row), args.names) for row in tops]
        flat = [(needs[index], f"Search the repository for the name `{name}`.")
                for index, names in enumerate(candidates) for name in names]
        values = iter(judge.score(ACTION, flat))
        lookups: list[list[str]] = []
        for names in candidates:
            scored = sorted(((next(values), name) for name in names), reverse=True)
            lookups.append([name for value, name in scored[:args.lookups] if value >= args.lookup_keep])
        wanted = sorted({name for row in lookups for name in row})
        hits = dict(zip(wanted, tools.search_many(wanted, args.lookup_limit, lexical=True)))
        judge_new([[key for name in row for key in add(pool, hits[name])] for row in lookups])
        settle()
        trace.append({"round": round_number, "candidates": len(pool), "reads": len(spots), "lookups": lookups,
                      "open": sum(bool(row) for row in tops),
                      "best": [round(best(index), 3) for index in range(len(needs))]})
    per_need = [[{**pool[key], "score": final(index, key)} for key in ranked(index)] for index in range(len(needs))]
    delivered = deliver(per_need, args.keep, args.per_need, args.budget, args.floor, args.relative, args.shares)
    return {"needs": [{"need": need, "weak": not per_need[index] or per_need[index][0]["score"] < args.weak,
                       "best": [{"path": item["path"], "start": item["start"], "end": item["end"],
                                               "score": item["score"]} for item in per_need[index][:5]]}
                      for index, need in enumerate(needs)],
            "trace": trace, "delivered": delivered, "tool_errors": tools.errors,
            # Every judged candidate, without text, so a miss can be told apart: never fetched, or fetched
            # and scored low.
            "judged": [[[key[0], key[1], key[2], final(index, key)] for key in ranked(index)]
                       for index in range(len(needs))]}


def reply_of(delivered: list[dict]) -> dict:
    """The delivered passages in the record shape ``gather_context_loss_stage.py`` reads."""
    return {"records": [{"operation": "read_file", "arguments": {"path": item["path"]},
                         "canonical_result": {"hits": [{"citation": {
                             "path": item["path"], "start_line": item["start"], "end_line": item["end"],
                             "location": {"Lines": {"start": item["start"], "end": item["end"]}}},
                             "text": item["text"]}]}} for item in delivered]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, type=pathlib.Path)
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--needs", required=True, type=pathlib.Path)
    parser.add_argument("--model", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--cases", required=True, nargs="+")
    parser.add_argument("--arm", choices=("needs", "task"), default="needs")
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--search-limit", type=int, default=24)
    parser.add_argument("--follow", type=int, default=3, help="best passages per need to read around")
    parser.add_argument("--margin", type=int, default=60, help="lines read on each side of a passage")
    parser.add_argument("--names", type=int, default=24, help="names offered to the model per need")
    parser.add_argument("--lookups", type=int, default=3, help="names looked up per need and round")
    parser.add_argument("--lookup-keep", type=float, default=0.3)
    parser.add_argument("--lookup-limit", type=int, default=8)
    parser.add_argument("--beside", type=int, default=3, help="neighbouring files opened per need")
    parser.add_argument("--beside-offer", type=int, default=60, help="neighbouring files offered per need")
    parser.add_argument("--head", type=int, default=160, help="lines read from the top of a neighbouring file")
    parser.add_argument("--keep", type=float, default=0.3, help="lowest score a delivered passage may have")
    parser.add_argument("--per-need", type=int, default=12)
    parser.add_argument("--floor", type=int, default=0, help="best passages per need delivered whatever they score")
    parser.add_argument("--relative", type=float, default=0.0,
                        help="dynamic cutoff: keep a passage scoring at least this share of its need's best "
                             "(0 uses --keep and --floor)")
    parser.add_argument("--shares", action="store_true", help="give each need its own part of the byte budget")
    parser.add_argument("--settled", type=float, default=0.0,
                        help="a need whose best passage scores this much stops fetching (0 keeps every need open)")
    parser.add_argument("--weak", type=float, default=0.2, help="a need whose best score is below this is weak")
    parser.add_argument("--second-pass", type=int, default=0,
                        help="best candidates per need given the stricter second judgment (0 turns it off)")
    parser.add_argument("--budget", type=int, default=100_000, help="delivered passage bytes per case")
    parser.add_argument("--max-tokens", type=int, default=2048)
    parser.add_argument("--batch", type=int, default=16)
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args(argv)
    written = json.loads(args.needs.read_text(encoding="utf-8"))["cases"]
    handle = common.load_handle(args.handle.resolve(strict=True))
    args.output.mkdir()
    judge = Judge(args.model, args.max_tokens, args.batch)
    rows = []
    for case_id in args.cases:
        repository, slot = case_id.split("-", 1)
        task = json.loads((args.scenarios / repository / f"{slot}.json").read_text(encoding="utf-8"))["instruction"]
        # The task itself is the last need, so evidence for the task as a whole (project rules, for one)
        # is judged too, after every stated need has been served.
        needs = [*written[case_id]["needs"], task] if args.arm == "needs" else [task]
        anchors = list(written[case_id].get("anchors") or []) if args.arm == "needs" else []
        tools = Tools(handle, repository, args.timeout, args.workers)
        pairs, seconds, started = judge.pairs, judge.seconds, time.monotonic()
        result = run_case(tools, judge, task, needs, anchors, args)
        delivered = result["delivered"]
        row = {"case": case_id, "needs": len(needs), "anchors": len(anchors), "tool_calls": tools.calls,
               "tool_failures": tools.failed, "pairs": judge.pairs - pairs,
               "judge_seconds": round(judge.seconds - seconds, 1),
               "seconds": round(time.monotonic() - started, 1), "passages": len(delivered),
               "delivered_bytes": sum(len(item["text"].encode()) for item in delivered)}
        rows.append(row)
        for name, value in ((f"{case_id}.reply.json", reply_of(delivered)), (f"{case_id}.trace.json", result)):
            with (args.output / name).open("x", encoding="utf-8", newline="\n") as file:
                file.write(json.dumps(value, indent=1, sort_keys=True) + "\n")
        print(" ".join(f"{key}={value}" for key, value in row.items()), flush=True)
    run = {"schema": SCHEMA, "handle_sha256": handle.sha256, "stages": handle.live_stages(),
           "model": str(args.model), "precision": "float16", "clipped_inputs": judge.clipped,
           "settings": {key: value for key, value in vars(args).items()
                        if isinstance(value, (int, float, str)) and key not in ("arm",)},
           "arm": args.arm, "rows": rows}
    with (args.output / "run.json").open("x", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(run, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
